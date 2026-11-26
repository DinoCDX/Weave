use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};

use glam::{Mat4, Vec4};
use wgpu::util::DeviceExt;

use crate::backend::Backend;
use crate::camera::{Camera, DirectionalLight, FrameParams};
use crate::graph::{PassContext, RenderGraph, Timing, default_passes};
use crate::mesh::cube_geometry;
use crate::shader::{self, EMBEDDED_CUBE, ShaderError, ShaderState, ShaderUpdate, content_hash};

const FRAME_BYTES: u64 = 112;
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

pub(crate) struct DrawResources {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) uniform: wgpu::Buffer,
    pub(crate) vertices: wgpu::Buffer,
    pub(crate) indices: wgpu::Buffer,
    pub(crate) index_count: u32,
    pub(crate) instances: wgpu::Buffer,
    pub(crate) instance_capacity: usize,
    pub(crate) instance_count: u32,
    pub(crate) unbatched: bool,
}

pub struct Renderer {
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
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|error| RenderError::Device(error.to_string()))?;
        let config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(RenderError::SurfaceUnsupported)?;
        surface.configure(&device, &config);

        let depth = create_depth(&device, config.width, config.height);
        let (draws, shader) = DrawResources::new(&device, config.format);
        let graph = RenderGraph::new(default_passes()).expect("the default passes are valid");

        Ok(Self {
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
        if let Some(pipeline) = self.shader.lookup(hash) {
            self.draws.pipeline = pipeline;
            self.shader.active = hash;
            self.shader.last_error = None;
            return Ok(ShaderUpdate::FromCache);
        }
        match build_pipeline(&self.device, &self.shader.layout, self.shader.format, text) {
            Ok(pipeline) => {
                self.shader.remember(hash, pipeline.clone());
                self.draws.pipeline = pipeline;
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
        light: &DirectionalLight,
        params: &FrameParams,
        instances: &[DrawInstance],
    ) -> Result<FrameStats, RenderError> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(FrameStats {
                    submitted: instances.len(),
                    drawn: 0,
                    cull: Duration::ZERO,
                });
            }
            _ => {
                return Ok(FrameStats {
                    submitted: instances.len(),
                    drawn: 0,
                    cull: Duration::ZERO,
                });
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let aspect = self.config.width as f32 / self.config.height as f32;
        let view_proj = camera.view_proj(aspect);
        self.scratch.clear();
        push_floats(&mut self.scratch, &view_proj.to_cols_array());
        push_floats(&mut self.scratch, &light.direction.extend(0.0).to_array());
        push_floats(
            &mut self.scratch,
            &light.color.extend(light.intensity).to_array(),
        );
        push_floats(
            &mut self.scratch,
            &[params.exposure, params.ambient, 0.0, 0.0],
        );
        self.queue
            .write_buffer(&self.draws.uniform, 0, &self.scratch);

        let cull = if self.culling {
            let start = Instant::now();
            cull_into(&mut self.visible, instances, view_proj);
            start.elapsed()
        } else {
            self.visible.clear();
            self.visible.extend_from_slice(instances);
            Duration::ZERO
        };
        self.upload_instances();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut cx = PassContext {
                encoder: &mut encoder,
                color: &view,
                depth: &self.depth,
                draws: &self.draws,
            };
            self.graph.execute(&mut cx);
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(FrameStats {
            submitted: instances.len(),
            drawn: self.draws.instance_count,
            cull,
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

pub(crate) fn cull_into(out: &mut Vec<DrawInstance>, instances: &[DrawInstance], view_proj: Mat4) {
    out.clear();
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
        let radius = 0.5 * 3.0_f32.sqrt() * scale;
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cube"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = build_pipeline(device, &pipeline_layout, format, EMBEDDED_CUBE)
            .expect("the embedded cube shader compiles");

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform"),
            size: FRAME_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });

        let (vertex_data, index_data) = cube_geometry();
        let mut vertex_bytes = Vec::with_capacity(vertex_data.len() * 24);
        for vertex in &vertex_data {
            push_floats(&mut vertex_bytes, &vertex.position);
            push_floats(&mut vertex_bytes, &vertex.normal);
        }
        let mut index_bytes = Vec::with_capacity(index_data.len() * 2);
        for index in &index_data {
            index_bytes.extend_from_slice(&index.to_le_bytes());
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: &vertex_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube indices"),
            contents: &index_bytes,
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_capacity = 64;
        let mut shader = ShaderState::new(pipeline_layout, format);
        shader.remember(shader.active, pipeline.clone());
        let draws = Self {
            pipeline,
            bind_group,
            uniform,
            vertices,
            indices,
            index_count: u32::try_from(index_data.len()).unwrap_or(u32::MAX),
            instances: create_instance_buffer(device, instance_capacity),
            instance_capacity,
            instance_count: 0,
            unbatched: false,
        };
        (draws, shader)
    }
}

pub(crate) fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    source: &str,
) -> Result<wgpu::RenderPipeline, String> {
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
    fn content_hash_is_stable_and_changes_with_the_text() {
        assert_eq!(content_hash("abc"), content_hash("abc"));
        assert_ne!(content_hash("abc"), content_hash("abd"));
        assert_eq!(content_hash(EMBEDDED_CUBE), content_hash(EMBEDDED_CUBE));
    }
}
