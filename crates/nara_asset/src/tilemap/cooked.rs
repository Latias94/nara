use nara_core::{AssetId, Vec2};
use uuid::Uuid;

use crate::image::{AddressMode, FilterMode, SamplerDescriptor};

use super::{
    TileAtlasLayout, TileDefinition, TileSetAsset, TileSetAssetError, validate_layout_and_count,
};

const MAGIC: &[u8; 4] = b"NTLS";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 64;
const TILE_BYTES: usize = 5;
const SOLID: u8 = 1;
const ONE_WAY: u8 = 2;

impl TileSetAsset {
    /// Encodes the deterministic NTLS v1 format. Numeric fields are little-endian.
    ///
    /// The 64-byte header holds `NTLS`, version `u16`, five sampler tags (min, mag, mipmap,
    /// U, V), one reserved zero byte, 16 UUID bytes, columns and rows as `u32`, and `tile_size`,
    /// `margin`, and `spacing` as pairs of `f32`. The final header field is the tile count `u32`.
    /// Filters use 0 for nearest and 1 for linear. Addresses use 0 for clamp, 1 for repeat,
    /// and 2 for mirror repeat. Each tile record is a flags byte (bit 0 solid, bit 1 one-way)
    /// followed by its `u32` collision group.
    pub fn encode_cooked(&self) -> Result<Vec<u8>, TileSetAssetError> {
        let count = self.tiles.len();
        validate_layout_and_count(self.layout, count)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(HEADER_BYTES + count * TILE_BYTES)?;
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&[
            filter_tag(self.sampler.min_filter),
            filter_tag(self.sampler.mag_filter),
            filter_tag(self.sampler.mipmap_filter),
            address_tag(self.sampler.address_mode_u),
            address_tag(self.sampler.address_mode_v),
            0,
        ]);
        bytes.extend_from_slice(self.image.as_uuid().as_bytes());
        bytes.extend_from_slice(&self.layout.columns.to_le_bytes());
        bytes.extend_from_slice(&self.layout.rows.to_le_bytes());
        for vector in [
            self.layout.tile_size,
            self.layout.margin,
            self.layout.spacing,
        ] {
            bytes.extend_from_slice(&vector.x.to_le_bytes());
            bytes.extend_from_slice(&vector.y.to_le_bytes());
        }
        bytes.extend_from_slice(&(count as u32).to_le_bytes());
        for tile in &self.tiles {
            bytes.push((u8::from(tile.solid) * SOLID) | (u8::from(tile.one_way) * ONE_WAY));
            bytes.extend_from_slice(&tile.collision_group.to_le_bytes());
        }
        Ok(bytes)
    }

    /// Decodes an NTLS v1 payload after checking its header and exact record extent.
    pub fn from_cooked(bytes: &[u8]) -> Result<Self, TileSetAssetError> {
        let (header, records) = bytes
            .split_first_chunk::<HEADER_BYTES>()
            .ok_or(TileSetAssetError::InvalidCooked("truncated header"))?;
        if &header[..4] != MAGIC {
            return Err(TileSetAssetError::InvalidCooked("incorrect magic"));
        }
        let version = u16::from_le_bytes(field(header, 4));
        if version != VERSION {
            return Err(TileSetAssetError::UnsupportedVersion(version));
        }
        if header[11] != 0 {
            return Err(TileSetAssetError::InvalidCooked("nonzero reserved byte"));
        }
        let sampler = SamplerDescriptor {
            min_filter: read_filter(header[6])?,
            mag_filter: read_filter(header[7])?,
            mipmap_filter: read_filter(header[8])?,
            address_mode_u: read_address(header[9])?,
            address_mode_v: read_address(header[10])?,
        };
        let image = AssetId::try_from_uuid(Uuid::from_bytes(field(header, 12)))?;
        let layout = TileAtlasLayout {
            columns: u32::from_le_bytes(field(header, 28)),
            rows: u32::from_le_bytes(field(header, 32)),
            tile_size: read_vec2(header, 36),
            margin: read_vec2(header, 44),
            spacing: read_vec2(header, 52),
        };
        let count = u32::from_le_bytes(field(header, 60)) as usize;
        validate_layout_and_count(layout, count)?;
        if records.len() != count * TILE_BYTES {
            return Err(TileSetAssetError::InvalidCooked(
                "incorrect tile data length",
            ));
        }

        let mut tiles = Vec::new();
        tiles.try_reserve_exact(count)?;
        for row in records.as_chunks::<TILE_BYTES>().0 {
            let flags = row[0];
            if flags & !(SOLID | ONE_WAY) != 0 {
                return Err(TileSetAssetError::InvalidCooked("unknown tile flags"));
            }
            tiles.push(TileDefinition {
                solid: flags & SOLID != 0,
                one_way: flags & ONE_WAY != 0,
                collision_group: u32::from_le_bytes(field(row, 1)),
            });
        }
        Ok(Self {
            image,
            layout,
            sampler,
            tiles,
        })
    }
}

fn field<const LEN: usize>(bytes: &[u8], offset: usize) -> [u8; LEN] {
    bytes[offset..offset + LEN]
        .try_into()
        .expect("fixed NTLS field")
}

fn read_vec2(header: &[u8; HEADER_BYTES], offset: usize) -> Vec2 {
    Vec2::new(
        f32::from_le_bytes(field(header, offset)),
        f32::from_le_bytes(field(header, offset + 4)),
    )
}

fn filter_tag(filter: FilterMode) -> u8 {
    match filter {
        FilterMode::Nearest => 0,
        FilterMode::Linear => 1,
    }
}

fn read_filter(tag: u8) -> Result<FilterMode, TileSetAssetError> {
    match tag {
        0 => Ok(FilterMode::Nearest),
        1 => Ok(FilterMode::Linear),
        _ => Err(TileSetAssetError::InvalidCooked("unknown sampler filter")),
    }
}

fn address_tag(address: AddressMode) -> u8 {
    match address {
        AddressMode::ClampToEdge => 0,
        AddressMode::Repeat => 1,
        AddressMode::MirrorRepeat => 2,
    }
}

fn read_address(tag: u8) -> Result<AddressMode, TileSetAssetError> {
    match tag {
        0 => Ok(AddressMode::ClampToEdge),
        1 => Ok(AddressMode::Repeat),
        2 => Ok(AddressMode::MirrorRepeat),
        _ => Err(TileSetAssetError::InvalidCooked(
            "unknown sampler address mode",
        )),
    }
}
