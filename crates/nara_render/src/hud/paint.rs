use std::{collections::HashMap, ops::Range};

use nara_asset::image::{ImageAsset, ImageContentHash};
use nara_ui::kernel::{
    UiEnvironment, UiImageId, UiMesh, UiPaintPrimitive, UiPhysicalRect, UiRect, UiResolvedBrush,
};
use thiserror::Error;

use crate::Extent2d;

use super::{
    HudFrame,
    atlas::{GlyphAtlas, GlyphError},
};

#[cfg(test)]
mod geometry_tests;
#[cfg(test)]
mod typography_tests;

const MAX_VERTICES: usize = 65_536 + 16_384 * 8;
const MAX_INDICES: usize = 196_608 + 16_384 * 12;
pub(crate) const MAX_IMAGES: usize = 256;

#[derive(Debug, Error)]
pub(crate) enum PaintError {
    #[error(transparent)]
    Glyph(#[from] GlyphError),
    #[error("HUD glyph cannot be placed in physical coordinates")]
    GlyphPosition,
    #[error("HUD image {0} has no captured asset")]
    MissingImage(UiImageId),
    #[error("HUD references too many images")]
    ImageLimit,
    #[error("HUD geometry exceeds its vertex or index limit")]
    GeometryLimit,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
#[cfg_attr(feature = "wgpu", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) local: [f32; 2],
    pub(crate) half_size: [f32; 2],
    pub(crate) radii: [f32; 4],
    pub(crate) border: f32,
    pub(crate) mode: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Texture {
    White,
    Glyph(usize),
    Image(ImageContentHash),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Scissor(pub(crate) [u32; 4]);

impl Scissor {
    fn new(rect: UiPhysicalRect, extent: Extent2d) -> Option<Self> {
        let clamp = |edge: i32, limit| (edge.max(0) as u32).min(limit);
        let left = clamp(rect.min_x(), extent.width);
        let top = clamp(rect.min_y(), extent.height);
        let right = clamp(rect.max_x(), extent.width);
        let bottom = clamp(rect.max_y(), extent.height);
        (left < right && top < bottom).then_some(Self([left, top, right, bottom]))
    }

    fn intersects(self, [min_x, min_y, max_x, max_y]: [i32; 4]) -> bool {
        let [left, top, right, bottom] = self.0.map(i64::from);
        min_x < max_x
            && min_y < max_y
            && i64::from(min_x) < right
            && i64::from(min_y) < bottom
            && i64::from(max_x) > left
            && i64::from(max_y) > top
    }
}

pub(crate) struct Draw {
    pub(crate) indices: Range<u32>,
    pub(crate) texture: Texture,
    pub(crate) clip: Scissor,
}

#[derive(Default)]
pub(crate) struct Geometry {
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) draws: Vec<Draw>,
    pub(crate) images: HashMap<ImageContentHash, ImageAsset>,
    pub(crate) glyph_count: u64,
}

impl Geometry {
    pub(crate) fn prepare(
        &mut self,
        hud: &HudFrame,
        extent: Extent2d,
        atlas: &mut GlyphAtlas,
    ) -> Result<(), PaintError> {
        self.vertices.clear();
        self.indices.clear();
        self.draws.clear();
        self.images.clear();
        self.glyph_count = 0;
        let frame = hud.frame();
        let environment = frame.environment();
        let transform = environment.content_transform();
        let place = Projection {
            extent,
            origin: [
                transform.physical_content_origin().x(),
                transform.physical_content_origin().y(),
            ],
            scale: transform.content_scale_factor(),
        };

        for primitive in frame.paint() {
            let Some(clip) = scissor(environment, primitive.clip_rect(), extent) else {
                continue;
            };
            match primitive {
                UiPaintPrimitive::Rect {
                    physical_rect,
                    brush,
                    border,
                    radii,
                    ..
                } => {
                    let bounds = [
                        physical_rect.min_x(),
                        physical_rect.min_y(),
                        physical_rect.max_x(),
                        physical_rect.max_y(),
                    ];
                    if !clip.intersects(bounds) {
                        continue;
                    }
                    let half_limit = physical_rect.width().min(physical_rect.height()) as f32 * 0.5;
                    let radii = radii
                        .values()
                        .map(|radius| (radius * place.scale).min(half_limit));
                    if let Some(brush) = brush {
                        let (texture, color) = match brush {
                            UiResolvedBrush::Solid(color) => (Texture::White, color.components()),
                            UiResolvedBrush::Image { image, tint } => {
                                (self.image(hud, image)?, tint.components())
                            }
                        };
                        self.quad(
                            place,
                            clip,
                            Quad {
                                bounds,
                                color,
                                texture,
                                radii,
                                ..Quad::default()
                            },
                        )?;
                    }
                    if let Some(border) = border.filter(|border| border.width() > 0.0) {
                        self.quad(
                            place,
                            clip,
                            Quad {
                                bounds,
                                color: border.color().components(),
                                border: (border.width() * place.scale).min(half_limit),
                                radii,
                                ..Quad::default()
                            },
                        )?;
                    }
                }
                UiPaintPrimitive::Mesh { rect, mesh, .. } => self.mesh(place, clip, *rect, mesh)?,
                UiPaintPrimitive::Text {
                    rect,
                    color,
                    layout,
                    ..
                } => {
                    let origin = [rect.origin().x(), rect.origin().y()];
                    for glyph in layout.glyphs() {
                        let placement = glyph
                            .physical_placement(origin, place.origin, place.scale)
                            .ok_or(PaintError::GlyphPosition)?;
                        let Some(slot) = atlas.glyph(placement.key())? else {
                            continue;
                        };
                        let [left, top, right, bottom] = slot.bounds;
                        let bounds = placement
                            .raster_bounds(slot.left, slot.top, right - left, bottom - top)
                            .ok_or(PaintError::GlyphPosition)?;
                        if clip.intersects(bounds) {
                            self.quad(
                                place,
                                clip,
                                Quad {
                                    bounds,
                                    uv: slot.uv(),
                                    color: color.components(),
                                    texture: Texture::Glyph(slot.page),
                                    ..Quad::default()
                                },
                            )?;
                            self.glyph_count += 1;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn image(&mut self, hud: &HudFrame, id: &UiImageId) -> Result<Texture, PaintError> {
        let image = hud
            .image(id)
            .ok_or_else(|| PaintError::MissingImage(id.clone()))?;
        let hash = image.content_hash();
        if !self.images.contains_key(&hash) {
            if self.images.len() == MAX_IMAGES {
                return Err(PaintError::ImageLimit);
            }
            self.images.insert(hash, image.clone());
        }
        Ok(Texture::Image(hash))
    }

    fn quad(&mut self, place: Projection, clip: Scissor, quad: Quad) -> Result<(), PaintError> {
        self.reserve(4, 6)?;
        let [left, top, right, bottom] = quad.bounds.map(|value| value as f32);
        let [min_u, min_v, max_u, max_v] = quad.uv;
        let half_size = [(right - left) * 0.5, (bottom - top) * 0.5];
        let [half_width, half_height] = half_size;
        let mode = if matches!(quad.texture, Texture::Glyph(_)) {
            1
        } else if quad.border > 0.0 || quad.radii != [0.0; 4] {
            2
        } else {
            0
        };
        let base = self.vertices.len() as u32;
        let start = self.indices.len() as u32;
        for (position, uv, local) in [
            ([left, top], [min_u, min_v], [-half_width, -half_height]),
            ([right, top], [max_u, min_v], [half_width, -half_height]),
            ([right, bottom], [max_u, max_v], [half_width, half_height]),
            ([left, bottom], [min_u, max_v], [-half_width, half_height]),
        ] {
            self.vertices.push(Vertex {
                position: place.clip(position),
                uv,
                color: quad.color,
                local,
                half_size,
                radii: quad.radii,
                border: quad.border,
                mode,
            });
        }
        self.indices
            .extend([0, 1, 2, 0, 2, 3].map(|index| index + base));
        self.draw(quad.texture, clip, start);
        Ok(())
    }

    fn mesh(
        &mut self,
        place: Projection,
        clip: Scissor,
        rect: UiRect,
        mesh: &UiMesh,
    ) -> Result<(), PaintError> {
        self.reserve(mesh.vertices().len(), mesh.indices().len())?;
        let base = self.vertices.len() as u32;
        let start = self.indices.len() as u32;
        self.vertices.extend(mesh.vertices().iter().map(|vertex| {
            let local = vertex.position();
            Vertex {
                position: place.clip([
                    place.origin[0] + (rect.origin().x() + local.x()) * place.scale,
                    place.origin[1] + (rect.origin().y() + local.y()) * place.scale,
                ]),
                color: vertex.color().components(),
                ..Vertex::default()
            }
        }));
        self.indices
            .extend(mesh.indices().iter().map(|index| base + index));
        self.draw(Texture::White, clip, start);
        Ok(())
    }

    fn reserve(&mut self, vertices: usize, indices: usize) -> Result<(), PaintError> {
        if self.vertices.len() + vertices > MAX_VERTICES
            || self.indices.len() + indices > MAX_INDICES
        {
            return Err(PaintError::GeometryLimit);
        }
        self.vertices
            .try_reserve(vertices)
            .map_err(|_| PaintError::GeometryLimit)?;
        self.indices
            .try_reserve(indices)
            .map_err(|_| PaintError::GeometryLimit)
    }

    fn draw(&mut self, texture: Texture, clip: Scissor, start: u32) {
        let end = self.indices.len() as u32;
        if let Some(last) = self.draws.last_mut()
            && last.texture == texture
            && last.clip == clip
        {
            last.indices.end = end;
        } else {
            self.draws.push(Draw {
                indices: start..end,
                texture,
                clip,
            });
        }
    }
}

fn scissor(environment: UiEnvironment, clip: Option<UiRect>, extent: Extent2d) -> Option<Scissor> {
    Scissor::new(environment.physical_clip_rect(clip)?, extent)
}

struct Quad {
    bounds: [i32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    texture: Texture,
    radii: [f32; 4],
    border: f32,
}

impl Default for Quad {
    fn default() -> Self {
        Self {
            bounds: [0; 4],
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            texture: Texture::White,
            radii: [0.0; 4],
            border: 0.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Projection {
    extent: Extent2d,
    origin: [f32; 2],
    scale: f32,
}

impl Projection {
    fn clip(self, [x, y]: [f32; 2]) -> [f32; 2] {
        [
            x * (2.0 / self.extent.width as f32) - 1.0,
            1.0 - y * (2.0 / self.extent.height as f32),
        ]
    }
}
