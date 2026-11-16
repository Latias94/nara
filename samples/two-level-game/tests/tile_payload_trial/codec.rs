//! EXPERIMENTAL persistent-record boundary probe using the stock cell representation.

use std::collections::BTreeSet;

use nara::{
    asset::AssetRef,
    core::{Color, Vec2},
    ecs::Component,
    reflect::{
        ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistryError, ComponentSchema,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
        decode_asset_reference_value, encode_asset_reference_value,
    },
    tilemap::{TileCell, TileCoord, TileIndex},
};

pub(super) const CONFIG: &str = "trial.tilemap.Configuration";
pub(super) const CELLS: &str = "trial.tilemap.Cells";

#[derive(Component)]
pub(super) struct Configuration {
    pub tile_size: Vec2,
    pub layer: i32,
    pub sort_key: i32,
    pub y_sort_anchor: Option<Vec2>,
    pub tileset: Option<AssetRef>,
}

#[derive(Component)]
pub(super) struct Cells {
    pub entries: Vec<(TileCoord, TileCell)>,
}

pub(super) fn register(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    let mut fields = Vec::new();
    for (id, alias, axis) in [
        ("tile_size.x", "Tile width", "x"),
        ("tile_size.y", "Tile height", "y"),
    ] {
        fields.push(ComponentFieldSchema::required(
            ComponentFieldId::new(id),
            alias,
            ComponentFieldPath::from_fields(["tile_size", axis]),
            ComponentValueKind::F64,
        ));
    }
    for (id, alias, kind, default) in [
        (
            "layer",
            "Layer",
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
        (
            "sort_key",
            "Sort group",
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
        (
            "y_sort_anchor",
            "Y sort anchor",
            ComponentValueKind::Map,
            ComponentValue::Null,
        ),
        (
            "tileset",
            "Tileset",
            ComponentValueKind::AssetRef,
            ComponentValue::Null,
        ),
    ] {
        let mut field = ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new(id),
            alias,
            ComponentFieldPath::from_fields([id]),
            kind,
            default,
        );
        if id == "tileset" {
            field = field.with_capability(ComponentCapability::AssetRef);
        }
        fields.push(field);
    }
    let configuration = ComponentSchema::new(
        ComponentTypeId::new(CONFIG),
        "Configuration",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(
        fields
            .into_iter()
            .map(|field| field.with_capabilities(ComponentCapability::SCENE_AUTHORING)),
    );
    registry.register_persistent_component_with_codec(
        configuration,
        decode_configuration,
        encode_configuration,
    )?;
    let cells = ComponentSchema::new(
        ComponentTypeId::new(CELLS),
        "Cells",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("cells"),
        "Cells",
        ComponentFieldPath::from_fields(["cells"]),
        ComponentValueKind::List,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    registry.register_persistent_component_with_codec(cells, decode_cells, encode_cells)?;
    Ok(())
}

pub(super) fn decode_configuration(
    value: &ComponentValue,
) -> Result<Configuration, ComponentCodecError> {
    let tile_size = read_vec2(value.field("tile_size")?, "tile_size")?;
    validate_tile_size(tile_size)?;
    Ok(Configuration {
        tile_size,
        layer: read_i32(value, "layer", "layer")?,
        sort_key: read_i32(value, "sort_key", "sort_key")?,
        y_sort_anchor: match value.field("y_sort_anchor")? {
            ComponentValue::Null => None,
            value => Some(read_vec2(value, "y_sort_anchor")?),
        },
        tileset: match value.field("tileset")? {
            ComponentValue::Null => None,
            value => Some(decode_asset_reference_value(value).map_err(|_| {
                ComponentCodecError::invalid_field("tileset", "canonical asset reference")
            })?),
        },
    })
}

pub(super) fn encode_configuration(
    value: &Configuration,
) -> Result<ComponentValue, ComponentCodecError> {
    validate_tile_size(value.tile_size)?;
    Ok(ComponentValue::map([
        ("tile_size", encode_vec2(value.tile_size)?),
        ("layer", ComponentValue::I64(i64::from(value.layer))),
        ("sort_key", ComponentValue::I64(i64::from(value.sort_key))),
        (
            "y_sort_anchor",
            value
                .y_sort_anchor
                .map(encode_vec2)
                .transpose()?
                .unwrap_or(ComponentValue::Null),
        ),
        (
            "tileset",
            value
                .tileset
                .as_ref()
                .map(encode_asset_reference_value)
                .unwrap_or(ComponentValue::Null),
        ),
    ]))
}

pub(super) fn decode_cells(value: &ComponentValue) -> Result<Cells, ComponentCodecError> {
    let ComponentValue::List(entries) = value.field("cells")? else {
        return Err(ComponentCodecError::invalid_field("cells", "list"));
    };
    let mut coordinates = BTreeSet::new();
    let mut cells = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let (coord, cell) = read_cell(entry)?;
        if !coordinates.insert(coord) {
            return Err(ComponentCodecError::invalid_field(
                format!("cells[{index}].coord"),
                "unique tile coordinate",
            ));
        }
        cells.push((coord, cell));
    }
    Ok(Cells { entries: cells })
}

pub(super) fn encode_cells(value: &Cells) -> Result<ComponentValue, ComponentCodecError> {
    let mut entries = value.entries.iter().collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(coord, _)| *coord);
    for (index, pair) in entries.windows(2).enumerate() {
        if pair[0].0 == pair[1].0 {
            return Err(ComponentCodecError::invalid_field(
                format!("cells[{}].coord", index + 1),
                "unique tile coordinate",
            ));
        }
    }
    let cells = entries
        .into_iter()
        .map(|(coord, cell)| encode_cell(*coord, *cell))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ComponentValue::map([(
        "cells",
        ComponentValue::List(cells),
    )]))
}

