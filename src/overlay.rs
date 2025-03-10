use std::ops::Range;

use crate::{
    light::LightPassTarget,
    prepass::{PrepassTarget, DEBUG_FORMAT},
};
use bevy::{
    asset::load_internal_asset,
    core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    ecs::{
        query::ROQueryItem,
        system::{lifetimeless::SRes, SystemParamItem},
    },
    math::FloatOrd,
    prelude::*,
    render::{
        camera::ExtractedCamera,
        render_graph::{NodeRunError, RenderGraphContext, ViewNode},
        render_phase::{
            AddRenderCommand, CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions,
            PhaseItem, PhaseItemExtraIndex, RenderCommand, RenderCommandResult, SetItemPipeline,
            SortedPhaseItem, TrackedRenderPass, ViewSortedRenderPhases,
        },
        render_resource::*,
        renderer::{RenderContext, RenderDevice},
        view::{ExtractedView, ViewTarget},
        Extract, Render, RenderApp, RenderSet,
    },
};

pub const OVERLAY_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(332238352525531982437701789663104912412);

pub struct OverlayPlugin;
impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            OVERLAY_SHADER_HANDLE,
            "shaders/overlay.wgsl",
            Shader::from_wgsl
        );

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<DrawFunctions<Overlay>>()
                .init_resource::<SpecializedRenderPipelines<OverlayPipeline>>()
                .init_resource::<OverlayBindGroup>()
                .init_resource::<ViewSortedRenderPhases<Overlay>>()
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

#[derive(Resource)]
pub struct OverlayPipeline {
    pub overlay_layout: BindGroupLayout,
}

#[derive(Default, Resource)]
pub struct OverlayBindGroup {
    bind_group: Option<BindGroup>,
}

impl FromWorld for OverlayPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let overlay_layout = render_device.create_bind_group_layout(
            "overlay pipeline layout",
            &[
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
        );

        Self { overlay_layout }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OverlayPipelineKey;

// [0.8] refer MeshPipeline
impl SpecializedRenderPipeline for OverlayPipeline {
    type Key = OverlayPipelineKey;

    fn specialize(&self, _key: Self::Key) -> RenderPipelineDescriptor {
        let bind_group_layout = vec![self.overlay_layout.clone()];

        let shader_defs = Vec::new();

        RenderPipelineDescriptor {
            label: None,
            layout: bind_group_layout,
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader: OVERLAY_SHADER_HANDLE,
                shader_defs: shader_defs.clone(),
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: DEBUG_FORMAT,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            push_constant_ranges: Vec::new(),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
        }
    }
}

// [0.8] refer extract_core_3d_camera_phases
fn extract_overlay_camera_phases(
    mut commands: Commands,
    cameras_3d: Extract<Query<(Entity, &Camera), With<Camera3d>>>,
    mut overlay_phase: ResMut<ViewSortedRenderPhases<Overlay>>,
) {
    for (entity, camera) in cameras_3d.iter() {
        if camera.is_active {
            commands.get_or_spawn(entity);
            overlay_phase.insert_or_clear(entity);
        }
    }
}

fn prepare_overlay_bind_group(
    render_device: Res<RenderDevice>,
    pipeline: Res<OverlayPipeline>,
    _prepass_target: Query<(Entity, &PrepassTarget)>,
    query: Query<(Entity, &LightPassTarget)>,
    mut overlay_bind_group: ResMut<OverlayBindGroup>,
) {
    // for (_entity, prepass_target) in &prepass_target {
    //     let texture = &prepass_target.position;
    //     let bind_group = render_device.create_bind_group(
    //         None,
    //         &pipeline.overlay_layout,
    //         &[
    //             BindGroupEntry {
    //                 binding: 0,
    //                 resource: BindingResource::TextureView(&texture.texture_view),
    //             },
    //             BindGroupEntry {
    //                 binding: 1,
    //                 resource: BindingResource::Sampler(&texture.sampler),
    //             },
    //         ],
    //     );
    //     overlay_bind_group.bind_group = Some(bind_group);
    // }

    for (entity, target) in &query {
        trace!("over bind group entity is {:?}", entity);
        // let texture_view = &target.render.texture_view;
        // let sampler = &target.render.sampler;
        // let texture = &target.reservoir[0].random;
        let texture = &target.render;

        let bind_group = render_device.create_bind_group(
            None,
            &pipeline.overlay_layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&texture.texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&texture.sampler),
                },
            ],
        );
        overlay_bind_group.bind_group = Some(bind_group);
    }
}

