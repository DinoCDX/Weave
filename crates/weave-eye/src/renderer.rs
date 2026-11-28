use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};

use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::backend::Backend;
use crate::camera::{Camera, FrameParams};
use crate::cluster::{self, CLUSTER_COUNT, Clip, ClusterData, SLICES, TILES_X, TILES_Y};
use crate::dynres::DynamicResolution;
use crate::gpu_timer::GpuTimer;
use crate::graph::{PassContext, RenderGraph, Timing, default_passes};
use crate::lights::{Light, MAX_DIRECTIONAL, MAX_LIGHTS};
use crate::mesh::{Mesh, cube};
use crate::shader::{self, EMBEDDED_CUBE, ShaderError, ShaderState, ShaderUpdate, content_hash};

const FRAME_BYTES: u64 =
    (16 + 16 + 4 + 4 + 4 + 16 + 4 + MAX_DIRECTIONAL as u64 * 16 + 16 + 16 + 4 + 4 + 4) * 4;
const AO_RADIUS: f32 = 0.9;
const AO_BIAS: f32 = 0.03;
const AO_STRENGTH: f32 = 2.2;
const SKY_COLOR: [f32; 3] = [0.62, 0.70, 0.84];
const GROUND_COLOR: [f32; 3] = [0.30, 0.27, 0.24];
const SHADOW_SIZE: u32 = 2048;
const SHADOW_HALF_EXTENT: f32 = 22.0;
const SHADOW_DISTANCE: f32 = 45.0;
const SHADOW_NEAR: f32 = 5.0;
const SHADOW_FAR: f32 = 100.0;
const SHADOW_BIAS: f32 = 0.0008;
const SHADOW_NORMAL_OFFSET: f32 = 0.04;
const LIGHT_BYTES: u64 = 64;
pub const DIRECT_LIGHT_LIMIT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightPath {
    Auto,
    Direct,
    Clustered,
}
const CLUSTER_BLOCK: &str = "    let cluster = cluster_index(input.clip.xy, input.depth);\n    let range = cluster_ranges[cluster];\n    for (var k = 0u; k < range.y; k++) {\n        let light = lights[light_indices[range.x + k]];\n        direct = direct + shade(light, surf);\n    }";
const DIRECT_BLOCK: &str = "    for (var k = 0u; k < u32(frame.viewport.w); k++) {\n        direct = direct + shade(lights[k], surf);\n    }";

pub(crate) fn direct_source(clustered: &str) -> Result<String, String> {
    if !clustered.contains(CLUSTER_BLOCK) {
        return Err("the clustered light loop was not found in the shader".to_owned());
    }
    Ok(clustered.replace(CLUSTER_BLOCK, DIRECT_BLOCK))
}
const VERTEX_BYTES: u64 = 24;
pub(crate) const INSTANCE_BYTES: u64 = 64;
const CULL_PARAM_BYTES: u64 = 128;
pub(crate) const MAX_LODS: usize = 4;
pub(crate) const ARGS_BYTES: u64 = 20;
const SLOT_ARGS_BYTES: u64 = ARGS_BYTES * MAX_LODS as u64;
const CAMERA_BYTES: u64 = 160;
const LOD_PIXELS: [f32; 3] = [200.0, 80.0, 30.0];
const HZB_L0_SOURCE: &str = include_str!("../shaders/hzb_l0.wgsl");
const HZB_DOWN_SOURCE: &str = include_str!("../shaders/hzb_down.wgsl");
const WORKGROUP_SIZE: u32 = 64;
const DEFAULT_MATERIAL: [f32; 4] = [0.0, 0.5, 0.0, 0.0];
const PASS_ALL: [[f32; 4]; 6] = [[0.0, 0.0, 0.0, 1.0]; 6];
const CULL_SOURCE: &str = include_str!("../shaders/cull.wgsl");
const DEPTH_SOURCE: &str = include_str!("../shaders/depth.wgsl");
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawInstance {
    pub model: Mat4,
    pub color: [f32; 4],
    pub material: [f32; 4],
}

impl DrawInstance {
    pub fn new(model: Mat4, color: [f32; 4]) -> Self {
        Self {
            model,
            color,
            material: DEFAULT_MATERIAL,
        }
    }

    pub fn with_material(mut self, metallic: f32, roughness: f32) -> Self {
        self.material = [metallic, roughness, 0.0, 0.0];
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameStats {
    pub render_scale: f32,
    pub submitted: usize,
    pub cull: Duration,
    pub lights: usize,
    pub lights_dropped: usize,
    pub upload_bytes: usize,
    pub shadow_reused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    Surface(String),
    NoAdapter(String),
    Device(String),
    SurfaceUnsupported,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Surface(text) => write!(f, "could not create the window surface: {text}"),
            Self::NoAdapter(text) => write!(f, "no suitable GPU adapter was found: {text}"),
            Self::Device(text) => write!(f, "could not create a GPU device: {text}"),
            Self::SurfaceUnsupported => write!(f, "the window surface is not supported by the GPU"),
        }
    }
}

impl std::error::Error for RenderError {}

#[allow(dead_code)]
pub(crate) struct GpuLod {
    pub(crate) vertices: wgpu::Buffer,
    pub(crate) indices: wgpu::Buffer,
    pub(crate) index_count: u32,
    pub(crate) clusters: wgpu::Buffer,
    pub(crate) cluster_count: u32,
}

pub(crate) struct GpuMesh {
    pub(crate) lods: Vec<GpuLod>,
    pub(crate) radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshHandle(usize);

pub const CUBE: MeshHandle = MeshHandle(0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Batch<'a> {
    pub mesh: MeshHandle,
    pub instances: &'a [DrawInstance],
    pub version: Option<u64>,
}

impl<'a> Batch<'a> {
    pub fn new(mesh: MeshHandle, instances: &'a [DrawInstance]) -> Self {
        Self {
            mesh,
            instances,
            version: None,
        }
    }

    pub fn versioned(mesh: MeshHandle, instances: &'a [DrawInstance], version: u64) -> Self {
        Self {
            mesh,
            instances,
            version: Some(version),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BatchDraw {
    pub(crate) mesh: usize,
    pub(crate) first: u32,
    pub(crate) count: u32,
}

pub(crate) struct BatchSlot {
    pub(crate) params: [wgpu::Buffer; 3],
    pub(crate) args: [wgpu::Buffer; 3],
}

impl BatchSlot {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            params: [
                create_uniform(device, CULL_PARAM_BYTES, "cull params"),
                create_uniform(device, CULL_PARAM_BYTES, "cull params phase 2"),
                create_uniform(device, CULL_PARAM_BYTES, "shadow cull params"),
            ],
            args: [
                create_args(device),
                create_args(device),
                create_args(device),
            ],
        }
    }
}

pub(crate) struct Hzb {
    _texture: wgpu::Texture,
    pub(crate) all: wgpu::TextureView,
    pub(crate) levels: u32,
    pub(crate) dims: Vec<(u32, u32)>,
    pub(crate) l0: wgpu::BindGroup,
    pub(crate) down: Vec<wgpu::BindGroup>,
}

pub(crate) struct DrawResources {
    pub(crate) shadow_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_bind_group: wgpu::BindGroup,
    pub(crate) shadow_uniform: wgpu::Buffer,
    pub(crate) shadow_view: wgpu::TextureView,
    pub(crate) shadow_sampler: wgpu::Sampler,
    pub(crate) shadows_enabled: bool,
    pub(crate) ambient_occlusion_enabled: bool,
    pub(crate) ao_raw: wgpu::TextureView,
    pub(crate) ao_blur: wgpu::TextureView,
    pub(crate) ao_sampler: wgpu::Sampler,
    pub(crate) ao_layout: wgpu::BindGroupLayout,
    pub(crate) ao_blur_layout: wgpu::BindGroupLayout,
    pub(crate) ao_pipeline: wgpu::RenderPipeline,
    pub(crate) ao_blur_pipeline: wgpu::RenderPipeline,
    pub(crate) ao_bind: Option<wgpu::BindGroup>,
    pub(crate) ao_blur_bind: Option<wgpu::BindGroup>,
    pub(crate) ao_valid: bool,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) direct: wgpu::RenderPipeline,
    pub(crate) depth_pipeline: wgpu::RenderPipeline,
    pub(crate) use_direct: bool,
    pub(crate) bind_layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) uniform: wgpu::Buffer,
    pub(crate) lights_store: wgpu::Buffer,
    pub(crate) indices: wgpu::Buffer,
    pub(crate) indices_capacity: usize,
    pub(crate) ranges: wgpu::Buffer,
    pub(crate) meshes: Vec<GpuMesh>,
    pub(crate) batches: Vec<BatchDraw>,
    pub(crate) groups: Vec<u32>,
    pub(crate) cull_layout: wgpu::BindGroupLayout,
    pub(crate) cull_pipeline: wgpu::ComputePipeline,
    pub(crate) hzb_l0_layout: wgpu::BindGroupLayout,
    pub(crate) hzb_down_layout: wgpu::BindGroupLayout,
    pub(crate) hzb_l0_pipeline: wgpu::ComputePipeline,
    pub(crate) hzb_down_pipeline: wgpu::ComputePipeline,
    pub(crate) hzb: Hzb,
    pub(crate) camera_ubo: wgpu::Buffer,
    pub(crate) raw_instances: wgpu::Buffer,
    pub(crate) flags: wgpu::Buffer,
    pub(crate) camera_compact: wgpu::Buffer,
    pub(crate) camera_compact2: wgpu::Buffer,
    pub(crate) shadow_compact: wgpu::Buffer,
    pub(crate) capacity: usize,
    pub(crate) slots: Vec<BatchSlot>,
    pub(crate) binds: Vec<[wgpu::BindGroup; 3]>,
    pub(crate) binds_valid: bool,
    pub(crate) build_hzb: bool,
    pub(crate) occlusion_active: bool,
    pub(crate) hzb_ready: bool,
    pub(crate) uploaded: Vec<u8>,
    pub(crate) upload_bytes: usize,
    pub(crate) instances_dirty: bool,
    pub(crate) batch_versions: Vec<Option<u64>>,
    pub(crate) layout_changed: bool,
    pub(crate) shadow_reuse: bool,
    pub(crate) shadow_valid: bool,
    pub(crate) shadow_matrix: [f32; 16],
}

