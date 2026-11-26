use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(crate) const EMBEDDED_CUBE: &str = include_str!("../shaders/cube.wgsl");

pub(crate) const CACHE_LIMIT: usize = 8;

pub fn content_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderUpdate {
    Unchanged,
    Compiled,
    FromCache,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderError {
    Io(String),
    Compile(String),
}

impl fmt::Display for ShaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(text) => write!(f, "could not read the shader file: {text}"),
            Self::Compile(text) => write!(
                f,
                "shader did not compile; keeping the previous one: {text}"
            ),
        }
    }
}

impl std::error::Error for ShaderError {}

pub(crate) struct ShaderState {
    pub(crate) layout: wgpu::PipelineLayout,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) path: Option<PathBuf>,
    pub(crate) stamp: Option<SystemTime>,
    pub(crate) active: u64,
    pub(crate) cache: Vec<(u64, wgpu::RenderPipeline)>,
    pub(crate) last_error: Option<String>,
}

impl ShaderState {
    pub(crate) fn new(layout: wgpu::PipelineLayout, format: wgpu::TextureFormat) -> Self {
        Self {
            layout,
            format,
            path: None,
            stamp: None,
            active: content_hash(EMBEDDED_CUBE),
            cache: Vec::new(),
            last_error: None,
        }
    }

    pub(crate) fn remember(&mut self, hash: u64, pipeline: wgpu::RenderPipeline) {
        if let Some(entry) = self.cache.iter_mut().find(|(h, _)| *h == hash) {
            entry.1 = pipeline;
            return;
        }
        if self.cache.len() >= CACHE_LIMIT
            && let Some(position) = self.cache.iter().position(|(h, _)| *h != self.active)
        {
            self.cache.remove(position);
        }
        self.cache.push((hash, pipeline));
    }

    pub(crate) fn lookup(&self, hash: u64) -> Option<wgpu::RenderPipeline> {
        self.cache
            .iter()
            .find(|(h, _)| *h == hash)
            .map(|(_, pipeline)| pipeline.clone())
    }
}

pub(crate) fn modified_time(path: &Path) -> Result<SystemTime, ShaderError> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map_err(|error| ShaderError::Io(error.to_string()))
}

pub(crate) fn read_text(path: &Path) -> Result<String, ShaderError> {
    std::fs::read_to_string(path).map_err(|error| ShaderError::Io(error.to_string()))
}
