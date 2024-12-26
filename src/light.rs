use crate::{
    mesh_material::{MeshMaterialBindGroup, MeshMaterialBindGroupLayout, TextureBindGroupLayout},
    prepass::PrepassTarget,
    NoiseTexture, LIGHT_SHADER_HANDLE, NOISE_TEXTURE_COUNT, WORKGROUP_SIZE,
};
use bevy::{
    pbr::{
        GlobalLightMeta, GpuLights, GpuPointLights, LightMeta, MeshPipeline, ShadowPipeline,
        ViewClusterBindings, ViewLightsUniformOffset, ViewShadowBindings,
    },
    prelude::*,
    render::{
        camera::ExtractedCamera,
        extract_resource::ExtractResourcePlugin,
        render_asset::RenderAssets,
        render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType},
        render_resource::*,
        renderer::{RenderContext, RenderDevice, RenderQueue},
        texture::{GpuImage, TextureCache},
        view::{ViewUniform, ViewUniformOffset, ViewUniforms},
        RenderApp, RenderStage,
    },
};
use std::num::NonZeroU32;

pub const RENDER_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
pub const RESERVOIR_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
pub const RADIANCE_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
pub const POSITION_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba32Float;
pub const NORMAL_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba8Snorm;
pub const RANDOM_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

pub struct LightPlugin;
impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(ExtractResourcePlugin::<NoiseTexture>::default());

        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<FrameCounter>()
                .init_resource::<LightPipeline>()
                .init_resource::<SpecializedComputePipelines<LightPipeline>>()
                .init_resource::<FrameUniform>()
                .add_system_to_stage(RenderStage::Prepare, prepare_light_pass_targets)
                .add_system_to_stage(RenderStage::Prepare, prepare_frame_uniform)
                .add_system_to_stage(RenderStage::Queue, queue_view_bind_groups)
                .add_system_to_stage(RenderStage::Queue, queue_light_bind_groups)
                .add_system_to_stage(RenderStage::Queue, queue_light_pipelines);
        }
    }
}

pub struct LightPipeline {
    pub view_layout: BindGroupLayout,
    pub deferred_layout: BindGroupLayout,
    pub mesh_material_layout: BindGroupLayout,
    pub texture_layout: Option<BindGroupLayout>,
    pub frame_layout: BindGroupLayout,
    pub render_layout: BindGroupLayout,
    pub dummy_white_gpu_image: GpuImage,
}