pub struct Renderer {
    gpu_timer: Option<GpuTimer>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    graph: RenderGraph,
    draws: DrawResources,
    shader: ShaderState,
    scratch: Vec<u8>,
    culling: bool,
    occlusion: bool,
    storage_lights: Vec<Light>,
    clusters: ClusterData,
    light_path: LightPath,
    shadows: bool,
    ambient_occlusion: bool,
    target: (u32, u32),
    scene: Option<wgpu::TextureView>,
    upscaler: Upscaler,
    dynres: Option<DynamicResolution>,
    scale: f32,
    last_frame: Option<Instant>,
    frame_index: u64,
    scale_changed_at: u64,
}

pub(crate) struct Upscaler {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bind: Option<wgpu::BindGroup>,
}

impl Upscaler {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("upscale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("upscale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("upscale"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/upscale.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("upscale"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("upscale"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            sampler,
            bind: None,
        }
    }

    fn make_bind(&self, device: &wgpu::Device, scene: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("upscale"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        timing: Option<(&wgpu::QuerySet, u32)>,
    ) {
        let Some(bind) = self.bind.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("upscale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timing.map(|(set, index)| wgpu::RenderPassTimestampWrites {
                query_set: set,
                beginning_of_pass_write_index: Some(index),
                end_of_pass_write_index: Some(index + 1),
            }),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn internal_size(width: u32, height: u32, scale: f32) -> (u32, u32) {
    (
        (width as f32 * scale).round().max(1.0) as u32,
        (height as f32 * scale).round().max(1.0) as u32,
    )
}

fn create_scene(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("scene color"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

impl Renderer {
    pub fn add_mesh(&mut self, mesh: &Mesh) -> MeshHandle {
        self.add_mesh_lods(std::slice::from_ref(mesh))
    }

    pub fn add_mesh_lods(&mut self, lods: &[Mesh]) -> MeshHandle {
        assert!(
            !lods.is_empty() && lods.len() <= MAX_LODS,
            "a mesh needs between 1 and {MAX_LODS} levels of detail"
        );
        self.draws.meshes.push(upload_mesh(&self.device, lods));
        MeshHandle(self.draws.meshes.len() - 1)
    }
}

impl Renderer {
    pub fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
        backend: Backend,
    ) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: backend.wgpu_backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(target)
            .map_err(|error| RenderError::Surface(error.to_string()))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .map_err(|error| RenderError::NoAdapter(error.to_string()))?;
        let timestamps_supported = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let descriptor = wgpu::DeviceDescriptor {
            required_features: if timestamps_supported {
                wgpu::Features::TIMESTAMP_QUERY
            } else {
                wgpu::Features::empty()
            },
            ..wgpu::DeviceDescriptor::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))
            .map_err(|error| RenderError::Device(error.to_string()))?;
        let config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(RenderError::SurfaceUnsupported)?;
        surface.configure(&device, &config);

        let upscaler = Upscaler::new(&device, config.format);
        let depth = create_depth(&device, config.width, config.height);
        let (draws, shader) =
            DrawResources::new(&device, config.format, config.width, config.height, &depth);
        let graph = RenderGraph::new(default_passes()).expect("the default passes are valid");
        let gpu_timer =
            timestamps_supported.then(|| GpuTimer::new(&device, &queue, graph.slot_count()));

        Ok(Self {
            gpu_timer,
            surface,
            device,
            queue,
            config,
            depth,
            graph,
            draws,
            shader,
            scratch: Vec::new(),
            culling: false,
            occlusion: true,
            storage_lights: Vec::new(),
            clusters: ClusterData::default(),
            light_path: LightPath::Auto,
            shadows: true,
            ambient_occlusion: true,
            target: (0, 0),
            scene: None,
            upscaler,
            dynres: None,
            scale: 1.0,
            last_frame: None,
            frame_index: 0,
            scale_changed_at: 0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.target = (0, 0);
        self.ensure_targets();
    }

    pub fn set_dynamic_resolution(&mut self, config: Option<DynamicResolution>) {
        self.scale = config.map_or(1.0, |controller| controller.scale());
        self.dynres = config;
        self.last_frame = None;
    }

    pub fn render_scale(&self) -> f32 {
        self.scale
    }

    fn ensure_targets(&mut self) {
        let size = internal_size(self.config.width, self.config.height, self.scale);
        if size == self.target {
            return;
        }
        self.target = size;
        self.depth = create_depth(&self.device, size.0, size.1);
        let ao_dims = ao_size(size);
        self.draws.ao_raw = create_ao_texture(&self.device, ao_dims.0, ao_dims.1);
        self.draws.ao_blur = create_ao_texture(&self.device, ao_dims.0, ao_dims.1);
        self.draws.ao_valid = false;
        self.draws.hzb = create_hzb(
            &self.device,
            size.0,
            size.1,
            &self.depth,
            &self.draws.hzb_l0_layout,
            &self.draws.hzb_down_layout,
        );
        self.draws.hzb_ready = false;
        self.draws.binds_valid = false;
        self.scene = (size != (self.config.width, self.config.height))
            .then(|| create_scene(&self.device, self.config.format, size));
        let bind = self
            .scene
            .as_ref()
            .map(|view| self.upscaler.make_bind(&self.device, view));
        self.upscaler.bind = bind;
    }

    pub fn visible_counts(&self) -> Vec<u32> {
        let count = self.draws.batches.len();
        if count == 0 {
            return Vec::new();
        }
        let words = 2 * MAX_LODS;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("visible counts"),
            size: (count * words * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("visible counts"),
            });
        for (index, slot) in self.draws.slots.iter().take(count).enumerate() {
            for phase in 0..2 {
                for lod in 0..MAX_LODS {
                    let word = index * words + phase * MAX_LODS + lod;
                    encoder.copy_buffer_to_buffer(
                        &slot.args[phase],
                        lod as u64 * ARGS_BYTES + 4,
                        &staging,
                        word as u64 * 4,
                        4,
                    );
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        let (sender, receiver) = std::sync::mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.wait_idle();
        if !matches!(receiver.recv(), Ok(Ok(()))) {
            return Vec::new();
        }
        let counts: Vec<u32> = {
            let Ok(data) = staging.slice(..).get_mapped_range() else {
                staging.unmap();
                return Vec::new();
            };
            data.chunks_exact(words * 4)
                .map(|batch| {
                    batch
                        .chunks_exact(4)
                        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .sum()
                })
                .collect()
        };
        staging.unmap();
        counts
    }

    fn rebuild_ao_binds(&mut self) {
        self.draws.ao_bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ao"),
            layout: &self.draws.ao_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.draws.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.depth),
                },
            ],
        }));
        self.draws.ao_blur_bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ao blur"),
            layout: &self.draws.ao_blur_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.draws.ao_raw),
            }],
        }));
        self.draws.bind_group = make_bind_group(
            &self.device,
            &self.draws.bind_layout,
            &self.draws.uniform,
            &self.draws.lights_store,
            &self.draws.indices,
            &self.draws.ranges,
            &self.draws.shadow_view,
            &self.draws.shadow_sampler,
            &self.draws.ao_blur,
            &self.draws.ao_sampler,
        );
        self.draws.ao_valid = true;
    }

    pub fn set_ambient_occlusion(&mut self, enabled: bool) {
        self.ambient_occlusion = enabled;
    }

    pub fn set_shadows(&mut self, enabled: bool) {
        self.shadows = enabled;
        self.draws.shadow_valid = false;
    }

    pub fn set_light_path(&mut self, path: LightPath) {
        self.light_path = path;
    }

    pub fn set_culling(&mut self, enabled: bool) {
        self.culling = enabled;
    }

    pub fn set_occlusion_culling(&mut self, enabled: bool) {
        self.occlusion = enabled;
    }

    pub fn use_shader_file(&mut self, path: impl AsRef<Path>) -> Result<ShaderUpdate, ShaderError> {
        let path = path.as_ref().to_path_buf();
        let stamp = shader::modified_time(&path)?;
        let text = shader::read_text(&path)?;
        self.shader.path = Some(path);
        self.shader.stamp = Some(stamp);
        self.install_shader(&text)
    }

    pub fn reload_shader(&mut self) -> Result<ShaderUpdate, ShaderError> {
        let Some(path) = self.shader.path.clone() else {
            return Ok(ShaderUpdate::Unchanged);
        };
        let stamp = shader::modified_time(&path)?;
        if self.shader.stamp == Some(stamp) {
            return Ok(ShaderUpdate::Unchanged);
        }
        self.shader.stamp = Some(stamp);
        let text = shader::read_text(&path)?;
        self.install_shader(&text)
    }

    pub fn shader_error(&self) -> Option<&str> {
        self.shader.last_error.as_deref()
    }

    fn install_shader(&mut self, text: &str) -> Result<ShaderUpdate, ShaderError> {
        let hash = content_hash(text);
        if hash == self.shader.active {
            return Ok(ShaderUpdate::Unchanged);
        }
        let direct_text = match direct_source(text) {
            Ok(direct_text) => direct_text,
            Err(message) => {
                self.shader.last_error = Some(message.clone());
                return Err(ShaderError::Compile(message));
            }
        };
        let direct = match build_pipeline(
            &self.device,
            &self.shader.layout,
            self.shader.format,
            &direct_text,
            false,
            wgpu::CompareFunction::LessEqual,
        ) {
            Ok(direct) => direct,
            Err(message) => {
                self.shader.last_error = Some(message.clone());
                return Err(ShaderError::Compile(message));
            }
        };
        if let Some(pipeline) = self.shader.lookup(hash) {
            self.draws.pipeline = pipeline;
            self.draws.direct = direct;
            self.shader.active = hash;
            self.shader.last_error = None;
            return Ok(ShaderUpdate::FromCache);
        }
        match build_pipeline(
            &self.device,
            &self.shader.layout,
            self.shader.format,
            text,
            false,
            wgpu::CompareFunction::LessEqual,
        ) {
            Ok(pipeline) => {
                self.shader.remember(hash, pipeline.clone());
                self.draws.pipeline = pipeline;
                self.draws.direct = direct;
                self.shader.active = hash;
                self.shader.last_error = None;
                Ok(ShaderUpdate::Compiled)
            }
            Err(message) => {
                self.shader.last_error = Some(message.clone());
                Err(ShaderError::Compile(message))
            }
        }
    }

    pub fn draw(
        &mut self,
        camera: &Camera,
        lights: &[Light],
        params: &FrameParams,
        batches: &[Batch<'_>],
    ) -> Result<FrameStats, RenderError> {
        let now = Instant::now();
        let previous = self.last_frame.replace(now);
        let gpu_frame = self.graph.take_gpu_frame();
        self.frame_index += 1;
        if let Some(controller) = self.dynres.as_mut() {
            let sample = if self.gpu_timer.is_some() {
                gpu_frame
            } else {
                previous.map(|earlier| now - earlier)
            };
            let settled = self.gpu_timer.is_none()
                || self.frame_index - self.scale_changed_at > crate::gpu_timer::RING as u64 + 1;
            if let Some(frame) = sample.filter(|_| settled) {
                let before = self.scale;
                self.scale = controller.observe(frame);
                if self.scale != before {
                    self.scale_changed_at = self.frame_index;
                }
            }
        }
        self.ensure_targets();
        if !self.draws.ao_valid {
            self.rebuild_ao_binds();
        }
        self.draws.ambient_occlusion_enabled = self.ambient_occlusion;
        let scale_now = self.scale;
        let submitted: usize = batches.iter().map(|batch| batch.instances.len()).sum();
        let total_lights = lights.len().min(MAX_LIGHTS + MAX_DIRECTIONAL);
        let dropped_before_upload = lights.len().saturating_sub(total_lights);
        let stats_for_skip = |cull: Duration| FrameStats {
            submitted,
            cull,
            lights: total_lights,
            lights_dropped: dropped_before_upload,
            upload_bytes: 0,
            shadow_reused: false,
            render_scale: scale_now,
        };
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(stats_for_skip(Duration::ZERO));
            }
            _ => return Ok(stats_for_skip(Duration::ZERO)),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let aspect = self.target.0 as f32 / self.target.1 as f32;
        let view_proj = camera.view_proj(aspect);
        let mut directional = [Light::default(); MAX_DIRECTIONAL];
        let (directional_count, light_dropped) =
            classify(lights, &mut self.storage_lights, &mut directional);
        let light_total = directional_count + self.storage_lights.len();
        self.draws.use_direct = match self.light_path {
            LightPath::Auto => self.storage_lights.len() <= DIRECT_LIGHT_LIMIT,
            LightPath::Direct => true,
            LightPath::Clustered => false,
        };
        if !self.draws.use_direct {
            cluster::build(
                Clip {
                    near: camera.near,
                    far: camera.far,
                },
                camera.view(),
                camera.projection(aspect),
                &self.storage_lights,
                &mut self.clusters,
            );
        }

        self.scratch.clear();
        push_floats(&mut self.scratch, &view_proj.to_cols_array());
        push_floats(&mut self.scratch, &camera.view().to_cols_array());
        push_floats(
            &mut self.scratch,
            &[params.exposure, params.ambient, camera.near, camera.far],
        );
        push_floats(
            &mut self.scratch,
            &[TILES_X as f32, TILES_Y as f32, SLICES as f32, 0.0],
        );
        push_floats(
            &mut self.scratch,
            &[
                self.target.0 as f32,
                self.target.1 as f32,
                directional_count as f32,
                self.storage_lights.len() as f32,
            ],
        );
        let shadows_enabled = self.shadows && directional_count > 0;
        let shadow_view_proj = match directional.first() {
            Some(Light::Directional { direction, .. }) if shadows_enabled => {
                key_shadow_matrix(*direction)
            }
            _ => Mat4::IDENTITY,
        };
        self.draws.shadows_enabled = shadows_enabled;
        push_floats(&mut self.scratch, &shadow_view_proj.to_cols_array());
        push_floats(
            &mut self.scratch,
            &[
                SHADOW_BIAS,
                1.0 / SHADOW_SIZE as f32,
                if shadows_enabled { 1.0 } else { 0.0 },
                SHADOW_NORMAL_OFFSET,
            ],
        );
        self.queue.write_buffer(
            &self.draws.shadow_uniform,
            0,
            &shadow_view_proj_bytes(shadow_view_proj),
        );
        for (index, light) in directional.iter().enumerate() {
            if index < directional_count {
                push_floats(&mut self.scratch, &light.encode());
            } else {
                push_floats(&mut self.scratch, &[0.0; 16]);
            }
        }
        let projection = camera.projection(aspect);
        push_floats(&mut self.scratch, &projection.inverse().to_cols_array());
        push_floats(&mut self.scratch, &projection.to_cols_array());
        push_floats(
            &mut self.scratch,
            &[
                AO_RADIUS,
                AO_BIAS,
                AO_STRENGTH,
                if self.ambient_occlusion { 1.0 } else { 0.0 },
            ],
        );
        push_floats(
            &mut self.scratch,
            &[SKY_COLOR[0], SKY_COLOR[1], SKY_COLOR[2], 0.0],
        );
        push_floats(
            &mut self.scratch,
            &[GROUND_COLOR[0], GROUND_COLOR[1], GROUND_COLOR[2], 0.0],
        );
        self.queue
            .write_buffer(&self.draws.uniform, 0, &self.scratch);
        self.upload_lights();

        let cull_start = Instant::now();
        self.draws.occlusion_active = self.occlusion && self.culling && self.draws.hzb_ready;
        self.draws.build_hzb = self.culling;
        let camera_planes = self.plane_set(view_proj);
        let shadow_planes = shadows_enabled.then(|| self.plane_set(shadow_view_proj));
        let camera_screen = [
            self.target.0 as f32,
            self.target.1 as f32,
            self.draws.hzb.levels as f32,
            if self.draws.occlusion_active {
                1.0
            } else {
                0.0
            },
        ];
        let camera_lods = [LOD_PIXELS[0], LOD_PIXELS[1], LOD_PIXELS[2], camera.near];
        let camera_data = camera_bytes(
            camera.view(),
            camera.projection(aspect),
            camera_screen,
            camera_lods,
        );
        self.queue
            .write_buffer(&self.draws.camera_ubo, 0, &camera_data);
        let shadow_candidate = shadows_enabled
            && self.draws.shadow_valid
            && self.draws.shadow_matrix == shadow_view_proj.to_cols_array();
        self.upload_batches(batches, camera_planes, shadow_planes, shadow_candidate);
        let cull = cull_start.elapsed();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let timing_frame = match self.gpu_timer.as_mut() {
            Some(timer) => timer.begin_frame(&self.device, &mut self.graph),
            None => false,
        };
        let timestamps = if timing_frame {
            self.gpu_timer.as_ref().map(GpuTimer::query_set)
        } else {
            None
        };
        {
            let mut cx = PassContext {
                encoder: &mut encoder,
                color: self.scene.as_ref().unwrap_or(&view),
                depth: &self.depth,
                draws: &self.draws,
                timestamps,
                slot: 0,
            };
            self.graph.execute(&mut cx);
        }
        let blit_timed = self.scene.is_some() && timing_frame;
        let blit_index = u32::try_from(self.graph.slot_count() * 2).unwrap_or(0);
        self.upscaler.record(
            &mut encoder,
            &view,
            timestamps
                .filter(|_| blit_timed)
                .map(|set| (set, blit_index)),
        );
        if timing_frame && let Some(timer) = self.gpu_timer.as_mut() {
            timer.finish_frame(&mut encoder, blit_timed);
        }
        self.queue.submit(Some(encoder.finish()));
        if !shadows_enabled {
            self.draws.shadow_valid = false;
        } else if !self.draws.shadow_reuse {
            self.draws.shadow_valid = true;
            self.draws.shadow_matrix = shadow_view_proj.to_cols_array();
        }
        self.draws.hzb_ready = self.culling;
        if timing_frame && let Some(timer) = self.gpu_timer.as_mut() {
            timer.map_after_submit();
        }
        self.queue.present(frame);
        Ok(FrameStats {
            submitted,
            cull,
            lights: light_total,
            lights_dropped: light_dropped,
            upload_bytes: self.draws.upload_bytes,
            shadow_reused: self.draws.shadow_reuse,
            render_scale: self.scale,
        })
    }

    pub fn wait_idle(&self) {
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
    }

    pub fn graph_report(&self) -> String {
        self.graph.report()
    }

    pub fn pass_names(&self) -> Vec<&'static str> {
        self.graph.pass_names()
    }

    pub fn pass_timing(&self, name: &str) -> Option<Timing> {
        self.graph.timing(name)
    }

    pub fn gpu_timing(&self, name: &str) -> Option<Timing> {
        self.graph.gpu_timing(name)
    }

    pub fn gpu_timestamps_supported(&self) -> bool {
        self.gpu_timer.is_some()
    }

    fn upload_lights(&mut self) {
        self.scratch.clear();
        for light in &self.storage_lights {
            push_floats(&mut self.scratch, &light.encode());
        }
        if !self.scratch.is_empty() {
            self.queue
                .write_buffer(&self.draws.lights_store, 0, &self.scratch);
        }

        if self.draws.use_direct {
            return;
        }
        self.scratch.clear();
        for range in &self.clusters.ranges {
            self.scratch.extend_from_slice(&range[0].to_le_bytes());
            self.scratch.extend_from_slice(&range[1].to_le_bytes());
        }
        self.queue
            .write_buffer(&self.draws.ranges, 0, &self.scratch);

        let needed = self.clusters.indices.len().max(1);
        if needed > self.draws.indices_capacity {
            let capacity = needed.next_power_of_two();
            self.draws.indices = create_storage(&self.device, capacity as u64 * 4, "light indices");
            self.draws.indices_capacity = capacity;
            self.draws.bind_group = make_bind_group(
                &self.device,
                &self.draws.bind_layout,
                &self.draws.uniform,
                &self.draws.lights_store,
                &self.draws.indices,
                &self.draws.ranges,
                &self.draws.shadow_view,
                &self.draws.shadow_sampler,
                &self.draws.ao_blur,
                &self.draws.ao_sampler,
            );
        }
        self.scratch.clear();
        for index in &self.clusters.indices {
            self.scratch.extend_from_slice(&index.to_le_bytes());
        }
        if !self.scratch.is_empty() {
            self.queue
                .write_buffer(&self.draws.indices, 0, &self.scratch);
        }
    }

    fn plane_set(&self, view_proj: Mat4) -> [[f32; 4]; 6] {
        if self.culling {
            frustum_planes(view_proj).map(|plane| plane.to_array())
        } else {
            PASS_ALL
        }
    }

    fn upload_batches(
        &mut self,
        batches: &[Batch<'_>],
        camera: [[f32; 4]; 6],
        shadow: Option<[[f32; 4]; 6]>,
        shadow_candidate: bool,
    ) {
        let total: usize = batches.iter().map(|batch| batch.instances.len()).sum();
        let previous_batches = std::mem::take(&mut self.draws.batches);
        self.draws.groups.clear();
        let mut first = 0usize;
        for batch in batches {
            let count = u32::try_from(batch.instances.len()).unwrap_or(u32::MAX);
            self.draws.batches.push(BatchDraw {
                mesh: batch.mesh.0,
                first: u32::try_from(first).unwrap_or(u32::MAX),
                count,
            });
            self.draws.groups.push(count.div_ceil(WORKGROUP_SIZE));
            first += batch.instances.len();
        }
        self.draws.layout_changed = previous_batches != self.draws.batches;
        let reallocated = total > self.draws.capacity;
        if reallocated {
            self.draws.capacity = total.max(1).next_power_of_two();
            self.draws.raw_instances = create_instance_buffer(&self.device, self.draws.capacity);
            self.draws.camera_compact =
                create_compact_buffer(&self.device, self.draws.capacity * MAX_LODS);
            self.draws.camera_compact2 =
                create_compact_buffer(&self.device, self.draws.capacity * MAX_LODS);
            self.draws.shadow_compact =
                create_compact_buffer(&self.device, self.draws.capacity * MAX_LODS);
            self.draws.flags =
                create_storage(&self.device, self.draws.capacity as u64 * 4, "flags");
            self.draws.binds_valid = false;
            self.draws.uploaded.clear();
        }
        let unchanged_versions = versions_unchanged(
            batches.iter().map(|batch| batch.version),
            &self.draws.batch_versions,
        );
        self.draws.batch_versions.clear();
        self.draws
            .batch_versions
            .extend(batches.iter().map(|batch| batch.version));
        let static_scene = unchanged_versions && !self.draws.layout_changed && !reallocated;
        let (upload_bytes, instances_dirty) = if static_scene {
            (0, false)
        } else {
            self.scratch.clear();
            for batch in batches {
                for instance in batch.instances {
                    push_instance(&mut self.scratch, instance);
                }
            }
            sync_instances(
                &self.queue,
                &self.draws.raw_instances,
                &mut self.draws.uploaded,
                &self.scratch,
            )
        };
        self.draws.upload_bytes = upload_bytes;
        self.draws.instances_dirty = instances_dirty;
        let reuse =
            shadow_candidate && !instances_dirty && !self.draws.layout_changed && !reallocated;
        self.draws.shadow_reuse = reuse;
        while self.draws.slots.len() < batches.len() {
            self.draws.slots.push(BatchSlot::new(&self.device));
        }
        let capacity = u32::try_from(self.draws.capacity).unwrap_or(u32::MAX);
        let phase1 = u32::from(self.draws.occlusion_active);
        for (index, batch) in self.draws.batches.iter().enumerate() {
            let slot = &self.draws.slots[index];
            let mesh = &self.draws.meshes[batch.mesh];
            let lod_count = u32::try_from(mesh.lods.len()).unwrap_or(1);
            let radius = mesh.radius;
            let args = lod_args_bytes(mesh.lods.iter().map(|lod| lod.index_count));
            let (first, count) = (batch.first, batch.count);
            self.queue.write_buffer(
                &slot.params[0],
                0,
                &cull_params_bytes(
                    camera, first, count, lod_count, phase1, capacity, radius, 0.0,
                ),
            );
            self.queue.write_buffer(
                &slot.params[1],
                0,
                &cull_params_bytes(camera, first, count, lod_count, 2, capacity, radius, 0.0),
            );
            self.queue.write_buffer(
                &slot.params[2],
                0,
                &cull_params_bytes(
                    shadow.unwrap_or(camera),
                    first,
                    count,
                    lod_count,
                    3,
                    capacity,
                    radius,
                    shadow_pixels_per_unit(),
                ),
            );
            let written = if reuse { 2 } else { 3 };
            for args_buffer in &slot.args[..written] {
                self.queue.write_buffer(args_buffer, 0, &args);
            }
        }
        if !self.draws.binds_valid || self.draws.binds.len() != self.draws.slots.len() {
            self.draws.binds.clear();
            let hzb = &self.draws.hzb.all;
            for slot in &self.draws.slots {
                let phase_one = make_cull_bind(
                    &self.device,
                    &self.draws.cull_layout,
                    &CullBinding {
                        params: &slot.params[0],
                        raw: &self.draws.raw_instances,
                        out: &self.draws.camera_compact,
                        args: &slot.args[0],
                        flags: &self.draws.flags,
                        camera: &self.draws.camera_ubo,
                        hzb,
                    },
                );
                let phase_two = make_cull_bind(
                    &self.device,
                    &self.draws.cull_layout,
                    &CullBinding {
                        params: &slot.params[1],
                        raw: &self.draws.raw_instances,
                        out: &self.draws.camera_compact2,
                        args: &slot.args[1],
                        flags: &self.draws.flags,
                        camera: &self.draws.camera_ubo,
                        hzb,
                    },
                );
                let shadow_bind = make_cull_bind(
                    &self.device,
                    &self.draws.cull_layout,
                    &CullBinding {
                        params: &slot.params[2],
                        raw: &self.draws.raw_instances,
                        out: &self.draws.shadow_compact,
                        args: &slot.args[2],
                        flags: &self.draws.flags,
                        camera: &self.draws.camera_ubo,
                        hzb,
                    },
                );
                self.draws.binds.push([phase_one, phase_two, shadow_bind]);
            }
            self.draws.binds_valid = true;
        }
    }
}

