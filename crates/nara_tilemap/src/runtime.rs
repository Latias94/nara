//! Source-independent TileSet CPU content and its concrete processed Runtime format.

use std::{error::Error, fmt};

#[cfg(any(feature = "serde", test))]
use nara_asset::Handle;
use nara_asset::{AssetDomain, AssetProductRef, ImportedProductId};
use nara_core::Color;
#[cfg(any(feature = "serde", test))]
use nara_image::ImageAsset;
use nara_image::ImageExtent;
use nara_material::{AlphaMode2d, SamplerDescriptor};

#[cfg(any(feature = "serde", test))]
use crate::TileSet;
use crate::{TileAtlasLayout, atlas::matches_image_extent};

/// A complete TileSet CPU descriptor without source provenance or receiving Runtime handles.
///
/// Both the TileSet and its required Image name explicit primary or auxiliary Products. These
/// identities grant no content, dependency or publication authority. The content owner verifies
/// actual Image content and captures its receiving handle before lowering this descriptor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileSetDescriptor {
    product: AssetProductRef,
    image: AssetProductRef,
    atlas: TileAtlasLayout,
    sampler: SamplerDescriptor,
    alpha_mode: AlphaMode2d,
    tint: Color,
}

impl TileSetDescriptor {
    pub fn new(
        product: AssetProductRef,
        image: AssetProductRef,
        atlas: TileAtlasLayout,
    ) -> Result<Self, TileSetDescriptorError> {
        let descriptor = Self {
            product,
            image,
            atlas,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::default(),
            tint: Color::WHITE,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    #[must_use]
    pub const fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.sampler = sampler;
        self
    }

    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    pub fn with_tint(mut self, tint: Color) -> Result<Self, TileSetDescriptorError> {
        self.tint = tint;
        self.validate()?;
        Ok(self)
    }

    #[must_use]
    pub const fn product(&self) -> AssetProductRef {
        self.product
    }

    #[must_use]
    pub const fn image_product(&self) -> AssetProductRef {
        self.image
    }

    #[must_use]
    pub const fn atlas(&self) -> TileAtlasLayout {
        self.atlas
    }

    #[must_use]
    pub const fn sampler(&self) -> SamplerDescriptor {
        self.sampler
    }

    #[must_use]
    pub const fn alpha_mode(&self) -> AlphaMode2d {
        self.alpha_mode
    }

    #[must_use]
    pub const fn tint(&self) -> Color {
        self.tint
    }

    pub fn validate(&self) -> Result<(), TileSetDescriptorError> {
        if !self.atlas.is_valid() {
            return Err(TileSetDescriptorError::InvalidAtlas);
        }
        if ![self.tint.r, self.tint.g, self.tint.b, self.tint.a]
            .iter()
            .all(|channel| channel.is_finite())
        {
            return Err(TileSetDescriptorError::InvalidTint);
        }
        Ok(())
    }

    /// Requires exact atlas arithmetic and exact agreement with the rendered f32 extent.
    pub fn validate_image_extent(
        &self,
        image_extent: ImageExtent,
    ) -> Result<(), TileSetDescriptorError> {
        self.validate()?;
        matches_image_extent(self.atlas, image_extent)
            .then_some(())
            .ok_or(TileSetDescriptorError::ImageExtentMismatch)
    }

    /// Pure lowering after the content owner admits the actual required Image and its handle.
    #[cfg(any(feature = "serde", test))]
    pub(crate) fn into_tileset(
        self,
        image: Handle<ImageAsset>,
    ) -> Result<TileSet, TileSetDescriptorError> {
        self.validate()?;
        Ok(TileSet::from_image(image, self.atlas)
            .with_sampler(self.sampler)
            .with_alpha_mode(self.alpha_mode)
            .with_tint(self.tint))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileSetDescriptorError {
    InvalidAtlas,
    InvalidTint,
    ImageExtentMismatch,
}

impl fmt::Display for TileSetDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAtlas => "tileset atlas layout is invalid",
            Self::InvalidTint => "tileset tint channels must be finite",
            Self::ImageExtentMismatch => "tileset atlas extent does not match image extent",
        })
    }
}

impl Error for TileSetDescriptorError {}

/// One TileSet CPU Product, independent of source importing or native publication.
#[derive(Debug, Clone, Copy, Default)]
pub struct TileSetDomain;

impl AssetDomain for TileSetDomain {
    type Product = TileSetDescriptor;

    fn contains_product(product: &TileSetDescriptor, id: ImportedProductId) -> bool {
        product.product().product() == id
    }
}