impl FromWorld for LightPipeline {
    fn from_world(world: &mut World) -> Self {
        let buffer_binding_type = BufferBindingType::Storage { read_only: true };

        let render_device = world.resource::<RenderDevice>();
        let mesh_pipeline = world.resource::<MeshPipeline>();
        let mesh_material_layout = world.resource::<MeshMaterialBindGroupLayout>().0.clone();

        let view_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                // View
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(ViewUniform::min_size()),
                    },
                    count: None,
                },
                // Lights
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(GpuLights::min_size()),
                    },
                    count: None,
                },
                // Point Shadow Texture Cube Array
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Depth,
                        #[cfg(not(feature = "webgl"))]
                        view_dimension: TextureViewDimension::CubeArray,
                        #[cfg(feature = "webgl")]
                        view_dimension: TextureViewDimension::Cube,
                    },
                    count: None,
                },
                // Point Shadow Texture Array Sampler
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::Comparison),
                    count: None,
                },
                // Directional Shadow Texture Array
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Depth,
                        #[cfg(not(feature = "webgl"))]
                        view_dimension: TextureViewDimension::D2Array,
                        #[cfg(feature = "webgl")]
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Directional Shadow Texture Array Sampler
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::Comparison),
                    count: None,
                },
                // PointLights
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: buffer_binding_type,
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuPointLights::min_size(buffer_binding_type)),
                    },
                    count: None,
                },
                // Clustered Light Index Lists
                BindGroupLayoutEntry {
                    binding: 7,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: buffer_binding_type,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            ViewClusterBindings::min_size_cluster_light_index_lists(
                                buffer_binding_type,
                            ),
                        ),
                    },
                    count: None,
                },
                // Cluster Offsets And Counts
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: buffer_binding_type,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            ViewClusterBindings::min_size_cluster_offsets_and_counts(
                                buffer_binding_type,
                            ),
                        ),
                    },
                    count: None,
                },
            ],
            label: None,
        });

        let deferred_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                // Position Buffer
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::all(),
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::all(),
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
                // Normal-velocity Buffer
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::all(),
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::all(),
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
                // UV Buffer
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::all(),
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::all(),
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
                // Instance-material Buffer
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::all(),
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Uint,
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let frame_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                // Frame Uniform
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuFrame::min_size()),
                    },
                    count: None,
                },
                // Noise Texture
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: NonZeroU32::new(NOISE_TEXTURE_COUNT as u32),
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let render_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                // Render
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: RENDER_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Reservoir
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: RESERVOIR_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Reservoir Radiance
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: RADIANCE_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Reservoir Random
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: RANDOM_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Reservoir Visible Position
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: POSITION_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Reservoir Visible Normal
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: NORMAL_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Reservoir Sample Position
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: POSITION_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Reservoir Sample Normal
                BindGroupLayoutEntry {
                    binding: 7,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: NORMAL_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Previous Reservoir Textures
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: NonZeroU32::new(7),
                },
                BindGroupLayoutEntry {
                    binding: 9,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: NonZeroU32::new(7),
                },
            ],
        });

        Self {
            view_layout,
            deferred_layout,
            mesh_material_layout,
            texture_layout: None,
            frame_layout,
            render_layout,
            dummy_white_gpu_image: mesh_pipeline.dummy_white_gpu_image.clone(),
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct LightPipelineKey {
    pub entry_point: String,
    pub texture_count: usize,
}

impl SpecializedComputePipeline for LightPipeline {
    type Key = LightPipelineKey;

    fn specialize(&self, key: Self::Key) -> ComputePipelineDescriptor {
        ComputePipelineDescriptor {
            label: None,
            layout: Some(vec![
                self.view_layout.clone(),
                self.deferred_layout.clone(),
                self.mesh_material_layout.clone(),
                self.texture_layout.clone().unwrap(),
                self.frame_layout.clone(),
                self.render_layout.clone(),
            ]),
            shader: LIGHT_SHADER_HANDLE.typed::<Shader>(),
            shader_defs: vec![],
            entry_point: key.entry_point.into(),
        }
    }
}

pub struct Reservoir {
    pub reservoir: GpuImage,
    pub radiance: GpuImage,
    pub random: GpuImage,
    pub visible_position: GpuImage,
    pub visible_normal: GpuImage,
    pub sample_position: GpuImage,
    pub sample_normal: GpuImage,
}

#[derive(Component)]
pub struct LightPassTarget {
    pub render: GpuImage,
    pub reservoir: [Reservoir; 2],
}

fn prepare_light_pass_targets(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    mut texture_cache: ResMut<TextureCache>,
    cameras: Query<(Entity, &ExtractedCamera)>,
) {
    for (entity, camera) in &cameras {
        if let Some(size) = camera.physical_target_size {
            let extent = Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            };
            let size = size.as_vec2();
            let texture_usage = TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING;

            let mut create_texture = |texture_format, filter_mode| -> GpuImage {
                let sampler = render_device.create_sampler(&SamplerDescriptor {
                    label: None,
                    address_mode_u: AddressMode::ClampToEdge,
                    address_mode_v: AddressMode::ClampToEdge,
                    address_mode_w: AddressMode::ClampToEdge,
                    mag_filter: filter_mode,
                    min_filter: filter_mode,
                    mipmap_filter: filter_mode,
                    ..Default::default()
                });
                let texture = texture_cache.get(
                    &render_device,
                    TextureDescriptor {
                        label: None,
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: texture_format,
                        usage: texture_usage,
                    },
                );
                GpuImage {
                    texture: texture.texture,
                    texture_view: texture.default_view,
                    texture_format,
                    sampler,
                    size,
                }
            };

            let reservoir = [(); 2].map(|_| Reservoir {
                reservoir: create_texture(RESERVOIR_TEXTURE_FORMAT, FilterMode::Nearest),
                radiance: create_texture(RADIANCE_TEXTURE_FORMAT, FilterMode::Nearest),
                random: create_texture(RANDOM_TEXTURE_FORMAT, FilterMode::Nearest),
                visible_position: create_texture(POSITION_TEXTURE_FORMAT, FilterMode::Nearest),
                visible_normal: create_texture(NORMAL_TEXTURE_FORMAT, FilterMode::Nearest),
                sample_position: create_texture(POSITION_TEXTURE_FORMAT, FilterMode::Nearest),
                sample_normal: create_texture(NORMAL_TEXTURE_FORMAT, FilterMode::Nearest),
            });

            commands.entity(entity).insert(LightPassTarget {
                render: create_texture(RADIANCE_TEXTURE_FORMAT, FilterMode::Linear),
                reservoir,
            });
        }
    }
}

#[derive(Default)]
pub struct FrameCounter(usize);

#[derive(Debug, Default, Clone, Copy, ShaderType)]
pub struct GpuFrame {
    pub number: u32,
    pub kernel: [Vec3; 25],
}

#[derive(Default)]
pub struct FrameUniform {
    pub buffer: UniformBuffer<GpuFrame>,
}

