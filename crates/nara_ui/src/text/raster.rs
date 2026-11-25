use std::sync::Arc;

use swash::{
    FontRef,
    scale::{ScaleContext, outline::Outline},
    zeno::{Format, Mask, Origin, Placement, Scratch, Vector},
};
use thiserror::Error;

use super::{GlyphRasterKey, PositionedGlyph, TextEngine};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphPlacement {
    key: GlyphRasterKey,
    baseline: [i32; 2],
}

impl GlyphPlacement {
    #[must_use]
    pub const fn key(self) -> GlyphRasterKey {
        self.key
    }

    #[must_use]
    pub fn raster_bounds(self, left: i32, top: i32, width: u32, height: u32) -> Option<[i32; 4]> {
        let x0 = self.baseline[0].checked_add(left)?;
        let y0 = self.baseline[1].checked_sub(top)?;
        let x1 = i32::try_from(i64::from(x0) + i64::from(width)).ok()?;
        let y1 = i32::try_from(i64::from(y0) + i64::from(height)).ok()?;
        Some([x0, y0, x1, y1])
    }
}

impl PositionedGlyph {
    #[must_use]
    pub fn physical_placement(
        &self,
        origin: [f32; 2],
        physical_origin: [f32; 2],
        scale: f32,
    ) -> Option<GlyphPlacement> {
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let (key, [x, y]) = self
            .key
            .at_position([self.x, self.y], origin, physical_origin, scale);
        Some(GlyphPlacement {
            key,
            baseline: [checked_pixel(x)?, checked_pixel(y)?],
        })
    }
}

impl GlyphRasterKey {
    pub(crate) fn at_position(
        mut self,
        position: [f32; 2],
        origin: [f32; 2],
        physical_origin: [f32; 2],
        scale: f32,
    ) -> (Self, [f64; 2]) {
        let (x, subpixel_x) =
            quarter_pixel_position(position[0], origin[0], physical_origin[0], scale);
        let (y, subpixel_y) =
            quarter_pixel_position(position[1], origin[1], physical_origin[1], scale);
        self.subpixel_x = subpixel_x;
        self.subpixel_y = subpixel_y;
        (self, [x, y])
    }
}

fn quarter_pixel_position(
    position: f32,
    origin: f32,
    physical_origin: f32,
    scale: f32,
) -> (f64, u8) {
    let physical =
        f64::from(physical_origin) + (f64::from(origin) + f64::from(position)) * f64::from(scale);
    let quarters = (physical * 4.0 + 0.5).floor();
    let baseline = (quarters * 0.25).floor();
    (baseline, (quarters - baseline * 4.0) as u8)
}

fn checked_pixel(pixel: f64) -> Option<i32> {
    (f64::from(i32::MIN)..=f64::from(i32::MAX))
        .contains(&pixel)
        .then(|| pixel as i32)
}

#[derive(Default)]
pub(crate) struct GlyphRasterizer {
    scale: ScaleContext,
    outline: Outline,
    scratch: Scratch,
}

impl GlyphRasterizer {
    fn render(&mut self, font: FontRef<'_>, key: GlyphRasterKey) -> Option<(Placement, Vec<u8>)> {
        let mut scaler = self
            .scale
            .builder(font)
            .size(key.physical_size())
            .hint(true)
            .build();
        if !scaler.scale_outline_into(key.glyph_id(), &mut self.outline)
            || self.outline.points().is_empty()
        {
            return None;
        }

        let (screen_offset_x, screen_offset_y) = key.subpixel_offset();
        let offset = Vector::new(screen_offset_x, -screen_offset_y);
        let mut mask = Vec::new();
        let placement = Mask::with_scratch(self.outline.path(), &mut self.scratch)
            .format(Format::Alpha)
            .origin(Origin::BottomLeft)
            .offset(offset)
            .render_offset(offset)
            .inspect(|format, width, height| mask.resize(format.buffer_size(width, height), 0))
            .render_into(&mut mask, None);
        Some((placement, mask))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphRaster {
    key: GlyphRasterKey,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    mask: Arc<[u8]>,
}

impl GlyphRaster {
    #[must_use]
    pub const fn key(&self) -> GlyphRasterKey {
        self.key
    }

    #[must_use]
    pub const fn left(&self) -> i32 {
        self.left
    }

    #[must_use]
    pub const fn top(&self) -> i32 {
        self.top
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn mask(&self) -> &[u8] {
        &self.mask
    }
}

impl TextEngine {
    pub fn rasterize(&mut self, key: GlyphRasterKey) -> Result<GlyphRaster, GlyphRasterError> {
        self.work_stats.raster_attempts = self.work_stats.raster_attempts.saturating_add(1);
        if !self.contains_font(key.face_id()) {
            return Err(GlyphRasterError::FaceMismatch);
        }
        let physical_size = key.physical_size();
        if !physical_size.is_finite() || physical_size <= 0.0 {
            return Err(GlyphRasterError::InvalidPhysicalSize {
                value: physical_size,
            });
        }

        let (font, rasterizer) = self
            .raster_context(key.face_id())
            .ok_or(GlyphRasterError::FaceMismatch)?;
        let Some((placement, mask)) = rasterizer.render(font, key) else {
            return Ok(GlyphRaster {
                key,
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                mask: Arc::from([]),
            });
        };
        self.work_stats.raster_output_bytes = self
            .work_stats
            .raster_output_bytes
            .saturating_add(mask.len() as u64);
        if placement.width > self.limits.max_glyph_extent
            || placement.height > self.limits.max_glyph_extent
        {
            return Err(GlyphRasterError::ExtentExceeded {
                width: placement.width,
                height: placement.height,
                maximum: self.limits.max_glyph_extent,
            });
        }
        let expected = usize::try_from(placement.width)
            .ok()
            .and_then(|width| {
                usize::try_from(placement.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(GlyphRasterError::InvalidMaskLength {
                actual: mask.len(),
                expected: usize::MAX,
            })?;
        if mask.len() != expected {
            return Err(GlyphRasterError::InvalidMaskLength {
                actual: mask.len(),
                expected,
            });
        }

        Ok(GlyphRaster {
            key,
            left: placement.left,
            top: placement.top,
            width: placement.width,
            height: placement.height,
            mask: mask.into(),
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum GlyphRasterError {
    #[error("glyph raster key belongs to a font face outside the admitted set")]
    FaceMismatch,
    #[error("glyph physical size {value} is invalid")]
    InvalidPhysicalSize { value: f32 },
    #[error("glyph rasterizer returned non-mask content")]
    UnexpectedContent,
    #[error("glyph raster is {width}x{height}; maximum extent is {maximum}")]
    ExtentExceeded {
        width: u32,
        height: u32,
        maximum: u32,
    },
    #[error("glyph raster has {actual} mask bytes; expected {expected}")]
    InvalidMaskLength { actual: usize, expected: usize },
}
