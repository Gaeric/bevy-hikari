use super::{
    GpuMesh, GpuMeshSlice, GpuNodeBuffer, GpuPrimitiveBuffer, GpuVertexBuffer,
    MeshMaterialSystems,
};
use bevy::{
    prelude::*,
    render::{
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        Extract, Render, RenderApp, RenderSet,
    },
    utils::{HashMap, HashSet},
};
use std::collections::BTreeMap;

pub struct MeshPlugin;
impl Plugin for MeshPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<GpuMeshes>()
                .init_resource::<MeshRenderAssets>()
                .init_resource::<MeshAssetState>()
                .add_systems(
                    ExtractSchedule,
                    extract_mesh_assets.in_set(RenderSet::ExtractCommands),
                )
                .add_systems(
                    Render,
                    prepare_mesh_assets
                        .in_set(RenderSet::PrepareAssets)
                        .in_set(MeshMaterialSystems::PrepareAssets)
                        .after(MeshMaterialSystems::PrePrepareAssets),
                );
        }
    }
}

/// Acceleration structures on GPU.
#[derive(Default, Resource)]
pub struct MeshRenderAssets {
    pub vertex_buffer: StorageBuffer<GpuVertexBuffer>,
    pub primitive_buffer: StorageBuffer<GpuPrimitiveBuffer>,
    pub node_buffer: StorageBuffer<GpuNodeBuffer>,
}

impl MeshRenderAssets {
    pub fn clear(&mut self) {
        self.vertex_buffer.get_mut().data.clear();
        self.primitive_buffer.get_mut().data.clear();
        self.node_buffer.get_mut().data.clear();
        self.node_buffer.get_mut().count = 0;
    }

    pub fn write_buffer(&mut self, device: &RenderDevice, queue: &RenderQueue) {
        self.vertex_buffer.write_buffer(device, queue);
        self.primitive_buffer.write_buffer(device, queue);
        self.node_buffer.write_buffer(device, queue);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub enum MeshAssetState {
    /// No updates for all mesh assets.
    #[default]
    Clean,
    /// There are upcoming updates but mesh assets haven't been prepared.
    Dirty,
    /// There were asset updates and mesh assets have been prepared.
    Updated,
}

/// Holds all GPU representatives of mesh assets.
#[derive(Default, Deref, DerefMut, Resource)]
pub struct GpuMeshes(HashMap<AssetId<Mesh>, (GpuMesh, GpuMeshSlice)>);

#[derive(Default, Resource)]
pub struct ExtractedMeshes {
    extracted: Vec<(AssetId<Mesh>, Mesh)>,
    removed: Vec<AssetId<Mesh>>,
}


/// [0.12] refer extract_materials
/// Extract Mesh to ExtracedMeshes Resource
fn extract_mesh_assets(
    mut commands: Commands,
    mut events: Extract<EventReader<AssetEvent<Mesh>>>,
    mut state: ResMut<MeshAssetState>,
    assets: Extract<Res<Assets<Mesh>>>,
) {
    let mut changed_assets = HashSet::default();
    let mut removed = Vec::new();
    for event in events.read() {
        match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                changed_assets.insert(*id);
            }
            AssetEvent::Removed { id } => {
                changed_assets.remove(id);
                removed.push(*id);
            }
            AssetEvent::LoadedWithDependencies { .. } => {
                
            }
        }
    }

    let mut extracted = Vec::new();
    for id in changed_assets.drain() {
        if let Some(mesh) = assets.get(id) {
            extracted.push((id, mesh.clone()));
        }
    }

    *state = if !extracted.is_empty() || !removed.is_empty() {
        MeshAssetState::Dirty
    } else {
        MeshAssetState::Clean
    };

    commands.insert_resource(ExtractedMeshes { extracted, removed });
}

/// [0.12] refer prepare_materials
/// write mesh data to gpu buffer
fn prepare_mesh_assets(
    mut extracted_assets: ResMut<ExtractedMeshes>,
    mut asset_state: ResMut<MeshAssetState>,
    mut assets: Local<BTreeMap<AssetId<Mesh>, GpuMesh>>,
    mut meshes: ResMut<GpuMeshes>,
    mut render_assets: ResMut<MeshRenderAssets>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    if *asset_state == MeshAssetState::Clean {
        return;
    }

    for handle in extracted_assets.removed.drain(..) {
        assets.remove(&handle);
        meshes.remove(&handle);
    }
    for (handle, mesh) in extracted_assets.extracted.drain(..) {
        assets.insert(handle, GpuMesh::from_mesh(mesh).unwrap());
    }

    render_assets.clear();
    for (handle, mesh) in assets.iter() {
        let vertex = render_assets.vertex_buffer.get().data.len() as u32;
        let primitive = render_assets.primitive_buffer.get().data.len() as u32;
        let node_offset = render_assets.node_buffer.get().data.len() as u32;
        let node_len = mesh.nodes.len() as u32;

        render_assets
            .vertex_buffer
            .get_mut()
            .data
            .append(&mut mesh.vertices.clone());
        render_assets
            .primitive_buffer
            .get_mut()
            .data
            .append(&mut mesh.primitives.clone());
        render_assets
            .node_buffer
            .get_mut()
            .data
            .append(&mut mesh.nodes.clone());

        meshes.insert(
            *handle,
            (
                mesh.clone(),
                GpuMeshSlice {
                    vertex,
                    primitive,
                    node_offset,
                    node_len,
                },
            ),
        );
    }
    render_assets.write_buffer(&render_device, &render_queue);

    *asset_state = MeshAssetState::Updated;
}