fn push_instance(out: &mut Vec<u8>, instance: &DrawInstance) {
    let m = instance.model.to_cols_array();
    push_floats(
        out,
        &[
            m[0], m[1], m[2], m[12], m[4], m[5], m[6], m[13], m[8], m[9], m[10], m[14],
        ],
    );
    out.extend_from_slice(&pack_unorm8(instance.color));
    out.extend_from_slice(&pack_unorm8([
        instance.material[0],
        instance.material[1],
        0.0,
        0.0,
    ]));
    out.extend_from_slice(&[0u8; 8]);
}

fn pack_unorm8(values: [f32; 4]) -> [u8; 4] {
    values.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn dirty_runs(bytes: &[u8], uploaded: &[u8]) -> Vec<std::ops::Range<usize>> {
    let stride = INSTANCE_BYTES as usize;
    let count = bytes.len() / stride;
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for index in 0..=count {
        let differs = index < count && {
            let range = index * stride..(index + 1) * stride;
            bytes[range.clone()] != uploaded[range]
        };
        match (differs, start) {
            (true, None) => start = Some(index),
            (false, Some(first)) => {
                runs.push(first * stride..index * stride);
                start = None;
            }
            _ => {}
        }
    }
    runs
}

fn sync_instances(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    uploaded: &mut Vec<u8>,
    bytes: &[u8],
) -> (usize, bool) {
    if uploaded.len() != bytes.len() {
        uploaded.clear();
        uploaded.extend_from_slice(bytes);
        if !bytes.is_empty() {
            queue.write_buffer(buffer, 0, bytes);
        }
        return (bytes.len(), !bytes.is_empty());
    }
    let mut written = 0;
    for range in dirty_runs(bytes, uploaded) {
        queue.write_buffer(buffer, range.start as u64, &bytes[range.clone()]);
        uploaded[range.clone()].copy_from_slice(&bytes[range.clone()]);
        written += range.len();
    }
    (written, written > 0)
}

pub(crate) fn versions_unchanged(
    versions: impl ExactSizeIterator<Item = Option<u64>>,
    previous: &[Option<u64>],
) -> bool {
    versions.len() == previous.len()
        && versions
            .zip(previous)
            .all(|(version, last)| version.is_some() && version == *last)
}

#[cfg(test)]
mod versioned_upload_tests {
    use super::versions_unchanged;

    #[test]
    fn a_versioned_scene_is_unchanged_only_when_every_version_matches() {
        assert!(versions_unchanged(
            [Some(1), Some(2)].into_iter(),
            &[Some(1), Some(2)]
        ));
        assert!(!versions_unchanged(
            [Some(1), Some(3)].into_iter(),
            &[Some(1), Some(2)]
        ));
    }

    #[test]
    fn an_unversioned_batch_is_never_treated_as_unchanged() {
        assert!(!versions_unchanged([None].into_iter(), &[None]));
        assert!(!versions_unchanged(
            [Some(1), None].into_iter(),
            &[Some(1), None]
        ));
    }

    #[test]
    fn a_change_in_the_number_of_batches_is_a_change() {
        assert!(!versions_unchanged([Some(1)].into_iter(), &[]));
        assert!(!versions_unchanged([].into_iter(), &[Some(1)]));
        assert!(versions_unchanged([].into_iter(), &[]));
    }
}

fn shadow_pixels_per_unit() -> f32 {
    SHADOW_SIZE as f32 * 0.5 / SHADOW_HALF_EXTENT
}

#[allow(clippy::too_many_arguments)]
fn cull_params_bytes(
    planes: [[f32; 4]; 6],
    first: u32,
    count: u32,
    lod_count: u32,
    phase: u32,
    capacity: u32,
    radius: f32,
    shadow_scale: f32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(CULL_PARAM_BYTES as usize);
    for plane in planes {
        push_floats(&mut out, &plane);
    }
    for value in [first, count, lod_count, phase, capacity] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&radius.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&shadow_scale.to_le_bytes());
    out
}

