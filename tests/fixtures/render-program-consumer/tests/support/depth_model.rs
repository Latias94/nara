use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    GraphLimits, TextureAlpha, TextureInterpretation, TextureTransfer,
    WgpuProgramTextureDescriptor, WgpuRenderProgramError,
};

pub const WIDTH: u32 = 64;
pub const HEIGHT: u32 = 48;
pub const COLOR_BYTES: u64 = WIDTH as u64 * HEIGHT as u64 * 16;
pub const DEPTH_BYTES: u64 = WIDTH as u64 * HEIGHT as u64 * 4;
pub const GRAPH_BYTES: u64 = 3 * COLOR_BYTES + 2 * DEPTH_BYTES;
pub const VERTEX_BYTES: u64 = 12 * 7 * 4;
pub const UNIFORM_BYTES: u64 = 24 * 4;
pub const INPUT_BYTES: u64 = VERTEX_BYTES + UNIFORM_BYTES;
pub const TOLERANCE: f64 = 1.0e-6;
pub const RASTER_SCISSOR: [u32; 4] = [8, 6, 48, 36];
pub const PRESERVE_SCISSOR: [u32; 4] = [28, 20, 8, 8];

#[derive(Debug)]
pub struct Color;
#[derive(Debug)]
pub struct Depth;
#[derive(Debug)]
pub struct ResolvedDepth;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawOrder {
    NearFirst,
    FarFirst,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub eye_m: [f64; 3],
    pub near_m: f64,
    pub far_m: f64,
    pub tan_half_fov_y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    pub positions: [[f32; 3]; 4],
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub camera: Camera,
    pub meters_per_unit: f64,
    pub quads: [Quad; 2],
    pub order: DrawOrder,
}

impl Scene {
    pub fn meters(order: DrawOrder) -> Self {
        Self {
            camera: Camera {
                eye_m: [0.0; 3],
                near_m: 1.0,
                far_m: 9.0,
                tan_half_fov_y: 1.0,
            },
            meters_per_unit: 1.0,
            quads: [
                Quad {
                    positions: [
                        [-0.75, -0.5, 2.0],
                        [0.75, -0.5, 2.0],
                        [0.75, 0.5, 2.0],
                        [-0.75, 0.5, 2.0],
                    ],
                    color: [1.0, 0.0, 0.0, 1.0],
                },
                Quad {
                    positions: [
                        [-3.0, -3.0, 6.0],
                        [3.0, -3.0, 6.0],
                        [3.0, 3.0, 6.0],
                        [-3.0, 3.0, 6.0],
                    ],
                    color: [0.0, 0.0, 1.0, 1.0],
                },
            ],
            order,
        }
    }

    pub fn centimeters(order: DrawOrder) -> Self {
        let mut scene = Self::meters(order);
        scene.meters_per_unit = 0.01;
        for quad in &mut scene.quads {
            for position in &mut quad.positions {
                for value in position {
                    *value *= 100.0;
                }
            }
        }
        scene
    }

    pub fn preserved(&self) -> Self {
        let mut scene = self.clone();
        for position in &mut scene.quads[0].positions {
            position[2] = (1.5 / self.meters_per_unit) as f32;
        }
        scene.quads[0].color = [0.0, 1.0, 0.0, 1.0];
        scene
    }

