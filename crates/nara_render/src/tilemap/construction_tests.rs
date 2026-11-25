use crate::tilemap::{DEFAULT_TILE_SIZE, TileCell, TileCoord, TileIndex, TileSet, Tilemap};
use nara_asset::{AssetRef, AssetServer, AssetSourcePath};
use nara_core::{Color, Vec2};
use nara_ecs::World;
use nara_reflect::{
    ComponentCodecError, ComponentDecodeContext, ComponentFieldPath, ComponentRegistryBuilder,
    ComponentRegistrySnapshot, ComponentTypeId, ComponentValue,
};

fn registry() -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistryBuilder::new();
    crate::tilemap::register_tilemap_components(&mut registry).unwrap();
    registry.freeze().unwrap()
}

fn tilemap_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.tilemap.Tilemap")
}

fn vec2_value(value: Vec2) -> ComponentValue {
    ComponentValue::map([
        ("x", ComponentValue::f64(f64::from(value.x)).unwrap()),
        ("y", ComponentValue::f64(f64::from(value.y)).unwrap()),
    ])
}

fn cell_value(coord: TileCoord, cell: TileCell) -> ComponentValue {
    ComponentValue::map([
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
                (
                    "color",
                    ComponentValue::map(
                        [
                            ("r", cell.color.r),
                            ("g", cell.color.g),
                            ("b", cell.color.b),
                            ("a", cell.color.a),
                        ]
                        .map(|(field, channel)| {
                            (field, ComponentValue::f64(f64::from(channel)).unwrap())
                        }),
                    ),
                ),
            ]),
        ),
    ])
}

fn tilemap_value(cells: Vec<ComponentValue>) -> ComponentValue {
    ComponentValue::map([
        ("tile_size", vec2_value(DEFAULT_TILE_SIZE)),
        ("cells", ComponentValue::List(cells)),
    ])
}

#[test]
fn codec_preserves_input_order_dirty_revisions_and_subsequent_edits() {
    let snapshot = registry();
    let tinted =
        TileCell::new(TileIndex::new(23)).with_color(Color::rgba(0.125, 0.375, 0.625, 0.875));
    let plain = TileCell::new(TileIndex::new(u32::MAX));
    let cells = [
        (TileCoord::new(65, -33), tinted),
        (TileCoord::new(-1, -32), plain),
        (TileCoord::new(i32::MAX, i32::MIN), tinted),
        (TileCoord::new(0, 0), plain),
        (TileCoord::new(-32, -1), tinted),
        (TileCoord::new(i32::MIN, i32::MAX), plain),
        (TileCoord::new(33, -34), plain),
        (TileCoord::new(64, -64), tinted),
        (TileCoord::new(-33, 32), plain),
        (TileCoord::new(31, 31), tinted),
    ];
    let mut value = tilemap_value(
        cells
            .iter()
            .map(|&(coord, cell)| cell_value(coord, cell))
            .collect(),
    );
    let tile_size = Vec2::new(7.5, 13.25);
    let anchor = Vec2::new(-2.5, 4.125);
    for (field, value_to_set) in [
        ("tile_size", vec2_value(tile_size)),
        ("layer", ComponentValue::I64(-6)),
        ("sort_key", ComponentValue::I64(19)),
        ("y_sort_anchor", vec2_value(anchor)),
    ] {
        value
            .set_path(&ComponentFieldPath::from_fields([field]), value_to_set)
            .unwrap();
    }
    let mut expected = Tilemap::new(tile_size)
        .with_layer(-6)
        .with_sort_key(19)
        .with_y_sort_anchor(anchor);
    for (coord, cell) in cells {
        expected.set_cell(coord, cell);
    }
    let original_source = value.clone();
    let prepared = snapshot
        .preflight_component(&tilemap_type_id(), &value)
        .unwrap();
    assert_eq!(value, original_source);
    value
        .set_path(
            &ComponentFieldPath::from_fields(["cells"]),
            ComponentValue::List(Vec::new()),
        )
        .unwrap();
    drop(value);
    drop(original_source);
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    prepared.apply(&mut world, entity).unwrap();
    assert!(!world.contains_resource::<AssetServer>());
    let actual = world.get::<Tilemap>(entity).unwrap();
    assert_eq!(actual, &expected);
    assert_eq!(
        actual.cells().collect::<Vec<_>>(),
        expected.cells().collect::<Vec<_>>(),
    );
    assert_eq!(
        actual.dirty_chunks().collect::<Vec<_>>(),
        expected.dirty_chunks().collect::<Vec<_>>(),
    );
    {
        let mut actual = world.get_mut::<Tilemap>(entity).unwrap();
        assert_eq!(
            actual.set_cell(cells[0].0, plain),
            expected.set_cell(cells[0].0, plain),
        );
        assert_eq!(&*actual, &expected);
        actual.clear_dirty_chunks();
        expected.clear_dirty_chunks();
        assert_eq!(&*actual, &expected);
        for coord in [TileCoord::new(111, 222), cells[2].0, cells[2].0] {
            assert_eq!(actual.remove_cell(coord), expected.remove_cell(coord));
            assert_eq!(&*actual, &expected);
        }
        assert_eq!(
            actual.set_cell(TileCoord::new(-65, 1), tinted),
            expected.set_cell(TileCoord::new(-65, 1), tinted),
        );
        assert_eq!(&*actual, &expected);
    }
    let expected_entity = world.spawn(expected).id();
    assert_eq!(
        snapshot
            .encode_component(&tilemap_type_id(), &world, entity)
            .unwrap(),
        snapshot
            .encode_component(&tilemap_type_id(), &world, expected_entity)
            .unwrap(),
    );
}

