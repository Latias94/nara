//! The Image product family and its source-independent processed format.

use nara_asset::{AssetDomain, ImportedProductId};

use crate::ImageAsset;

/// An immutable Image CPU product, independent of source importing and GPU preparation.
///
/// A persistent value carries one complete Product identity, either primary or auxiliary; it does
/// not implicitly contain the primary product. Anonymous procedural content has no membership.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageDomain;

impl AssetDomain for ImageDomain {
    type Product = ImageAsset;

    fn contains_product(product: &ImageAsset, id: ImportedProductId) -> bool {
        product
            .product()
            .is_some_and(|reference| reference.product() == id)
    }
}

/// Version 1 of the compact processed Image format.
///
/// Bytes are `NRI\0`, a little-endian `u32` metadata byte length, the exact JSON
/// [`nara_core::PersistentFileEnvelope`] and uncompressed RGBA bytes. Metadata has kind
/// `runtime_image`, format version 1 and minimum engine version `0.1.0`; its payload contains
/// only `product`, `extent`, `format`, `color_space` and `pixel_bytes`. Records must be JSON
/// objects. Source paths, import recipes, artifact identities and trusted content hashes are
/// absent. The decoded value recomputes its descriptor-and-pixel hash.
///
/// This format independently limits metadata to 16 KiB, each dimension to 16,384, pixels to
/// 16,777,216 and RGBA storage to 64 MiB. All length and semantic checks precede pixel copying.
/// The codec promises 128 MiB plus 128 KiB of modeled work and 64 MiB plus 1 MiB of modeled
/// retained output. These are conservative logical costs, not allocator interception or RSS
/// measurements. Encoded input and output reservations remain the caller's responsibility.
///
/// Decoded-output costing covers only values with a complete Product identity, valid Runtime
/// dimensions and pixels, no source provenance and no attached retention owner, as returned by
/// this decoder. The cost is `size_of::<ImageAsset>() + pixels.len() + size_of::<Box<[u8]>>() +
/// 2 * size_of::<usize>()`: the inline value, fixed-size pixel allocation and modeled Arc storage
/// for its boxed-slice handle and two reference counters. Shared pixels count in full. Captured
/// encoded input, the immutable base, codec scratch and the Host wrapper are excluded. Source
/// provenance and attached external owners are rejected because this codec cannot bound them.
///
/// Decoding creates an immutable product without a retention owner. A publishing Host must keep
/// its decoded-output reservation alive through every escaped pixel or prepared-render owner;
/// the codec neither manufactures an import lease nor performs I/O or publication.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageRuntimeCodec;

#[cfg(feature = "serde")]
mod codec {
    use std::{
        fmt,
        io::{self, Write},
    };

    use nara_asset::{AssetProductRef, AssetRuntimeError, AssetRuntimeLimits, RuntimeAssetCodec};
    use nara_core::{
        ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
        PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
        PersistentFileEnvelope, PersistentFileHeader, SerdeShapeLimits,
        decode_persistent_file_with_preflight, preflight_serde_shape,
    };
    use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
    use serde::{
        Deserialize, Serialize,
        de::{self, MapAccess, Visitor},
    };

