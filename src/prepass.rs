use crate::{
    mesh_material::{
        DynamicInstanceIndex, InstanceIndex, InstanceRenderAssets, PreviousMeshUniform,
    },
    view::{PreviousViewUniform, PreviousViewUniformOffset, PreviousViewUniforms},
    PREPASS_SHADER_HANDLE,
};
use bevy::{
    ecs::system::{
        lifetimeless::{Read, SRes},
        SystemParamItem,
    },
    pbr::{
        DrawMesh, MeshPipelineKey, MeshUniform, MAX_CASCADES_PER_LIGHT, MAX_DIRECTIONAL_LIGHTS,
        SHADOW_FORMAT,
    },
    prelude::*,
    render::{
        camera::ExtractedCamera,
        extract_component::{ComponentUniforms, DynamicUniformIndex},
        mesh::MeshVertexBufferLayout,
        render_asset::RenderAssets,
        render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType},
        render_phase::{
            sort_phase_system, AddRenderCommand, CachedRenderPipelinePhaseItem, DrawFunctionId,
            DrawFunctions, PhaseItem, RenderCommand, RenderCommandResult, RenderPhase,
            SetItemPipeline, TrackedRenderPass,
        },
        render_resource::*,
        renderer::{RenderContext, RenderDevice},
        texture::{GpuImage, TextureCache},
        view::{ExtractedView, ViewUniform, ViewUniformOffset, ViewUniforms, VisibleEntities},
        Extract, Render, RenderApp, RenderSet,
    },
    utils::FloatOrd,
};

pub const POSITION_FORMAT: TextureFormat = TextureFormat::Rgba32Float;
pub const NORMAL_FORMAT: TextureFormat = TextureFormat::Rgba8Snorm;
pub const INSTANCE_MATERIAL_FORMAT: TextureFormat = TextureFormat::Rg16Uint;
pub const VELOCITY_UV_FORMAT: TextureFormat = TextureFormat::Rgba16Snorm;

pub struct PrepassPlugin;
impl Plugin for PrepassPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                // [0.8] refer Opaque3d
                .init_resource::<DrawFunctions<Prepass>>()
                // [0.8] animate_shader: CustomMaterialPlugin CustomPipeline
                .init_resource::<PrepassPipeline>()
                .init_resource::<SpecializedMeshPipelines<PrepassPipeline>>()
                .add_render_command::<Prepass, DrawPrepass>()
                .add_systems(
                    ExtractSchedule,
                    extract_prepass_camera_phases.in_set(RenderSet::ExtractCommands),
                )
                .add_systems(
                    Render,
                    (
                        prepare_prepass_targets.in_set(RenderSet::Prepare),
                        queue_prepass_meshes.in_set(RenderSet::Queue),
                        queue_prepass_bind_group.in_set(RenderSet::Queue),
                        sort_phase_system::<Prepass>.in_set(RenderSet::PhaseSort),
                    ),
                );
        }
    }
}

#[derive(Resource)]
pub struct PrepassPipeline {
    pub view_layout: BindGroupLayout,
    pub mesh_layout: BindGroupLayout,
}

impl FromWorld for PrepassPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        let view_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(ViewUniform::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(PreviousViewUniform::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let mesh_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(MeshUniform::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(PreviousMeshUniform::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(InstanceIndex::min_size()),
                    },
                    count: None,
                },
            ],
        });

        Self {
            view_layout,
            mesh_layout,
        }
    }
}