#[test]
fn codec_reports_original_late_duplicate_index() {
    let snapshot = registry();
    let mut cells = (0..96)
        .rev()
        .map(|index| {
            cell_value(
                TileCoord::new(index * 37 - 1500, -index),
                TileCell::new(TileIndex::new(7)),
            )
        })
        .collect::<Vec<_>>();
    cells.push(cells[7].clone());
    let error = snapshot
        .preflight_component(&tilemap_type_id(), &tilemap_value(cells))
        .err()
        .expect("a repeated coordinate must reject the complete candidate");
    assert_eq!(
        error,
        ComponentCodecError::invalid_field("cells[96].coord", "unique tile coordinate"),
    );
}

#[test]
fn codec_reads_malformed_duplicate_color_before_rejecting_coordinate() {
    let snapshot = registry();
    for malformed in [
        ComponentValue::Bool(true),
        ComponentValue::f64(f64::MAX).unwrap(),
    ] {
        let mut cells = (0..32)
            .rev()
            .map(|index| {
                cell_value(
                    TileCoord::new(index - 16, index * -33),
                    TileCell::new(TileIndex::new(3)),
                )
            })
            .collect::<Vec<_>>();
        let mut duplicate = cells[4].clone();
        duplicate
            .set_path(
                &ComponentFieldPath::from_fields(["cell", "color", "g"]),
                malformed,
            )
            .unwrap();
        cells.push(duplicate);
        let error = snapshot
            .preflight_component(&tilemap_type_id(), &tilemap_value(cells))
            .err()
            .expect("the malformed duplicate payload must reject preflight");
        assert_eq!(
            error,
            ComponentCodecError::invalid_field("cell.color.g", "finite f32"),
        );
    }
}

#[test]
fn codec_omitted_and_explicit_defaults_encode_the_same_empty_map() {
    let snapshot = registry();
    let omitted = ComponentValue::map([("tile_size", vec2_value(DEFAULT_TILE_SIZE))]);
    let explicit = ComponentValue::map([
        ("tile_size", vec2_value(DEFAULT_TILE_SIZE)),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
        ("y_sort_anchor", ComponentValue::Null),
        ("tileset", ComponentValue::Null),
        ("cells", ComponentValue::List(Vec::new())),
    ]);
    for value in [&omitted, &explicit] {
        let prepared = snapshot
            .preflight_component(&tilemap_type_id(), value)
            .unwrap();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        prepared.apply(&mut world, entity).unwrap();
        assert_eq!(world.get::<Tilemap>(entity), Some(&Tilemap::default()));
        assert!(!world.contains_resource::<AssetServer>());
        assert_eq!(
            snapshot
                .encode_component(&tilemap_type_id(), &world, entity)
                .unwrap(),
            Some(explicit.clone()),
        );
    }
}

