use std::ops::Range;

use crate::{light::LightPassTarget, OVERLAY_SHADER_HANDLE, QUAD_HANDLE};
use bevy::{
    core_pipeline::clear_color::ClearColorConfig,
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    pbr::{DrawMesh, MeshPipelineKey},
    prelude::{shape::Quad, *},
    render::{
        camera::ExtractedCamera,
        mesh::MeshVertexBufferLayout,
        render_asset::RenderAssets,
        render_graph::{NodeRunError, RenderGraphContext, ViewNode},
        render_phase::{
            AddRenderCommand, CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions,
            PhaseItem, RenderCommand, RenderCommandResult, RenderPhase, SetItemPipeline,
            TrackedRenderPass,
        },
        render_resource::*,
        renderer::{RenderContext, RenderDevice},
        texture::BevyDefault,
        view::ViewTarget,
        Extract, Render, RenderApp, RenderSet,
    },
    utils::{nonmax::NonMaxU32, FloatOrd},
};

pub struct OverlayPlugin;
impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);

        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<DrawFunctions<Overlay>>()
                .init_resource::<SpecializedMeshPipelines<OverlayPipeline>>()
                .init_resource::<OverlayBindGroup>()
                .add_render_command::<Overlay, DrawOverlay>()
                .add_systems(
                    ExtractSchedule,
                    extract_overlay_camera_phases.in_set(RenderSet::ExtractCommands),
                )
                .add_systems(
                    Render,
                    (
                        prepare_overlay_bind_group.in_set(RenderSet::PrepareBindGroups),
                        queue_overlay_mesh.in_set(RenderSet::Queue),
                    ),
                );
        }
    }

    fn finish(&self, app: &mut App) {
        app.sub_app_mut(RenderApp)
            .init_resource::<OverlayPipeline>();
    }
}

fn setup(mut meshes: ResMut<Assets<Mesh>>) {
    let mesh: Mesh = Quad::new(Vec2::new(2.0, 2.0)).into();
    meshes.insert(QUAD_HANDLE, mesh);
}

#[derive(Resource)]
pub struct OverlayPipeline {
    pub overlay_layout: BindGroupLayout,
}

impl FromWorld for OverlayPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let overlay_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        Self { overlay_layout }
    }
}

// [0.8] refer MeshPipeline
impl SpecializedMeshPipeline for OverlayPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayout,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let vertex_attributes = vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0)];
        let vertex_buffer_layout = layout.get_layout(&vertex_attributes)?;
        let bind_group_layout = vec![self.overlay_layout.clone()];

        let shader_defs = Vec::new();

        Ok(RenderPipelineDescriptor {
            label: None,
            layout: bind_group_layout,
            vertex: VertexState {
                shader: OVERLAY_SHADER_HANDLE,
                shader_defs: shader_defs.clone(),
                entry_point: "vertex".into(),
                buffers: vec![vertex_buffer_layout],
            },
            fragment: Some(FragmentState {
                shader: OVERLAY_SHADER_HANDLE,
                shader_defs: shader_defs.clone(),
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::bevy_default(),
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
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
            depth_stencil: None,
            multisample: MultisampleState {
                count: key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        })
    }
}

// [0.8] refer extract_core_3d_camera_phases
fn extract_overlay_camera_phases(
    mut commands: Commands,
    cameras_3d: Extract<Query<(Entity, &Camera), With<Camera3d>>>,
) {
    for (entity, camera) in cameras_3d.iter() {
        if camera.is_active {
            commands
                .get_or_spawn(entity)
                .insert(RenderPhase::<Overlay>::default());
        }
    }
}

#[derive(Default, Resource)]
pub struct OverlayBindGroup {
    bind_group: Option<BindGroup>,
}

fn prepare_overlay_bind_group(
    render_device: Res<RenderDevice>,
    pipeline: Res<OverlayPipeline>,
    query: Query<(Entity, &LightPassTarget)>,
    mut overlay_bind_group: ResMut<OverlayBindGroup>,
) {
    for (entity, target) in &query {
        info!("over bind group entity is {:?}", entity);
        let bind_group = render_device.create_bind_group(
            None,
            &pipeline.overlay_layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&target.render.texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&target.render.sampler),
                },
            ],
        );
        overlay_bind_group.bind_group = Some(bind_group);
    }
}