impl SpecializedMeshPipeline for PrepassPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayout,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let vertex_attributes = vec![
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
        ];
        let vertex_buffer_layout = layout.get_layout(&vertex_attributes)?;
        let bind_group_layout = vec![self.view_layout.clone(), self.mesh_layout.clone()];

        let mut shader_defs = Vec::new();
        shader_defs.push(ShaderDefVal::Int(
            "MAX_DIRECTIONAL_LIGHTS".to_string(),
            MAX_DIRECTIONAL_LIGHTS as i32,
        ));
        shader_defs.push(ShaderDefVal::Int(
            "MAX_CASCADES_PER_LIGHT".to_string(),
            MAX_CASCADES_PER_LIGHT as i32,
        ));

        Ok(RenderPipelineDescriptor {
            label: None,
            layout: bind_group_layout,
            vertex: VertexState {
                shader: PREPASS_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: shader_defs.clone(),
                entry_point: "vertex".into(),
                buffers: vec![vertex_buffer_layout],
            },
            fragment: Some(FragmentState {
                shader: PREPASS_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: shader_defs.clone(),
                entry_point: "fragment".into(),
                targets: vec![
                    Some(ColorTargetState {
                        format: POSITION_FORMAT,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    }),
                    Some(ColorTargetState {
                        format: NORMAL_FORMAT,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    }),
                    Some(ColorTargetState {
                        format: INSTANCE_MATERIAL_FORMAT,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    }),
                    Some(ColorTargetState {
                        format: VELOCITY_UV_FORMAT,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    }),
                ],
            }),
            push_constant_ranges: Vec::new(),
            primitive: PrimitiveState {
                topology: key.primitive_topology(),
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: true,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState::default(),
        })
    }
}

fn extract_prepass_camera_phases(
    mut commands: Commands,
    cameras_3d: Extract<Query<(Entity, &Camera), With<Camera3d>>>,
) {
    for (entity, camera) in cameras_3d.iter() {
        if camera.is_active {
            commands
                .get_or_spawn(entity)
                .insert(RenderPhase::<Prepass>::default());
        }
    }
}

#[derive(Component)]
pub struct PrepassTarget {
    pub position: GpuImage,
    pub normal: GpuImage,
    pub instance_material: GpuImage,
    pub velocity_uv: GpuImage,
    pub depth: GpuImage,
}

fn prepare_prepass_targets(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    mut texture_cache: ResMut<TextureCache>,
    cameras: Query<(Entity, &ExtractedCamera), With<RenderPhase<Prepass>>>,
) {
    for (entity, camera) in &cameras {
        if let Some(size) = camera.physical_target_size {
            let extent = Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            };
            let size = size.as_vec2();
            let texture_usage = TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;

            let mut create_texture = |texture_format| -> GpuImage {
                let sampler = render_device.create_sampler(&SamplerDescriptor {
                    label: None,
                    address_mode_u: AddressMode::ClampToEdge,
                    address_mode_v: AddressMode::ClampToEdge,
                    address_mode_w: AddressMode::ClampToEdge,
                    mag_filter: FilterMode::Nearest,
                    min_filter: FilterMode::Nearest,
                    mipmap_filter: FilterMode::Nearest,
                    ..Default::default()
                });
                let texture = texture_cache.get(
                    &render_device,
                    TextureDescriptor {
                        label: None,
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: texture_format,
                        usage: texture_usage,
                        view_formats: &[],
                    },
                );
                GpuImage {
                    texture: texture.texture,
                    texture_view: texture.default_view,
                    texture_format,
                    sampler,
                    size,
                    mip_level_count: 1,
                }
            };

            let position = create_texture(POSITION_FORMAT);
            let normal = create_texture(NORMAL_FORMAT);
            let instance_material = create_texture(INSTANCE_MATERIAL_FORMAT);
            let velocity_uv = create_texture(VELOCITY_UV_FORMAT);
            let depth = create_texture(SHADOW_FORMAT);

            commands.entity(entity).insert(PrepassTarget {
                position,
                normal,
                instance_material,
                velocity_uv,
                depth,
            });
        }
    }
}

