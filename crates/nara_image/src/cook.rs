//! Pure CPU cooking into source-independent Runtime Image products.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    num::NonZeroU32,
};

use nara_core::ByteLimit;

use crate::runtime::{
    MAX_DIMENSION as RUNTIME_MAX_DIMENSION, MAX_PIXELS as RUNTIME_MAX_PIXELS,
    MAX_RGBA_BYTES as RUNTIME_MAX_RGBA_BYTES,
};
use crate::{ImageAsset, ImageAssetCreateError, ImageExtent, ImageFormat};

const SHARED_PIXEL_OVERHEAD: usize = size_of::<Box<[u8]>>() + 2 * size_of::<usize>();

/// The concrete device fact consumed when selecting the output extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageCookTarget {
    max_texture_dimension_2d: NonZeroU32,
}

impl ImageCookTarget {
    #[must_use]
    pub const fn new(max_texture_dimension_2d: NonZeroU32) -> Self {
        Self {
            max_texture_dimension_2d,
        }
    }

    #[must_use]
    pub const fn max_texture_dimension_2d(self) -> NonZeroU32 {
        self.max_texture_dimension_2d
    }
}

/// How cooking treats an image larger than the selected target's dimension cap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ImageOversizePolicy {
    #[default]
    Reject,
    /// Shrink with integer aspect fitting and exact nearest-neighbor texel selection.
    ///
    /// This is an intentional pixel-art option, with no filtering, color conversion or alpha
    /// premultiplication. Images already within the target cap are never enlarged.
    FitNearest,
}

/// The concrete extent-selection policy used for this cook.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ImageCookSettings {
    pub oversize: ImageOversizePolicy,
}

/// Explicit per-call bounds on modeled retained output and allocation peak.
///
/// For `N` output pixel bytes and `H = size_of::<Box<[u8]>>() + 2 * size_of::<usize>()`,
/// retained output costs `size_of::<ImageAsset>() + H + N`, matching the Runtime codec's
/// source-free value model. The allocation peak conservatively costs retained output plus `N`
/// more bytes, allowing pixel storage normalization to overlap the original output buffer.
/// There is no image-sized scratch allocation besides those two potential pixel buffers.
///
/// These are logical costs, excluding allocator metadata and size-class rounding. Borrowed
/// input, encoded Runtime bytes and Host wrappers are excluded. The caller retains and charges
/// the input independently, reserves the complete overlap, and keeps output custody alive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageCookLimits {
    max_output_bytes: ByteLimit,
    max_allocation_bytes: ByteLimit,
}

impl ImageCookLimits {
    #[must_use]
    pub const fn new(max_output_bytes: ByteLimit, max_allocation_bytes: ByteLimit) -> Self {
        Self {
            max_output_bytes,
            max_allocation_bytes,
        }
    }

    #[must_use]
    pub const fn max_output_bytes(self) -> ByteLimit {
        self.max_output_bytes
    }

    #[must_use]
    pub const fn max_allocation_bytes(self) -> ByteLimit {
        self.max_allocation_bytes
    }
}

/// Failure to produce a bounded, persistently identified Runtime Image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageCookError {
    AnonymousProduct,
    TargetDimensionExceeded {
        extent: ImageExtent,
        max_texture_dimension_2d: NonZeroU32,
    },
    /// The selected extent violates runtime_image version 1 dimensions or pixel count.
    RuntimeExtentExceeded {
        extent: ImageExtent,
    },
    SizeOverflow,
    OutputBytesExceeded {
        required: usize,
        limit: ByteLimit,
    },
    AllocationBytesExceeded {
        required: usize,
        limit: ByteLimit,
    },
    AllocationFailed {
        pixel_bytes: usize,
    },
    InvalidPixelStorage,
    Image(ImageAssetCreateError),
}

