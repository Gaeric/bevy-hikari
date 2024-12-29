use bevy::{pbr::PbrPlugin, prelude::*, render::camera::CameraRenderGraph};
use bevy_hikari::prelude::*;
use std::f32::consts::PI;

fn main() {
    App::new()
        .insert_resource(Msaa::Sample4)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Hikari".into(),
                resolution: (800., 600.).into(),
                ..default()
            }),
            ..default()
        }))
        // .add_plugin(PbrPlugin::default())
        .add_plugin(HikariPlugin::default())
        .add_startup_system(setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Ground
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Cube::default())),
        material: materials.add(StandardMaterial {
            base_color: Color::rgb(0.3, 0.5, 0.3),
            perceptual_roughness: 0.5,
            ..Default::default()
        }),
        transform: Transform {
            translation: Vec3::new(0.0, -0.5, 0.0),
            rotation: Default::default(),
            scale: Vec3::new(6.0, 1.0, 6.0),
        },
        ..Default::default()
    });
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Plane::default())),
        material: materials.add(StandardMaterial {
            base_color: Color::GRAY,
            perceptual_roughness: 1.0,
            ..Default::default()
        }),
        transform: Transform {
            translation: Vec3::new(0.0, -1.0, 0.0),
            scale: Vec3::new(400.0, 1.0, 400.0),
            ..Default::default()
        },
        ..Default::default()
    });
    // .insert(RayCastMesh::<RaycastSet>::default());

    // Sphere
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::UVSphere {
            radius: 0.5,
            ..Default::default()
        })),
        material: materials.add(StandardMaterial {
            base_color_texture: Some(asset_server.load("models/Earth/earth_daymap.jpg")),
            emissive: Color::rgba(1.0, 1.0, 1.0, 0.1),
            emissive_texture: Some(asset_server.load("models/Earth/earth_daymap.jpg")),
            ..Default::default()
        }),
        transform: Transform::from_xyz(2.0, 0.5, 0.0),
        ..Default::default()
    });
    // Model
    commands.spawn(SceneBundle {
        scene: asset_server.load("models/FlightHelmet/FlightHelmet.gltf#Scene0"),
        transform: Transform::from_scale(Vec3::splat(2.0)),
        ..default()
    });

    // Only directional light is supported
    const HALF_SIZE: f32 = 5.0;
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 100000.0,
            shadows_enabled: true,
            ..Default::default()
        },
        transform: Transform {
            translation: Vec3::new(0.0, 5.0, 0.0),
            rotation: Quat::from_euler(EulerRot::XYZ, -PI / 8.0, -PI / 4.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    });

    // Camera
    commands.spawn(Camera3dBundle {
        camera_render_graph: CameraRenderGraph::new(bevy_hikari::graph::NAME),
        transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..Default::default()
    });
}