fn camera_bytes(view: Mat4, projection: Mat4, screen: [f32; 4], lods: [f32; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(CAMERA_BYTES as usize);
    push_floats(&mut out, &view.to_cols_array());
    push_floats(&mut out, &projection.to_cols_array());
    push_floats(&mut out, &screen);
    push_floats(&mut out, &lods);
    out
}

fn lod_args_bytes(index_counts: impl Iterator<Item = u32>) -> Vec<u8> {
    let mut out = Vec::with_capacity(SLOT_ARGS_BYTES as usize);
    let mut counts = index_counts;
    for _ in 0..MAX_LODS {
        let index_count = counts.next().unwrap_or(0);
        for value in [index_count, 0, 0, 0, 0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

fn cull_bind_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let buffer = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cull"),
        entries: &[
            buffer(0, wgpu::BufferBindingType::Uniform),
            buffer(1, wgpu::BufferBindingType::Storage { read_only: true }),
            buffer(2, wgpu::BufferBindingType::Storage { read_only: false }),
            buffer(3, wgpu::BufferBindingType::Storage { read_only: false }),
            buffer(4, wgpu::BufferBindingType::Storage { read_only: false }),
            buffer(5, wgpu::BufferBindingType::Uniform),
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

fn hzb_layout(device: &wgpu::Device, label: &str, depth: bool) -> wgpu::BindGroupLayout {
    let sample_type = if depth {
        wgpu::TextureSampleType::Depth
    } else {
        wgpu::TextureSampleType::Float { filterable: false }
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::R32Float,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    })
}

fn build_compute(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    source: &str,
    entry: &str,
) -> wgpu::ComputePipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some(entry),
        compilation_options: Default::default(),
        cache: None,
    })
}

fn create_hzb(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    depth: &wgpu::TextureView,
    l0_layout: &wgpu::BindGroupLayout,
    down_layout: &wgpu::BindGroupLayout,
) -> Hzb {
    let width = width.max(1);
    let height = height.max(1);
    let levels = 32 - width.max(height).leading_zeros();
    let mut dims = Vec::with_capacity(levels as usize);
    let (mut w, mut h) = (width, height);
    for _ in 0..levels {
        dims.push((w, h));
        w = w.div_ceil(2);
        h = h.div_ceil(2);
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hzb"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });
    let level_view = |mip: u32| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            base_mip_level: mip,
            mip_level_count: Some(1),
            ..Default::default()
        })
    };
    let all = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let l0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("hzb level 0"),
        layout: l0_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&level_view(0)),
            },
        ],
    });
    let down = (1..levels)
        .map(|mip| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hzb down"),
                layout: down_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&level_view(mip - 1)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&level_view(mip)),
                    },
                ],
            })
        })
        .collect();
    Hzb {
        _texture: texture,
        all,
        levels,
        dims,
        l0,
        down,
    }
}

