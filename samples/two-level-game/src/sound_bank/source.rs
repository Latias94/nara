//! Bounded source descriptor, strict settings and tracked raw WAV dependencies.

use std::fmt;

use nara::{
    asset::{
        AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetPath,
        AssetProductRef, AssetSourceKind, SourceAssetBuild, SourceAssetImporter,
    },
    core::{DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape},
    diagnostic::Diagnostic,
};
use serde::{Deserialize, Deserializer, Serialize, de};

use super::{
    MAX_RETAINED_BYTES, MAX_SAMPLE_BYTES, MAX_SOURCE_BYTES, MAX_WAV_BYTES, Rejection, SoundBank,
    SoundBankCodec, SoundBankDomain, bank_product, byte_limit, wav,
};

/// The bank has no configurable import policy; its only accepted settings value is `{}`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SoundBankSettings {}

impl<'de> Deserialize<'de> for SoundBankSettings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EmptySettings;

        impl<'de> de::Visitor<'de> for EmptySettings {
            type Value = SoundBankSettings;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("exactly an empty JSON object")
            }

            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                if map.next_key::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("sound bank settings must be empty"));
                }
                Ok(SoundBankSettings {})
            }
        }

        deserializer.deserialize_map(EmptySettings)
    }
}

impl AssetDomain for SoundBankDomain {
    type Product = SoundBank;
}

impl SourceAssetImporter for SoundBankDomain {
    type Settings = SoundBankSettings;

    const SOURCE_KIND: &'static str = "two-level-game.sound-bank";

    /// Reserves metadata admission, bounded JSON preflight and overlapping Vec/Arc sample storage.
    /// Raw read buffers and their observation storage remain separately charged by the Host.
    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            byte_limit(MAX_SOURCE_BYTES),
            byte_limit(8 * 64 * 1024 + 8 * MAX_SOURCE_BYTES + 2 * MAX_SAMPLE_BYTES + 4096),
            byte_limit(MAX_RETAINED_BYTES),
        )
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<SoundBank, AssetImportError<E>> {
        let identity = source.metadata().identity();
        if identity.stable_id() != bank_product().source()
            || !matches!(identity.source_kind(), AssetSourceKind::Other(kind) if kind == Self::SOURCE_KIND)
        {
            return Err(import_error(Rejection::Identity));
        }
        source
            .metadata()
            .admit::<Self::Settings>()
            .map_err(|_| import_error(Rejection::Settings))?;
        let descriptor = decode_source(source.bytes()).map_err(import_error)?;
        let pickup = source.read_dependency(&descriptor.pickup, byte_limit(MAX_WAV_BYTES))?;
        let travel = source.read_dependency(&descriptor.travel, byte_limit(MAX_WAV_BYTES))?;
        let pickup = wav::preflight(&pickup).map_err(import_error)?;
        let travel = wav::preflight(&travel).map_err(import_error)?;
        Ok(SoundBank {
            product: bank_product(),
            pickup: wav::samples(pickup),
            travel: wav::samples(travel),
        })
    }
}

impl SourceAssetBuild for SoundBankDomain {
    type Codec = SoundBankCodec;

    const BUILD_ID: &'static str = "two-level-game.sound-bank.source-build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic> {
        Ok(settings)
    }

    fn visit_products<E>(
        product: &SoundBank,
        mut visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        visitor(product.product())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDescriptor {
    pickup: AssetPath,
    travel: AssetPath,
}

fn decode_source(bytes: &[u8]) -> Result<SourceDescriptor, Rejection> {
    if bytes.len() > MAX_SOURCE_BYTES
        || bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{')
    {
        return Err(Rejection::Source);
    }
    let shape = SerdeShapeLimits::new(
        DepthLimit::new(3).expect("fixed nonzero source depth limit"),
        ItemLimit::new(8).expect("fixed nonzero source node limit"),
        ItemLimit::new(2).expect("fixed nonzero source field limit"),
        byte_limit(128),
        byte_limit(512),
    );
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut parser, shape).map_err(|_| Rejection::Source)?;
    parser.end().map_err(|_| Rejection::Source)?;
    serde_json::from_slice(bytes).map_err(|_| Rejection::Source)
}

fn import_error<E>(error: Rejection) -> AssetImportError<E> {
    AssetImportError::domain(error.diagnostic())
}
