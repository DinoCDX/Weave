mod backend;
mod camera;
mod cluster;
mod dynres;
mod gpu_timer;
mod graph;
mod lights;
mod mesh;
mod meshlet;
mod renderer;
mod scene;
mod shader;

pub use backend::Backend;
pub use camera::{Camera, FrameParams};
pub use dynres::DynamicResolution;
pub use graph::{GraphError, Pass, PassContext, RenderGraph, Timing};
pub use lights::{Light, MAX_DIRECTIONAL, MAX_LIGHTS};
pub use mesh::{Mesh, Vertex, cube, ground, lobed_sphere};
pub use renderer::{
    Batch, CUBE, DIRECT_LIGHT_LIMIT, DrawInstance, FrameStats, LightPath, MeshHandle, RenderError,
    Renderer,
};
pub use scene::{Renderable, collect_instances};
pub use shader::{ShaderError, ShaderUpdate, content_hash};