fn build_cull_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cull"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cull"),
        source: wgpu::ShaderSource::Wgsl(CULL_SOURCE.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("cull"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("cull_main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

struct CullBinding<'a> {
    params: &'a wgpu::Buffer,
    raw: &'a wgpu::Buffer,
    out: &'a wgpu::Buffer,
    args: &'a wgpu::Buffer,
    flags: &'a wgpu::Buffer,
    camera: &'a wgpu::Buffer,
    hzb: &'a wgpu::TextureView,
}

fn make_cull_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    binding: &CullBinding<'_>,
) -> wgpu::BindGroup {
    let entries = [
        wgpu::BindGroupEntry {
            binding: 0,
            resource: binding.params.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: binding.raw.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 2,
            resource: binding.out.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 3,
            resource: binding.args.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 4,
            resource: binding.flags.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 5,
            resource: binding.camera.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 6,
            resource: wgpu::BindingResource::TextureView(binding.hzb),
        },
    ];
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cull"),
        layout,
        entries: &entries,
    })
}

fn build_depth_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("depth prepass"),
        source: wgpu::ShaderSource::Wgsl(DEPTH_SOURCE.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("depth prepass"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_BYTES,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &VERTEX_ATTRIBUTES,
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: INSTANCE_BYTES,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &INSTANCE_ATTRIBUTES,
                }),
            ],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: None,
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn classify(
    lights: &[Light],
    storage: &mut Vec<Light>,
    directional: &mut [Light; MAX_DIRECTIONAL],
) -> (usize, usize) {
    storage.clear();
    let mut directional_count = 0;
    let mut dropped = 0;
    for light in lights {
        if let Light::Directional { .. } = light {
            if directional_count < MAX_DIRECTIONAL {
                directional[directional_count] = *light;
                directional_count += 1;
            } else {
                dropped += 1;
            }
        } else if storage.len() < MAX_LIGHTS {
            storage.push(*light);
        } else {
            dropped += 1;
        }
    }
    (directional_count, dropped)
}

