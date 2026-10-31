//! Finite captured drawing data and an independent double-precision pixel oracle.

use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{GraphLimits, TextureAlpha, TextureTransfer, WgpuProgramTextureDescriptor};

pub const SCENE_WIDTH: u32 = 8;
pub const SCENE_HEIGHT: u32 = 6;
pub const WIDTH: u32 = 32;
pub const HEIGHT: u32 = 24;
pub const SCENE_BYTES: u64 = SCENE_WIDTH as u64 * SCENE_HEIGHT as u64 * 16;
pub const FINAL_BYTES: u64 = WIDTH as u64 * HEIGHT as u64 * 16;
pub const DRAWING_UNIFORM_BYTES: u64 = 12 * 16;
pub const SMALL_UNIFORM_BYTES: u64 = 16;
pub const TOLERANCE: f64 = 1.0e-6;

pub struct SceneColor;
pub struct DrawingColor;
pub struct EffectColor;

/// Logical coordinates use half-open rectangles; physical pixel centers are mapped by DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub dpi: f32,
    pub physical_origin: [f32; 2],
    pub scene: [f32; 4],
    pub scene_clip: [f32; 4],
    pub background: [f32; 4],
    pub control: [f32; 4],
    pub control_clip: [f32; 4],
    pub control_color: [f32; 4],
    pub transparent: [f32; 4],
    pub transparent_clip: [f32; 4],
    pub transparent_color: [f32; 4],
    pub popup: [f32; 4],
    pub popup_clip: [f32; 4],
    pub popup_color: [f32; 4],
}

impl Placement {
    pub fn first() -> Self {
        Self {
            dpi: 1.25,
            physical_origin: [0.75, 1.25],
            scene: [1.25, 1.0, 18.0, 16.0],
            scene_clip: [2.5, 3.0, 14.0, 10.5],
            background: [0.03125, 0.0625, 0.09375, 0.25],
            control: [8.0, 5.0, 13.0, 9.0],
            control_clip: [6.5, 4.0, 12.25, 10.0],
            control_color: [0.3125, 0.0625, 0.1875, 0.5],
            transparent: [0.0, 0.0, 25.0, 18.0],
            transparent_clip: [0.0, 0.0, 25.0, 18.0],
            transparent_color: [0.0; 4],
            popup: [10.25, 7.25, 8.5, 5.5],
            popup_clip: [9.5, 6.5, 8.75, 5.25],
            popup_color: [0.09375, 0.375, 0.1875, 0.75],
        }
    }

    pub fn moved() -> Self {
        Self {
            dpi: 1.5,
            physical_origin: [0.5, 1.0],
            scene: [2.0, 1.0, 15.0, 13.0],
            scene_clip: [4.0, 2.5, 10.0, 9.0],
            control: [7.0, 4.0, 11.5, 8.5],
            control_clip: [9.0, 5.0, 7.0, 7.0],
            popup: [8.5, 6.0, 7.0, 5.5],
            popup_clip: [10.0, 7.0, 4.0, 4.0],
            ..Self::first()
        }
    }

    pub fn valid(&self) -> bool {
        let rects = [
            self.scene,
            self.scene_clip,
            self.control,
            self.control_clip,
            self.transparent,
            self.transparent_clip,
            self.popup,
            self.popup_clip,
        ];
        let colors = [
            self.background,
            self.control_color,
            self.transparent_color,
            self.popup_color,
        ];
        self.dpi.is_finite()
            && (0.5..=4.0).contains(&self.dpi)
            && self.transparent_color == [0.0; 4]
            && self
                .physical_origin
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= 64.0)
            && rects.into_iter().all(|r| {
                r.into_iter().all(|v| v.is_finite() && v.abs() <= 64.0) && r[2] > 0.0 && r[3] > 0.0
            })
            && colors.into_iter().all(|c| {
                c.into_iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
                    && c[..3].iter().all(|v| *v <= c[3])
            })
    }

    pub fn uniform_bytes(&self) -> Vec<u8> {
        [
            [
                self.dpi,
                self.physical_origin[0],
                self.physical_origin[1],
                0.0,
            ],
            self.scene,
            self.scene_clip,
            self.background,
            self.control,
            self.control_clip,
            self.control_color,
            self.transparent,
            self.transparent_clip,
            // The alpha-zero layer is fixed in this bounded tracer, so it needs no uniform.
            self.popup,
            self.popup_clip,
            self.popup_color,
        ]
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect()
    }
}