    use super::ImageRuntimeCodec;
    use crate::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat};

    pub(super) const MAGIC: &[u8; 4] = b"NRI\0";
    pub(super) const PREFIX_BYTES: usize = 8;
    pub(super) const MAX_METADATA_BYTES: usize = 16 * 1024;
    pub(super) const MAX_RGBA_BYTES: usize = 64 * 1024 * 1024;
    const MAX_DIMENSION: u32 = 16_384;
    const MAX_PIXELS: u64 = 16_777_216;
    const KIND: &str = "runtime_image";

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Metadata {
        product: AssetProductRef,
        extent: Extent,
        format: ImageFormat,
        color_space: ImageColorSpace,
        pixel_bytes: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Extent {
        width: u32,
        height: u32,
    }

    impl RuntimeAssetCodec<ImageAsset> for ImageRuntimeCodec {
        const RUNTIME_KIND: &'static str = KIND;
        const RUNTIME_VERSION: u32 = 1;

        fn runtime_limits() -> AssetRuntimeLimits {
            AssetRuntimeLimits::new(
                byte_limit(MAX_RGBA_BYTES + MAX_METADATA_BYTES + PREFIX_BYTES),
                byte_limit(128 * 1024 * 1024 + 128 * 1024),
                byte_limit(MAX_RGBA_BYTES + 1024 * 1024),
            )
        }

        fn encode_runtime(product: &ImageAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
            let reference = product.product().ok_or_else(product_error)?;
            let pixel_bytes = checked_pixel_bytes(product.extent(), product.format())?;
            if pixel_bytes != product.pixels().len() {
                return Err(pixel_length_error());
            }
            let envelope = PersistentFileEnvelope::canonical_v1(
                kind()?,
                version("0.1.0")?,
                FormatGenerator::new("nara_image", version(env!("CARGO_PKG_VERSION"))?)
                    .map_err(|_| metadata_error())?,
                Metadata {
                    product: reference,
                    extent: Extent {
                        width: product.extent().width,
                        height: product.extent().height,
                    },
                    format: product.format(),
                    color_space: product.color_space(),
                    pixel_bytes: pixel_bytes as u64,
                },
            );
            // Count the borrowed representation before allocating an output buffer. Pixels are
            // never passed through serde, and the same metadata is written directly to output.
            let mut counter = MetadataCounter(0);
            serde_json::to_writer(&mut counter, &envelope).map_err(|_| metadata_error())?;
            let length = counter.0;
            let wire_length = u32::try_from(length).map_err(|_| metadata_error())?;
            let total = PREFIX_BYTES
                .checked_add(length)
                .and_then(|bytes| bytes.checked_add(pixel_bytes))
                .ok_or_else(pixel_length_error)?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(total)
                .map_err(|_| allocation_error())?;
            bytes.extend_from_slice(MAGIC);
            bytes.extend_from_slice(&wire_length.to_le_bytes());
            serde_json::to_writer(&mut bytes, &envelope).map_err(|_| metadata_error())?;
            preflight(&bytes[PREFIX_BYTES..])?;
            bytes.extend_from_slice(product.pixels());
            Ok(bytes.into_boxed_slice())
        }

        fn decode_runtime(bytes: &[u8]) -> Result<ImageAsset, AssetRuntimeError> {
            if bytes.len() > Self::runtime_limits().encoded_bytes().get()
                || bytes.len() < PREFIX_BYTES
                || bytes.get(..4) != Some(MAGIC.as_slice())
            {
                return Err(framing_error());
            }
            let length = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
            if length == 0 || length > MAX_METADATA_BYTES {
                return Err(metadata_error());
            }
            let end = PREFIX_BYTES.checked_add(length).ok_or_else(framing_error)?;
            let metadata = bytes.get(PREFIX_BYTES..end).ok_or_else(framing_error)?;
            let contract =
                PersistentFileContract::canonical_v1(kind()?, version(env!("CARGO_PKG_VERSION"))?);
            let envelope = decode_persistent_file_with_preflight(
                metadata,
                byte_limit(MAX_METADATA_BYTES),
                &contract,
                preflight,
                |metadata| {
                    serde_json::from_slice::<PersistentFileHeader>(metadata)
                        .map_err(|_| metadata_error())
                },
                |metadata| {
                    serde_json::from_slice::<PersistentFileEnvelope<Metadata>>(metadata)
                        .map_err(|_| metadata_error())
                },
            )
            .map_err(decode_error)?;
            let Metadata {
                product,
                extent,
                format,
                color_space,
                pixel_bytes,
            } = envelope.into_payload();
            let extent = ImageExtent::new(extent.width, extent.height);
            let expected = checked_pixel_bytes(extent, format)?;
            if pixel_bytes != expected as u64 || bytes.len() - end != expected {
                return Err(pixel_length_error());
            }
            // The product constructor recomputes content identity from validated descriptors
            // and bytes. The wire carries neither a precomputed hash nor source provenance.
            let mut pixels = Vec::new();
            pixels
                .try_reserve_exact(expected)
                .map_err(|_| allocation_error())?;
            pixels.extend_from_slice(&bytes[end..]);
            ImageAsset::from_product(product, extent, format, color_space, pixels)
                .map_err(|_| pixel_length_error())
        }

        fn runtime_retained_bytes(product: &ImageAsset) -> Result<usize, AssetRuntimeError> {
            if product.product().is_none() {
                return Err(product_error());
            }
            let pixel_bytes = checked_pixel_bytes(product.extent(), product.format())?;
            if product.pixels().len() != pixel_bytes {
                return Err(pixel_length_error());
            }
            product.runtime_retained_bytes().ok_or_else(retention_error)
        }

        fn contains_runtime_product(product: &ImageAsset, reference: AssetProductRef) -> bool {
            product.product() == Some(reference)
        }
    }

    fn checked_pixel_bytes(
        extent: ImageExtent,
        format: ImageFormat,
    ) -> Result<usize, AssetRuntimeError> {
        if extent.width == 0
            || extent.height == 0
            || extent.width > MAX_DIMENSION
            || extent.height > MAX_DIMENSION
        {
            return Err(dimensions_error());
        }
        let pixels = u64::from(extent.width)
            .checked_mul(u64::from(extent.height))
            .filter(|pixels| *pixels <= MAX_PIXELS)
            .ok_or_else(dimensions_error)?;
        let bytes = match format {
            ImageFormat::Rgba8 => pixels.checked_mul(4),
        }
        .and_then(|bytes| usize::try_from(bytes).ok())
        .filter(|bytes| *bytes <= MAX_RGBA_BYTES)
        .ok_or_else(pixel_length_error)?;
        Ok(bytes)
    }

    struct MetadataCounter(usize);

    impl Write for MetadataCounter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let total = self
                .0
                .checked_add(bytes.len())
                .filter(|total| *total <= MAX_METADATA_BYTES)
                .ok_or_else(|| io::Error::other("Runtime Image metadata exceeds its byte limit"))?;
            self.0 = total;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn preflight(bytes: &[u8]) -> Result<(), AssetRuntimeError> {
        let limits = SerdeShapeLimits::new(
            DepthLimit::new(8).expect("fixed nonzero Image metadata depth"),
            ItemLimit::new(64).expect("fixed nonzero Image metadata node limit"),
            ItemLimit::new(8).expect("fixed nonzero Image metadata container limit"),
            byte_limit(256),
            byte_limit(4096),
        );
        let mut parser = serde_json::Deserializer::from_slice(bytes);
        preflight_serde_shape(&mut parser, limits).map_err(|_| metadata_error())?;
        parser.end().map_err(|_| metadata_error())?;
        // All compound values in this concrete grammar are records. Derived serde structs
        // would also accept positional arrays, including envelope/generator/extent arrays.
        // Reject those without building a JSON tree or duplicating the envelope semantics.
        let mut parser = serde_json::Deserializer::from_slice(bytes);
        RecordsOnly::deserialize(&mut parser).map_err(|_| metadata_error())?;
        parser.end().map_err(|_| metadata_error())
    }

    struct RecordsOnly;

    impl<'de> Deserialize<'de> for RecordsOnly {
        fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct RecordVisitor;

            impl<'de> Visitor<'de> for RecordVisitor {
                type Value = RecordsOnly;

                fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str("a JSON record or scalar, never a positional array")
                }

                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                    while map.next_entry::<de::IgnoredAny, RecordsOnly>()?.is_some() {}
                    Ok(RecordsOnly)
                }

                fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
                    Ok(RecordsOnly)
                }

                fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
                    Ok(RecordsOnly)
                }

                fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
                    Ok(RecordsOnly)
                }

                fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
                    Ok(RecordsOnly)
                }

                fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
                    Ok(RecordsOnly)
                }

                fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                    Ok(RecordsOnly)
                }
            }

            deserializer.deserialize_any(RecordVisitor)
        }
    }

    fn byte_limit(bytes: usize) -> ByteLimit {
        ByteLimit::new(bytes).expect("fixed nonzero Runtime Image byte limit")
    }

    fn kind() -> Result<FormatKind, AssetRuntimeError> {
        FormatKind::new(KIND).map_err(|_| metadata_error())
    }

    fn version(value: &str) -> Result<EngineVersion, AssetRuntimeError> {
        EngineVersion::parse(value).map_err(|_| metadata_error())
    }

    fn decode_error(error: PersistentFileDecodeError<AssetRuntimeError>) -> AssetRuntimeError {
        match error {
            PersistentFileDecodeError::Shape(error)
            | PersistentFileDecodeError::Header(error)
            | PersistentFileDecodeError::Payload(error) => error,
            PersistentFileDecodeError::EncodedBytesExceeded { .. } => metadata_error(),
            PersistentFileDecodeError::Contract(error) => match error {
                PersistentFileContractError::WrongKind { .. } => failure(
                    "image.runtime-kind",
                    "Runtime Image requires the runtime_image file kind",
                ),
                PersistentFileContractError::UnsupportedFormatVersion { .. } => failure(
                    "image.runtime-version",
                    "Runtime Image requires format version one",
                ),
                PersistentFileContractError::EngineVersionTooOld { .. } => failure(
                    "image.runtime-engine-version",
                    "Runtime Image requires a newer engine version",
                ),
            },
        }
    }

    fn failure(code: &'static str, summary: &'static str) -> AssetRuntimeError {
        AssetRuntimeError::new(Diagnostic::error(
            DiagnosticCode::new(code).expect("fixed valid Runtime Image diagnostic code"),
            SafeSummary::new(summary).expect("fixed safe Runtime Image summary"),
        ))
    }

    fn framing_error() -> AssetRuntimeError {
        failure(
            "image.runtime-framing",
            "Runtime Image framing or encoded length is invalid",
        )
    }

    fn metadata_error() -> AssetRuntimeError {
        failure(
            "image.runtime-metadata",
            "Runtime Image requires bounded strict JSON metadata records",
        )
    }

    fn pixel_length_error() -> AssetRuntimeError {
        failure(
            "image.runtime-pixel-length",
            "Runtime Image requires exactly the bounded RGBA byte length",
        )
    }

    fn product_error() -> AssetRuntimeError {
        failure(
            "image.runtime-product",
            "Runtime Image requires an explicit complete Product identity",
        )
    }

    fn retention_error() -> AssetRuntimeError {
        failure(
            "image.runtime-retention",
            "Runtime Image costing requires source-free pixels without attached retention owners",
        )
    }

    fn dimensions_error() -> AssetRuntimeError {
        failure(
            "image.runtime-dimensions",
            "Runtime Image dimensions or pixel count exceed the format limits",
        )
    }

    fn allocation_error() -> AssetRuntimeError {
        failure(
            "image.runtime-allocation",
            "Runtime Image output allocation could not be reserved",
        )
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests;
