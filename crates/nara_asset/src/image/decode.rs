use std::io::Cursor;

use ::image::{ColorType, DynamicImage, ImageDecoder, Limits, codecs::png::PngDecoder};

use super::{
    ImageAsset, ImageAssetCreateError, ImageColorSpace, ImageExtent, ImageFormat,
    MAX_IMPORT_DIMENSION, MAX_IMPORT_PIXELS,
};

pub(super) const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;
const PNG_DECODER_SCRATCH_BYTES: u64 = 16 * 1024 * 1024;
const RGBA16_BYTES_PER_PIXEL: u64 = 8;
const RGBA8_BYTES_PER_PIXEL: u64 = 4;
const MAX_PNG_PIXEL_BUFFER_BYTES: u64 =
    MAX_IMPORT_PIXELS * (RGBA16_BYTES_PER_PIXEL + RGBA8_BYTES_PER_PIXEL);

#[derive(Debug, thiserror::Error)]
pub enum ImageDecodeError {
    #[error("encoded PNG exceeds the {limit}-byte limit")]
    EncodedTooLarge { limit: usize },
    #[error("PNG extent {extent:?} exceeds the {limit}-pixel limit")]
    TooManyPixels { extent: ImageExtent, limit: u64 },
    #[error("PNG decoding failed: {0}")]
    Decode(#[from] ::image::ImageError),
    #[error(transparent)]
    InvalidImage(#[from] ImageAssetCreateError),
}

impl ImageAsset {
    /// Decodes a PNG into tightly packed RGBA8 pixels in the chosen color space.
    ///
    /// PNG input is limited to 16 MiB, each dimension to 16,384, and total area to 16,777,216
    /// pixels. The pixel-buffer budget includes both the decoded image and its RGBA8 conversion.
    /// The color space controls how the renderer interprets the decoded channel values.
    pub fn from_png(bytes: &[u8], color_space: ImageColorSpace) -> Result<Self, ImageDecodeError> {
        if bytes.len() > MAX_PNG_BYTES {
            return Err(ImageDecodeError::EncodedTooLarge {
                limit: MAX_PNG_BYTES,
            });
        }
        let mut decoder_limits = Limits::default();
        decoder_limits.max_image_width = Some(MAX_IMPORT_DIMENSION);
        decoder_limits.max_image_height = Some(MAX_IMPORT_DIMENSION);
        decoder_limits.max_alloc = Some(PNG_DECODER_SCRATCH_BYTES);
        let mut decoder = PngDecoder::with_limits(Cursor::new(bytes), decoder_limits)?;
        let (width, height) = decoder.dimensions();
        let extent = ImageExtent::new(width, height);
        if extent.pixel_count() > MAX_IMPORT_PIXELS {
            return Err(ImageDecodeError::TooManyPixels {
                extent,
                limit: MAX_IMPORT_PIXELS,
            });
        }
        let mut pixel_limits = Limits::no_limits();
        pixel_limits.max_alloc = Some(MAX_PNG_PIXEL_BUFFER_BYTES);
        let mut pixel_budget = pixel_limits.clone();
        pixel_budget.reserve(decoder.total_bytes())?;
        if decoder.color_type() != ColorType::Rgba8 {
            pixel_budget.reserve(extent.pixel_count() * RGBA8_BYTES_PER_PIXEL)?;
        }
        decoder.set_limits(pixel_limits)?;
        let pixels = DynamicImage::from_decoder(decoder)?.into_rgba8().into_raw();
        Ok(Self::new(extent, ImageFormat::Rgba8, color_space, pixels)?)
    }
}

#[cfg(test)]
mod tests;
