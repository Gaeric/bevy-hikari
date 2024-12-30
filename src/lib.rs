use bevy::{
    asset::load_internal_asset,
    prelude::*,
    reflect::TypeUuid,
    render::{
        extract_resource::ExtractResource,
        render_graph::{RenderGraph, SlotInfo, SlotType},
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
    pub const NAME: &str = "hikari";
    pub mod input {
        pub const VIEW_ENTITY: &str = "view_entity";
    }
    pub mod node {
        pub const PREPASS: &str = "prepass";
        pub const LIGHT_PASS: &str = "light_direct_pass";
        pub const OVERLAY_PASS: &str = "overlay_pass";
        pub const UPSCALING: &str = "upscaling";
    }
}

pub const WORKGROUP_SIZE: u32 = 8;
pub const NOISE_TEXTURE_COUNT: usize = 64;

// refer mesh.rs
pub const MESH_MATERIAL_TYPES_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 15819591594687298858);
pub const MESH_MATERIAL_BINDINGS_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 5025976374517268);
pub const DEFERRED_BINDINGS_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 14467895678105108252);
pub const PREPASS_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 4693612430004931427);
pub const LIGHT_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 9657319286592943583);
pub const OVERLAY_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 10969344919103020615);
pub const QUAD_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Mesh::TYPE_UUID, 4740146776519512271);

pub struct HikariPlugin {
    noise_folder: String,
}

impl HikariPlugin {
    pub fn new(noise_folder: &str) -> Self {
        Self {
            noise_folder: noise_folder.into(),
        }
    }
}

impl Default for HikariPlugin {
    fn default() -> Self {
        Self {
            noise_folder: "textures/blue_noise".into(),
        }
    }
}

// [0.8] refer from compute_shader_game_of_life GameOfLifeImage
//
#[derive(Clone, Deref, DerefMut, Resource, ExtractResource)]
pub struct NoiseTexture(pub Vec<Handle<Image>>);

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
        load_internal_asset!(
            app,
            OVERLAY_SHADER_HANDLE,
            "shaders/overlay.wgsl",
            Shader::from_wgsl
        );

        let noise_path = self.noise_folder.clone();
        let load_system = move |mut commands: Commands, asset_server: Res<AssetServer>| {
            let handles = (0..NOISE_TEXTURE_COUNT)
                .map(|id| {
                    let name = format!("{}/LDR_RGBA_{}.png", noise_path, id);
                    asset_server.load(&name)
                })
                .collect();
            commands.insert_resource(NoiseTexture(handles));
        };

        app.add_plugins((
            TransformPlugin,
            ViewPlugin,
            MeshMaterialPlugin,
            PrepassPlugin,
            LightPlugin,
            OverlayPlugin,
        ))
        .add_systems(Startup, load_system);

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        let prepass_node = PrepassNode::new(&mut render_app.world);
        let light_pass_node = LightPassNode::new(&mut render_app.world);
        let overlay_pass_node = OverlayPassNode::new(&mut render_app.world);
        // let upscaling = core_pipeline::upscaling::UpscalingNode::new(&mut render_app.world);

        let mut graph = render_app.world.resource_mut::<RenderGraph>();

        let mut hikari = RenderGraph::default();
        hikari.add_node(graph::node::PREPASS, prepass_node);
        hikari.add_node(graph::node::LIGHT_PASS, light_pass_node);
        hikari.add_node(graph::node::OVERLAY_PASS, overlay_pass_node);
        // hikari.add_node(graph::node::UPSCALING, upscaling);

        let input_node_id = hikari.set_input(vec![SlotInfo::new(
            graph::input::VIEW_ENTITY,
            SlotType::Entity,
        )]);

        hikari.add_slot_edge(
            input_node_id,
            graph::input::VIEW_ENTITY,
            graph::node::PREPASS,
            PrepassNode::IN_VIEW,
        );

        hikari.add_slot_edge(
            input_node_id,
            graph::input::VIEW_ENTITY,
            graph::node::LIGHT_PASS,
            LightPassNode::IN_VIEW,
        );

        hikari.add_slot_edge(
            input_node_id,
            graph::input::VIEW_ENTITY,
            graph::node::OVERLAY_PASS,
            OverlayPassNode::IN_VIEW,
        );

        // hikari.add_slot_edge(
        //     input_node_id,
        //     graph::input::VIEW_ENTITY,
        //     core_3d::graph::node::UPSCALING,
        //     core_pipeline::upscaling::UpscalingNode::IN_VIEW,
        // );

        hikari.add_node_edge(graph::node::PREPASS, graph::node::LIGHT_PASS);
        hikari.add_node_edge(graph::node::LIGHT_PASS, graph::node::OVERLAY_PASS);

        graph.add_sub_graph(graph::NAME, hikari);
    }
}
