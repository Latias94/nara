use std::{error::Error, fmt::Display};

use nara_asset::{AssetRef, Handle};
use nara_core::{
    ByteLimit, Color, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
    PersistentFileContract, PersistentFileEnvelope, SerdeShapeLimits,
    decode_persistent_file_with_preflight, preflight_serde_shape,
};
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, SamplerDescriptor};

use crate::{TileAtlasLayout, TileSet};

pub const TILESET_FORMAT_KIND: &str = "tileset";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileSetSourceFileLimits {
    encoded_bytes: ByteLimit,
    shape: SerdeShapeLimits,
}

impl Default for TileSetSourceFileLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(64 * 1024).expect("tileset limit is non-zero"),
            shape: SerdeShapeLimits::new(
                DepthLimit::new(16).expect("tileset depth limit is non-zero"),
                ItemLimit::new(256).expect("tileset node limit is non-zero"),
                ItemLimit::new(64).expect("tileset item limit is non-zero"),
                ByteLimit::new(8 * 1024).expect("tileset string limit is non-zero"),
                ByteLimit::new(32 * 1024).expect("tileset total string limit is non-zero"),
            ),
        }
    }
}

impl TileSetSourceFileLimits {
    #[must_use]
    pub const fn encoded_bytes(self) -> ByteLimit {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn shape(self) -> SerdeShapeLimits {
        self.shape
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileSetSource {
    pub image: AssetRef,
    pub atlas: TileAtlasLayout,
    #[serde(default)]
    pub sampler: SamplerDescriptor,
    #[serde(default)]
    pub alpha_mode: AlphaMode2d,
    #[serde(default = "default_tint")]
    pub tint: Color,
}

impl TileSetSource {
    #[must_use]
    pub fn new(image: AssetRef, atlas: TileAtlasLayout) -> Self {
        Self {
            image,
            atlas,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::default(),
            tint: default_tint(),
        }
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

    #[must_use]
    pub const fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }

    pub fn into_tileset(
        self,
        image: Handle<ImageAsset>,
    ) -> Result<TileSet, TileSetSourceFormatError> {
        self.validate()?;
        Ok(TileSet::from_image(image, self.atlas)
            .with_sampler(self.sampler)
            .with_alpha_mode(self.alpha_mode)
            .with_tint(self.tint))
    }

    pub fn decode_ron_bytes(
        input: &[u8],
        limits: TileSetSourceFileLimits,
    ) -> Result<Self, TileSetSourceFormatError> {
        let contract = PersistentFileContract::canonical_v1(
            FormatKind::new(TILESET_FORMAT_KIND)
                .map_err(|error| TileSetSourceFormatError::Envelope(error.to_string()))?,
            current_engine_version(),
        );
        let envelope: PersistentFileEnvelope<TileSetSource> =
            decode_persistent_file_with_preflight(
                input,
                limits.encoded_bytes(),
                &contract,
                |bytes| {
                    let mut deserializer = ron::de::Deserializer::from_bytes(bytes)
                        .map_err(|error| error.to_string())?;
                    preflight_serde_shape(&mut deserializer, limits.shape())
                        .map_err(|error| error.to_string())
                },
                |bytes| ron::de::from_bytes(bytes).map_err(|error| error.to_string()),
                |bytes| ron::de::from_bytes(bytes).map_err(|error| error.to_string()),
            )
            .map_err(|error| TileSetSourceFormatError::Envelope(error.to_string()))?;
        let source = envelope.into_payload();
        source.validate()?;
        Ok(source)
    }

    pub fn encode_ron_bytes(&self) -> Result<String, TileSetSourceFormatError> {
        self.validate()?;
        let envelope = PersistentFileEnvelope::canonical_v1(
            FormatKind::new(TILESET_FORMAT_KIND)
                .map_err(|error| TileSetSourceFormatError::Envelope(error.to_string()))?,
            current_engine_version(),
            FormatGenerator::new("nara", current_engine_version())
                .map_err(|error| TileSetSourceFormatError::Envelope(error.to_string()))?,
            self,
        );
        ron::ser::to_string_pretty(&envelope, ron::ser::PrettyConfig::default())
            .map_err(|error| TileSetSourceFormatError::Envelope(error.to_string()))
    }

    fn validate(&self) -> Result<(), TileSetSourceFormatError> {
        if !self.atlas.is_valid() {
            return Err(TileSetSourceFormatError::InvalidAtlas);
        }
        if ![self.tint.r, self.tint.g, self.tint.b, self.tint.a]
            .iter()
            .all(|channel| channel.is_finite())
        {
            return Err(TileSetSourceFormatError::InvalidTint);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileSetSourceFormatError {
    Envelope(String),
    InvalidAtlas,
    InvalidTint,
}

impl Display for TileSetSourceFormatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Envelope(error) => formatter.write_str(error),
            Self::InvalidAtlas => formatter.write_str("tileset atlas layout is invalid"),
            Self::InvalidTint => formatter.write_str("tileset tint channels must be finite"),
        }
    }
}

impl Error for TileSetSourceFormatError {}

fn current_engine_version() -> EngineVersion {
    EngineVersion::parse(env!("CARGO_PKG_VERSION")).expect("workspace version is valid")
}

fn default_tint() -> Color {
    Color::WHITE
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_core::Vec2;

    #[test]
    fn source_envelope_round_trips() {
        let source = TileSetSource::new(
            AssetRef::path("textures/terrain.png").unwrap(),
            TileAtlasLayout::grid(Vec2::new(16.0, 16.0), 4, 2),
        );
        let encoded = source.encode_ron_bytes().unwrap();
        let decoded =
            TileSetSource::decode_ron_bytes(encoded.as_bytes(), TileSetSourceFileLimits::default())
                .unwrap();

        assert_eq!(decoded, source);
    }

    #[test]
    fn source_rejects_wrong_envelope_kind() {
        let source = TileSetSource::new(
            AssetRef::path("textures/terrain.png").unwrap(),
            TileAtlasLayout::grid(Vec2::new(16.0, 16.0), 4, 2),
        );
        let encoded = source
            .encode_ron_bytes()
            .unwrap()
            .replace("tileset", "scene");

        assert!(matches!(
            TileSetSource::decode_ron_bytes(encoded.as_bytes(), TileSetSourceFileLimits::default()),
            Err(TileSetSourceFormatError::Envelope(_))
        ));
    }

    #[test]
    fn source_rejects_non_finite_tint_at_each_boundary() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let source = TileSetSource::new(
                AssetRef::path("textures/terrain.png").unwrap(),
                TileAtlasLayout::grid(Vec2::ONE, 1, 1),
            )
            .with_tint(Color::rgba(invalid, 1.0, 1.0, 1.0));
            assert_eq!(
                source.encode_ron_bytes(),
                Err(TileSetSourceFormatError::InvalidTint)
            );
            assert_eq!(
                source
                    .clone()
                    .into_tileset(nara_asset::allocate_anonymous_handle().unwrap()),
                Err(TileSetSourceFormatError::InvalidTint),
            );
            let envelope = PersistentFileEnvelope::canonical_v1(
                FormatKind::new(TILESET_FORMAT_KIND).unwrap(),
                current_engine_version(),
                FormatGenerator::new("nara", current_engine_version()).unwrap(),
                source,
            );
            let encoded = ron::ser::to_string(&envelope).unwrap();

            assert_eq!(
                TileSetSource::decode_ron_bytes(
                    encoded.as_bytes(),
                    TileSetSourceFileLimits::default()
                ),
                Err(TileSetSourceFormatError::InvalidTint),
            );
        }
    }

    #[test]
    fn source_rejects_invalid_atlas_and_unsupported_version() {
        let source = TileSetSource::new(
            AssetRef::path("textures/terrain.png").unwrap(),
            TileAtlasLayout::grid(Vec2::ONE, 1, 1),
        );
        let encoded = source.encode_ron_bytes().unwrap();
        let future = encoded.replace("format_version: 1", "format_version: 2");
        assert_ne!(future, encoded);
        assert!(
            TileSetSource::decode_ron_bytes(future.as_bytes(), TileSetSourceFileLimits::default())
                .is_err()
        );
        let invalid = encoded.replace("columns: 1", "columns: 0");
        assert_ne!(invalid, encoded);
        assert_eq!(
            TileSetSource::decode_ron_bytes(invalid.as_bytes(), TileSetSourceFileLimits::default()),
            Err(TileSetSourceFormatError::InvalidAtlas),
        );
    }
}
