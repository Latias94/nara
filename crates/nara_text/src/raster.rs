use std::sync::Arc;

use swash::{
    FontRef,
    scale::{Render, Source, image::Content},
    zeno::{Format, Vector},
};
use thiserror::Error;

use crate::{GlyphRasterKey, TextEngine};

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
        if key.face_id() != self.face.id() {
            return Err(GlyphRasterError::FaceMismatch);
        }
        let physical_size = key.physical_size();
        if !physical_size.is_finite() || physical_size <= 0.0 {
            return Err(GlyphRasterError::InvalidPhysicalSize {
                value: physical_size,
            });
        }

        let font = FontRef {
            data: self.face.bytes(),
            offset: self.swash_offset,
            key: self.swash_key,
        };
        let mut scaler = self
            .scale_context
            .builder(font)
            .size(physical_size)
            .hint(true)
            .build();
        let (offset_x, offset_y) = key.subpixel_offset();
        let sources = [Source::Outline];
        let image = Render::new(&sources)
            .format(Format::Alpha)
            .offset(Vector::new(offset_x, offset_y))
            .render(&mut scaler, key.glyph_id());

        let Some(image) = image else {
            return Ok(GlyphRaster {
                key,
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                mask: Arc::from([]),
            });
        };
        if image.content != Content::Mask {
            return Err(GlyphRasterError::UnexpectedContent);
        }
        if image.placement.width > self.limits.max_glyph_extent
            || image.placement.height > self.limits.max_glyph_extent
        {
            return Err(GlyphRasterError::ExtentExceeded {
                width: image.placement.width,
                height: image.placement.height,
                maximum: self.limits.max_glyph_extent,
            });
        }
        let expected = usize::try_from(image.placement.width)
            .ok()
            .and_then(|width| {
                usize::try_from(image.placement.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(GlyphRasterError::InvalidMaskLength {
                actual: image.data.len(),
                expected: usize::MAX,
            })?;
        if image.data.len() != expected {
            return Err(GlyphRasterError::InvalidMaskLength {
                actual: image.data.len(),
                expected,
            });
        }

        Ok(GlyphRaster {
            key,
            left: image.placement.left,
            top: image.placement.top,
            width: image.placement.width,
            height: image.placement.height,
            mask: image.data.into(),
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum GlyphRasterError {
    #[error("glyph raster key belongs to another font face")]
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
