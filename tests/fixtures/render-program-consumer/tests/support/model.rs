use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{GraphLimits, TextureAlpha, TextureTransfer, WgpuProgramTextureDescriptor};

pub const WIDTH: u32 = 64;
pub const HEIGHT: u32 = 64;
pub const IMAGE_BYTES: u64 = WIDTH as u64 * HEIGHT as u64 * 16;
pub const COUNTS_BYTES: u64 = 16;

#[derive(Debug)]
pub struct SceneColor;
#[derive(Debug)]
pub struct EffectColor;
#[derive(Debug)]
pub struct HistogramColor;
#[derive(Debug)]
pub struct RawCounts;
#[derive(Debug)]
pub struct Frequencies;

pub fn source() -> RenderFrameSourceGeneration {
    RenderFrameSourceGeneration::try_acquire()
        .expect("allocate an independent public source identity")
}

pub fn limits() -> GraphLimits {
    GraphLimits {
        max_resources: 12,
        max_operations: 12,
        max_accesses: 48,
        max_outputs: 8,
        max_texture_dimension_2d: WIDTH,
        max_buffer_bytes: 1024,
        max_modeled_resource_bytes: 2 * 1024 * 1024,
        max_modeled_metadata_bytes: 256 * 1024,
    }
}

pub fn texture_descriptor() -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("public-program-consumer-color"),
            size: extent(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    }
}

pub fn buffer_descriptor() -> wgpu::BufferDescriptor<'static> {
    wgpu::BufferDescriptor {
        label: Some("public-program-consumer-statistics"),
        size: COUNTS_BYTES,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    }
}

pub fn extent() -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    }
}