impl Display for ImageCookError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnonymousProduct => {
                formatter.write_str("image cooking requires a complete persistent Product identity")
            }
            Self::TargetDimensionExceeded {
                extent,
                max_texture_dimension_2d,
            } => write!(
                formatter,
                "image extent {}x{} exceeds target dimension cap {max_texture_dimension_2d}",
                extent.width, extent.height
            ),
            Self::RuntimeExtentExceeded { extent } => write!(
                formatter,
                "image extent {}x{} exceeds Runtime Image format dimensions or pixel count",
                extent.width, extent.height
            ),
            Self::SizeOverflow => {
                formatter.write_str("image cook storage size is not representable")
            }
            Self::OutputBytesExceeded { required, limit } => write!(
                formatter,
                "image cook retained output requires {required} bytes, exceeding limit {}",
                limit.get()
            ),
            Self::AllocationBytesExceeded { required, limit } => write!(
                formatter,
                "image cook allocation peak requires {required} bytes, exceeding limit {}",
                limit.get()
            ),
            Self::AllocationFailed { pixel_bytes } => write!(
                formatter,
                "image cook could not reserve {pixel_bytes} output pixel bytes"
            ),
            Self::InvalidPixelStorage => formatter
                .write_str("image cook input pixels do not cover the selected source texel"),
            Self::Image(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ImageCookError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Image(error) => Some(error),
            _ => None,
        }
    }
}

/// Cooks borrowed CPU pixels into a new source-independent Runtime product.
///
/// The complete primary or auxiliary Product identity, format and color space are preserved.
/// Anonymous procedural images are rejected. Source provenance and attached retention owners
/// are not copied; the result owns fresh pixels, including when the extent stays unchanged.
///
/// With [`ImageOversizePolicy::FitNearest`], let `L` be the larger input dimension and `C` the
/// target cap. If `L > C`, each output dimension is `max(1, floor(input_dimension * C / L))`.
/// Thus the longest dimension equals `C`; aspect ratio is preserved subject to integer rounding
/// and the minimum one-texel axis. Runtime format limits remain independent admission checks,
/// rather than another implicit resize policy.
///
/// Each output coordinate `d` on an axis of length `D` selects the source texel
/// `floor((2*d + 1) * S / (2*D))` on the source axis of length `S`. This maps pixel centers
/// with exact integer arithmetic; a tie selects the higher source coordinate. The complete
/// selected texel is copied unchanged, including RGB under zero alpha.
///
/// Product identity, output format constraints and both checked byte bounds are admitted before
/// any output allocation. This function performs no I/O, publication, task scheduling or global
/// budget management. Encoded bytes and aggregate input/output custody remain caller-owned.
pub fn cook_runtime_image(
    image: &ImageAsset,
    settings: ImageCookSettings,
    target: ImageCookTarget,
    limits: ImageCookLimits,
) -> Result<ImageAsset, ImageCookError> {
    let product = image.product().ok_or(ImageCookError::AnonymousProduct)?;
    let input_extent = image.extent();
    let output_extent = fit_extent(input_extent, settings.oversize, target)?;
    let pixel_bytes = admit_output(output_extent, image.format(), limits)?;
    let bytes_per_texel = match image.format() {
        ImageFormat::Rgba8 => 4,
    };
    let source_stride = usize::try_from(input_extent.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_texel))
        .ok_or(ImageCookError::SizeOverflow)?;
    if input_extent == output_extent && image.pixels().len() != pixel_bytes {
        return Err(ImageCookError::InvalidPixelStorage);
    }

    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(pixel_bytes)
        .map_err(|_| ImageCookError::AllocationFailed { pixel_bytes })?;
    if input_extent == output_extent {
        // ImageAsset constructors validate exact pixel storage. The explicit length check above
        // also binds this unchanged-image copy to the admitted pixel_bytes before allocation.
        pixels.extend_from_slice(image.pixels());
    } else {
        for y in 0..output_extent.height {
            let source_y = nearest_coordinate(y, input_extent.height, output_extent.height);
            let row_start = usize::try_from(source_y)
                .ok()
                .and_then(|row| row.checked_mul(source_stride))
                .ok_or(ImageCookError::SizeOverflow)?;
            for x in 0..output_extent.width {
                let source_x = nearest_coordinate(x, input_extent.width, output_extent.width);
                let offset = usize::try_from(source_x)
                    .ok()
                    .and_then(|column| column.checked_mul(bytes_per_texel))
                    .and_then(|column| row_start.checked_add(column))
                    .ok_or(ImageCookError::SizeOverflow)?;
                let texel = image
                    .pixels()
                    .get(offset..)
                    .and_then(|remaining| remaining.get(..bytes_per_texel))
                    .ok_or(ImageCookError::InvalidPixelStorage)?;
                pixels.extend_from_slice(texel);
            }
        }
    }

    ImageAsset::from_product(
        product,
        output_extent,
        image.format(),
        image.color_space(),
        pixels,
    )
    .map_err(ImageCookError::Image)
}

