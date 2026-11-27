use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};

use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::backend::Backend;
use crate::camera::{Camera, FrameParams};
use crate::cluster::{self, CLUSTER_COUNT, Clip, ClusterData, SLICES, TILES_X, TILES_Y};
use crate::gpu_timer::GpuTimer;
use crate::graph::{PassContext, RenderGraph, Timing, default_passes};
use crate::lights::{Light, MAX_DIRECTIONAL, MAX_LIGHTS};
use crate::mesh::{Mesh, cube};
use crate::shader::{self, EMBEDDED_CUBE, ShaderError, ShaderState, ShaderUpdate, content_hash};

const FRAME_BYTES: u64 = (16 + 16 + 4 + 4 + 4 + 16 + 4 + MAX_DIRECTIONAL as u64 * 16) * 4;
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
const CLUSTER_BLOCK: &str = "    let cluster = cluster_index(input.clip.xy, input.depth);\n    let range = cluster_ranges[cluster];\n    for (var k = 0u; k < range.y; k++) {\n        let light = lights[light_indices[range.x + k]];\n        direct = direct + shade(light, n, input.world);\n    }";
const DIRECT_BLOCK: &str = "    for (var k = 0u; k < u32(frame.viewport.w); k++) {\n        direct = direct + shade(lights[k], n, input.world);\n    }";

