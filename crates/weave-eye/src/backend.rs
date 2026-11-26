use weave_heart::PlatformKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Dx12,
    Vulkan,
    Auto,
}

impl Backend {
    pub fn for_platform(platform: Option<PlatformKey>) -> Self {
        match platform {
            Some(PlatformKey::Windows) => Self::Dx12,
            Some(PlatformKey::Linux) => Self::Vulkan,
            _ => Self::Auto,
        }
    }

    pub(crate) fn wgpu_backends(self) -> wgpu::Backends {
        match self {
            Self::Dx12 => wgpu::Backends::DX12,
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Auto => wgpu::Backends::all(),
        }
    }
}