#[cfg(test)]
pub(crate) fn cull_append(
    out: &mut Vec<DrawInstance>,
    instances: &[DrawInstance],
    view_proj: Mat4,
    mesh_radius: f32,
) {
    let planes = frustum_planes(view_proj);
    for instance in instances {
        let center = instance.model.w_axis.truncate();
        let scale = instance
            .model
            .x_axis
            .truncate()
            .length()
            .max(instance.model.y_axis.truncate().length())
            .max(instance.model.z_axis.truncate().length());
        let radius = mesh_radius * scale;
        let inside = planes
            .iter()
            .all(|plane| plane.truncate().dot(center) + plane.w >= -radius);
        if inside {
            out.push(*instance);
        }
    }
}

pub(crate) fn frustum_planes(view_proj: Mat4) -> [Vec4; 6] {
    let r0 = view_proj.row(0);
    let r1 = view_proj.row(1);
    let r2 = view_proj.row(2);
    let r3 = view_proj.row(3);
    let raw = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2];
    raw.map(|plane| {
        let length = plane.truncate().length();
        if length > 0.0 { plane / length } else { plane }
    })
}

impl DrawResources {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        depth: &wgpu::TextureView,
    ) -> (Self, ShaderState) {
        let storage = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(1),
                storage(2),
                storage(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cube"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let pipeline = build_pipeline(
            device,
            &pipeline_layout,
            format,
            EMBEDDED_CUBE,
            false,
            wgpu::CompareFunction::LessEqual,
        )
        .expect("the embedded cube shader compiles");
        let direct_text =
            direct_source(EMBEDDED_CUBE).expect("the embedded shader has the cluster block");
        let direct = build_pipeline(
            device,
            &pipeline_layout,
            format,
            &direct_text,
            false,
            wgpu::CompareFunction::LessEqual,
        )
        .expect("the direct cube shader compiles");
        let depth_pipeline = build_depth_pipeline(device, &pipeline_layout);
        let cull_layout = cull_bind_layout(device);
        let cull_pipeline = build_cull_pipeline(device, &cull_layout);

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform"),
            size: FRAME_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lights_store = create_storage(device, MAX_LIGHTS as u64 * LIGHT_BYTES, "lights");
        let indices_capacity = 1024;
        let indices = create_storage(device, indices_capacity as u64 * 4, "light indices");
        let ranges = create_storage(device, CLUSTER_COUNT as u64 * 8, "cluster ranges");
        let shadow_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("shadow map"),
                size: wgpu::Extent3d {
                    width: SHADOW_SIZE,
                    height: SHADOW_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let ao_placeholder = create_ao_texture(device, 1, 1);
        let ao_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ao"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        let ao_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ao"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let ao_blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ao blur"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let ao_pipeline = build_fullscreen_pipeline(device, &ao_layout, AO_SOURCE, "ao");
        let ao_blur_pipeline =
            build_fullscreen_pipeline(device, &ao_blur_layout, BLUR_SOURCE, "ao blur");
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: Some(wgpu::CompareFunction::LessEqual),
            anisotropy_clamp: 1,
            border_color: None,
        });
        let bind_group = make_bind_group(
            device,
            &bind_layout,
            &uniform,
            &lights_store,
            &indices,
            &ranges,
            &shadow_view,
            &shadow_sampler,
            &ao_placeholder,
            &ao_sampler,
        );

        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let shadow_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow uniform"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: shadow_uniform.as_entire_binding(),
            }],
        });
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow"),
                bind_group_layouts: &[Some(&shadow_layout)],
                immediate_size: 0,
            });
        let shadow_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shadow.wgsl").into()),
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: VERTEX_BYTES,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &VERTEX_ATTRIBUTES,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: INSTANCE_BYTES,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &INSTANCE_ATTRIBUTES,
                    }),
                ],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });

        let cube_mesh = upload_mesh(device, &[cube()]);

        let hzb_l0_layout = hzb_layout(device, "hzb level 0", true);
        let hzb_down_layout = hzb_layout(device, "hzb down", false);
        let hzb_l0_pipeline = build_compute(
            device,
            "hzb level 0",
            &hzb_l0_layout,
            HZB_L0_SOURCE,
            "hzb_l0",
        );
        let hzb_down_pipeline = build_compute(
            device,
            "hzb down",
            &hzb_down_layout,
            HZB_DOWN_SOURCE,
            "hzb_down",
        );
        let hzb = create_hzb(
            device,
            width,
            height,
            depth,
            &hzb_l0_layout,
            &hzb_down_layout,
        );
        let camera_ubo = create_uniform(device, CAMERA_BYTES, "camera occlusion");
        let instance_capacity = 64;
        let mut shader = ShaderState::new(pipeline_layout, format);
        shader.remember(shader.active, pipeline.clone());
        let draws = Self {
            shadow_pipeline,
            shadow_bind_group,
            shadow_uniform,
            shadow_view,
            shadow_sampler,
            shadows_enabled: false,
            ambient_occlusion_enabled: true,
            ao_raw: ao_placeholder.clone(),
            ao_blur: ao_placeholder.clone(),
            ao_sampler,
            ao_layout,
            ao_blur_layout,
            ao_pipeline,
            ao_blur_pipeline,
            ao_bind: None,
            ao_blur_bind: None,
            ao_valid: false,
            pipeline,
            direct,
            depth_pipeline,
            use_direct: false,
            bind_layout,
            bind_group,
            uniform,
            lights_store,
            indices,
            indices_capacity,
            ranges,
            meshes: vec![cube_mesh],
            batches: Vec::new(),
            groups: Vec::new(),
            cull_layout,
            cull_pipeline,
            hzb_l0_layout,
            hzb_down_layout,
            hzb_l0_pipeline,
            hzb_down_pipeline,
            hzb,
            camera_ubo,
            raw_instances: create_instance_buffer(device, instance_capacity),
            flags: create_storage(device, instance_capacity as u64 * 4, "flags"),
            camera_compact: create_compact_buffer(device, instance_capacity * MAX_LODS),
            camera_compact2: create_compact_buffer(device, instance_capacity * MAX_LODS),
            shadow_compact: create_compact_buffer(device, instance_capacity * MAX_LODS),
            capacity: instance_capacity,
            slots: Vec::new(),
            binds: Vec::new(),
            binds_valid: false,
            build_hzb: false,
            occlusion_active: false,
            hzb_ready: false,
            uploaded: Vec::new(),
            upload_bytes: 0,
            instances_dirty: false,
            batch_versions: Vec::new(),
            layout_changed: true,
            shadow_reuse: false,
            shadow_valid: false,
            shadow_matrix: [0.0; 16],
        };
        (draws, shader)
    }
}