fn prepare_frame_uniform(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut uniform: ResMut<FrameUniform>,
    mut counter: ResMut<FrameCounter>,
) {
    let mut kernel = [Vec3::ZERO; 25];
    for i in 0..5 {
        for j in 0..5 {
            let offset = IVec2::new(i - 2, j - 2);
            let index = (i + 5 * j) as usize;
            let value = match (offset.x.abs(), offset.y.abs()) {
                (0, 0) => 9.0 / 64.0,
                (0, 1) | (1, 0) => 3.0 / 32.0,
                (1, 1) => 1.0 / 16.0,
                (0, 2) | (2, 0) => 3.0 / 128.0,
                (1, 2) | (2, 1) => 1.0 / 64.0,
                (2, 2) => 1.0 / 256.0,
                _ => 0.0,
            };
            kernel[index] = Vec3::new(offset.x as f32, offset.y as f32, value);
        }
    }

    uniform.buffer.set(GpuFrame {
        number: counter.0 as u32,
        kernel,
    });
    uniform.buffer.write_buffer(&render_device, &render_queue);
    counter.0 += 1;
}

#[allow(dead_code)]
pub struct CachedLightPipelines {
    direct_lit: CachedComputePipelineId,
}

fn queue_light_pipelines(
    mut commands: Commands,
    layout: Res<TextureBindGroupLayout>,
    mut pipeline: ResMut<LightPipeline>,
    mut pipelines: ResMut<SpecializedComputePipelines<LightPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
) {
    pipeline.texture_layout = Some(layout.layout.clone());

    let [direct_lit] = ["direct_lit"].map(|entry_point| {
        let key = LightPipelineKey {
            entry_point: entry_point.into(),
            texture_count: layout.count,
        };
        pipelines.specialize(&mut pipeline_cache, &pipeline, key)
    });

    commands.insert_resource(CachedLightPipelines { direct_lit })
}

#[derive(Component)]
pub struct ViewBindGroup(pub BindGroup);

#[allow(clippy::too_many_arguments)]
pub fn queue_view_bind_groups(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline: Res<LightPipeline>,
    shadow_pipeline: Res<ShadowPipeline>,
    light_meta: Res<LightMeta>,
    global_light_meta: Res<GlobalLightMeta>,
    view_uniforms: Res<ViewUniforms>,
    views: Query<(Entity, &ViewShadowBindings, &ViewClusterBindings)>,
) {
    if let (Some(view_binding), Some(light_binding), Some(point_light_binding)) = (
        view_uniforms.uniforms.binding(),
        light_meta.view_gpu_lights.binding(),
        global_light_meta.gpu_point_lights.binding(),
    ) {
        for (entity, view_shadow_bindings, view_cluster_bindings) in &views {
            let bind_group = render_device.create_bind_group(&BindGroupDescriptor {
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: view_binding.clone(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: light_binding.clone(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureView(
                            &view_shadow_bindings.point_light_depth_texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: BindingResource::Sampler(&shadow_pipeline.point_light_sampler),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: BindingResource::TextureView(
                            &view_shadow_bindings.directional_light_depth_texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: BindingResource::Sampler(
                            &shadow_pipeline.directional_light_sampler,
                        ),
                    },
                    BindGroupEntry {
                        binding: 6,
                        resource: point_light_binding.clone(),
                    },
                    BindGroupEntry {
                        binding: 7,
                        resource: view_cluster_bindings.light_index_lists_binding().unwrap(),
                    },
                    BindGroupEntry {
                        binding: 8,
                        resource: view_cluster_bindings.offsets_and_counts_binding().unwrap(),
                    },
                ],
                label: None,
                layout: &pipeline.view_layout,
            });

            commands.entity(entity).insert(ViewBindGroup(bind_group));
        }
    }
}

#[derive(Component, Clone)]
pub struct LightBindGroup {
    pub deferred: BindGroup,
    pub frame: BindGroup,
    pub render: BindGroup,
}

#[allow(clippy::too_many_arguments)]
fn queue_light_bind_groups(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline: Res<LightPipeline>,
    counter: Res<FrameCounter>,
    frame_uniform: Res<FrameUniform>,
    noise_texture: Res<NoiseTexture>,
    images: Res<RenderAssets<Image>>,
    query: Query<(Entity, &PrepassTarget, &LightPassTarget)>,
) {
    let mut noise_texture_views = vec![];
    for handle in noise_texture.iter() {
        let image = match images.get(handle) {
            Some(image) => image,
            None => {
                return;
            }
        };
        noise_texture_views.push(&*image.texture_view);
    }

    let noise_sampler = render_device.create_sampler(&SamplerDescriptor {
        label: None,
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::Repeat,
        address_mode_w: AddressMode::Repeat,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: FilterMode::Nearest,
        ..Default::default()
    });

    for (entity, prepass, light_pass) in &query {
        if let Some(frame_binding) = frame_uniform.buffer.binding() {
            let deferred = render_device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &pipeline.deferred_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&prepass.position.texture_view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&prepass.position.sampler),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureView(&prepass.normal.texture_view),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: BindingResource::Sampler(&prepass.normal.sampler),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: BindingResource::TextureView(&prepass.velocity_uv.texture_view),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: BindingResource::Sampler(&prepass.velocity_uv.sampler),
                    },
                    BindGroupEntry {
                        binding: 6,
                        resource: BindingResource::TextureView(
                            &prepass.instance_material.texture_view,
                        ),
                    },
                ],
            });

            let frame = render_device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &pipeline.frame_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: frame_binding,
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::TextureViewArray(&noise_texture_views),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::Sampler(&noise_sampler),
                    },
                ],
            });

            let current_id = counter.0 % 2;
            let current_reservoir = &light_pass.reservoir[current_id];
            let previous_reservoir = &light_pass.reservoir[1 - current_id];
            let render = render_device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &pipeline.render_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&light_pass.render.texture_view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::TextureView(
                            &current_reservoir.reservoir.texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureView(
                            &current_reservoir.radiance.texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: BindingResource::TextureView(
                            &current_reservoir.random.texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: BindingResource::TextureView(
                            &current_reservoir.visible_position.texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: BindingResource::TextureView(
                            &current_reservoir.visible_normal.texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 6,
                        resource: BindingResource::TextureView(
                            &current_reservoir.sample_position.texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 7,
                        resource: BindingResource::TextureView(
                            &current_reservoir.sample_normal.texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 8,
                        resource: BindingResource::TextureViewArray(&[
                            &previous_reservoir.reservoir.texture_view,
                            &previous_reservoir.radiance.texture_view,
                            &previous_reservoir.random.texture_view,
                            &previous_reservoir.visible_position.texture_view,
                            &previous_reservoir.visible_normal.texture_view,
                            &previous_reservoir.sample_position.texture_view,
                            &previous_reservoir.sample_normal.texture_view,
                        ]),
                    },
                    BindGroupEntry {
                        binding: 9,
                        resource: BindingResource::SamplerArray(&[
                            &previous_reservoir.reservoir.sampler,
                            &previous_reservoir.radiance.sampler,
                            &previous_reservoir.random.sampler,
                            &previous_reservoir.visible_position.sampler,
                            &previous_reservoir.visible_normal.sampler,
                            &previous_reservoir.sample_position.sampler,
                            &previous_reservoir.sample_normal.sampler,
                        ]),
                    },
                ],
            });

            commands.entity(entity).insert(LightBindGroup {
                deferred,
                frame,
                render,
            });
        }
    }
}

