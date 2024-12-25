use bevy::{
    asset::load_internal_asset,
    core_pipeline::{
        core_3d::{MainOpaquePass3dNode, MainTransparentPass3dNode},
        tonemapping::TonemappingNode,
        upscaling::UpscalingNode,
    },
    prelude::*,
    reflect::TypeUuid,
    render::{
        render_graph::{EmptyNode, RenderGraphApp, ViewNodeRunner},
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

        app.add_plugins((
            TransformPlugin,
            ViewPlugin,
            MeshPlugin,
            PrepassPlugin,
            LightPlugin,
        ));

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        use bevy::core_pipeline::core_3d::graph::node::*;
        render_app
            .add_render_sub_graph(graph::NAME)
            .add_render_graph_node::<ViewNodeRunner<PrepassNode>>(graph::NAME, &graph::node::PREPASS)
            .add_render_graph_node::<ViewNodeRunner<LightPassNode>>(graph::NAME, &graph::node::LIGHT_DIRECT_PASS)
            .add_render_graph_node::<EmptyNode>(graph::NAME, START_MAIN_PASS)
            .add_render_graph_node::<ViewNodeRunner<MainOpaquePass3dNode>>(
                graph::NAME,
                MAIN_OPAQUE_PASS,
            )
            .add_render_graph_node::<ViewNodeRunner<MainTransparentPass3dNode>>(
                graph::NAME,
                MAIN_TRANSPARENT_PASS,
            )
            .add_render_graph_node::<EmptyNode>(graph::NAME, END_MAIN_PASS)
            .add_render_graph_node::<ViewNodeRunner<TonemappingNode>>(graph::NAME, TONEMAPPING)
            .add_render_graph_node::<EmptyNode>(graph::NAME, END_MAIN_PASS_POST_PROCESSING)
            .add_render_graph_node::<ViewNodeRunner<UpscalingNode>>(graph::NAME, UPSCALING)
            .add_render_graph_edges(
                graph::NAME,
                &[
                    PREPASS,
                    START_MAIN_PASS,
                    MAIN_OPAQUE_PASS,
                    MAIN_TRANSPARENT_PASS,
                    END_MAIN_PASS,
                    TONEMAPPING,
                    END_MAIN_PASS_POST_PROCESSING,
                    UPSCALING,
                ],
            );
    }
}
