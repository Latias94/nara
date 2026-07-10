//! Tilemap authoring data for 2D scenes.

use std::collections::BTreeMap;

use nara_app::{App, Plugin, PluginError};
use nara_asset::{AssetRef, AssetRefError, AssetServer, AssetSourceKind, Assets, Handle};
use nara_core::{Color, Vec2};
use nara_ecs::{Component, World};
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_reflect::{
    ComponentCodecError, ComponentDecodeContext, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentRegistryError, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, PreparedComponent,
};

pub const DEFAULT_TILE_SIZE: Vec2 = Vec2::new(16.0, 16.0);
pub const DEFAULT_CHUNK_SIZE: i32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn chunk(self) -> TileChunkCoord {
        self.chunk_with_size(DEFAULT_CHUNK_SIZE)
    }

    #[must_use]
    pub fn chunk_with_size(self, chunk_size: i32) -> TileChunkCoord {
        TileChunkCoord::from_tile_coord(self, chunk_size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileChunkCoord {
    pub x: i32,
    pub y: i32,
}

impl TileChunkCoord {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn from_tile_coord(coord: TileCoord, chunk_size: i32) -> Self {
        assert!(chunk_size > 0, "tile chunk size must be positive");
        Self {
            x: coord.x.div_euclid(chunk_size),
            y: coord.y.div_euclid(chunk_size),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileIndex(pub u32);

impl TileIndex {
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileCell {
    pub tile: TileIndex,
    pub color: Color,
}

impl TileCell {
    #[must_use]
    pub const fn new(tile: TileIndex) -> Self {
        Self {
            tile,
            color: Color::WHITE,
        }
    }

    #[must_use]
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileLayer {
    pub index: i32,
}

impl TileLayer {
    #[must_use]
    pub const fn new(index: i32) -> Self {
        Self { index }
    }
}

impl Default for TileLayer {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileAtlasLayout {
    pub tile_size: Vec2,
    pub columns: u32,
    pub rows: u32,
    pub margin: Vec2,
    pub spacing: Vec2,
}

impl TileAtlasLayout {
    #[must_use]
    pub fn new(tile_size: Vec2, columns: u32, rows: u32) -> Option<Self> {
        let layout = Self {
            tile_size,
            columns,
            rows,
            margin: Vec2::ZERO,
            spacing: Vec2::ZERO,
        };
        layout.is_valid().then_some(layout)
    }

    #[must_use]
    pub fn grid(tile_size: Vec2, columns: u32, rows: u32) -> Self {
        Self::new(tile_size, columns, rows).expect("tile atlas layout must be valid")
    }

    #[must_use]
    pub fn with_margin(mut self, margin: Vec2) -> Self {
        self.margin = margin;
        assert!(self.is_valid(), "tile atlas margin must be valid");
        self
    }

    #[must_use]
    pub fn with_spacing(mut self, spacing: Vec2) -> Self {
        self.spacing = spacing;
        assert!(self.is_valid(), "tile atlas spacing must be valid");
        self
    }

    #[must_use]
    pub fn atlas_size(self) -> Vec2 {
        Vec2::new(
            self.margin
                .x
                .mul_add(2.0, self.tile_size.x * self.columns as f32)
                + self.spacing.x * self.columns.saturating_sub(1) as f32,
            self.margin
                .y
                .mul_add(2.0, self.tile_size.y * self.rows as f32)
                + self.spacing.y * self.rows.saturating_sub(1) as f32,
        )
    }

    #[must_use]
    pub fn normalized_region(self, tile: TileIndex) -> Option<TileAtlasRegion> {
        if !self.is_valid() {
            return None;
        }
        let raw = tile.raw();
        let x = raw % self.columns;
        let y = raw / self.columns;
        if y >= self.rows {
            return None;
        }

        let atlas_size = self.atlas_size();
        let min = Vec2::new(
            self.margin.x + (self.tile_size.x + self.spacing.x) * x as f32,
            self.margin.y + (self.tile_size.y + self.spacing.y) * y as f32,
        ) / atlas_size;
        let size = self.tile_size / atlas_size;
        Some(TileAtlasRegion { min, size })
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let atlas_size = self.atlas_size();
        self.columns > 0
            && self.rows > 0
            && self.tile_size.is_finite()
            && self.margin.is_finite()
            && self.spacing.is_finite()
            && self.tile_size.x > 0.0
            && self.tile_size.y > 0.0
            && self.margin.x >= 0.0
            && self.margin.y >= 0.0
            && self.spacing.x >= 0.0
            && self.spacing.y >= 0.0
            && atlas_size.is_finite()
            && atlas_size.x > 0.0
            && atlas_size.y > 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileAtlasRegion {
    pub min: Vec2,
    pub size: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileSet {
    pub tile_size: Vec2,
    pub material: TileSetMaterial,
    pub atlas: Option<TileAtlasLayout>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileSetMaterial {
    pub image: Option<Handle<ImageAsset>>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: Color,
}

impl TileSetMaterial {
    #[must_use]
    pub fn from_color(tint: Color) -> Self {
        Self {
            image: None,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint,
        }
    }

    #[must_use]
    pub fn from_image(image: Handle<ImageAsset>) -> Self {
        Self {
            image: Some(image),
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint: Color::WHITE,
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
}

impl Default for TileSetMaterial {
    fn default() -> Self {
        Self::from_color(Color::WHITE)
    }
}

impl TileSet {
    #[must_use]
    pub fn new(tile_size: Vec2) -> Self {
        Self {
            tile_size,
            material: TileSetMaterial::default(),
            atlas: None,
        }
    }

    #[must_use]
    pub fn from_image(image: Handle<ImageAsset>, atlas: TileAtlasLayout) -> Self {
        Self {
            tile_size: atlas.tile_size,
            material: TileSetMaterial::from_image(image),
            atlas: Some(atlas),
        }
    }

    #[must_use]
    pub fn with_image(mut self, image: Handle<ImageAsset>, atlas: TileAtlasLayout) -> Self {
        self.tile_size = atlas.tile_size;
        self.material.image = Some(image);
        self.atlas = Some(atlas);
        self
    }

    #[must_use]
    pub const fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.material.sampler = sampler;
        self
    }

    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.material.alpha_mode = alpha_mode;
        self
    }

    #[must_use]
    pub const fn with_tint(mut self, tint: Color) -> Self {
        self.material.tint = tint;
        self
    }

    #[must_use]
    pub fn normalized_region(self, tile: TileIndex) -> Option<TileAtlasRegion> {
        let _image = self.material.image?;
        self.atlas?.normalized_region(tile)
    }
}

impl Default for TileSet {
    fn default() -> Self {
        Self::new(DEFAULT_TILE_SIZE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirtyTileChunk {
    pub coord: TileChunkCoord,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Component)]
pub struct Tilemap {
    pub tileset: Option<Handle<TileSet>>,
    pub tile_size: Vec2,
    pub layer: TileLayer,
    pub sort_key: i32,
    cells: BTreeMap<TileCoord, TileCell>,
    dirty_chunks: BTreeMap<TileChunkCoord, u64>,
    next_dirty_revision: u64,
}

impl Tilemap {
    #[must_use]
    pub fn new(tile_size: Vec2) -> Self {
        Self {
            tileset: None,
            tile_size,
            layer: TileLayer::default(),
            sort_key: 0,
            cells: BTreeMap::new(),
            dirty_chunks: BTreeMap::new(),
            next_dirty_revision: 1,
        }
    }

    #[must_use]
    pub fn with_tileset(mut self, tileset: Handle<TileSet>) -> Self {
        self.tileset = Some(tileset);
        self
    }

    #[must_use]
    pub fn with_layer(mut self, layer: i32) -> Self {
        self.layer = TileLayer::new(layer);
        self
    }

    #[must_use]
    pub fn with_sort_key(mut self, sort_key: i32) -> Self {
        self.sort_key = sort_key;
        self
    }

    pub fn set_cell(&mut self, coord: TileCoord, cell: TileCell) -> Option<TileCell> {
        let previous = self.cells.insert(coord, cell);
        self.mark_dirty(coord);
        previous
    }

    pub fn remove_cell(&mut self, coord: TileCoord) -> Option<TileCell> {
        let previous = self.cells.remove(&coord);
        if previous.is_some() {
            self.mark_dirty(coord);
        }
        previous
    }

    #[must_use]
    pub fn get_cell(&self, coord: TileCoord) -> Option<&TileCell> {
        self.cells.get(&coord)
    }

    pub fn cells(&self) -> impl Iterator<Item = (TileCoord, &TileCell)> + '_ {
        self.cells.iter().map(|(coord, cell)| (*coord, cell))
    }

    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn dirty_chunks(&self) -> impl Iterator<Item = DirtyTileChunk> + '_ {
        self.dirty_chunks
            .iter()
            .map(|(coord, revision)| DirtyTileChunk {
                coord: *coord,
                revision: *revision,
            })
    }

    pub fn clear_dirty_chunks(&mut self) {
        self.dirty_chunks.clear();
    }

    fn mark_dirty(&mut self, coord: TileCoord) {
        let revision = self.next_dirty_revision;
        self.next_dirty_revision = self.next_dirty_revision.saturating_add(1);
        self.dirty_chunks.insert(coord.chunk(), revision);
    }
}

impl Default for Tilemap {
    fn default() -> Self {
        Self::new(DEFAULT_TILE_SIZE)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TilemapPlugin;

impl Plugin for TilemapPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.tilemap"),
            nara_app::PluginCategory::Runtime,
        )
    }

    fn preflight(&self, app: &App) -> Result<(), PluginError> {
        let Some(registry) = app.world().get_resource::<ComponentRegistry>() else {
            return Ok(());
        };
        let component_id = ComponentTypeId::new("nara.tilemap.Tilemap");
        registry
            .validate_component_registration::<Tilemap>(&component_id)
            .map_err(|error| {
                PluginError::component_registration(self.plugin_id(), component_id.as_str(), error)
            })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Assets<TileSet>>()?;
        app.init_resource::<ComponentRegistry>()?;
        let component_id = ComponentTypeId::new("nara.tilemap.Tilemap");
        register_tilemap_components(&mut app.world_mut()?.resource_mut::<ComponentRegistry>())
            .map_err(|error| {
                PluginError::component_registration(self.plugin_id(), component_id.as_str(), error)
            })?;
        Ok(())
    }
}

pub fn register_tilemap_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let component_id = ComponentTypeId::new("nara.tilemap.Tilemap");
    registry.register_component_codec_with_context_and_fields::<Tilemap, _, _>(
        component_id.clone(),
        ComponentSchemaVersion(1),
        tilemap_fields(),
        |value, context| {
            let tile_size = read_vec2(value.field("tile_size")?, "tile_size")?;
            let layer = optional_i32(value, "layer")?.unwrap_or(0);
            let sort_key = optional_i32(value, "sort_key")?.unwrap_or(0);
            let tileset_ref = read_optional_asset_ref(value.get("tileset"), "tileset")?;
            let tileset = prepare_optional_tileset(context, tileset_ref)?;
            let cells = read_cells(value.get("cells"))?;

            Ok(PreparedComponent::new(move |world, entity| {
                let tileset = resolve_prepared_tileset(world, tileset)?;
                let mut tilemap = Tilemap::new(tile_size)
                    .with_layer(layer)
                    .with_sort_key(sort_key);
                if let Some(tileset) = tileset {
                    tilemap = tilemap.with_tileset(tileset);
                }
                for (coord, cell) in cells {
                    tilemap.set_cell(coord, cell);
                }

                let mut entity_mut = world
                    .get_entity_mut(entity)
                    .map_err(|_| ComponentCodecError::EntityMissing)?;
                entity_mut.insert(tilemap);
                Ok(())
            }))
        },
        |world, entity, context| {
            let Some(tilemap) = world.get::<Tilemap>(entity) else {
                return Ok(None);
            };
            let tileset = match tilemap.tileset {
                Some(handle) => Some(asset_ref_value(
                    &AssetRef::from_handle_with_policy(
                        world.get_resource::<AssetServer>().ok_or_else(|| {
                            ComponentCodecError::Message(
                                "AssetServer resource is missing".to_string(),
                            )
                        })?,
                        handle,
                        context.asset_ref_export_policy(),
                    )
                    .map_err(|error| ComponentCodecError::Message(error.to_string()))?,
                )?),
                None => None,
            };

            Ok(Some(ComponentValue::map([
                ("tile_size", vec2_value(tilemap.tile_size)?),
                ("layer", ComponentValue::I64(i64::from(tilemap.layer.index))),
                ("sort_key", ComponentValue::I64(i64::from(tilemap.sort_key))),
                ("tileset", tileset.unwrap_or(ComponentValue::Null)),
                ("cells", cells_value(tilemap)?),
            ])))
        },
    )?;
    Ok(())
}

fn tilemap_fields() -> [ComponentFieldSchema; 6] {
    [
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["tile_size", "x"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["tile_size", "y"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["layer"]),
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["sort_key"]),
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["tileset"]),
            ComponentValueKind::AssetRef,
            ComponentValue::Null,
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["cells"]),
            ComponentValueKind::List,
            ComponentValue::List(Vec::new()),
        ),
    ]
}

enum PreparedTileset {
    Resolved(Handle<TileSet>),
    Deferred(AssetRef),
}

fn prepare_optional_tileset(
    context: &mut ComponentDecodeContext<'_>,
    tileset_ref: Option<AssetRef>,
) -> Result<Option<PreparedTileset>, ComponentCodecError> {
    let Some(tileset_ref) = tileset_ref else {
        return Ok(None);
    };
    prepare_tileset_handle(context, "tileset.value", tileset_ref).map(Some)
}

fn prepare_tileset_handle(
    context: &mut ComponentDecodeContext<'_>,
    field: &str,
    asset_ref: AssetRef,
) -> Result<PreparedTileset, ComponentCodecError> {
    let expected_source_kind = tileset_asset_source_kind();
    if let Some(result) =
        context.resolve_asset_ref_with_kind::<TileSet>(&asset_ref, &expected_source_kind)
    {
        return result
            .map(PreparedTileset::Resolved)
            .map_err(|error| invalid_asset_ref(field, &asset_ref, error));
    }

    if let Some(result) = context.validate_asset_ref_with_kind(&asset_ref, &expected_source_kind) {
        return match result {
            Ok(()) => Ok(PreparedTileset::Deferred(asset_ref)),
            Err(error) => Err(invalid_asset_ref(field, &asset_ref, error)),
        };
    }

    if let Some(stable_id) = asset_ref.as_stable_id() {
        let Some(database) = context.project_asset_database() else {
            return Err(invalid_asset_ref(
                field,
                &asset_ref,
                AssetRefError::MissingProjectDatabase(stable_id),
            ));
        };
        database.resolve_ref(&asset_ref).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(field, asset_ref.to_string(), error.to_string())
        })?;
    }

    Ok(PreparedTileset::Deferred(asset_ref))
}

fn tileset_asset_source_kind() -> AssetSourceKind {
    AssetSourceKind::Other("tileset".to_string())
}

fn resolve_prepared_tileset(
    world: &mut World,
    tileset: Option<PreparedTileset>,
) -> Result<Option<Handle<TileSet>>, ComponentCodecError> {
    match tileset {
        None => Ok(None),
        Some(PreparedTileset::Resolved(handle)) => Ok(Some(handle)),
        Some(PreparedTileset::Deferred(tileset_ref)) => {
            resolve_optional_tileset(world, Some(&tileset_ref))
        }
    }
}

fn resolve_optional_tileset(
    world: &mut World,
    tileset_ref: Option<&AssetRef>,
) -> Result<Option<Handle<TileSet>>, ComponentCodecError> {
    let Some(tileset_ref) = tileset_ref else {
        return Ok(None);
    };
    if world.get_resource::<AssetServer>().is_none() {
        world.insert_resource(AssetServer::new());
    }
    tileset_ref
        .resolve::<TileSet>(&mut world.resource_mut::<AssetServer>())
        .map(Some)
        .map_err(|error| invalid_asset_ref("tileset.value", tileset_ref, error))
}

fn invalid_asset_ref(
    field: &str,
    asset_ref: &AssetRef,
    error: AssetRefError,
) -> ComponentCodecError {
    ComponentCodecError::invalid_asset_ref(field, asset_ref.to_string(), error.to_string())
}

fn optional_i32(value: &ComponentValue, field: &str) -> Result<Option<i32>, ComponentCodecError> {
    value
        .get(field)
        .map(|value| {
            let value = value
                .as_i64()
                .ok_or_else(|| ComponentCodecError::invalid_field(field, "i32"))?;
            i32::try_from(value).map_err(|_| ComponentCodecError::invalid_field(field, "i32"))
        })
        .transpose()
}

fn read_vec2(value: &ComponentValue, field: &str) -> Result<Vec2, ComponentCodecError> {
    Ok(Vec2::new(
        read_f32(value.field("x")?, &format!("{field}.x"))?,
        read_f32(value.field("y")?, &format!("{field}.y"))?,
    ))
}

fn read_f32(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f32"))?;
    if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(ComponentCodecError::invalid_field(field, "finite f32"));
    }
    Ok(value as f32)
}

fn vec2_value(value: Vec2) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("x", ComponentValue::f64(f64::from(value.x))?),
        ("y", ComponentValue::f64(f64::from(value.y))?),
    ]))
}