#[allow(clippy::too_many_arguments)]
fn queue_overlay_mesh(
    mut commands: Commands,
    msaa: Res<Msaa>,
    draw_functions: Res<DrawFunctions<Overlay>>,
    render_meshes: Res<RenderAssets<Mesh>>,
    overlay_pipeline: Res<OverlayPipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<OverlayPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    mut views: Query<&mut RenderPhase<Overlay>>,
) {
    let draw_function = draw_functions.read().get_id::<DrawOverlay>().unwrap();
    for mut overlay_phase in &mut views {
        let mesh_handle = QUAD_HANDLE;
        if let Some(mesh) = render_meshes.get(&mesh_handle) {
            let key = MeshPipelineKey::from_msaa_samples(msaa.samples())
                | MeshPipelineKey::from_primitive_topology(mesh.primitive_topology);
            let pipeline_id =
                pipelines.specialize(&mut pipeline_cache, &overlay_pipeline, key, &mesh.layout);
            let pipeline_id = match pipeline_id {
                Ok(id) => id,
                Err(err) => {
                    error!("{}", err);
                    return;
                }
            };
            let entity = commands.spawn_empty().insert(mesh_handle.clone()).id();
            overlay_phase.add(Overlay {
                distance: 0.0,
                entity,
                pipeline: pipeline_id,
                draw_function,
                batch_range: 0..1,
                dynamic_offset: None,
            });
        }
    }
}

// [0.8] maybe refer from Qpaque3d
pub struct Overlay {
    pub distance: f32,
    pub entity: Entity,
    pub pipeline: CachedRenderPipelineId,
    pub draw_function: DrawFunctionId,
    pub batch_range: Range<u32>,
    pub dynamic_offset: Option<NonMaxU32>,
}

impl PhaseItem for Overlay {
    type SortKey = FloatOrd;

    #[inline]
    fn entity(&self) -> Entity {
        self.entity
    }

    #[inline]
    fn sort_key(&self) -> Self::SortKey {
        FloatOrd(self.distance)
    }

    #[inline]
    fn draw_function(&self) -> DrawFunctionId {
        self.draw_function
    }

    #[inline]
    fn batch_range(&self) -> &Range<u32> {
        &self.batch_range
    }

    #[inline]
    fn batch_range_mut(&mut self) -> &mut Range<u32> {
        &mut self.batch_range
    }

    #[inline]
    fn dynamic_offset(&self) -> Option<NonMaxU32> {
        self.dynamic_offset
    }

    #[inline]
    fn dynamic_offset_mut(&mut self) -> &mut Option<NonMaxU32> {
        &mut self.dynamic_offset
    }
}

impl CachedRenderPipelinePhaseItem for Overlay {
    #[inline]
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}

// [0.8] refer DrawWireframes
type DrawOverlay = (SetItemPipeline, SetOverlayBindGroup<0>, DrawMesh);

pub struct SetOverlayBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetOverlayBindGroup<I> {
    type Param = SRes<OverlayBindGroup>;
    type ViewWorldQuery = ();
    type ItemWorldQuery = ();

    #[inline]
    fn render<'w>(
        _item: &P,
        _view: bevy::ecs::query::ROQueryItem<'w, Self::ViewWorldQuery>,
        _entity: bevy::ecs::query::ROQueryItem<'w, Self::ItemWorldQuery>,
        bind_group: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let overlay_bind_group = bind_group.into_inner();
        info!("render overlay");
        // todo! sometimes bindgroup is None
        pass.set_bind_group(I, overlay_bind_group.bind_group.as_ref().unwrap(), &[]);
        RenderCommandResult::Success
    }
}

#[derive(Default)]
pub struct OverlayPassNode;

impl ViewNode for OverlayPassNode {
    type ViewQuery = (
        &'static ExtractedCamera,
        &'static RenderPhase<Overlay>,
        &'static Camera3d,
        &'static ViewTarget,
    );

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (camera, overlay_phase, camera_3d, target): bevy::ecs::query::QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        info!("overlay pass node run");

        // [0.8] refer MainPass3dNode::run() main_opaque_pass_3d section
        {
            // let _main_prepass_span = info_span!("main_prepass").entered();

            let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("main_prepass"),
                color_attachments: &[Some(target.get_color_attachment(Operations {
                    load: match camera_3d.clear_color {
                        ClearColorConfig::Default => {
                            LoadOp::Clear(world.resource::<ClearColor>().0.into())
                        }
                        ClearColorConfig::Custom(color) => LoadOp::Clear(color.into()),
                        ClearColorConfig::None => LoadOp::Load,
                    },
                    store: true,
                }))],
                depth_stencil_attachment: None,
            });
            if let Some(viewport) = camera.viewport.as_ref() {
                render_pass.set_camera_viewport(viewport);
            }
            for item in overlay_phase.items.iter() {
                info!("overlay phase item is {:?}", item.entity());
            }

            let view_entity = graph.view_entity();
            info!("overlay phase view_entity: {:?}", view_entity);

            overlay_phase.render(&mut render_pass, world, view_entity);
        }

        info!("finish overplay node run.");

        Ok(())
    }
}