#[allow(clippy::too_many_arguments)]
fn queue_overlay_mesh(
    draw_functions: Res<DrawFunctions<Overlay>>,
    overlay_pipeline: Res<OverlayPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<OverlayPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    mut overlay_render_phase: ResMut<ViewSortedRenderPhases<Overlay>>,
    mut views: Query<(Entity, &ExtractedView)>,
) {
    let draw_function = draw_functions.read().get_id::<DrawOverlay>().unwrap();
    for (view_entity, _view) in &mut views {
        let Some(overlay_phase) = overlay_render_phase.get_mut(&view_entity) else {
            continue;
        };

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
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::NONE,
        });
    }
}

// [0.8] maybe refer from Qpaque3d
pub struct Overlay {
    pub distance: f32,
    pub entity: Entity,
    pub pipeline: CachedRenderPipelineId,
    pub draw_function: DrawFunctionId,
    pub batch_range: Range<u32>,
    pub extra_index: PhaseItemExtraIndex,
}

impl PhaseItem for Overlay {
    #[inline]
    fn entity(&self) -> Entity {
        self.entity
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

    // #[inline]
    // fn dynamic_offset(&self) -> Option<NonMaxU32> {
    //     self.dynamic_offset
    // }

    // #[inline]
    // fn dynamic_offset_mut(&mut self) -> &mut Option<NonMaxU32> {
    //     &mut self.dynamic_offset
    // }

    #[inline]
    fn extra_index(&self) -> PhaseItemExtraIndex {
        self.extra_index
    }

    #[inline]
    fn batch_range_and_extra_index_mut(&mut self) -> (&mut Range<u32>, &mut PhaseItemExtraIndex) {
        (&mut self.batch_range, &mut self.extra_index)
    }
}

impl SortedPhaseItem for Overlay {
    type SortKey = FloatOrd;

    #[inline]
    fn sort_key(&self) -> Self::SortKey {
        FloatOrd(self.distance)
    }
}

impl CachedRenderPipelinePhaseItem for Overlay {
    #[inline]
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}

// [0.8] refer DrawWireframes
// type DrawOverlay = (SetItemPipeline, SetOverlayBindGroup<0>, DrawMesh);
type DrawOverlay = (SetItemPipeline, SetOverlayBindGroup<0>);

pub struct SetOverlayBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetOverlayBindGroup<I> {
    type Param = SRes<OverlayBindGroup>;
    type ViewQuery = ();
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        _item: &P,
        _view: ROQueryItem<'w, Self::ViewQuery>,
        _entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        bind_group: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        if let Some(overlay_bind_group) = &bind_group.into_inner().bind_group {
            pass.set_bind_group(I, &overlay_bind_group, &[]);
            pass.draw(0..3, 0..1);
            RenderCommandResult::Success
        } else {
            RenderCommandResult::Failure
        }
    }
}

#[derive(Default)]
pub struct OverlayPassNode;

impl ViewNode for OverlayPassNode {
    type ViewQuery = (
        &'static ExtractedCamera,
        &'static Camera3d,
        &'static ViewTarget,
    );

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (camera, _camera_3d, target): bevy::ecs::query::QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        trace!("overlay pass node run");
        let view_entity = graph.view_entity();
        let Some(overlay_phases) = world.get_resource::<ViewSortedRenderPhases<Overlay>>() else {
            return Ok(());
        };
        let Some(overlay_phase) = overlay_phases.get(&view_entity) else {
            return Ok(());
        };
        // [0.8] refer MainPass3dNode::run() main_opaque_pass_3d section
        {
            // let _main_prepass_span = info_span!("main_prepass").entered();

            // let ops = Operations {
            //     load: match camera_3d.clear_color {
            //         ClearColorConfig::Default => {
            //             LoadOp::Clear(world.resource::<ClearColor>().0.into())
            //         }
            //         ClearColorConfig::Custom(color) => LoadOp::Clear(color.into()),
            //         ClearColorConfig::None => LoadOp::Load,
            //     },
            //     store: true,
            // };

            // info!("ops is {ops:?}");
            // info!("viewtarget sampled {:?}", target.get_color_attachment(ops));

            let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("main_prepass"),
                // color_attachments: &[Some(target.get_color_attachment(ops))],
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target.out_texture(),
                    resolve_target: None,
                    ops: Operations {
                        // load: LoadOp::Clear(Color::NONE.into()),
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if let Some(viewport) = camera.viewport.as_ref() {
                render_pass.set_camera_viewport(viewport);
            }
            for item in overlay_phase.items.iter() {
                trace!("overlay phase item is {:?}", item.entity());
            }

            trace!("overlay phase view_entity: {:?}", view_entity);

            overlay_phase.render(&mut render_pass, world, view_entity);
        }

        trace!("finish overplay node run.");

        Ok(())
    }
}