fn read_color(value: &ComponentValue, field: &str) -> Result<Color, ComponentCodecError> {
    Ok(Color::rgba(
        read_f32(value.field("r")?, &format!("{field}.r"))?,
        read_f32(value.field("g")?, &format!("{field}.g"))?,
        read_f32(value.field("b")?, &format!("{field}.b"))?,
        read_f32(value.field("a")?, &format!("{field}.a"))?,
    ))
}

fn color_value(value: Color) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("r", ComponentValue::f64(f64::from(value.r))?),
        ("g", ComponentValue::f64(f64::from(value.g))?),
        ("b", ComponentValue::f64(f64::from(value.b))?),
        ("a", ComponentValue::f64(f64::from(value.a))?),
    ]))
}

fn read_cells(
    value: Option<&ComponentValue>,
) -> Result<Vec<(TileCoord, TileCell)>, ComponentCodecError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let ComponentValue::List(entries) = value else {
        return Err(ComponentCodecError::invalid_field("cells", "list"));
    };
    entries
        .iter()
        .map(|entry| {
            let coord = entry.field("coord")?;
            let cell = entry.field("cell")?;
            Ok((
                TileCoord::new(
                    read_i32(coord, "x", "cells[].coord.x")?,
                    read_i32(coord, "y", "cells[].coord.y")?,
                ),
                TileCell::new(TileIndex::new(read_u32(cell, "tile", "cells[].cell.tile")?))
                    .with_color(read_color(cell.field("color")?, "cell.color")?),
            ))
        })
        .collect()
}

