use bevy::{
    asset::load_internal_asset,
    prelude::*,
    render::{
        render_graph::{RenderGraphApp, ViewNodeRunner},
        RenderApp,
    },
};

use light::{LightPassNode, LightPlugin};
use mesh_material::MeshMaterialPlugin;
use overlay::{OverlayPassNode, OverlayPlugin};
use prepass::{PrepassNode, PrepassPlugin};
use transform::TransformPlugin;
use view::ViewPlugin;

pub mod light;
pub mod mesh_material;
pub mod overlay;
pub mod prelude;
pub mod prepass;
pub mod transform;
pub mod view;

pub mod graph {
    use bevy::render::render_graph::{RenderLabel, RenderSubGraph};

    #[derive(Debug, Hash, PartialEq, Eq, Clone, RenderSubGraph)]
    pub struct HikariGraph;

    pub mod input {
        pub const VIEW_ENTITY: &str = "view_entity";
    }

    #[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
    pub enum HikariNode {
        Prepass,
        LightPass,
        OverlayPass,
    }
}

pub const WORKGROUP_SIZE: u32 = 8;
pub const NOISE_TEXTURE_COUNT: usize = 64;

// refer mesh.rs
pub const MESH_MATERIAL_TYPES_HANDLE: Handle<Shader> = Handle::weak_from_u128(15819591594687298858);
pub const MESH_MATERIAL_BINDINGS_HANDLE: Handle<Shader> = Handle::weak_from_u128(5025976374517268);
pub const DEFERRED_BINDINGS_HANDLE: Handle<Shader> = Handle::weak_from_u128(14467895678105108252);
pub const PREPASS_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(4693612430004931427);
pub const LIGHT_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(9657319286592943583);

pub struct HikariPlugin;

// [0.8] refer PbrPlugin
impl Plugin for HikariPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            MESH_MATERIAL_TYPES_HANDLE,
            "shaders/mesh_material_types.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            MESH_MATERIAL_BINDINGS_HANDLE,
            "shaders/mesh_material_bindings.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            DEFERRED_BINDINGS_HANDLE,
            "shaders/deferred_bindings.wgsl",
            Shader::from_wgsl
        );
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
            MeshMaterialPlugin,
            PrepassPlugin,
            // LightPlugin,
            // OverlayPlugin,
        ));

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Some(render_app) => render_app,
            None => return,
        };

        render_app
            .add_render_sub_graph(graph::HikariGraph)
            .add_render_graph_node::<ViewNodeRunner<PrepassNode>>(
                graph::HikariGraph,
                graph::HikariNode::Prepass,
            )
            // .add_render_graph_node::<ViewNodeRunner<LightPassNode>>(
            //     graph::HikariGraph,
            //     graph::HikariNode::LightPass,
            // )
            // .add_render_graph_node::<ViewNodeRunner<OverlayPassNode>>(
            //     graph::HikariGraph,
            //     graph::HikariNode::OverlayPass,
            // )
;

        render_app.add_render_graph_edges(
            graph::HikariGraph,
            (
                graph::HikariNode::Prepass,
                // graph::HikariNode::LightPass,
                // graph::HikariNode::OverlayPass,
            ),
        );
    }
}
