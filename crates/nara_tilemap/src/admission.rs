use std::{error::Error, fmt, mem::size_of};

use nara_asset::AssetRef;
use nara_reflect::{ComponentCodecError, ComponentValue, asset_reference_decode_work_bytes};

use crate::{TileAtlasLayout, TileCoord, TileIndex, read_cell, read_optional_asset_ref};

type CellSelection = (TileCoord, TileIndex, usize);

/// Checked logical scratch reserved before atlas membership validation.
///
/// The validator allocates one exact-capacity selection vector, then sorts it in place to check
/// unique coordinates. The bound also covers resource-reference decoding, including an empty
/// cell list. It allocates no typed cell vector or tree. Encoded values and error diagnostics
/// remain under their existing owner bounds; this is not allocator/RSS accounting.
#[must_use]
pub fn tilemap_membership_work_bytes(value: &ComponentValue) -> Option<usize> {
    let Some(tileset) = value
        .get("tileset")
        .filter(|value| !matches!(value, ComponentValue::Null))
    else {
        return Some(0);
    };
    // Malformed wire records refuse before decoding their payload; diagnostics have a separate
    // owner bound. Valid-shaped but invalid identities still contribute their parser scratch.
    let reference_bytes = asset_reference_decode_work_bytes(tileset).unwrap_or(0);
    let entries = match value.get("cells") {
        Some(ComponentValue::List(entries)) => entries.len(),
        _ => 0,
    };
    if entries == 0 {
        return Some(reference_bytes);
    }
    entries
        .checked_mul(size_of::<CellSelection>())?
        .checked_add(size_of::<Vec<CellSelection>>())
        .and_then(|bytes| bytes.checked_add(reference_bytes))
}

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
    let entries = match value.get("cells") {
        None => &[][..],
        Some(ComponentValue::List(entries)) => entries.as_slice(),
        _ => {
            return Err(TilemapMembershipError::InvalidValue(
                ComponentCodecError::invalid_field("cells", "list"),
            ));
        }
    };
    if entries.is_empty() {
        return Ok(());
    }
    let mut cells = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let (coord, cell) = read_cell(entry).map_err(TilemapMembershipError::InvalidValue)?;
        cells.push((coord, cell.tile, index));
    }
    cells.sort_unstable_by_key(|(coord, _, index)| (*coord, *index));
    if let Some(pair) = cells.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(TilemapMembershipError::InvalidValue(
            ComponentCodecError::invalid_field(
                format!("cells[{}].coord", pair[1].2),
                "unique tile coordinate",
            ),
        ));
    }
    let atlas =
        resolve_atlas(&tileset).ok_or_else(|| TilemapMembershipError::TileSetUnavailable {
            tileset: tileset.clone(),
        })?;
    for (coord, tile, _) in cells {
        if !atlas.contains_tile(tile) {
            return Err(TilemapMembershipError::TileOutsideAtlas {
                tileset,
                coord,
                tile,
            });
        }
    }
    Ok(())
}
