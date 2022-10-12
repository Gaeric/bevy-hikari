#define_import_path bevy_hikari::mesh_view_types

struct Frame {
    kernel: mat3x3<f32>,
    number: u32,
    validation_interval: u32,
    second_bounce_chance: f32,
    solar_angle: f32,
};

struct PreviousView {
    view_proj: mat4x4<f32>,
    inverse_view_proj: mat4x4<f32>,
};

struct PreviousMesh {
    model: mat4x4<f32>,
    inverse_transpose_model: mat4x4<f32>,
};

struct InstanceIndex {
    instance: u32,
    material: u32
};