use bevy::{
    asset::load_internal_asset,
    core_pipeline::{
        core_3d::MainPass3dNode, tonemapping::TonemappingNode, upscaling::UpscalingNode,
    },
    prelude::*,
    reflect::TypeUuid,
    render::{
        render_graph::{EmptyNode, RenderGraph, SlotInfo, SlotType},
        RenderApp,
    },
};
use light::{LightPassNode, LightPlugin};
use mesh::MeshPlugin;
use prepass::{PrepassNode, PrepassPlugin};
use transform::TransformPlugin;
use view::ViewPlugin;

pub mod light;
pub mod mesh;
pub mod prelude;
pub mod prepass;
pub mod transform;
pub mod view;

pub mod graph {
    pub const NAME: &str = "hikari";
    pub mod input {
        pub const VIEW_ENTITY: &str = "view_entity";
    }
    pub mod node {
        pub const PREPASS: &str = "prepass";
        pub const LIGHT_DIRECT_PASS: &str = "light_direct_pass";
        pub const LIGHT_INDIRECT_PASS: &str = "light_indirect_pass";
    }
}

pub const PREPASS_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 4693612430004931427);
pub const LIGHT_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 9657319286592943583);
// pub const RAY_TRACING_TYPES_HANDLE: HandleUntyped =
//     HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 15819591594687298858);
// pub const RAY_TRACING_BINDINGS_HANDLE: HandleUntyped =
//     HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 5025976374517268);
// pub const RAY_TRACING_FUNCTIONS_HANDLE: HandleUntyped =
//     HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 6789977396118176997);

pub struct HikariPlugin;
impl Plugin for HikariPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            PREPASS_SHADER_HANDLE,
            "shaders/prepass.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            LIGHT_SHADER_HANDLE,
            "shaders/light.wgsl",
            Shader::from_wgsl
        );
        // load_internal_asset!(
        //     app,
        //     RAY_TRACING_TYPES_HANDLE,
        //     "shaders/ray_tracing_types.wgsl",
        //     Shader::from_wgsl
        // );
        // load_internal_asset!(
        //     app,
        //     RAY_TRACING_BINDINGS_HANDLE,
        //     "shaders/ray_tracing_bindings.wgsl",
        //     Shader::from_wgsl
        // );
        // load_internal_asset!(
        //     app,
        //     RAY_TRACING_FUNCTIONS_HANDLE,
        //     "shaders/ray_tracing_functions.wgsl",
        //     Shader::from_wgsl
        // );

        app.add_plugin(TransformPlugin)
            .add_plugin(ViewPlugin)
            .add_plugin(MeshPlugin)
            .add_plugin(PrepassPlugin)
            .add_plugin(LightPlugin);

        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            // shine node
            let prepass_node = PrepassNode::new(&mut render_app.world);
            let light_pass_node = LightPassNode::new(&mut render_app.world);

            // core3d
            // let prepass_node = PrepassNode::new(&mut render_app.world);
            let pass_node_3d = MainPass3dNode::new(&mut render_app.world);
            let tonemapping = TonemappingNode::new(&mut render_app.world);
            let upscaling = UpscalingNode::new(&mut render_app.world);

            let mut graph = render_app.world.resource_mut::<RenderGraph>();

            let mut shine_graph = RenderGraph::default();

            shine_graph.add_node(
                bevy::core_pipeline::core_3d::graph::node::PREPASS,
                prepass_node,
            );

            shine_graph.add_node(
                bevy::core_pipeline::core_3d::graph::node::MAIN_PASS,
                pass_node_3d,
            );
            shine_graph.add_node(
                bevy::core_pipeline::core_3d::graph::node::TONEMAPPING,
                tonemapping,
            );
            shine_graph.add_node(
                bevy::core_pipeline::core_3d::graph::node::END_MAIN_PASS_POST_PROCESSING,
                EmptyNode,
            );
            shine_graph.add_node(
                bevy::core_pipeline::core_3d::graph::node::UPSCALING,
                upscaling,
            );

            shine_graph.add_node(graph::node::LIGHT_DIRECT_PASS, light_pass_node);

            let input_node_id = shine_graph.set_input(vec![SlotInfo::new(
                graph::input::VIEW_ENTITY,
                SlotType::Entity,
            )]);

            shine_graph.add_slot_edge(
                input_node_id,
                graph::input::VIEW_ENTITY,
                graph::node::PREPASS,
                PrepassNode::IN_VIEW,
            );
            shine_graph.add_slot_edge(
                input_node_id,
                graph::input::VIEW_ENTITY,
                graph::node::LIGHT_DIRECT_PASS,
                LightPassNode::IN_VIEW,
            );
            shine_graph.add_slot_edge(
                input_node_id,
                graph::input::VIEW_ENTITY,
                bevy::core_pipeline::core_3d::graph::node::MAIN_PASS,
                MainPass3dNode::IN_VIEW,
            );
            shine_graph.add_slot_edge(
                input_node_id,
                graph::input::VIEW_ENTITY,
                bevy::core_pipeline::core_3d::graph::node::TONEMAPPING,
                TonemappingNode::IN_VIEW,
            );
            shine_graph.add_slot_edge(
                input_node_id,
                graph::input::VIEW_ENTITY,
                bevy::core_pipeline::core_3d::graph::node::UPSCALING,
                UpscalingNode::IN_VIEW,
            );
            shine_graph.add_node_edge(
                graph::node::PREPASS,
                bevy::core_pipeline::core_3d::graph::node::MAIN_PASS,
            );
            shine_graph.add_node_edge(graph::node::PREPASS, graph::node::LIGHT_DIRECT_PASS);
            shine_graph.add_node_edge(
                graph::node::LIGHT_DIRECT_PASS,
                bevy::core_pipeline::core_3d::graph::node::MAIN_PASS,
            );
            shine_graph.add_node_edge(
                bevy::core_pipeline::core_3d::graph::node::MAIN_PASS,
                bevy::core_pipeline::core_3d::graph::node::TONEMAPPING,
            );
            shine_graph.add_node_edge(
                bevy::core_pipeline::core_3d::graph::node::TONEMAPPING,
                bevy::core_pipeline::core_3d::graph::node::END_MAIN_PASS_POST_PROCESSING,
            );
            shine_graph.add_node_edge(
                bevy::core_pipeline::core_3d::graph::node::END_MAIN_PASS_POST_PROCESSING,
                bevy::core_pipeline::core_3d::graph::node::UPSCALING,
            );

            graph.add_sub_graph(graph::NAME, shine_graph);
        }
    }
}
