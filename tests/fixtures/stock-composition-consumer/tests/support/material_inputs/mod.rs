//! Premultiplied linear palette samples are explicitly unpremultiplied by the Material Shader.
//! The original stock Blend draw returns premultiplied world color over transparent black.

mod capture;
mod expected;
mod feature;
mod policy;

use nara::render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    GraphLimits, TextureAlpha, TextureTransfer, WgpuProgramTextureDescriptor,
    WgpuRenderProgramBuilder,
};
use nara_stock_grade_effect::GradeParameters;

pub use capture::{ACCENT_ID, Fixture, PALETTE_ID, SCALE, SCALE_ID};
pub use expected::Expected;
pub use policy::{Scenario, Source, Wiring, publish};

#[derive(Debug)]
pub struct Palette;
#[derive(Debug)]
pub struct Branch;

pub const PALETTE_A: [[f32; 4]; 4] = [
    [0.25, 0.0, 0.0, 0.5],
    [0.0, 0.5, 0.125, 0.75],
    [0.125, 0.0625, 0.5, 1.0],
    [0.0, 0.0, 0.0, 0.0],
];
pub const PALETTE_B: [[f32; 4]; 4] = [
    [0.0, 0.125, 0.25, 0.5],
    [0.5, 0.0, 0.125, 0.75],
    [0.375, 0.25, 0.0, 1.0],
    [0.0625, 0.125, 0.0625, 0.25],
];
pub const GRADE: GradeParameters = GradeParameters {
    exposure_stops: -0.5,
    saturation: 0.65,
    contrast: 1.1,
};
pub const TOLERANCE: u8 = 3;

pub fn descriptor(format: wgpu::TextureFormat) -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("external-material-2x2-palette"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        },
        transfer: if format.has_srgb_suffix() {
            TextureTransfer::Srgb
        } else {
            TextureTransfer::Linear
        },
        alpha: TextureAlpha::Premultiplied,
    }
}

pub fn graph() -> WgpuRenderProgramBuilder {
    WgpuRenderProgramBuilder::new(
        RenderFrameSourceGeneration::try_acquire().unwrap(),
        1,
        GraphLimits::default(),
    )
    .unwrap()
}