/// Version 1 of the bounded `runtime_tileset` JSON format, using Core's persistent envelope.
///
/// Every record is an object with explicit fields. Atlas vectors retain their existing Vec2
/// two-element array representation. The payload contains only complete `product` and `image`
/// references, `atlas`, `sampler`, `alpha_mode` and `tint`. It carries no source paths, hashes,
/// cache facts or native handles. Decode reuses the actual atlas and material types and validates
/// the descriptor; actual Image dependency admission remains the captured content base's owner.
///
/// Encoded bytes are bounded to E = 4 KiB. Core's structural preflight bounds depth D = 8,
/// nodes N = 128, items per container = 8, each string = 256 bytes and total strings = 2 KiB.
/// Modeled work is `8 * E + 256 * N + 1024 * D + size_of::<TileSetDescriptor>()`, covering
/// structural preflight, the temporary JSON tree, header/payload decoding, encoding's counting
/// pass and the one required-Product visit. These are conservative logical reservations, not
/// allocator interception or RSS measurements. Encoding counts before allocating its output.
/// The descriptor owns no heaps; promised and actual retained bytes are both exactly
/// `size_of::<TileSetDescriptor>()`. Encoded input/output and Host/base custody are separate.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, Default)]
pub struct TileSetRuntimeCodec;

#[cfg(feature = "serde")]
mod codec {
    use std::{
        io::{self, Write},
        mem::size_of,
    };

    use nara_asset::{AssetRuntimeError, AssetRuntimeLimits, RuntimeAssetCodec};
    use nara_core::{
        ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
        PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
        PersistentFileEnvelope, PersistentFileHeader, SerdeShapeLimits,
        decode_persistent_file_with_preflight, preflight_serde_shape,
    };
    use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    use super::*;

    pub(super) const MAX_ENCODED_BYTES: usize = 4 * 1024;
    pub(super) const MAX_NODES: usize = 128;
    pub(super) const MAX_DEPTH: usize = 8;
    pub(super) const MAX_WORK_BYTES: usize =
        8 * MAX_ENCODED_BYTES + 256 * MAX_NODES + 1024 * MAX_DEPTH + size_of::<TileSetDescriptor>();
    const KIND: &str = "runtime_tileset";

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Payload {
        product: AssetProductRef,
        image: AssetProductRef,
        atlas: TileAtlasLayout,
        sampler: SamplerDescriptor,
        alpha_mode: AlphaMode2d,
        tint: Color,
    }

    impl RuntimeAssetCodec<TileSetDescriptor> for TileSetRuntimeCodec {
        const RUNTIME_KIND: &'static str = KIND;
        const RUNTIME_VERSION: u32 = 1;

        fn runtime_limits() -> AssetRuntimeLimits {
            AssetRuntimeLimits::new(
                byte_limit(MAX_ENCODED_BYTES),
                byte_limit(MAX_WORK_BYTES),
                byte_limit(size_of::<TileSetDescriptor>()),
            )
        }

        fn encode_runtime(product: &TileSetDescriptor) -> Result<Box<[u8]>, AssetRuntimeError> {
            product.validate().map_err(descriptor_error)?;
            let envelope = PersistentFileEnvelope::canonical_v1(
                kind()?,
                version("0.1.0")?,
                FormatGenerator::new("nara_tilemap", version(env!("CARGO_PKG_VERSION"))?)
                    .map_err(|_| metadata_error())?,
                Payload {
                    product: product.product(),
                    image: product.image_product(),
                    atlas: product.atlas(),
                    sampler: product.sampler(),
                    alpha_mode: product.alpha_mode(),
                    tint: product.tint(),
                },
            );
            let mut counter = EncodedCounter(0);
            serde_json::to_writer(&mut counter, &envelope).map_err(|_| metadata_error())?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(counter.0)
                .map_err(|_| allocation_error())?;
            serde_json::to_writer(&mut bytes, &envelope).map_err(|_| metadata_error())?;
            preflight(&bytes)?;
            Ok(bytes.into_boxed_slice())
        }

        fn decode_runtime(bytes: &[u8]) -> Result<TileSetDescriptor, AssetRuntimeError> {
            let contract =
                PersistentFileContract::canonical_v1(kind()?, version(env!("CARGO_PKG_VERSION"))?);
            let envelope = decode_persistent_file_with_preflight(
                bytes,
                byte_limit(MAX_ENCODED_BYTES),
                &contract,
                preflight,
                |bytes| {
                    serde_json::from_slice::<PersistentFileHeader>(bytes)
                        .map_err(|_| metadata_error())
                },
                |bytes| {
                    serde_json::from_slice::<PersistentFileEnvelope<Payload>>(bytes)
                        .map_err(|_| metadata_error())
                },
            )
            .map_err(decode_error)?;
            let payload = envelope.into_payload();
            TileSetDescriptor::new(payload.product, payload.image, payload.atlas)
                .and_then(|descriptor| {
                    descriptor
                        .with_sampler(payload.sampler)
                        .with_alpha_mode(payload.alpha_mode)
                        .with_tint(payload.tint)
                })
                .map_err(descriptor_error)
        }

        fn runtime_retained_bytes(product: &TileSetDescriptor) -> Result<usize, AssetRuntimeError> {
            product.validate().map_err(descriptor_error)?;
            Ok(size_of::<TileSetDescriptor>())
        }