pub struct LightPassNode {
    query: QueryState<(
        &'static ExtractedCamera,
        &'static ViewUniformOffset,
        &'static ViewLightsUniformOffset,
        &'static ViewBindGroup,
        &'static LightBindGroup,
    )>,
}

impl LightPassNode {
    pub const IN_VIEW: &'static str = "view";

    pub fn new(world: &mut World) -> Self {
        Self {
            query: world.query_filtered(),
        }
    }
}

impl Node for LightPassNode {
    fn input(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::IN_VIEW, SlotType::Entity)]
    }

    fn update(&mut self, world: &mut World) {
        self.query.update_archetypes(world);
    }

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let entity = graph.get_input_entity(Self::IN_VIEW)?;
        let (camera, view_uniform, view_lights, view_bind_group, light_bind_group) =
            match self.query.get_manual(world, entity) {
                Ok(query) => query,
                Err(_) => return Ok(()),
            };
        let mesh_material_bind_group = match world.get_resource::<MeshMaterialBindGroup>() {
            Some(bind_group) => bind_group,
            None => return Ok(()),
        };
        let pipelines = world.resource::<CachedLightPipelines>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let mut pass = render_context
            .command_encoder
            .begin_compute_pass(&ComputePassDescriptor::default());

        pass.set_bind_group(
            0,
            &view_bind_group.0,
            &[view_uniform.offset, view_lights.offset],
        );
        pass.set_bind_group(1, &light_bind_group.deferred, &[]);
        pass.set_bind_group(2, &mesh_material_bind_group.mesh_material, &[]);
        pass.set_bind_group(3, &mesh_material_bind_group.texture, &[]);
        pass.set_bind_group(4, &light_bind_group.frame, &[]);
        pass.set_bind_group(5, &light_bind_group.render, &[]);

        if let Some(pipeline) = pipeline_cache.get_compute_pipeline(pipelines.direct_lit) {
            pass.set_pipeline(pipeline);

            let size = camera.physical_target_size.unwrap();
            let count = (size + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            pass.dispatch_workgroups(count.x, count.y, 1);
        }

        Ok(())
    }
}
