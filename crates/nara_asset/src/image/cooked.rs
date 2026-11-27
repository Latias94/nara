use std::collections::TryReserveError;

use super::{
    ImageAsset, ImageAssetCreateError, ImageColorSpace, ImageExtent, ImageFormat,
    MAX_IMPORT_DIMENSION, MAX_IMPORT_PIXELS, pixel_byte_len,
};

const MAGIC: &[u8; 4] = b"NIMG";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 16;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CookedImageError {
    #[error("invalid cooked image header: {0}")]
    InvalidHeader(&'static str),
    #[error("unsupported cooked image version {0}")]
    UnsupportedVersion(u16),
    #[error("cooked image extent {extent:?} exceeds the RGBA8 profile limits")]
    ExtentLimit { extent: ImageExtent },
    #[error("could not allocate cooked image bytes: {0}")]
    Capacity(#[from] TryReserveError),
    #[error(transparent)]
    InvalidImage(#[from] ImageAssetCreateError),
}

impl ImageAsset {
    /// Encodes the NIMG v1 RGBA8 profile.
    ///
    /// The 16-byte header is `NIMG`, version `u16`, format `u8` (RGBA8 = 1), color space
    /// `u8` (sRGB = 1, linear = 2), width `u32`, and height `u32`. Integers are little-endian.
    /// Exactly `width * height * 4` packed RGBA bytes follow. Each dimension is limited to
    /// 16,384 and the total area to 16,777,216 pixels.
    pub fn encode_cooked(&self) -> Result<Vec<u8>, CookedImageError> {
        validate_extent(self.extent)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(HEADER_BYTES + self.pixels().len())?;
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.push(match self.format {
            ImageFormat::Rgba8 => 1,
        });
        bytes.push(match self.color_space {
            ImageColorSpace::Srgb => 1,
            ImageColorSpace::Linear => 2,
        });
        bytes.extend_from_slice(&self.extent.width.to_le_bytes());
        bytes.extend_from_slice(&self.extent.height.to_le_bytes());
        bytes.extend_from_slice(self.pixels());
        Ok(bytes)
    }

    /// Decodes a checked NIMG v1 payload without the authoring image decoder.
    pub fn from_cooked(bytes: &[u8]) -> Result<Self, CookedImageError> {
        let (header, pixels) = bytes
            .split_first_chunk::<HEADER_BYTES>()
            .ok_or(CookedImageError::InvalidHeader("truncated header"))?;
        if &header[..4] != MAGIC {
            return Err(CookedImageError::InvalidHeader("incorrect magic"));
        }
        let version = u16::from_le_bytes([header[4], header[5]]);
        if version != VERSION {
            return Err(CookedImageError::UnsupportedVersion(version));
        }
        let format = match header[6] {
            1 => ImageFormat::Rgba8,
            _ => return Err(CookedImageError::InvalidHeader("unknown pixel format")),
        };
        let color_space = match header[7] {
            1 => ImageColorSpace::Srgb,
            2 => ImageColorSpace::Linear,
            _ => return Err(CookedImageError::InvalidHeader("unknown color space")),
        };
        let extent = ImageExtent::new(
            u32::from_le_bytes(header[8..12].try_into().expect("fixed width field")),
            u32::from_le_bytes(header[12..16].try_into().expect("fixed height field")),
        );
        validate_extent(extent)?;
        let expected = pixel_byte_len(extent, format)?;
        if pixels.len() != expected {
            return Err(ImageAssetCreateError::PixelLengthMismatch {
                extent,
                expected,
                actual: pixels.len(),
            }
            .into());
        }
        let mut owned = Vec::new();
        owned.try_reserve_exact(expected)?;
        owned.extend_from_slice(pixels);
        Ok(Self::new(extent, format, color_space, owned)?)
    }
}

fn validate_extent(extent: ImageExtent) -> Result<(), CookedImageError> {
    if extent.width > MAX_IMPORT_DIMENSION
        || extent.height > MAX_IMPORT_DIMENSION
        || extent.pixel_count() > MAX_IMPORT_PIXELS
    {
        return Err(CookedImageError::ExtentLimit { extent });
    }
    Ok(())
}
