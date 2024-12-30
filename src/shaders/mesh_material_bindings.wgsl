#define_import_path bevy_hikari::mesh_material_bindings

#import bevy_hikari::mesh_material_types

@group(2) @binding(0)
var<storage> vertex_buffer: bevy_hikari::mesh_material_types::Vertices;
@group(2) @binding(1)
var<storage> primitive_buffer: bevy_hikari::mesh_material_types::Primitives;
@group(2) @binding(2)
var<storage> asset_node_buffer: bevy_hikari::mesh_material_types::Nodes;
@group(2) @binding(3)
var<storage> instance_buffer: bevy_hikari::mesh_material_types::Instances;
@group(2) @binding(4)
var<storage> instance_node_buffer: bevy_hikari::mesh_material_types::Nodes;
@group(2) @binding(5)
var<storage> material_buffer: bevy_hikari::mesh_material_types::Materials;