#[test]
fn deferred_path_candidate_owns_cells_and_isolates_live_map_and_catalogue() {
    let snapshot = registry();
    let mut server = AssetServer::new();
    let live_tileset = server.reserve::<TileSet>("tilesets/live.ron").unwrap();
    let resolved_tileset = server.reserve::<TileSet>("tilesets/target.ron").unwrap();
    server
        .reserve::<String>("tilesets/conflicting.ron")
        .unwrap();
    let catalogue = server.clone();
    let mut live = Tilemap::default().with_tileset(live_tileset).with_layer(8);
    live.set_cell(TileCoord::new(-1, 0), TileCell::new(TileIndex::new(9)));
    let mut world = World::new();
    world.insert_resource(server);
    let entity = world.spawn(live.clone()).id();
    let cells = [
        (
            TileCoord::new(64, -33),
            TileCell::new(TileIndex::new(2)).with_color(Color::rgba(0.25, 0.5, 0.75, 0.125)),
        ),
        (TileCoord::new(-1, -1), TileCell::new(TileIndex::new(7))),
        (TileCoord::new(65, -34), TileCell::new(TileIndex::new(1))),
    ];
    for path in [
        "tilesets/discarded.ron",
        "tilesets/conflicting.ron",
        "tilesets/target.ron",
    ] {
        let mut value = tilemap_value(
            cells
                .iter()
                .map(|&(coord, cell)| cell_value(coord, cell))
                .collect(),
        );
        value
            .set_path(
                &ComponentFieldPath::from_fields(["tileset"]),
                nara_reflect::encode_asset_reference_value(&AssetRef::path(path).unwrap()),
            )
            .unwrap();
        let mut context = ComponentDecodeContext::new();
        let prepared = snapshot
            .preflight_component_with_context(&tilemap_type_id(), &value, &mut context)
            .unwrap();
        assert!(!context.asset_server_touched());
        assert_eq!(world.get::<Tilemap>(entity), Some(&live));
        assert_eq!(world.resource::<AssetServer>(), &catalogue);
        value
            .set_path(
                &ComponentFieldPath::from_fields(["cells"]),
                ComponentValue::List(Vec::new()),
            )
            .unwrap();
        value
            .set_path(
                &ComponentFieldPath::from_fields(["tileset"]),
                ComponentValue::Null,
            )
            .unwrap();
        drop(value);
        if path == "tilesets/discarded.ron" {
            drop(prepared);
        } else if path == "tilesets/conflicting.ron" {
            assert!(matches!(
                prepared.apply(&mut world, entity),
                Err(ComponentCodecError::InvalidAssetRef { field, .. }) if field == "tileset.value"
            ));
        } else {
            prepared.apply(&mut world, entity).unwrap();
            let mut expected = Tilemap::default().with_tileset(resolved_tileset);
            for (coord, cell) in cells {
                expected.set_cell(coord, cell);
            }
            assert_eq!(world.get::<Tilemap>(entity), Some(&expected));
            let encoded = snapshot
                .encode_component(&tilemap_type_id(), &world, entity)
                .unwrap()
                .unwrap();
            assert_eq!(
                encoded.field("tileset").unwrap(),
                &nara_reflect::encode_asset_reference_value(&AssetRef::path(path).unwrap()),
            );
        }
        if path != "tilesets/target.ron" {
            assert_eq!(world.get::<Tilemap>(entity), Some(&live));
        }
        assert_eq!(world.resource::<AssetServer>(), &catalogue);
    }
}

#[test]
fn diagnostic_validation_preserves_codec_errors_and_asset_context_effects() {
    let snapshot = registry();
    let first = cell_value(
        TileCoord::new(-33, 65),
        TileCell::new(TileIndex::new(7)).with_color(Color::rgba(0.125, 0.25, 0.5, 0.75)),
    );
    let mut malformed = first.clone();
    malformed
        .set_path(
            &ComponentFieldPath::from_fields(["cell", "color", "g"]),
            ComponentValue::f64(f64::MAX).unwrap(),
        )
        .unwrap();
    let values = [
        tilemap_value(Vec::new()),
        tilemap_value(vec![first.clone()]),
        tilemap_value(vec![first.clone(), first.clone()]),
        tilemap_value(vec![first.clone(), malformed.clone()]),
        tilemap_value(vec![first.clone(), first.clone(), malformed]),
    ];
    for value in values {
        for asset in [
            None,
            Some("tilesets/valid.ron"),
            Some("tilesets/conflict.ron"),
        ] {
            let mut value = value.clone();
            if let Some(path) = asset {
                value
                    .set_path(
                        &ComponentFieldPath::from_fields(["tileset"]),
                        nara_reflect::encode_asset_reference_value(&AssetRef::path(path).unwrap()),
                    )
                    .unwrap();
            }
            let original = value.clone();
            let expected = snapshot
                .preflight_component(&tilemap_type_id(), &value)
                .map(drop);
            assert_eq!(
                snapshot.validate_component_with_context(
                    &tilemap_type_id(),
                    &value,
                    &mut ComponentDecodeContext::new(),
                ),
                expected,
            );
            let mut native_server = AssetServer::new();
            native_server
                .reserve::<String>("tilesets/conflict.ron")
                .unwrap();
            let untyped = native_server
                .reserve_path_id(AssetSourcePath::new("tilesets/valid.ron").unwrap())
                .unwrap();
            assert!(native_server.get_handle::<TileSet>(untyped).is_none());
            let mut validation_server = native_server.clone();
            let mut native_context = ComponentDecodeContext::with_asset_server(&mut native_server);
            let mut validation_context =
                ComponentDecodeContext::with_asset_server(&mut validation_server);
            let expected = snapshot
                .preflight_component_with_context(&tilemap_type_id(), &value, &mut native_context)
                .map(drop);
            let actual = snapshot.validate_component_with_context(
                &tilemap_type_id(),
                &value,
                &mut validation_context,
            );
            assert_eq!(actual, expected);
            assert_eq!(
                validation_context.asset_server_touched(),
                native_context.asset_server_touched(),
            );
            drop((native_context, validation_context));
            assert_eq!(validation_server, native_server);
            assert_eq!(
                validation_server.get_handle::<TileSet>(untyped).is_some(),
                asset == Some("tilesets/valid.ron"),
            );
            assert_eq!(value, original);
        }
    }
}