fn create_storage(device: &wgpu::Device, size: u64, label: &'static str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[allow(clippy::too_many_arguments)]
fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    lights: &wgpu::Buffer,
    indices: &wgpu::Buffer,
    ranges: &wgpu::Buffer,
    shadow_view: &wgpu::TextureView,
    shadow_sampler: &wgpu::Sampler,
    ao_view: &wgpu::TextureView,
    ao_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: lights.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: indices.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: ranges.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(ao_view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(ao_sampler),
            },
        ],
    })
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 12,
        shader_location: 1,
    },
];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 3,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 16,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 32,
        shader_location: 5,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Unorm8x4,
        offset: 48,
        shader_location: 7,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Unorm8x4,
        offset: 52,
        shader_location: 8,
    },
];

pub(crate) fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    source: &str,
    depth_write: bool,
    depth_compare: wgpu::CompareFunction,
) -> Result<wgpu::RenderPipeline, String> {
    let guard = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cube"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cube"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_BYTES,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &VERTEX_ATTRIBUTES,
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: INSTANCE_BYTES,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &INSTANCE_ATTRIBUTES,
                }),
            ],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    match pollster::block_on(guard.pop()) {
        Some(error) => Err(error.to_string()),
        None => Ok(pipeline),
    }
}

pub(crate) fn key_shadow_matrix(direction: Vec3) -> Mat4 {
    let direction = direction.normalize_or_zero();
    let up = if direction.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let eye = -direction * SHADOW_DISTANCE;
    let view = view::look_at_mat4(eye, Vec3::ZERO, up);
    let projection = directx::orthographic(
        -SHADOW_HALF_EXTENT,
        SHADOW_HALF_EXTENT,
        -SHADOW_HALF_EXTENT,
        SHADOW_HALF_EXTENT,
        SHADOW_NEAR,
        SHADOW_FAR,
    );
    projection * view
}

fn shadow_view_proj_bytes(matrix: Mat4) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(64);
    push_floats(&mut bytes, &matrix.to_cols_array());
    bytes
}

fn upload_mesh(device: &wgpu::Device, lods: &[Mesh]) -> GpuMesh {
    let uploaded = lods.iter().map(|mesh| upload_lod(device, mesh)).collect();
    GpuMesh {
        lods: uploaded,
        radius: lods[0].bounding_radius,
    }
}

fn upload_lod(device: &wgpu::Device, mesh: &Mesh) -> GpuLod {
    let mut vertex_bytes = Vec::with_capacity(mesh.vertices.len() * 24);
    for vertex in &mesh.vertices {
        push_floats(&mut vertex_bytes, &vertex.position);
        push_floats(&mut vertex_bytes, &vertex.normal);
    }
    let mut index_bytes = Vec::with_capacity(mesh.indices.len() * 4);
    for index in &mesh.indices {
        index_bytes.extend_from_slice(&index.to_le_bytes());
    }
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh vertices"),
        contents: &vertex_bytes,
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh indices"),
        contents: &index_bytes,
        usage: wgpu::BufferUsages::INDEX,
    });
    let clusters_list = crate::meshlet::build_clusters(mesh, crate::meshlet::CLUSTER_TRIANGLES);
    let clusters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh clusters"),
        contents: &crate::meshlet::cluster_bytes(&clusters_list),
        usage: wgpu::BufferUsages::STORAGE,
    });
    GpuLod {
        vertices,
        indices,
        index_count: u32::try_from(mesh.indices.len()).unwrap_or(u32::MAX),
        clusters,
        cluster_count: u32::try_from(clusters_list.len()).unwrap_or(u32::MAX),
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: capacity as u64 * INSTANCE_BYTES,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_compact_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("compacted instances"),
        size: capacity as u64 * INSTANCE_BYTES,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    })
}

