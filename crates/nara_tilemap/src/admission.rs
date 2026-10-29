use std::{error::Error, fmt};

use nara_asset::{AssetRef, AssetRefError};
use nara_reflect::{ComponentCodecError, ComponentValue};

use crate::{
    TileAtlasLayout, TileCoord, TileIndex, invalid_asset_ref, read_cells, read_optional_asset_ref,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TilemapMembershipError {
    InvalidValue(ComponentCodecError),
    TileSetUnavailable {
        tileset: AssetRef,
    },
    TileOutsideAtlas {
        tileset: AssetRef,
        coord: TileCoord,
        tile: TileIndex,
    },
}

impl fmt::Display for TilemapMembershipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue(error) => fmt::Display::fmt(error, formatter),
            Self::TileSetUnavailable { .. } => {
                formatter.write_str("tilemap tileset was not admitted")
            }
            Self::TileOutsideAtlas { .. } => {
                formatter.write_str("tilemap cell is outside its tileset atlas")
            }
        }
    }
}

impl Error for TilemapMembershipError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidValue(error) => Some(error),
            _ => None,
        }
    }
}

pub fn validate_persisted_tilemap_membership(
    value: &ComponentValue,
    resolve_atlas: impl FnOnce(&AssetRef) -> Option<TileAtlasLayout>,
) -> Result<(), TilemapMembershipError> {
    let Some(tileset) = read_optional_asset_ref(value.get("tileset"), "tileset")
        .map_err(TilemapMembershipError::InvalidValue)?
    else {
        return Ok(());
    };
    if let Some(reference) = tileset.as_product()
        && !tileset.is_primary()
    {
        return Err(TilemapMembershipError::InvalidValue(invalid_asset_ref(
            "tileset.value",
            &tileset,
            AssetRefError::UnsupportedProductSelection(reference),
        )));
    }
    let cells = read_cells(value.get("cells")).map_err(TilemapMembershipError::InvalidValue)?;
    if cells.is_empty() {
        return Ok(());
    }
    let atlas =
        resolve_atlas(&tileset).ok_or_else(|| TilemapMembershipError::TileSetUnavailable {
            tileset: tileset.clone(),
        })?;
    for (coord, cell) in cells {
        if !atlas.contains_tile(cell.tile) {
            return Err(TilemapMembershipError::TileOutsideAtlas {
                tileset,
                coord,
                tile: cell.tile,
            });
        }
    }
    Ok(())
}