fn read_i32(
    value: &ComponentValue,
    field: &str,
    display_field: &str,
) -> Result<i32, ComponentCodecError> {
    let value = value.field_i64(field)?;
    i32::try_from(value).map_err(|_| ComponentCodecError::invalid_field(display_field, "i32"))
}

fn read_u32(
    value: &ComponentValue,
    field: &str,
    display_field: &str,
) -> Result<u32, ComponentCodecError> {
    let value = value.field_u64(field)?;
    u32::try_from(value).map_err(|_| ComponentCodecError::invalid_field(display_field, "u32"))
}

fn cells_value(tilemap: &Tilemap) -> Result<ComponentValue, ComponentCodecError> {
    let cells = tilemap
        .cells()
        .map(|(coord, cell)| {
            Ok(ComponentValue::map([
                (
                    "coord",
                    ComponentValue::map([
                        ("x", ComponentValue::I64(i64::from(coord.x))),
                        ("y", ComponentValue::I64(i64::from(coord.y))),
                    ]),
                ),
                (
                    "cell",
                    ComponentValue::map([
                        ("tile", ComponentValue::U64(u64::from(cell.tile.raw()))),
                        ("color", color_value(cell.color)?),
                    ]),
                ),
            ]))
        })
        .collect::<Result<Vec<_>, ComponentCodecError>>()?;
    Ok(ComponentValue::List(cells))
}

