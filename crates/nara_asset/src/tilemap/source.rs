use std::fmt;

use nara_core::Vec2;
use serde::{
    Deserialize, Deserializer,
    de::{Error, SeqAccess, Visitor},
};

use crate::image::SamplerDescriptor;

use super::{
    MAX_TILESET_SOURCE_BYTES, MAX_TILESET_TILES, TileAtlasLayout, TileDefinition, TileSetAsset,
    TileSetAssetError, validate_layout_and_count,
};

#[derive(Deserialize)]
#[serde(rename = "TileSet", deny_unknown_fields)]
struct TileSetSource {
    image: String,
    #[serde(with = "TileAtlasSource")]
    layout: TileAtlasLayout,
    #[serde(default)]
    sampler: SamplerDescriptor,
    #[serde(deserialize_with = "read_tiles")]
    tiles: Vec<TileDefinition>,
}

#[derive(Deserialize)]
#[serde(
    remote = "TileAtlasLayout",
    rename = "TileAtlasLayout",
    deny_unknown_fields
)]
struct TileAtlasSource {
    tile_size: Vec2,
    columns: u32,
    rows: u32,
    #[serde(default)]
    margin: Vec2,
    #[serde(default)]
    spacing: Vec2,
}

impl TileSetAsset {
    /// Parses a bounded `.tileset.ron` document with a quoted image UUID.
    /// The atlas margin and spacing default to zero and the sampler defaults to linear clamp.
    pub fn from_ron(source: &str) -> Result<Self, TileSetAssetError> {
        validate_source_len(source.len())?;
        let source: TileSetSource = ron::from_str(source)?;
        validate_layout_and_count(source.layout, source.tiles.len())?;
        Ok(Self {
            image: source.image.parse()?,
            layout: source.layout,
            sampler: source.sampler,
            tiles: source.tiles,
        })
    }
}

pub(super) fn validate_source_len(len: usize) -> Result<(), TileSetAssetError> {
    if len > MAX_TILESET_SOURCE_BYTES {
        return Err(TileSetAssetError::SourceTooLarge);
    }
    Ok(())
}

fn read_tiles<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<TileDefinition>, D::Error> {
    struct Tiles;

    impl<'de> Visitor<'de> for Tiles {
        type Value = Vec<TileDefinition>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "a list of at most {MAX_TILESET_TILES} tile definitions"
            )
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut tiles = Vec::new();
            while let Some(tile) = sequence.next_element()? {
                if tiles.len() == MAX_TILESET_TILES {
                    return Err(A::Error::custom(TileSetAssetError::TooManyTiles {
                        count: MAX_TILESET_TILES + 1,
                    }));
                }
                tiles.try_reserve(1).map_err(A::Error::custom)?;
                tiles.push(tile);
            }
            Ok(tiles)
        }
    }

    deserializer.deserialize_seq(Tiles)
}