fn read_cell(entry: &ComponentValue) -> Result<(TileCoord, TileCell), ComponentCodecError> {
    let coord = entry.field("coord")?;
    let cell = entry.field("cell")?;
    let coord = TileCoord::new(
        read_i32(coord, "x", "cells[].coord.x")?,
        read_i32(coord, "y", "cells[].coord.y")?,
    );
    let tile = u32::try_from(cell.field_u64("tile")?)
        .map_err(|_| ComponentCodecError::invalid_field("cells[].cell.tile", "u32"))?;
    let color = cell.field("color")?;
    Ok((
        coord,
        TileCell::new(TileIndex::new(tile)).with_color(Color::rgba(
            read_f32(color.field("r")?, "cell.color.r")?,
            read_f32(color.field("g")?, "cell.color.g")?,
            read_f32(color.field("b")?, "cell.color.b")?,
            read_f32(color.field("a")?, "cell.color.a")?,
        )),
    ))
}

fn encode_cell(coord: TileCoord, cell: TileCell) -> Result<ComponentValue, ComponentCodecError> {
    let color = ComponentValue::map([
        ("r", ComponentValue::f64(f64::from(cell.color.r))?),
        ("g", ComponentValue::f64(f64::from(cell.color.g))?),
        ("b", ComponentValue::f64(f64::from(cell.color.b))?),
        ("a", ComponentValue::f64(f64::from(cell.color.a))?),
    ]);
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
                ("color", color),
            ]),
        ),
    ]))
}

fn read_i32(value: &ComponentValue, field: &str, path: &str) -> Result<i32, ComponentCodecError> {
    i32::try_from(value.field_i64(field)?)
        .map_err(|_| ComponentCodecError::invalid_field(path, "i32"))
}

fn read_f32(value: &ComponentValue, path: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(path, "finite f32"))?;
    if !value.is_finite() || value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(ComponentCodecError::invalid_field(path, "finite f32"));
    }
    Ok(value as f32)
}

fn read_vec2(value: &ComponentValue, path: &str) -> Result<Vec2, ComponentCodecError> {
    Ok(Vec2::new(
        read_f32(value.field("x")?, &format!("{path}.x"))?,
        read_f32(value.field("y")?, &format!("{path}.y"))?,
    ))
}

fn encode_vec2(value: Vec2) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("x", ComponentValue::f64(f64::from(value.x))?),
        ("y", ComponentValue::f64(f64::from(value.y))?),
    ]))
}

fn validate_tile_size(value: Vec2) -> Result<(), ComponentCodecError> {
    for (axis, length) in [("x", value.x), ("y", value.y)] {
        if !length.is_finite() || length <= 0.0 {
            return Err(ComponentCodecError::invalid_field(
                format!("tile_size.{axis}"),
                "positive finite f32",
            ));
        }
    }
    Ok(())
}