fn queue_prepass_meshes(
    draw_functions: Res<DrawFunctions<Prepass>>,
    render_meshes: Res<RenderAssets<Mesh>>,
    prepass_pipeline: Res<PrepassPipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<PrepassPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    meshes: Query<(Entity, &Handle<Mesh>, &MeshUniform, &DynamicInstanceIndex)>,
    mut views: Query<(&ExtractedView, &VisibleEntities, &mut RenderPhase<Prepass>)>,
) {
    let draw_function = draw_functions.read().get_id::<DrawPrepass>().unwrap();
    for (view, visible_entities, mut prepass_phase) in &mut views {
        let rangefinder = view.rangefinder3d();

        let add_render_phase = |(entity, mesh_handle, mesh_uniform, _): (
            Entity,
            &Handle<Mesh>,
            &MeshUniform,
            &DynamicInstanceIndex,
        )| {
            if let Some(mesh) = render_meshes.get(mesh_handle) {
                let key = MeshPipelineKey::from_primitive_topology(mesh.primitive_topology);
                let pipeline_id =
                    pipelines.specialize(&mut pipeline_cache, &prepass_pipeline, key, &mesh.layout);
                let pipeline_id = match pipeline_id {
                    Ok(id) => id,
                    Err(err) => {
                        error!("{}", err);
                        return;
                    }
                };
                prepass_phase.add(Prepass {
                    distance: rangefinder.distance(&mesh_uniform.transform),
                    entity,
                    pipeline: pipeline_id,
                    draw_function,
                });
            }
        };

        visible_entities
            .entities
            .iter()
            .filter_map(|visible_entity| meshes.get(*visible_entity).ok())
            .for_each(add_render_phase);
    }
}

#[derive(Resource, Debug)]
pub struct PrepassBindGroup {
    pub view: BindGroup,
    pub mesh: BindGroup,
}

#[allow(clippy::too_many_arguments)]
fn queue_prepass_bind_group(
    mut commands: Commands,
    prepass_pipeline: Res<PrepassPipeline>,
    render_device: Res<RenderDevice>,
    mesh_uniforms: Res<ComponentUniforms<MeshUniform>>,
    previous_mesh_uniforms: Res<ComponentUniforms<PreviousMeshUniform>>,
    instance_render_assets: Res<InstanceRenderAssets>,
    view_uniforms: Res<ViewUniforms>,
    previous_view_uniforms: Res<PreviousViewUniforms>,
) {
    if let (
        Some(view_binding),
        Some(previous_view_binding),
        Some(mesh_binding),
        Some(previous_mesh_binding),
        Some(instance_indices_binding),
    ) = (
        view_uniforms.uniforms.binding(),
        previous_view_uniforms.uniforms.binding(),
        mesh_uniforms.binding(),
        previous_mesh_uniforms.binding(),
        instance_render_assets.instance_indices.binding(),
    ) {
        let view = render_device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &prepass_pipeline.view_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: view_binding,
                },
                BindGroupEntry {
                    binding: 1,
                    resource: previous_view_binding,
                },
            ],
        });
        let mesh = render_device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &prepass_pipeline.mesh_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: mesh_binding,
                },
                BindGroupEntry {
                    binding: 1,
                    resource: previous_mesh_binding,
                },
                BindGroupEntry {
                    binding: 2,
                    resource: instance_indices_binding,
                },
            ],
        });
        commands.insert_resource(PrepassBindGroup { view, mesh });
    }
}

pub struct Prepass {
    pub distance: f32,
    pub entity: Entity,
    pub pipeline: CachedRenderPipelineId,
    pub draw_function: DrawFunctionId,
}

impl PhaseItem for Prepass {
    type SortKey = FloatOrd;

    #[inline]
    fn sort_key(&self) -> Self::SortKey {
        FloatOrd(self.distance)
    }

    #[inline]
    fn draw_function(&self) -> DrawFunctionId {
        self.draw_function
    }

    #[inline]
    fn entity(&self) -> Entity {
        self.entity
    }
}

impl CachedRenderPipelinePhaseItem for Prepass {
    #[inline]
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}

// [0.8] maybe could refer DrawShadowMesh
type DrawPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassMeshBindGroup<1>,
    DrawMesh,
);

pub struct SetPrepassViewBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetPrepassViewBindGroup<I> {
    type Param = SRes<PrepassBindGroup>;
    type ViewWorldQuery = (Read<ViewUniformOffset>, Read<PreviousViewUniformOffset>);
    type ItemWorldQuery = ();