pub(crate) fn direct_source(clustered: &str) -> Result<String, String> {
    if !clustered.contains(CLUSTER_BLOCK) {
        return Err("the clustered light loop was not found in the shader".to_owned());
    }
    Ok(clustered.replace(CLUSTER_BLOCK, DIRECT_BLOCK))
}
const VERTEX_BYTES: u64 = 24;
const INSTANCE_BYTES: u64 = 80;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawInstance {
    pub model: Mat4,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawMode {
    Batched,
    Unbatched,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameStats {
    pub submitted: usize,
    pub drawn: u32,
    pub cull: Duration,
    pub lights: usize,
    pub lights_dropped: usize,
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

pub(crate) struct GpuMesh {
    pub(crate) vertices: wgpu::Buffer,
    pub(crate) indices: wgpu::Buffer,
    pub(crate) index_count: u32,
    pub(crate) radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshHandle(usize);

pub const CUBE: MeshHandle = MeshHandle(0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Batch<'a> {
    pub mesh: MeshHandle,
    pub instances: &'a [DrawInstance],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BatchDraw {
    pub(crate) mesh: usize,
    pub(crate) first: u32,
    pub(crate) count: u32,
}

pub(crate) struct DrawResources {
    pub(crate) shadow_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_bind_group: wgpu::BindGroup,
    pub(crate) shadow_uniform: wgpu::Buffer,
    pub(crate) shadow_view: wgpu::TextureView,
    pub(crate) shadow_sampler: wgpu::Sampler,
    pub(crate) shadows_enabled: bool,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) direct: wgpu::RenderPipeline,
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
    pub(crate) instances: wgpu::Buffer,
    pub(crate) instance_capacity: usize,
    pub(crate) instance_count: u32,
    pub(crate) unbatched: bool,
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
    visible: Vec<DrawInstance>,
    culling: bool,
    storage_lights: Vec<Light>,
    clusters: ClusterData,
    light_path: LightPath,
    shadows: bool,
}

impl Renderer {
    pub fn add_mesh(&mut self, mesh: &Mesh) -> MeshHandle {
        self.draws.meshes.push(upload_mesh(&self.device, mesh));
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

        let depth = create_depth(&device, config.width, config.height);
        let (draws, shader) = DrawResources::new(&device, config.format);
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
            visible: Vec::new(),
            culling: false,
            storage_lights: Vec::new(),
            clusters: ClusterData::default(),
            light_path: LightPath::Auto,
            shadows: true,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, width, height);
    }

    pub fn set_draw_mode(&mut self, mode: DrawMode) {
        self.draws.unbatched = mode == DrawMode::Unbatched;
    }

    pub fn set_shadows(&mut self, enabled: bool) {
        self.shadows = enabled;
    }

    pub fn set_light_path(&mut self, path: LightPath) {
        self.light_path = path;
    }

    pub fn set_culling(&mut self, enabled: bool) {
        self.culling = enabled;
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
        match build_pipeline(&self.device, &self.shader.layout, self.shader.format, text) {
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
        let submitted: usize = batches.iter().map(|batch| batch.instances.len()).sum();
        let total_lights = lights.len().min(MAX_LIGHTS + MAX_DIRECTIONAL);
        let dropped_before_upload = lights.len().saturating_sub(total_lights);
        let stats_for_skip = |cull: Duration| FrameStats {
            submitted,
            drawn: 0,
            cull,
            lights: total_lights,
            lights_dropped: dropped_before_upload,
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

        let aspect = self.config.width as f32 / self.config.height as f32;
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
                self.config.width as f32,
                self.config.height as f32,
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
        self.queue
            .write_buffer(&self.draws.uniform, 0, &self.scratch);
        self.upload_lights();

        self.visible.clear();
        self.draws.batches.clear();
        let mut cull = Duration::ZERO;
        for batch in batches {
            let mesh = batch.mesh.0;
            let radius = self.draws.meshes[mesh].radius;
            let first = self.visible.len();
            if self.culling {
                let start = Instant::now();
                cull_append(&mut self.visible, batch.instances, view_proj, radius);
                cull += start.elapsed();
            } else {
                self.visible.extend_from_slice(batch.instances);
            }
            let count = self.visible.len() - first;
            self.draws.batches.push(BatchDraw {
                mesh,
                first: u32::try_from(first).unwrap_or(u32::MAX),
                count: u32::try_from(count).unwrap_or(u32::MAX),
            });
        }
        self.upload_instances();

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
                color: &view,
                depth: &self.depth,
                draws: &self.draws,
                timestamps,
                slot: 0,
            };
            self.graph.execute(&mut cx);
        }
        if timing_frame && let Some(timer) = self.gpu_timer.as_mut() {
            timer.finish_frame(&mut encoder);
        }
        self.queue.submit(Some(encoder.finish()));
        if timing_frame && let Some(timer) = self.gpu_timer.as_mut() {
            timer.map_after_submit();
        }
        self.queue.present(frame);
        Ok(FrameStats {
            submitted,
            drawn: self.draws.instance_count,
            cull,
            lights: light_total,
            lights_dropped: light_dropped,
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

    fn upload_instances(&mut self) {
        let needed = self.visible.len().max(1);
        if needed > self.draws.instance_capacity {
            let capacity = needed.next_power_of_two();
            self.draws.instances = create_instance_buffer(&self.device, capacity);
            self.draws.instance_capacity = capacity;
        }
        self.scratch.clear();
        for instance in &self.visible {
            push_floats(&mut self.scratch, &instance.model.to_cols_array());
            push_floats(&mut self.scratch, &instance.color);
        }
        if !self.scratch.is_empty() {
            self.queue
                .write_buffer(&self.draws.instances, 0, &self.scratch);
        }
        self.draws.instance_count = u32::try_from(self.visible.len()).unwrap_or(u32::MAX);
    }
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
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> (Self, ShaderState) {
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cube"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let pipeline = build_pipeline(device, &pipeline_layout, format, EMBEDDED_CUBE)
            .expect("the embedded cube shader compiles");
        let direct_text =
            direct_source(EMBEDDED_CUBE).expect("the embedded shader has the cluster block");
        let direct = build_pipeline(device, &pipeline_layout, format, &direct_text)
            .expect("the direct cube shader compiles");

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

        let cube_mesh = upload_mesh(device, &cube());

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
            pipeline,
            direct,
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
            instances: create_instance_buffer(device, instance_capacity),
            instance_capacity,
            instance_count: 0,
            unbatched: false,
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
        format: wgpu::VertexFormat::Float32x4,
        offset: 48,
        shader_location: 6,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 64,
        shader_location: 7,
    },
];

pub(crate) fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    source: &str,
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
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
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

fn upload_mesh(device: &wgpu::Device, mesh: &Mesh) -> GpuMesh {
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
    GpuMesh {
        vertices,
        indices,
        index_count: u32::try_from(mesh.indices.len()).unwrap_or(u32::MAX),
        radius: mesh.bounding_radius,
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: capacity as u64 * INSTANCE_BYTES,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
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
        DrawInstance {
            model: Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                glam::Quat::IDENTITY,
                translation,
            ),
            color: [1.0; 4],
        }
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
            (16 + 16 + 4 + 4 + 4 + 16 + 4 + MAX_DIRECTIONAL as u64 * 16) * 4
        );
    }

    #[test]
    fn content_hash_is_stable_and_changes_with_the_text() {
        assert_eq!(content_hash("abc"), content_hash("abc"));
        assert_ne!(content_hash("abc"), content_hash("abd"));
        assert_eq!(content_hash(EMBEDDED_CUBE), content_hash(EMBEDDED_CUBE));
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
        let at_edge = [DrawInstance {
            model: Mat4::from_translation(edge),
            color: [1.0; 4],
        }];
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
            (16 + 16 + 4 + 4 + 4 + 16 + 4 + MAX_DIRECTIONAL as u64 * 16) * 4
        );
    }
}