pub fn source() -> RenderFrameSourceGeneration {
    RenderFrameSourceGeneration::try_acquire().expect("independent public source generation")
}

pub fn limits() -> GraphLimits {
    GraphLimits {
        max_resources: 3,
        max_operations: 2,
        max_accesses: 4,
        max_outputs: 2,
        max_texture_dimension_2d: WIDTH,
        max_buffer_bytes: DRAWING_UNIFORM_BYTES,
        max_modeled_resource_bytes: SCENE_BYTES + 2 * FINAL_BYTES,
        max_modeled_metadata_bytes: 1024 * 1024,
    }
}

pub fn descriptor(width: u32, height: u32) -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("finite-drawing-linear-premultiplied"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        },
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    }
}

/// Evaluated independently of GPU bytes, shader outputs, and the GPU uniform serializer.
pub fn scene_pixel(x: u32, y: u32, phase: u32) -> [f64; 4] {
    let alpha = [0.0, 0.25, 0.5, 1.0][((x + 2 * y + phase) % 4) as usize];
    [
        alpha * f64::from((x + phase) % 8) / 8.0,
        alpha * f64::from((2 * y + phase) % 8) / 8.0,
        alpha * f64::from((x + 3 * y + phase) % 8) / 8.0,
        alpha,
    ]
}

fn contains(rect: [f32; 4], point: [f64; 2]) -> bool {
    let [x, y, width, height] = rect.map(f64::from);
    point[0] >= x && point[0] < x + width && point[1] >= y && point[1] < y + height
}

fn over(front: [f64; 4], back: [f64; 4]) -> [f64; 4] {
    std::array::from_fn(|c| front[c] + back[c] * (1.0 - front[3]))
}

#[derive(Default, Debug)]
pub struct Coverage {
    pub scene_pixels: usize,
    pub clipped_scene_pixels: usize,
    pub letterbox_pixels: usize,
    pub scene_alpha_zero: usize,
    pub scene_alpha_fractional: usize,
    pub control_over_scene: usize,
    pub transparent_over_scene_or_control: usize,
    pub popup_over_control: usize,
    pub clipped_control_pixels: usize,
    pub clipped_popup_pixels: usize,
    pub background_pixels: usize,
    pub texels: std::collections::BTreeSet<(u32, u32)>,
}