fn fit_extent(
    extent: ImageExtent,
    policy: ImageOversizePolicy,
    target: ImageCookTarget,
) -> Result<ImageExtent, ImageCookError> {
    let longest = extent.width.max(extent.height);
    let cap = target.max_texture_dimension_2d.get();
    if longest <= cap {
        return Ok(extent);
    }
    if policy == ImageOversizePolicy::Reject {
        return Err(ImageCookError::TargetDimensionExceeded {
            extent,
            max_texture_dimension_2d: target.max_texture_dimension_2d,
        });
    }
    let fit = |axis| (u64::from(axis) * u64::from(cap) / u64::from(longest)).max(1) as u32;
    Ok(ImageExtent::new(fit(extent.width), fit(extent.height)))
}

fn admit_output(
    extent: ImageExtent,
    format: ImageFormat,
    limits: ImageCookLimits,
) -> Result<usize, ImageCookError> {
    if extent.width == 0
        || extent.height == 0
        || extent.width > RUNTIME_MAX_DIMENSION
        || extent.height > RUNTIME_MAX_DIMENSION
        || extent.pixel_count() > RUNTIME_MAX_PIXELS
    {
        return Err(ImageCookError::RuntimeExtentExceeded { extent });
    }
    let pixel_bytes = match format {
        ImageFormat::Rgba8 => extent.pixel_count().checked_mul(4),
    }
    .and_then(|bytes| usize::try_from(bytes).ok())
    .filter(|bytes| *bytes <= RUNTIME_MAX_RGBA_BYTES)
    .ok_or(ImageCookError::SizeOverflow)?;
    let output_bytes = size_of::<ImageAsset>()
        .checked_add(SHARED_PIXEL_OVERHEAD)
        .and_then(|bytes| bytes.checked_add(pixel_bytes))
        .ok_or(ImageCookError::SizeOverflow)?;
    let allocation_bytes = output_bytes
        .checked_add(pixel_bytes)
        .ok_or(ImageCookError::SizeOverflow)?;
    if output_bytes > limits.max_output_bytes.get() {
        return Err(ImageCookError::OutputBytesExceeded {
            required: output_bytes,
            limit: limits.max_output_bytes,
        });
    }
    if allocation_bytes > limits.max_allocation_bytes.get() {
        return Err(ImageCookError::AllocationBytesExceeded {
            required: allocation_bytes,
            limit: limits.max_allocation_bytes,
        });
    }
    Ok(pixel_bytes)
}

fn nearest_coordinate(coordinate: u32, source_axis: u32, output_axis: u32) -> u32 {
    // Admitted output axes are at most 16,384, so even u32::MAX source axes fit in u64.
    (((2 * u64::from(coordinate) + 1) * u64::from(source_axis)) / (2 * u64::from(output_axis)))
        as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_extent_admission_checks_dimensions_and_pixel_count_without_allocating_pixels() {
        let limits = ImageCookLimits::new(
            ByteLimit::new(usize::MAX).unwrap(),
            ByteLimit::new(usize::MAX).unwrap(),
        );
        for extent in [
            ImageExtent::new(0, 1),
            ImageExtent::new(1, 0),
            ImageExtent::new(16_385, 1),
            ImageExtent::new(1, 16_385),
            ImageExtent::new(4097, 4096),
            ImageExtent::new(u32::MAX, u32::MAX),
        ] {
            assert_eq!(
                admit_output(extent, ImageFormat::Rgba8, limits),
                Err(ImageCookError::RuntimeExtentExceeded { extent })
            );
        }
        for extent in [
            ImageExtent::new(16_384, 1),
            ImageExtent::new(1, 16_384),
            ImageExtent::new(4096, 4096),
        ] {
            assert_eq!(
                admit_output(extent, ImageFormat::Rgba8, limits).unwrap(),
                extent.pixel_count() as usize * 4
            );
        }
    }
}
