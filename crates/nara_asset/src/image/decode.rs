use std::io::Cursor;

use ::image::{DynamicImage, ImageDecoder, Limits, codecs::png::PngDecoder};

use super::{
    ImageAsset, ImageAssetCreateError, ImageColorSpace, ImageExtent, ImageFormat,
    MAX_IMPORT_DIMENSION, MAX_IMPORT_PIXELS,
};

pub(super) const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;

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
    /// pixels. The color space controls how the renderer interprets the decoded channel values.
    pub fn from_png(bytes: &[u8], color_space: ImageColorSpace) -> Result<Self, ImageDecodeError> {
        if bytes.len() > MAX_PNG_BYTES {
            return Err(ImageDecodeError::EncodedTooLarge {
                limit: MAX_PNG_BYTES,
            });
        }
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMPORT_DIMENSION);
        limits.max_image_height = Some(MAX_IMPORT_DIMENSION);
        limits.max_alloc = Some(MAX_PNG_BYTES as u64);
        let decoder = PngDecoder::with_limits(Cursor::new(bytes), limits)?;
        let (width, height) = decoder.dimensions();
        let extent = ImageExtent::new(width, height);
        if extent.pixel_count() > MAX_IMPORT_PIXELS {
            return Err(ImageDecodeError::TooManyPixels {
                extent,
                limit: MAX_IMPORT_PIXELS,
            });
        }
        let pixels = DynamicImage::from_decoder(decoder)?.into_rgba8().into_raw();
        Ok(Self::new(extent, ImageFormat::Rgba8, color_space, pixels)?)
    }
}