fn create_uniform(device: &wgpu::Device, size: u64, label: &'static str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_args(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("indirect args"),
        size: SLOT_ARGS_BYTES,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::INDIRECT
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn ao_size(target: (u32, u32)) -> (u32, u32) {
    ((target.0 / 2).max(1), (target.1 / 2).max(1))
}

fn create_ao_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("ambient occlusion"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn build_fullscreen_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    source: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::R8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

const AO_SOURCE: &str = concat!(
    include_str!("../shaders/frame.wgsl"),
    include_str!("../shaders/ao.wgsl")
);
const BLUR_SOURCE: &str = include_str!("../shaders/blur.wgsl");

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn push_floats(out: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn instance_at(translation: Vec3, scale: f32) -> DrawInstance {
        DrawInstance::new(
            Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                glam::Quat::IDENTITY,
                translation,
            ),
            [1.0; 4],
        )
    }

    fn cull_into(out: &mut Vec<DrawInstance>, instances: &[DrawInstance], view_proj: Mat4) {
        out.clear();
        cull_append(out, instances, view_proj, 0.866_025_4);
    }

    fn camera_matrix() -> Mat4 {
        crate::camera::Camera::default().view_proj(16.0 / 9.0)
    }

    #[test]
    fn a_cube_at_the_target_is_kept_and_one_behind_the_camera_is_dropped() {
        let instances = [
            instance_at(Vec3::ZERO, 1.0),
            instance_at(Vec3::new(0.0, 0.0, 50.0), 1.0),
        ];
        let mut out = Vec::new();
        cull_into(&mut out, &instances, camera_matrix());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].model.w_axis.truncate(), Vec3::ZERO);
    }

    #[test]
    fn a_cube_far_to_the_side_is_dropped() {
        let instances = [instance_at(Vec3::new(500.0, 0.0, 0.0), 1.0)];
        let mut out = Vec::new();
        cull_into(&mut out, &instances, camera_matrix());
        assert!(out.is_empty());
    }

    #[test]
    fn a_cube_straddling_the_edge_of_the_view_is_kept() {
        let camera = crate::camera::Camera::default();
        let aspect = 16.0 / 9.0;
        let half_width_at_origin_distance = (camera.fov_y_radians / 2.0).tan() * aspect;
        let distance = camera.eye.length();
        let edge_x = half_width_at_origin_distance * distance;
        let instances = [instance_at(Vec3::new(edge_x, 0.0, 0.0), 2.0)];
        let mut out = Vec::new();
        cull_into(&mut out, &instances, camera.view_proj(aspect));
        assert_eq!(out.len(), 1, "a sphere crossing the frustum edge is kept");
    }

    #[test]
    fn the_output_buffer_is_reused_between_frames() {
        let instances = [instance_at(Vec3::ZERO, 1.0); 4];
        let mut out = Vec::with_capacity(16);
        cull_into(&mut out, &instances, camera_matrix());
        let capacity = out.capacity();
        cull_into(&mut out, &instances, camera_matrix());
        assert_eq!(out.capacity(), capacity);
    }

    #[test]
    fn the_light_limits_split_directional_from_storage_and_count_drops() {
        let sun = Light::default();
        let point = Light::accent(Vec3::ZERO, Vec3::ONE, 1.0, 1.0);
        let mut directional = [Light::default(); MAX_DIRECTIONAL];
        let mut storage = Vec::new();
        let (count, dropped) = classify(&[sun, point, point], &mut storage, &mut directional);
        assert_eq!((count, dropped), (1, 0));
        assert_eq!(storage.len(), 2);

        let suns = vec![sun; MAX_DIRECTIONAL + 2];
        let (count, dropped) = classify(&suns, &mut storage, &mut directional);
        assert_eq!((count, dropped), (MAX_DIRECTIONAL, 2));

        let points = vec![point; MAX_LIGHTS + 3];
        let (_, dropped) = classify(&points, &mut storage, &mut directional);
        assert_eq!(storage.len(), MAX_LIGHTS);
        assert_eq!(dropped, 3);
    }

    #[test]
    fn the_frame_uniform_holds_the_matrix_header_and_every_light_slot() {
        assert_eq!(
            FRAME_BYTES,
            (16 + 16 + 4 + 4 + 4 + 16 + 4 + MAX_DIRECTIONAL as u64 * 16 + 16 + 16 + 4 + 4 + 4) * 4
        );
    }

    #[test]
    fn content_hash_is_stable_and_changes_with_the_text() {
        assert_eq!(content_hash("abc"), content_hash("abc"));
        assert_ne!(content_hash("abc"), content_hash("abd"));
        assert_eq!(content_hash(EMBEDDED_CUBE), content_hash(EMBEDDED_CUBE));
    }

    #[test]
    fn a_caster_outside_the_camera_still_casts_when_the_light_reaches_it() {
        let light = key_shadow_matrix(Vec3::Y);
        let caster = [instance_at(Vec3::new(0.0, 30.0, 0.0), 1.0)];
        let mut from_camera = Vec::new();
        cull_append(&mut from_camera, &caster, camera_matrix(), 0.866_025_4);
        assert!(from_camera.is_empty());
        let mut from_light = Vec::new();
        cull_append(&mut from_light, &caster, light, 0.866_025_4);
        assert_eq!(from_light.len(), 1);
    }

    #[test]
    fn the_gpu_records_match_their_shader_layouts() {
        let instance = instance_at(Vec3::ZERO, 1.0);
        let mut bytes = Vec::new();
        push_instance(&mut bytes, &instance);
        assert_eq!(bytes.len() as u64, INSTANCE_BYTES);
        assert_eq!(
            cull_params_bytes([[0.0; 4]; 6], 0, 0, 1, 0, 1, 1.0, 0.0).len() as u64,
            CULL_PARAM_BYTES
        );
        assert_eq!(
            camera_bytes(Mat4::IDENTITY, Mat4::IDENTITY, [1.0; 4], [1.0; 4]).len() as u64,
            CAMERA_BYTES
        );
        assert_eq!(
            lod_args_bytes([36u32].into_iter()).len() as u64,
            ARGS_BYTES * MAX_LODS as u64
        );
    }
}

#[cfg(test)]
mod upload_tests {
    use super::*;

    fn record(value: u8) -> Vec<u8> {
        vec![value; INSTANCE_BYTES as usize]
    }

    fn join(records: &[Vec<u8>]) -> Vec<u8> {
        records.concat()
    }

    #[test]
    fn identical_instances_upload_nothing() {
        let bytes = join(&[record(1), record(2), record(3)]);
        assert!(dirty_runs(&bytes, &bytes).is_empty());
    }

    #[test]
    fn one_changed_instance_is_one_run() {
        let before = join(&[record(1), record(2), record(3)]);
        let after = join(&[record(1), record(9), record(3)]);
        let stride = INSTANCE_BYTES as usize;
        assert_eq!(dirty_runs(&after, &before), vec![stride..2 * stride]);
    }

    #[test]
    fn adjacent_changes_merge_and_separate_ones_split() {
        let stride = INSTANCE_BYTES as usize;
        let before = join(&[record(1), record(2), record(3), record(4)]);
        let after = join(&[record(9), record(9), record(3), record(9)]);
        assert_eq!(
            dirty_runs(&after, &before),
            vec![0..2 * stride, 3 * stride..4 * stride]
        );
    }

    #[test]
    fn a_change_in_the_last_instance_closes_the_run() {
        let stride = INSTANCE_BYTES as usize;
        let before = join(&[record(1), record(2)]);
        let after = join(&[record(1), record(9)]);
        assert_eq!(dirty_runs(&after, &before), vec![stride..2 * stride]);
    }

    #[test]
    fn packed_records_match_the_gpu_layout() {
        let instance = DrawInstance::new(
            Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)),
            [1.0, 0.0, 0.5, 2.0],
        )
        .with_material(0.25, 1.0);
        let mut bytes = Vec::new();
        push_instance(&mut bytes, &instance);
        assert_eq!(bytes.len() as u64, INSTANCE_BYTES);
        let floats: Vec<f32> = bytes[..48]
            .chunks(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert_eq!(floats[3], 1.0);
        assert_eq!(floats[7], 2.0);
        assert_eq!(floats[11], 3.0);
        assert_eq!(&bytes[48..52], &[255, 0, 128, 255]);
        assert_eq!(&bytes[52..54], &[64, 255]);
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn a_bigger_mesh_is_culled_by_its_own_radius() {
        let camera = crate::camera::Camera::default();
        let view_proj = camera.view_proj(16.0 / 9.0);
        let edge = Vec3::new(3.0, 0.0, 0.0);
        let at_edge = [DrawInstance::new(Mat4::from_translation(edge), [1.0; 4])];
        let mut small = Vec::new();
        cull_append(&mut small, &at_edge, view_proj, 0.5);
        let mut large = Vec::new();
        cull_append(&mut large, &at_edge, view_proj, 3.0);
        assert!(
            large.len() >= small.len(),
            "a larger bounding radius keeps at least as much"
        );
        assert_eq!(large.len(), 1, "a large mesh reaching into view is kept");
    }
}

#[cfg(test)]
mod shadow_tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn the_key_shadow_frustum_is_centred_on_the_scene_and_keeps_depth_in_range() {
        let direction = Vec3::new(-0.5, -1.0, -0.4).normalize();
        let matrix = key_shadow_matrix(direction);
        let clip = matrix * Vec4::new(0.0, 0.0, 0.0, 1.0);
        let ndc = clip.truncate() / clip.w;
        assert!(
            ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5,
            "origin is centred: {ndc:?}"
        );
        assert!((0.0..=1.0).contains(&ndc.z), "depth is in range: {ndc:?}");
    }

    #[test]
    fn a_point_on_the_ground_near_the_scene_is_inside_the_shadow_frustum() {
        let matrix = key_shadow_matrix(Vec3::NEG_Y);
        let clip = matrix * Vec4::new(10.0, -1.4, 10.0, 1.0);
        let ndc = clip.truncate() / clip.w;
        assert!(
            ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0,
            "ground point is covered: {ndc:?}"
        );
    }

    #[test]
    fn the_frame_uniform_matches_the_shader_layout() {
        assert_eq!(
            FRAME_BYTES,
            (16 + 16 + 4 + 4 + 4 + 16 + 4 + MAX_DIRECTIONAL as u64 * 16 + 16 + 16 + 4 + 4 + 4) * 4
        );
    }
}

#[cfg(test)]
mod hd_shader_tests {
    use super::*;

    #[test]
    fn the_lit_and_ao_shaders_declare_the_same_frame_layout() {
        let frame = include_str!("../shaders/frame.wgsl");
        assert!(include_str!("../shaders/cube.wgsl").contains(frame));
        assert!(AO_SOURCE.contains(frame));
    }
}