fn read_optional_asset_ref(
    value: Option<&ComponentValue>,
    field: &str,
) -> Result<Option<AssetRef>, ComponentCodecError> {
    match value {
        None | Some(ComponentValue::Null) => Ok(None),
        Some(value) => read_asset_ref(value, field).map(Some),
    }
}

fn read_asset_ref(value: &ComponentValue, field: &str) -> Result<AssetRef, ComponentCodecError> {
    match value.field_str("kind")? {
        "path" => AssetRef::path(value.field_str("value")?).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                value.field_str("value").unwrap_or_default(),
                error.to_string(),
            )
        }),
        "stable_id" => AssetRef::stable_id(value.field_str("value")?).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                value.field_str("value").unwrap_or_default(),
                error.to_string(),
            )
        }),
        _ => Err(ComponentCodecError::invalid_field(
            format!("{field}.kind"),
            "'path' or 'stable_id'",
        )),
    }
}

fn asset_ref_value(asset_ref: &AssetRef) -> Result<ComponentValue, ComponentCodecError> {
    match asset_ref {
        AssetRef::Path(path) => Ok(ComponentValue::map([
            ("kind", ComponentValue::String("path".to_string())),
            ("value", ComponentValue::String(path.as_str().to_string())),
        ])),
        AssetRef::StableId(id) => Ok(ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_string())),
            ("value", ComponentValue::String(id.to_string())),
        ])),
    }
}