pub fn drawing(phase: u32, placement: Placement) -> (Vec<[f64; 4]>, Coverage) {
    assert!(placement.valid());
    let mut coverage = Coverage::default();
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT) as usize);
    // Aspect fit is solved in f64 from captured logical geometry, before any pixel is visited.
    let box_width = f64::from(placement.scene[2]);
    let box_height = f64::from(placement.scene[3]);
    let scale = (box_width / f64::from(SCENE_WIDTH)).min(box_height / f64::from(SCENE_HEIGHT));
    let image_origin = [
        f64::from(placement.scene[0]) + (box_width - scale * f64::from(SCENE_WIDTH)) / 2.0,
        f64::from(placement.scene[1]) + (box_height - scale * f64::from(SCENE_HEIGHT)) / 2.0,
    ];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let point = [
                (f64::from(x) + 0.5 - f64::from(placement.physical_origin[0]))
                    / f64::from(placement.dpi),
                (f64::from(y) + 0.5 - f64::from(placement.physical_origin[1]))
                    / f64::from(placement.dpi),
            ];
            let texel = [
                (point[0] - image_origin[0]) / scale,
                (point[1] - image_origin[1]) / scale,
            ];
            let inside_image = texel[0] >= 0.0
                && texel[0] < f64::from(SCENE_WIDTH)
                && texel[1] >= 0.0
                && texel[1] < f64::from(SCENE_HEIGHT);
            let in_scene_clip = contains(placement.scene_clip, point);
            let scene_visible = inside_image && in_scene_clip;
            let mut pixel = placement.background.map(f64::from);
            if scene_visible {
                let sample = (texel[0].floor() as u32, texel[1].floor() as u32);
                let source = scene_pixel(sample.0, sample.1, phase);
                pixel = over(source, pixel);
                coverage.scene_pixels += 1;
                coverage.texels.insert(sample);
                coverage.scene_alpha_zero += usize::from(source[3] == 0.0);
                coverage.scene_alpha_fractional += usize::from(source[3] > 0.0 && source[3] < 1.0);
            }
            coverage.clipped_scene_pixels += usize::from(inside_image && !in_scene_clip);
            coverage.letterbox_pixels +=
                usize::from(contains(placement.scene, point) && !inside_image);
            let control =
                contains(placement.control, point) && contains(placement.control_clip, point);
            coverage.clipped_control_pixels +=
                usize::from(contains(placement.control, point) && !control);
            if control {
                pixel = over(placement.control_color.map(f64::from), pixel);
                coverage.control_over_scene += usize::from(scene_visible);
            }
            if contains(placement.transparent, point) && contains(placement.transparent_clip, point)
            {
                pixel = over(placement.transparent_color.map(f64::from), pixel);
                coverage.transparent_over_scene_or_control += usize::from(scene_visible || control);
            }
            let popup = contains(placement.popup, point) && contains(placement.popup_clip, point);
            coverage.clipped_popup_pixels +=
                usize::from(contains(placement.popup, point) && !popup);
            if popup {
                pixel = over(placement.popup_color.map(f64::from), pixel);
                coverage.popup_over_control += usize::from(control);
            }
            coverage.background_pixels += usize::from(!scene_visible && !control && !popup);
            pixels.push(pixel);
        }
    }
    (pixels, coverage)
}

pub fn assert_coverage(coverage: &Coverage) {
    assert!(coverage.scene_pixels > 0, "{coverage:?}");
    assert!(coverage.clipped_scene_pixels > 0, "{coverage:?}");
    assert!(coverage.letterbox_pixels > 0, "{coverage:?}");
    assert!(coverage.scene_alpha_zero > 0, "{coverage:?}");
    assert!(coverage.scene_alpha_fractional > 0, "{coverage:?}");
    assert!(coverage.control_over_scene > 0, "{coverage:?}");
    assert!(
        coverage.transparent_over_scene_or_control > 0,
        "{coverage:?}"
    );
    assert!(coverage.popup_over_control > 0, "{coverage:?}");
    assert!(coverage.clipped_control_pixels > 0, "{coverage:?}");
    assert!(coverage.clipped_popup_pixels > 0, "{coverage:?}");
    assert!(coverage.background_pixels > 0, "{coverage:?}");
    assert!(
        coverage.texels.len() >= 12,
        "multiple distinguishable sampled texels: {coverage:?}"
    );
}

pub fn assert_pixels(actual: &[[f32; 4]], expected: &[[f64; 4]], width: u32) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..4 {
            let difference = (f64::from(actual[channel]) - expected[channel]).abs();
            assert!(
                actual[channel].is_finite() && difference <= TOLERANCE,
                "pixel ({}, {}), channel {channel}: actual={}, expected={}, error={difference}, tolerance={TOLERANCE}",
                index % width as usize,
                index / width as usize,
                actual[channel],
                expected[channel]
            );
        }
    }
}