    fn render<'w>(
        item: &P,
        (view_uniform, previous_view_uniform): bevy::ecs::query::ROQueryItem<
            'w,
            Self::ViewWorldQuery,
        >,
        entity: bevy::ecs::query::ROQueryItem<'w, Self::ItemWorldQuery>,
        bind_group: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(
            I,
            &bind_group.into_inner().view,
            &[view_uniform.offset, previous_view_uniform.offset],
        );

        RenderCommandResult::Success
    }
}

pub struct SetPrepassMeshBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetPrepassMeshBindGroup<I> {
    type Param = SRes<PrepassBindGroup>;

    type ViewWorldQuery = ();

    type ItemWorldQuery = (
        Read<DynamicUniformIndex<MeshUniform>>,
        Read<DynamicUniformIndex<PreviousMeshUniform>>,
        Read<DynamicInstanceIndex>,
    );

    fn render<'w>(
        item: &P,
        _view: bevy::ecs::query::ROQueryItem<'w, Self::ViewWorldQuery>,
        (mesh_uniform, previous_mesh_uniform, instance_index): bevy::ecs::query::ROQueryItem<
            'w,
            Self::ItemWorldQuery,
        >,
        bind_group: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(
            I,
            &bind_group.into_inner().mesh,
            &[
                mesh_uniform.index(),
                previous_mesh_uniform.index(),
                instance_index.0,
            ],
        );

        RenderCommandResult::Success
    }
}

pub struct PrepassNode {
    query: QueryState<
        (
            &'static ExtractedCamera,
            &'static RenderPhase<Prepass>,
            &'static Camera3d,
            &'static PrepassTarget,
        ),
        With<ExtractedView>,
    >,
}

impl PrepassNode {
    pub const IN_VIEW: &'static str = "view";

    pub fn new(world: &mut World) -> Self {
        Self {
            query: world.query_filtered(),
        }
    }
}

impl Node for PrepassNode {
    fn input(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::IN_VIEW, SlotType::Entity)]
    }

    fn update(&mut self, world: &mut World) {
        self.query.update_archetypes(world);
    }

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let entity = graph.get_input_entity(Self::IN_VIEW)?;

        trace!("entity is {:?}", entity);
        let (camera, prepass_phase, camera_3d, target) = match self.query.get_manual(world, entity)
        {
            Ok(query) => query,
            Err(_) => return Ok(()),
        };

        {
            #[cfg(feature = "trace")]
            let _main_prepass_span = info_span!("main_prepass").entered();
            let ops = Operations {
                load: LoadOp::Clear(Color::NONE.into()),
                store: true,
            };
            let pass_descriptor = RenderPassDescriptor {
                label: Some("main_prepass"),
                color_attachments: &[
                    Some(RenderPassColorAttachment {
                        view: &target.position.texture_view,
                        resolve_target: None,
                        ops,
                    }),
                    Some(RenderPassColorAttachment {
                        view: &target.normal.texture_view,
                        resolve_target: None,
                        ops,
                    }),
                    Some(RenderPassColorAttachment {
                        view: &target.instance_material.texture_view,
                        resolve_target: None,
                        ops,
                    }),
                    Some(RenderPassColorAttachment {
                        view: &target.velocity_uv.texture_view,
                        resolve_target: None,
                        ops,
                    }),
                ],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &target.depth.texture_view,
                    depth_ops: Some(Operations {
                        load: camera_3d.depth_load_op.clone().into(),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            };

            let mut render_pass = render_context.begin_tracked_render_pass(pass_descriptor);
            if let Some(viewport) = camera.viewport.as_ref() {
                render_pass.set_camera_viewport(viewport);
            }

            trace!("prepass phase render now");
            for item in prepass_phase.items.iter() {
                trace!("prepass phase item is {:?}", item.entity());
            }

            prepass_phase.render(&mut render_pass, world, entity);
        }

        Ok(())
    }
}