pub mod prelude {
    pub use crate::{
        DEFAULT_CHUNK_SIZE, DEFAULT_TILE_SIZE, DirtyTileChunk, TileAtlasLayout, TileAtlasRegion,
        TileCell, TileChunkCoord, TileCoord, TileIndex, TileLayer, TileSet, TileSetMaterial,
        Tilemap, TilemapPlugin,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::{
        AssetId, AssetPath, AssetRecord, AssetSourceKind, ProjectAssetDatabase, StableAssetId,
    };
    use nara_reflect::ComponentDecodeContext;

    #[test]
    fn tile_coordinates_floor_divide_negative_chunks() {
        assert_eq!(TileCoord::new(0, 0).chunk(), TileChunkCoord::new(0, 0));
        assert_eq!(TileCoord::new(31, 31).chunk(), TileChunkCoord::new(0, 0));
        assert_eq!(TileCoord::new(32, 32).chunk(), TileChunkCoord::new(1, 1));
        assert_eq!(TileCoord::new(-1, -1).chunk(), TileChunkCoord::new(-1, -1));
        assert_eq!(
            TileCoord::new(-32, -32).chunk(),
            TileChunkCoord::new(-1, -1)
        );
        assert_eq!(
            TileCoord::new(-33, -33).chunk(),
            TileChunkCoord::new(-2, -2)
        );
    }

    #[test]
    fn setting_same_coordinate_replaces_cell_and_marks_chunk_dirty() {
        let mut tilemap = Tilemap::default();
        let coord = TileCoord::new(2, 3);
        let first = TileCell::new(TileIndex::new(1));
        let second = TileCell::new(TileIndex::new(2));

        assert_eq!(tilemap.set_cell(coord, first), None);
        assert_eq!(tilemap.set_cell(coord, second), Some(first));

        assert_eq!(tilemap.get_cell(coord), Some(&second));
        assert_eq!(
            tilemap.dirty_chunks().collect::<Vec<_>>(),
            vec![DirtyTileChunk {
                coord: TileChunkCoord::new(0, 0),
                revision: 2,
            }]
        );
    }

    #[test]
    fn removing_cell_marks_affected_chunk_dirty() {
        let mut tilemap = Tilemap::default();
        let coord = TileCoord::new(-1, 0);
        let cell = TileCell::new(TileIndex::new(7));

        tilemap.set_cell(coord, cell);
        tilemap.clear_dirty_chunks();

        assert_eq!(tilemap.remove_cell(coord), Some(cell));
        assert!(tilemap.get_cell(coord).is_none());
        assert_eq!(
            tilemap.dirty_chunks().collect::<Vec<_>>(),
            vec![DirtyTileChunk {
                coord: TileChunkCoord::new(-1, 0),
                revision: 2,
            }]
        );
    }

    #[test]
    fn empty_tilemaps_are_allowed() {
        let tilemap = Tilemap::default();

        assert!(tilemap.is_empty());
        assert_eq!(tilemap.cell_count(), 0);
        assert_eq!(tilemap.cells().count(), 0);
        assert_eq!(tilemap.dirty_chunks().count(), 0);
    }

    #[test]
    fn iteration_order_is_deterministic() {
        let mut tilemap = Tilemap::default();
        tilemap.set_cell(TileCoord::new(2, 0), TileCell::new(TileIndex::new(2)));
        tilemap.set_cell(TileCoord::new(-1, 3), TileCell::new(TileIndex::new(1)));
        tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));

