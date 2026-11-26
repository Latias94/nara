use bytemuck::{Pod, Zeroable};
use nara_core::Vec2;
use nara_render::{Extent2d, ExtractedView, WorldCanvasFrame};

use crate::{OverlayFrame, OverlayGrid, OverlayPrimitive, OverlayStroke};

const LINE: u32 = 0;
const RECT: u32 = 1;
const CIRCLE: u32 = 2;
const AA_RADIUS: f32 = 0.5;
const MIN_GRID_SPACING_PX: f64 = 8.0;
const MAX_GRID_LINES_PER_AXIS: usize = 128;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Instance {
    clip_origin: [f32; 2],
    clip_axes: [f32; 4],
    points: [[f32; 4]; 2],
    premultiplied: [f32; 4],
    half_width: f32,
    kind: u32,
}

impl Instance {
    pub(super) const ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
        4 => Float32x4, 5 => Float32, 6 => Uint32
    ];
}

pub(super) struct ProjectedView {
    view: ExtractedView,
    raster_offset: Vec2,
    raster_scale: Vec2,
    pixels_per_clip: Vec2,
    clip_scale: Vec2,
    pub(super) scissor: [u32; 4],
}

impl ProjectedView {
    pub(super) fn new(
        view: &ExtractedView,
        canvas: Option<WorldCanvasFrame>,
        extent: Extent2d,
    ) -> Option<Self> {
        let target_size = Vec2::new(extent.width as f32, extent.height as f32);
        let viewport = view.viewport;
        let raster_min = Vec2::new(viewport.physical_x as f32, viewport.physical_y as f32);
        let raster_size = Vec2::new(
            viewport.physical_width as f32,
            viewport.physical_height as f32,
        );
        let (raster_offset, raster_scale) = canvas.map_or((Vec2::ZERO, Vec2::ONE), |canvas| {
            let mapping = canvas.mapping();
            let content = mapping.content_rect();
            let pixels_per_point = mapping.pixels_per_point();
            let canvas_min = Vec2::from_array(content.min) * pixels_per_point;
            let canvas_max = Vec2::from_array(content.max) * pixels_per_point;
            let canvas_size =
                Vec2::new(canvas.extent().width as f32, canvas.extent().height as f32);
            (canvas_min, (canvas_max - canvas_min) / canvas_size)
        });
        let min = raster_offset + raster_min * raster_scale;
        let size = raster_size * raster_scale;
        let pixel_edge = |edge: Vec2| {
            (edge - Vec2::splat(0.5))
                .ceil()
                .clamp(Vec2::ZERO, target_size)
        };
        let begin = pixel_edge(min);
        let end = pixel_edge(min + size);
        if end.x <= begin.x || end.y <= begin.y {
            return None;
        }
        Some(Self {
            view: *view,
            raster_offset,
            raster_scale,
            pixels_per_clip: size * Vec2::new(0.5, -0.5),
            clip_scale: Vec2::new(2.0, -2.0) / target_size,
            scissor: [
                begin.x as u32,
                begin.y as u32,
                (end.x - begin.x) as u32,
                (end.y - begin.y) as u32,
            ],
        })
    }

    fn project_point(&self, point: Vec2) -> Vec2 {
        self.raster_offset + self.view.world_to_viewport(point) * self.raster_scale
    }

    fn project_vector(&self, vector: Vec2) -> Vec2 {
        self.view.mapping.world_vector_to_clip(vector) * self.pixels_per_clip
    }

