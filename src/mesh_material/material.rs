use super::{
    GpuStandardMaterial, GpuStandardMaterialBuffer, GpuStandardMaterialOffset,
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
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};

pub struct MaterialPlugin;
impl Plugin for MaterialPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<MaterialRenderAssets>()
                .init_resource::<StandardMaterials>()
                .init_resource::<GpuStandardMaterials>()
                .add_systems(
                    Render,
                    prepare_material_assets
                        .in_set(RenderSet::PrepareAssets)
                        .in_set(MeshMaterialSystems::PrepareAssets)
                        .after(MeshMaterialSystems::PrePrepareAssets),
                );
        }
    }
}

#[derive(Default)]
pub struct GenericMaterialPlugin(PhantomData<StandardMaterial>);
impl Plugin for GenericMaterialPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_systems(
                    ExtractSchedule,
                    extract_material_assets.in_set(RenderSet::ExtractCommands),
                )
                .add_systems(
                    Render,
                    prepare_generic_material_assets
                        .in_set(RenderSet::PrepareAssets)
                        .in_set(MeshMaterialSystems::PrePrepareAssets),
                );
        }
    }
}

#[derive(Default, Resource)]
pub struct MaterialRenderAssets {
    pub buffer: StorageBuffer<GpuStandardMaterialBuffer>,
    pub textures: BTreeSet<Handle<Image>>,
}

#[derive(Default, Deref, DerefMut, Resource)]
pub struct StandardMaterials(BTreeMap<AssetId<StandardMaterial>, StandardMaterial>);

#[derive(Default, Deref, DerefMut, Resource)]
pub struct GpuStandardMaterials(
    HashMap<AssetId<StandardMaterial>, (GpuStandardMaterial, GpuStandardMaterialOffset)>,
);

#[derive(Default, Resource)]
pub struct ExtractedMaterials {
    extracted: Vec<(AssetId<StandardMaterial>, StandardMaterial)>,
    removed: Vec<AssetId<StandardMaterial>>,
}

fn extract_material_assets(
    mut commands: Commands,
    mut events: Extract<EventReader<AssetEvent<StandardMaterial>>>,
    assets: Extract<Res<Assets<StandardMaterial>>>,
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
            AssetEvent::LoadedWithDependencies { .. } => {}
        }
    }

    let mut extracted = Vec::new();
    for id in changed_assets.drain() {
        if let Some(asset) = assets.get(id) {
            extracted.push((id, asset.clone()));
        }
    }

    commands.insert_resource(ExtractedMaterials { extracted, removed });
}

fn prepare_generic_material_assets(
    mut extracted_assets: ResMut<ExtractedMaterials>,
    mut materials: ResMut<StandardMaterials>,
    render_assets: ResMut<MaterialRenderAssets>,
) {
    for id in extracted_assets.removed.drain(..) {
        materials.remove(&id);
    }

    let render_assets = render_assets.into_inner();
    for (id, material) in extracted_assets.extracted.drain(..) {
        if let Some(ref texture) = material.base_color_texture {
            render_assets.textures.insert(texture.clone_weak());
        }

        materials.insert(id, material);
    }
}

fn prepare_material_assets(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    materials: Res<StandardMaterials>,
    mut assets: ResMut<GpuStandardMaterials>,
    mut render_assets: ResMut<MaterialRenderAssets>,
) {
    if !materials.is_changed() {
        return;
    }

    assets.clear();

    // TODO: remove unused textures.
    let textures: Vec<_> = render_assets.textures.iter().cloned().collect();
    let texture_id = |handle: &Option<Handle<Image>>| {
        if let Some(handle) = handle {
            match textures.binary_search(handle) {
                Ok(id) | Err(id) => id as u32,
            }
        } else {
            u32::MAX
        }
    };

    let materials = materials
        .iter()
        .enumerate()
        .map(|(offset, (handle, material))| {
            let base_color = material.base_color.into();
            let base_color_texture = texture_id(&material.base_color_texture);

            let emissive = material.emissive.into();
            let emissive_texture = texture_id(&material.emissive_texture);

            let metallic_roughness_texture = texture_id(&material.metallic_roughness_texture);
            let normal_map_texture = texture_id(&material.normal_map_texture);
            let occlusion_texture = texture_id(&material.occlusion_texture);

            let material = GpuStandardMaterial {
                base_color,
                base_color_texture,
                emissive,
                emissive_texture,
                perceptual_roughness: material.perceptual_roughness,
                metallic: material.metallic,
                metallic_roughness_texture,
                reflectance: material.reflectance,
                normal_map_texture,
                occlusion_texture,
            };
            let offset = GpuStandardMaterialOffset {
                value: offset as u32,
            };

            // let handle = HandleUntyped::weak(*handle);
            assets.insert(*handle, (material, offset));
            material
        })
        .collect();

    render_assets.buffer.get_mut().data = materials;
    render_assets
        .buffer
        .write_buffer(&render_device, &render_queue);
}