        let coords = tilemap.cells().map(|(coord, _)| coord).collect::<Vec<_>>();

        assert_eq!(
            coords,
            vec![
                TileCoord::new(-1, 3),
                TileCoord::new(0, 0),
                TileCoord::new(2, 0),
            ]
        );
    }

    #[test]
    fn records_tileset_layer_and_sort_controls() {
        let tileset = Handle::new(AssetId::from_raw(11));
        let tilemap = Tilemap::new(Vec2::new(8.0, 8.0))
            .with_tileset(tileset)
            .with_layer(4)
            .with_sort_key(-9);

        assert_eq!(tilemap.tileset, Some(tileset));
        assert_eq!(tilemap.tile_size, Vec2::new(8.0, 8.0));
        assert_eq!(tilemap.layer, TileLayer::new(4));
        assert_eq!(tilemap.sort_key, -9);
    }

    #[test]
    fn tileset_records_image_handle_and_atlas_regions() {
        let image = Handle::new(AssetId::from_raw(31));
        let atlas = TileAtlasLayout::grid(Vec2::new(16.0, 8.0), 4, 2);
        let tileset = TileSet::from_image(image, atlas);

        assert_eq!(tileset.material.image, Some(image));
        assert_eq!(tileset.material.sampler, SamplerDescriptor::default());
        assert_eq!(tileset.material.alpha_mode, AlphaMode2d::Blend);
        assert_eq!(tileset.material.tint, Color::WHITE);
        assert_eq!(tileset.tile_size, Vec2::new(16.0, 8.0));
        assert_eq!(
            tileset.normalized_region(TileIndex::new(5)),
            Some(TileAtlasRegion {
                min: Vec2::new(0.25, 0.5),
                size: Vec2::new(0.25, 0.5),
            })
        );
        assert_eq!(tileset.normalized_region(TileIndex::new(8)), None);
    }

    #[test]
    fn tilemap_codec_resolves_stable_tileset_refs_during_preflight() {
        let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let database = test_database(stable_id, "tilesets/terrain.ron");
        let mut asset_server = AssetServer::new();
        let prepared = {
            let mut context = ComponentDecodeContext::with_asset_server(&mut asset_server)
                .with_project_asset_database(&database);
            let mut registry = ComponentRegistry::new();
            register_tilemap_components(&mut registry)
                .expect("component registration should succeed");

            let prepared = registry
                .preflight_component_with_context(
                    &tilemap_type_id(),
                    &tilemap_value(AssetRef::StableId(stable_id)),
                    &mut context,
                )
                .unwrap()
                .unwrap();
            assert!(context.asset_server_touched());
            prepared
        };
        let mut world = World::new();
        let entity = world.spawn_empty().id();

        prepared.apply(&mut world, entity).unwrap();

        let tilemap = world.get::<Tilemap>(entity).unwrap();
        let tileset = tilemap.tileset.unwrap();
        assert_eq!(
            asset_server.path(tileset.id()),
            Some("tilesets/terrain.ron")
        );
        assert_eq!(asset_server.stable_id(tileset.id()), Some(stable_id));
    }

    #[test]
    fn tilemap_codec_rejects_unknown_stable_tileset_refs_before_apply() {
        let known_stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let unknown_stable_id = stable_id("b73f0f16-09e8-4265-b090-b689b41c197e");
        let database = test_database(known_stable_id, "tilesets/terrain.ron");
        let mut asset_server = AssetServer::new();
        let mut context = ComponentDecodeContext::with_asset_server(&mut asset_server)
            .with_project_asset_database(&database);
        let mut registry = ComponentRegistry::new();
        register_tilemap_components(&mut registry).expect("component registration should succeed");

        let result = registry
            .preflight_component_with_context(
                &tilemap_type_id(),
                &tilemap_value(AssetRef::StableId(unknown_stable_id)),
                &mut context,
            )
            .unwrap();

        assert!(matches!(
            result,
            Err(ComponentCodecError::InvalidAssetRef {
                field,
                asset_ref,
                ..
            }) if field == "tileset.value"
                && asset_ref == format!("stable_id:{unknown_stable_id}")
        ));
        assert_eq!(asset_server.path(AssetId::from_raw(1)), None);
    }

    #[test]
    fn tilemap_codec_rejects_wrong_tileset_source_kind() {
        let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(AssetRecord::new(
                stable_id,
                AssetPath::new("textures/terrain.png").unwrap(),
                AssetSourceKind::Image,
            ))
            .unwrap();
        let mut asset_server = AssetServer::new();
        let mut context = ComponentDecodeContext::with_asset_server(&mut asset_server)
            .with_project_asset_database(&database);
        let mut registry = ComponentRegistry::new();
        register_tilemap_components(&mut registry).expect("component registration should succeed");

        let result = registry
            .preflight_component_with_context(
                &tilemap_type_id(),
                &tilemap_value(AssetRef::StableId(stable_id)),
                &mut context,
            )
            .unwrap();

        assert!(matches!(
            result,
            Err(ComponentCodecError::InvalidAssetRef { field, .. }) if field == "tileset.value"
        ));
        assert_eq!(asset_server.path(AssetId::from_raw(1)), None);
    }

    #[test]
    fn tilemap_codec_validates_path_refs_when_database_is_present() {
        let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let database = test_database(stable_id, "tilesets/terrain.ron");
        let mut context = ComponentDecodeContext::new().with_project_asset_database(&database);
        let mut registry = ComponentRegistry::new();
        register_tilemap_components(&mut registry).expect("component registration should succeed");

        let result = registry
            .preflight_component_with_context(
                &tilemap_type_id(),
                &tilemap_value(AssetRef::path("tilesets/missing.ron").unwrap()),
                &mut context,
            )
            .unwrap();

        assert!(matches!(
            result,
            Err(ComponentCodecError::InvalidAssetRef { field, .. }) if field == "tileset.value"
        ));
        assert!(!context.asset_server_touched());
    }

    #[test]
    fn tilemap_schema_exposes_authoring_fields() {
        let mut registry = ComponentRegistry::new();
        register_tilemap_components(&mut registry).expect("component registration should succeed");

        let schema = registry
            .schema(&ComponentTypeId::new("nara.tilemap.Tilemap"))
            .unwrap();

        assert_eq!(
            schema
                .fields
                .iter()
                .map(|field| (field.path.to_string(), field.value_kind, field.required))
                .collect::<Vec<_>>(),
            vec![
                ("cells".to_string(), ComponentValueKind::List, false),
                ("layer".to_string(), ComponentValueKind::I64, false),
                ("sort_key".to_string(), ComponentValueKind::I64, false),
                ("tile_size.x".to_string(), ComponentValueKind::F64, true),
                ("tile_size.y".to_string(), ComponentValueKind::F64, true),
                ("tileset".to_string(), ComponentValueKind::AssetRef, false),
            ]
        );
    }

    fn tilemap_value(tileset: AssetRef) -> ComponentValue {
        ComponentValue::map([
            ("tile_size", vec2_value(Vec2::new(16.0, 16.0)).unwrap()),
            ("layer", ComponentValue::I64(0)),
            ("sort_key", ComponentValue::I64(0)),
            ("tileset", asset_ref_value(&tileset).unwrap()),
            ("cells", ComponentValue::List(Vec::new())),
        ])
    }

    fn tilemap_type_id() -> ComponentTypeId {
        ComponentTypeId::new("nara.tilemap.Tilemap")
    }

    fn test_database(stable_id: StableAssetId, path: &str) -> ProjectAssetDatabase {
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(AssetRecord::new(
                stable_id,
                AssetPath::new(path).unwrap(),
                AssetSourceKind::Other("tileset".to_string()),
            ))
            .unwrap();
        database
    }

    fn stable_id(id: &str) -> StableAssetId {
        StableAssetId::parse_str(id).unwrap()
    }
}