    pub fn validate(&self) -> Result<(), WgpuRenderProgramError> {
        let finite_positive = |value: f64| {
            value.is_finite() && value > 0.0 && (value as f32).is_finite() && value as f32 > 0.0
        };
        if !finite_positive(self.meters_per_unit) {
            return Err(WgpuRenderProgramError::feature(
                "meters_per_unit must be finite, positive and representable",
            ));
        }
        let camera = self.camera;
        if !camera
            .eye_m
            .iter()
            .all(|value| value.is_finite() && (*value as f32).is_finite())
            || !finite_positive(camera.near_m)
            || !finite_positive(camera.far_m)
            || camera.far_m <= camera.near_m
            || (camera.far_m as f32) <= camera.near_m as f32
            || !finite_positive(camera.tan_half_fov_y)
        {
            return Err(WgpuRenderProgramError::feature(
                "camera requires a finite eye, positive FOV and 0 < near < far",
            ));
        }
        let a = camera.far_m / (camera.far_m - camera.near_m);
        let b = -camera.near_m * a;
        let sx = f64::from(HEIGHT) / (f64::from(WIDTH) * camera.tan_half_fov_y);
        let sy = 1.0 / camera.tan_half_fov_y;
        if ![a, b, sx, sy]
            .iter()
            .all(|value| value.is_finite() && (*value as f32).is_finite())
        {
            return Err(WgpuRenderProgramError::feature(
                "camera projection is not representable",
            ));
        }
        for quad in &self.quads {
            if !quad.positions.iter().flatten().all(|value| {
                value.is_finite() && ((f64::from(*value) * self.meters_per_unit) as f32).is_finite()
            }) || !quad.color.iter().all(|value| value.is_finite())
            {
                return Err(WgpuRenderProgramError::feature("geometry must be finite"));
            }
            let [a, b, c, d] = quad.positions;
            if !(a[0] < b[0] && a[1] < d[1])
                || a[1] != b[1]
                || b[0] != c[0]
                || c[1] != d[1]
                || d[0] != a[0]
                || [b[2], c[2], d[2]].iter().any(|z| *z != a[2])
            {
                return Err(WgpuRenderProgramError::feature(
                    "expected two axis-aligned planar quads",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct Reference {
    pub color: Vec<[f64; 4]>,
    pub depth: Vec<f64>,
}

pub fn reference(scene: &Scene) -> Reference {
    scene.validate().expect("valid independent camera geometry");
    let camera = scene.camera;
    let aspect = f64::from(WIDTH) / f64::from(HEIGHT);
    let mut result = Reference {
        color: Vec::with_capacity((WIDTH * HEIGHT) as usize),
        depth: Vec::with_capacity((WIDTH * HEIGHT) as usize),
    };
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let ray = [
                (2.0 * (f64::from(x) + 0.5) / f64::from(WIDTH) - 1.0)
                    * aspect
                    * camera.tan_half_fov_y,
                (1.0 - 2.0 * (f64::from(y) + 0.5) / f64::from(HEIGHT)) * camera.tan_half_fov_y,
            ];
            let mut closest = f64::INFINITY;
            let mut color = [0.0; 4];
            for quad in &scene.quads {
                let distance =
                    f64::from(quad.positions[0][2]) * scene.meters_per_unit - camera.eye_m[2];
                if distance < camera.near_m || distance > camera.far_m {
                    continue;
                }
                let bounds = [
                    f64::from(quad.positions[0][0]) * scene.meters_per_unit,
                    f64::from(quad.positions[1][0]) * scene.meters_per_unit,
                    f64::from(quad.positions[0][1]) * scene.meters_per_unit,
                    f64::from(quad.positions[3][1]) * scene.meters_per_unit,
                ];
                for (axis, lower, upper, pixels, scale) in [
                    (
                        0,
                        bounds[0],
                        bounds[1],
                        WIDTH,
                        aspect * camera.tan_half_fov_y,
                    ),
                    (1, bounds[2], bounds[3], HEIGHT, camera.tan_half_fov_y),
                ] {
                    let center = if axis == 0 {
                        f64::from(x) + 0.5
                    } else {
                        f64::from(y) + 0.5
                    };
                    for edge in [lower, upper] {
                        let ndc = (edge - camera.eye_m[axis]) / (distance * scale);
                        let projected =
                            if axis == 0 { ndc + 1.0 } else { 1.0 - ndc } * 0.5 * f64::from(pixels);
                        assert!((center - projected).abs() >= 0.25, "ambiguous pixel edge");
                    }
                }
                let hit_x = camera.eye_m[0] + ray[0] * distance;
                let hit_y = camera.eye_m[1] + ray[1] * distance;
                if hit_x > bounds[0]
                    && hit_x < bounds[1]
                    && hit_y > bounds[2]
                    && hit_y < bounds[3]
                    && distance < closest
                {
                    closest = distance;
                    color = quad.color.map(f64::from);
                }
            }
            let depth = if closest.is_finite() {
                (1.0 / camera.near_m - 1.0 / closest) / (1.0 / camera.near_m - 1.0 / camera.far_m)
            } else {
                1.0
            };
            assert!(depth.is_finite() && (0.0..=1.0).contains(&depth));
            result.color.push(color);
            result.depth.push(depth);
        }
    }
    result
}

pub fn preserved_reference(scene: &Scene) -> Reference {
    let mut result = reference(scene);
    let replacement = reference(&scene.preserved());
    let [left, top, width, height] = PRESERVE_SCISSOR;
    for y in top..top + height {
        for x in left..left + width {
            let pixel = (y * WIDTH + x) as usize;
            result.color[pixel] = replacement.color[pixel];
            result.depth[pixel] = replacement.depth[pixel];
        }
    }
    result
}

pub fn source() -> RenderFrameSourceGeneration {
    RenderFrameSourceGeneration::try_acquire().expect("independent depth consumer source identity")
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

pub fn extent() -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    }
}

pub fn color_descriptor() -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("independent-depth-consumer-color"),
            size: extent(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        interpretation: TextureInterpretation::Color {
            transfer: TextureTransfer::Linear,
            alpha: TextureAlpha::Premultiplied,
        },
    }
}

pub fn depth_descriptor() -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("independent-depth-consumer-depth"),
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            ..color_descriptor().native
        },
        interpretation: TextureInterpretation::Depth,
    }
}
