use crate::{light::LightPassTarget, OVERLAY_SHADER_HANDLE};
use bevy::{
    prelude::*,
    core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    ecs::system::{lifetimeless::Read, SystemParamItem},
    render::{
        camera::ExtractedCamera,
        render_graph::{NodeRunError, RenderGraphContext, ViewNode},
        render_phase::{
            AddRenderCommand, CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions,
            PhaseItem, RenderCommand, RenderCommandResult, RenderPhase, SetItemPipeline,
            TrackedRenderPass,
        },
        render_resource::*,
        renderer::{RenderContext, RenderDevice},
        view::ViewTarget,
        Extract, Render, RenderApp, RenderSet,
    },
    utils::FloatOrd,
};

pub struct OverlayPlugin;
impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<DrawFunctions<Overlay>>()
                .init_resource::<SpecializedRenderPipelines<OverlayPipeline>>()
                .add_render_command::<Overlay, DrawOverlay>()
                .add_systems(
                    ExtractSchedule,
                    extract_overlay_camera_phases.in_set(RenderSet::ExtractCommands),
                )
                .add_systems(
                    Render,
                    (
                        queue_overlay_bind_groups.in_set(RenderSet::Queue),
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

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct OverlayPipelineKey;

// [0.8] refer MeshPipeline
impl SpecializedRenderPipeline for OverlayPipeline {
    type Key = OverlayPipelineKey;

    fn specialize(&self, _key: Self::Key) -> RenderPipelineDescriptor {
        // let vertex_attributes = vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0)];
        // let vertex_buffer_layout = layout.get_layout(&vertex_attributes)?;
        let bind_group_layout = vec![self.overlay_layout.clone()];

        let shader_defs = Vec::new();

        RenderPipelineDescriptor {
            label: None,
            layout: bind_group_layout,
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader: OVERLAY_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: shader_defs.clone(),
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Bgra8UnormSrgb,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            push_constant_ranges: Vec::new(),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            // multisample: MultisampleState {
            //     count: key.msaa_samples(),
            //     mask: !0,
            //     alpha_to_coverage_enabled: false,
            // },
        }
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

#[derive(Component, Resource)]
pub struct OverlayBindGroup(pub BindGroup);

fn queue_overlay_bind_groups(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline: Res<OverlayPipeline>,
    query: Query<(Entity, &LightPassTarget)>,
) {
    for (entity, target) in &query {
        let bind_group = render_device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &pipeline.overlay_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&target.render.texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&target.render.sampler),
                },
            ],
        });
        commands.entity(entity).insert(OverlayBindGroup(bind_group));
    }
}

#[allow(clippy::too_many_arguments)]
fn queue_overlay_mesh(
    draw_functions: Res<DrawFunctions<Overlay>>,
    overlay_pipeline: Res<OverlayPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<OverlayPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    mut views: Query<(Entity, &mut RenderPhase<Overlay>)>,
) {
    let draw_function = draw_functions.read().get_id::<DrawOverlay>().unwrap();
    for (view_entity, mut overlay_phase) in &mut views {
        let pipeline_id = pipelines.specialize(
            &mut pipeline_cache,
            &overlay_pipeline,
            OverlayPipelineKey {},
        );
        overlay_phase.add(Overlay {
            distance: 0.0,
            entity: view_entity,
            pipeline: pipeline_id,
            draw_function,
        });
    }
}

// [0.8] maybe refer from Qpaque3d
pub struct Overlay {
    pub distance: f32,
    pub entity: Entity,
    pub pipeline: CachedRenderPipelineId,
    pub draw_function: DrawFunctionId,
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
}

impl CachedRenderPipelinePhaseItem for Overlay {
    #[inline]
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}

// [0.8] refer DrawWireframes
type DrawOverlay = (SetItemPipeline, SetOverlayBindGroup<0>);

pub struct SetOverlayBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetOverlayBindGroup<I> {
    type Param = ();
    type ViewWorldQuery = Read<OverlayBindGroup>;
    type ItemWorldQuery = ();

    #[inline]
    fn render<'w>(
        _item: &P,
        bind_group: bevy::ecs::query::ROQueryItem<'w, Self::ViewWorldQuery>,
        _: bevy::ecs::query::ROQueryItem<'w, Self::ItemWorldQuery>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        trace!("render overlay");
        pass.set_bind_group(I, &bind_group.0, &[]);
        pass.draw(0..3, 0..1);
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
        (camera, overlay_phase, _camera_3d, target): bevy::ecs::query::QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let view_entity = graph.view_entity();

        // [0.8] refer MainPass3dNode::run() main_opaque_pass_3d section
        {
            let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("main_prepass"),
                // color_attachments: &[Some(target.get_color_attachment(ops))],
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target.out_texture(),
                    resolve_target: None,
                    ops: Operations {
                        // load: LoadOp::Clear(Color::NONE.into()),
                        load: LoadOp::Load,
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            if let Some(viewport) = camera.viewport.as_ref() {
                render_pass.set_camera_viewport(viewport);
            }
            trace!("overlay phase render now");
            for item in overlay_phase.items.iter() {
                trace!("overlay phase item is {:?}", item.entity());
            }

            overlay_phase.render(&mut render_pass, world, view_entity);
        }

        Ok(())
    }
}
