use std::{error::Error, fmt, mem::size_of};

use nara_asset::AssetRef;
use nara_reflect::{ComponentCodecError, ComponentValue, asset_reference_decode_work_bytes};

use crate::{TileAtlasLayout, TileCoord, TileIndex, read_cell, read_optional_asset_ref};

type CellSelection = (TileCoord, TileIndex, usize);

/// Checked logical scratch reserved before preparing atlas membership requirements.
///
/// Preparation allocates one exact-capacity selection vector, then sorts it in place to check
/// unique coordinates. The bound also covers resource-reference decoding, including an empty
/// cell list. It allocates no typed cell vector or tree. Callers retaining a returned requirement
/// separately account for its storage and any owned asset-reference data. Encoded values and
/// error diagnostics remain under their existing owner bounds; this is not allocator/RSS
/// accounting. The direct validator uses the same preparation and scratch bound.
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

/// A nonempty Tilemap's locally checked requirement for an actual TileSet atlas.
///
/// Preparation checks every cell and unique coordinate, then keeps only the highest tile and
/// its actual coordinate. Equal highest tiles select the smallest coordinate. Tile indices in
/// a valid atlas form a continuous prefix, so checking this witness proves membership of every
/// prepared cell. This value owns no cell storage and borrows no input data. Its asset reference
/// may own path data; it does not establish target existence or native publication authority.
#[derive(Debug)]
#[must_use = "the requirement must be checked against its actual tileset atlas"]
pub struct TilemapMembershipRequirement {
    tileset: AssetRef,
    highest_tile: TileIndex,
    coord: TileCoord,
}

impl TilemapMembershipRequirement {
    /// The complete reference whose actual atlas must satisfy this requirement.
    #[must_use]
    pub const fn asset_ref(&self) -> &AssetRef {
        &self.tileset
    }

    /// Checks actual atlas availability and membership without parsing the cells again.
    ///
    /// An unavailable atlas reports the original complete asset reference. An invalid atlas or
    /// out-of-range tile reports the highest actual tile at its smallest coordinate, rather than
    /// the first out-of-range cell in coordinate order.
    pub fn validate_atlas(
        &self,
        atlas: Option<TileAtlasLayout>,
    ) -> Result<(), TilemapMembershipError> {
        let atlas = atlas.ok_or_else(|| TilemapMembershipError::TileSetUnavailable {
            tileset: self.tileset.clone(),
        })?;
        if !atlas.contains_tile(self.highest_tile) {
            return Err(TilemapMembershipError::TileOutsideAtlas {
                tileset: self.tileset.clone(),
                coord: self.coord,
                tile: self.highest_tile,
            });
        }
        Ok(())
    }
}

/// Parses and checks local Tilemap cells without resolving or retaining an atlas.
///
/// Reserve [`tilemap_membership_work_bytes`] before calling. A missing TileSet reference or an
/// empty cell list produces no requirement, preserving the direct validator's empty-map behavior.
/// A present reference is decoded even for an empty cell list. All nonempty cells are parsed and
/// checked for duplicate coordinates before the temporary selection vector is released. Callers
/// retaining the compact result separately admit its storage and owned reference data.
pub fn prepare_persisted_tilemap_membership(
    value: &ComponentValue,
) -> Result<Option<TilemapMembershipRequirement>, TilemapMembershipError> {
    let Some(tileset) = read_optional_asset_ref(value.get("tileset"), "tileset")
        .map_err(TilemapMembershipError::InvalidValue)?
    else {
        return Ok(None);
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
        return Ok(None);
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
    let &(mut coord, mut highest_tile, _) = cells
        .first()
        .expect("a nonempty cell list produces a nonempty selection");
    // Coordinate order makes the first occurrence of a highest tile its smallest coordinate.
    for &(candidate_coord, candidate_tile, _) in cells.iter().skip(1) {
        if candidate_tile > highest_tile {
            coord = candidate_coord;
            highest_tile = candidate_tile;
        }
    }
    Ok(Some(TilemapMembershipRequirement {
        tileset,
        highest_tile,
        coord,
    }))
}

/// Prepares local membership once, then checks the actual atlas supplied by the content owner.
///
/// The resolver is not called for an empty map or invalid local cells. Scratch admission uses
/// [`tilemap_membership_work_bytes`], as for [`prepare_persisted_tilemap_membership`].
pub fn validate_persisted_tilemap_membership(
    value: &ComponentValue,
    resolve_atlas: impl FnOnce(&AssetRef) -> Option<TileAtlasLayout>,
) -> Result<(), TilemapMembershipError> {
    let Some(requirement) = prepare_persisted_tilemap_membership(value)? else {
        return Ok(());
    };
    requirement.validate_atlas(resolve_atlas(requirement.asset_ref()))
}