        fn visit_required_runtime_products<E>(
            product: &TileSetDescriptor,
            mut visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
        ) -> Result<(), E> {
            visitor(product.image_product())
        }

        fn contains_runtime_product(
            product: &TileSetDescriptor,
            reference: AssetProductRef,
        ) -> bool {
            product.product() == reference
        }
    }

    struct EncodedCounter(usize);

    impl Write for EncodedCounter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|total| *total <= MAX_ENCODED_BYTES)
                .ok_or_else(|| {
                    io::Error::other("Runtime TileSet exceeds its encoded byte limit")
                })?;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn preflight(bytes: &[u8]) -> Result<(), AssetRuntimeError> {
        let limits = SerdeShapeLimits::new(
            DepthLimit::new(MAX_DEPTH).expect("fixed nonzero Runtime TileSet depth"),
            ItemLimit::new(MAX_NODES).expect("fixed nonzero Runtime TileSet node limit"),
            ItemLimit::new(8).expect("fixed nonzero Runtime TileSet container limit"),
            byte_limit(256),
            byte_limit(2 * 1024),
        );
        let mut parser = serde_json::Deserializer::from_slice(bytes);
        preflight_serde_shape(&mut parser, limits).map_err(|_| metadata_error())?;
        parser.end().map_err(|_| metadata_error())?;
        // Core rejects duplicates and bounds this tree before allocation. Keep explicit record
        // shape checks here because shared atlas/material structs accept unknown fields and
        // positional arrays through serde. Vec2's actual tuple representation remains unchanged.
        let value: Value = serde_json::from_slice(bytes).map_err(|_| metadata_error())?;
        for (pointer, fields) in [
            (
                "",
                &[
                    "kind",
                    "format_version",
                    "engine_min_version",
                    "generator",
                    "payload",
                ][..],
            ),
            ("/generator", &["name", "version"][..]),
            (
                "/payload",
                &["product", "image", "atlas", "sampler", "alpha_mode", "tint"][..],
            ),
            ("/payload/product", &["source", "product"][..]),
            ("/payload/image", &["source", "product"][..]),
            (
                "/payload/atlas",
                &["tile_size", "columns", "rows", "margin", "spacing"][..],
            ),
            (
                "/payload/sampler",
                &[
                    "min_filter",
                    "mag_filter",
                    "mipmap_filter",
                    "address_mode_u",
                    "address_mode_v",
                ][..],
            ),
            ("/payload/tint", &["r", "g", "b", "a"][..]),
        ] {
            let record = value
                .pointer(pointer)
                .and_then(Value::as_object)
                .ok_or_else(metadata_error)?;
            if record.len() != fields.len()
                || !fields.iter().all(|field| record.contains_key(*field))
            {
                return Err(metadata_error());
            }
        }
        // Product's own strict map reader owns the primary/auxiliary discriminant and fields.
        for pointer in ["/payload/product/product", "/payload/image/product"] {
            if !value.pointer(pointer).is_some_and(Value::is_object) {
                return Err(metadata_error());
            }
        }
        Ok(())
    }

    fn byte_limit(bytes: usize) -> ByteLimit {
        ByteLimit::new(bytes).expect("fixed nonzero Runtime TileSet byte limit")
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
                    "tileset.runtime-kind",
                    "Runtime TileSet requires the runtime_tileset file kind",
                ),
                PersistentFileContractError::UnsupportedFormatVersion { .. } => failure(
                    "tileset.runtime-version",
                    "Runtime TileSet requires format version one",
                ),
                PersistentFileContractError::EngineVersionTooOld { .. } => failure(
                    "tileset.runtime-engine-version",
                    "Runtime TileSet requires a newer engine version",
                ),
            },
        }
    }

    fn descriptor_error(error: TileSetDescriptorError) -> AssetRuntimeError {
        match error {
            TileSetDescriptorError::InvalidAtlas => failure(
                "tileset.runtime-atlas",
                "Runtime TileSet requires a valid atlas layout",
            ),
            TileSetDescriptorError::InvalidTint => failure(
                "tileset.runtime-tint",
                "Runtime TileSet requires finite tint channels",
            ),
            TileSetDescriptorError::ImageExtentMismatch => failure(
                "tileset.runtime-image-extent",
                "Runtime TileSet atlas does not exactly match its Image extent",
            ),
        }
    }

    fn metadata_error() -> AssetRuntimeError {
        failure(
            "tileset.runtime-metadata",
            "Runtime TileSet requires bounded strict JSON records",
        )
    }

    fn allocation_error() -> AssetRuntimeError {
        failure(
            "tileset.runtime-allocation",
            "Runtime TileSet output allocation could not be reserved",
        )
    }

    fn failure(code: &'static str, summary: &'static str) -> AssetRuntimeError {
        AssetRuntimeError::new(Diagnostic::error(
            DiagnosticCode::new(code).expect("fixed valid Runtime TileSet diagnostic code"),
            SafeSummary::new(summary).expect("fixed safe Runtime TileSet summary"),
        ))
    }
}

#[cfg(test)]
mod tests;