    pub(super) fn collect(&self, frame: &OverlayFrame, instances: &mut Vec<Instance>) {
        instances.clear();
        let mut geometry = Geometry {
            projection: self,
            instances,
        };
        if let Some(grid) = frame.grid {
            geometry.grid(grid);
        }
        let project = |point| self.project_point(point);
        for &primitive in &frame.primitives {
            match primitive {
                OverlayPrimitive::Line { from, to, stroke } => {
                    if let Some(paint) = Paint::new(stroke) {
                        geometry.line(project(from), project(to), paint);
                    }
                }
                OverlayPrimitive::Rect { corners, stroke } => {
                    if let Some(paint) = Paint::new(stroke) {
                        geometry.rect(corners.map(project), paint);
                    }
                }
                OverlayPrimitive::Circle {
                    center,
                    radius,
                    stroke,
                } => {
                    if let Some(paint) = Paint::new(stroke)
                        && radius.is_finite()
                        && radius > 0.0
                    {
                        geometry.circle(project(center), radius, paint);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Paint {
    premultiplied: [f32; 4],
    half_width: f32,
}

impl Paint {
    fn new(stroke: OverlayStroke) -> Option<Self> {
        let color = stroke.color;
        if !stroke.width_px.is_finite()
            || stroke.width_px <= 0.0
            || color.a <= 0.0
            || ![color.r, color.g, color.b, color.a]
                .into_iter()
                .all(f32::is_finite)
        {
            return None;
        }
        let alpha = color.a.min(1.0);
        Some(Self {
            premultiplied: [
                color.r.clamp(0.0, 1.0) * alpha,
                color.g.clamp(0.0, 1.0) * alpha,
                color.b.clamp(0.0, 1.0) * alpha,
                alpha,
            ],
            half_width: stroke.width_px * 0.5,
        })
    }

    fn padding(self) -> f32 {
        self.half_width + AA_RADIUS
    }
}

struct Quad {
    origin: Vec2,
    horizontal: Vec2,
    vertical: Vec2,
}

impl Quad {
    fn from_bounds(min: Vec2, max: Vec2) -> Self {
        Self {
            origin: min,
            horizontal: Vec2::new(max.x - min.x, 0.0),
            vertical: Vec2::new(0.0, max.y - min.y),
        }
    }

    fn bounds(&self) -> [Vec2; 2] {
        [
            self.origin + self.horizontal.min(Vec2::ZERO) + self.vertical.min(Vec2::ZERO),
            self.origin + self.horizontal.max(Vec2::ZERO) + self.vertical.max(Vec2::ZERO),
        ]
    }
}

struct Geometry<'a> {
    projection: &'a ProjectedView,
    instances: &'a mut Vec<Instance>,
}

impl Geometry<'_> {
    fn emit(&mut self, quad: Quad, points: [[f32; 4]; 2], paint: Paint, kind: u32) {
        let [x, y, width, height] = self.projection.scissor;
        let min = Vec2::new(x as f32, y as f32);
        let max = Vec2::new((x + width) as f32, (y + height) as f32);
        let [quad_min, quad_max] = quad.bounds();
        if !quad_min.is_finite()
            || !quad_max.is_finite()
            || quad_max.x <= min.x
            || quad_max.y <= min.y
            || quad_min.x >= max.x
            || quad_min.y >= max.y
        {
            return;
        }
        let scale = self.projection.clip_scale;
        self.instances.push(Instance {
            clip_origin: (quad.origin * scale + Vec2::new(-1.0, 1.0)).to_array(),
            clip_axes: pack(quad.horizontal * scale, quad.vertical * scale),
            points,
            premultiplied: paint.premultiplied,
            half_width: paint.half_width,
            kind,
        });
    }

    fn line(&mut self, from: Vec2, to: Vec2, paint: Paint) {
        let delta = to - from;
        let length = delta.x.hypot(delta.y);
        if !length.is_finite() {
            return;
        }
        let tangent = if length == 0.0 {
            Vec2::X
        } else {
            delta / length
        };
        let along = tangent * paint.padding();
        let across = along.perp();
        let quad = Quad {
            origin: from - along - across,
            horizontal: delta + 2.0 * along,
            vertical: 2.0 * across,
        };
        self.emit(quad, [pack(from, to), [0.0; 4]], paint, LINE);
    }

    fn rect(&mut self, corners: [Vec2; 4], paint: Paint) {
        let Some([min, max]) = bounds(corners) else {
            return;
        };
        let padding = Vec2::splat(paint.padding());
        self.emit(
            Quad::from_bounds(min - padding, max + padding),
            [pack(corners[0], corners[1]), pack(corners[2], corners[3])],
            paint,
            RECT,
        );
    }

    fn circle(&mut self, center: Vec2, radius: f32, paint: Paint) {
        let x_axis = self.projection.project_vector(Vec2::new(radius, 0.0));
        let y_axis = self.projection.project_vector(Vec2::new(0.0, radius));
        let determinant = x_axis.perp_dot(y_axis);
        if !determinant.is_finite() || determinant == 0.0 {
            return;
        }
        let inverse_x = Vec2::new(y_axis.y, -y_axis.x) / determinant;
        let inverse_y = Vec2::new(-x_axis.y, x_axis.x) / determinant;
        if !inverse_x.is_finite() || !inverse_y.is_finite() {
            return;
        }
        let padding =
            Vec2::new(x_axis.x.hypot(y_axis.x), x_axis.y.hypot(y_axis.y)) + paint.padding();
        self.emit(
            Quad::from_bounds(center - padding, center + padding),
            [pack(center, inverse_x), pack(inverse_y, Vec2::ZERO)],
            paint,
            CIRCLE,
        );
    }

    fn grid(&mut self, grid: OverlayGrid) {
        let corners = [
            Vec2::NEG_ONE,
            Vec2::new(1.0, -1.0),
            Vec2::ONE,
            Vec2::new(-1.0, 1.0),
        ];
        let mapping = self.projection.view.mapping;
        let Some([min, max]) = bounds(corners.map(|point| mapping.clip_to_world(point))) else {
            return;
        };
        if !grid.origin.is_finite() || !grid.spacing.is_finite() || grid.spacing <= 0.0 {
            return;
        }
        let x = self.projection.project_vector(Vec2::X).as_dvec2();
        let y = self.projection.project_vector(Vec2::Y).as_dvec2();
        let pixel_spacing = x.perp_dot(y).abs() / x.length().max(y.length());
        let span = (max.as_dvec2() - min.as_dvec2()).max_element();
        let spacing = f64::from(grid.spacing);
        let required =
            (MIN_GRID_SPACING_PX / pixel_spacing).max(span / (MAX_GRID_LINES_PER_AXIS - 1) as f64);
        let step = spacing * (required / spacing).max(1.0).log2().ceil().exp2();
        if let Some(paint) = Paint::new(grid.line)
            && step.is_finite()
        {
            for axis in 0..2 {
                let origin = f64::from(grid.origin[axis]);
                let first = ((f64::from(min[axis]) - origin) / step).ceil();
                let last = ((f64::from(max[axis]) - origin) / step).floor();
                let count = (last - first + 1.0).min(MAX_GRID_LINES_PER_AXIS as f64) as usize;
                for offset in 0..count {
                    let index = first + offset as f64;
                    if index != 0.0 {
                        self.grid_line(axis, index.mul_add(step, origin) as f32, [min, max], paint);
                    }
                }
            }
        }
        for (axis, stroke) in [grid.y_axis, grid.x_axis].into_iter().enumerate() {
            let origin = grid.origin[axis];
            if origin >= min[axis]
                && origin <= max[axis]
                && let Some(paint) = Paint::new(stroke)
            {
                self.grid_line(axis, origin, [min, max], paint);
            }
        }
    }

    fn grid_line(&mut self, axis: usize, value: f32, bounds: [Vec2; 2], paint: Paint) {
        let [mut from, mut to] = bounds;
        from[axis] = value;
        to[axis] = value;
        self.line(
            self.projection.project_point(from),
            self.projection.project_point(to),
            paint,
        );
    }
}

fn pack(first: Vec2, second: Vec2) -> [f32; 4] {
    [first.x, first.y, second.x, second.y]
}

fn bounds(points: [Vec2; 4]) -> Option<[Vec2; 2]> {
    let [first, rest @ ..] = points;
    points.into_iter().all(Vec2::is_finite).then(|| {
        [
            rest.into_iter().fold(first, Vec2::min),
            rest.into_iter().fold(first, Vec2::max),
        ]
    })
}

#[cfg(test)]
mod tests;
