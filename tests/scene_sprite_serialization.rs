use nara::prelude::*;

const PLAYER_STABLE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const TILESET_STABLE_ID: &str = "b73f0f16-09e8-4265-b090-b689b41c197e";
const UNKNOWN_STABLE_ID: &str = "4bf6d3ff-f6c6-47fb-9a39-4ab27598094f";

#[test]
fn sprite_stable_asset_id_resolves_before_world_spawn_and_exports_by_policy() {
    let registry = component_registry();
    let database = asset_database(PLAYER_STABLE_ID, "textures/player.png");
    let document = sprite_scene(asset_ref_value("stable_id", PLAYER_STABLE_ID));
    let mut world = World::new();

    let report = spawn_scene_with_asset_database(&mut world, &registry, &document, &database);

    assert!(!report.diagnostics.has_errors());
    let entity = report.entity_map.get(&scene_id("player")).unwrap();
    let sprite = world.get::<Sprite>(entity).unwrap();
    let texture = sprite.texture.expect("sprite texture should resolve");
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(asset_server.path(texture.id()), Some("textures/player.png"));
    assert_eq!(
        asset_server.stable_id(texture.id()),
        Some(stable_id(PLAYER_STABLE_ID))
    );

    let path_export = export_scene(&world, &registry);
    assert!(!path_export.diagnostics.has_errors());
    assert_eq!(
        exported_sprite_texture(&path_export.document),
        ("path", "textures/player.png")
    );

    let stable_export = export_scene_with_options(
        &world,
        &registry,
        SceneExportOptions {
            asset_ref_export_policy: AssetRefExportPolicy::StableIdWhenKnown,
        },
    );
    assert!(!stable_export.diagnostics.has_errors());
    assert_eq!(
        exported_sprite_texture(&stable_export.document),
        ("stable_id", PLAYER_STABLE_ID)
    );

    #[cfg(feature = "serde")]
    {
        let json = stable_export.document.to_json_string().unwrap();
        let ron = stable_export.document.to_ron_string().unwrap();
        assert!(json.contains(PLAYER_STABLE_ID));
        assert!(ron.contains(PLAYER_STABLE_ID));
        assert!(!json.contains("AssetId"));
        assert!(!ron.contains("AssetId"));
    }
}

#[test]
fn unknown_sprite_stable_asset_id_fails_without_world_mutation() {
    let registry = component_registry();
    let database = ProjectAssetDatabase::default();
    let document = sprite_scene(asset_ref_value("stable_id", UNKNOWN_STABLE_ID));
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_scene_with_asset_database(&mut world, &registry, &document, &database);
    let expected_asset_ref = format!("stable_id:{UNKNOWN_STABLE_ID}");

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(world.get_resource::<AssetServer>().is_none());
    assert!(report.entity_map.is_empty());
    assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "scene.invalid-component-payload"
            && diagnostic.context.entity_id.as_deref() == Some("player")
            && diagnostic.context.component_id.as_deref() == Some("nara.sprite.Sprite")
            && diagnostic.context.field_path.as_deref() == Some("texture.value")
            && diagnostic.context.asset_ref.as_deref() == Some(expected_asset_ref.as_str())
    }));
}

#[test]
fn sprite_path_asset_ref_still_resolves_without_project_database() {
    let registry = component_registry();
    let document = sprite_scene(asset_ref_value("path", "textures/player.png"));
    let mut world = World::new();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(!report.diagnostics.has_errors());
    let entity = report.entity_map.get(&scene_id("player")).unwrap();
    let sprite = world.get::<Sprite>(entity).unwrap();
    let texture = sprite.texture.expect("sprite texture should resolve");
    assert_eq!(
        world.resource::<AssetServer>().path(texture.id()),
        Some("textures/player.png")
    );
}

#[test]
fn prefab_stable_asset_id_uses_asset_database_preflight() {
    let registry = component_registry();
    let database = asset_database(PLAYER_STABLE_ID, "textures/player.png");
    let prefab = PrefabDocument::new([SceneEntityRecord::new(scene_id("player")).with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            sprite_value(asset_ref_value("stable_id", PLAYER_STABLE_ID)),
        ),
    )]);
    let mut world = World::new();

    let report = spawn_prefab_with_asset_database(&mut world, &registry, &prefab, &database);

    assert!(!report.diagnostics.has_errors());
    let entity = report.entity_map.get(&scene_id("player")).unwrap();
    let sprite = world.get::<Sprite>(entity).unwrap();
    assert!(sprite.texture.is_some());
}

#[test]
fn tilemap_stable_tileset_id_resolves_before_world_spawn() {
    let mut registry = component_registry();
    nara::tilemap::register_tilemap_components(&mut registry);
    let database = asset_database_with_kind(
        TILESET_STABLE_ID,
        "tilesets/basic.tileset.ron",
        AssetSourceKind::Other("tileset".to_string()),
    );
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("tiles")).with_component(
        ComponentTypeId::new("nara.tilemap.Tilemap"),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            tilemap_value(asset_ref_value("stable_id", TILESET_STABLE_ID)),
        ),
    )]);
    let mut world = World::new();

    let report = spawn_scene_with_asset_database(&mut world, &registry, &document, &database);

    assert!(!report.diagnostics.has_errors());
    let entity = report.entity_map.get(&scene_id("tiles")).unwrap();
    let tilemap = world.get::<Tilemap>(entity).unwrap();
    let tileset = tilemap.tileset.expect("tileset handle should resolve");
    assert_eq!(
        world.resource::<AssetServer>().stable_id(tileset.id()),
        Some(stable_id(TILESET_STABLE_ID))
    );
}

fn component_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    nara::scene::register_scene_components(&mut registry);
    nara::sprite::register_sprite_components(&mut registry);
    registry
}

fn asset_database(stable_id: &str, path: &str) -> ProjectAssetDatabase {
    asset_database_with_kind(stable_id, path, AssetSourceKind::Image)
}

fn asset_database_with_kind(
    stable_id_value: &str,
    path: &str,
    source_kind: AssetSourceKind,
) -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            stable_id(stable_id_value),
            AssetPath::new(path).unwrap(),
            source_kind,
        ))
        .unwrap();
    database
}

fn stable_id(id: &str) -> StableAssetId {
    StableAssetId::parse_str(id).unwrap()
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

fn sprite_scene(texture: ComponentValue) -> SceneDocument {
    SceneDocument::new([SceneEntityRecord::new(scene_id("player"))
        .with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion(1),
                ComponentValue::String("Player".to_string()),
            ),
        )
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion(1), sprite_value(texture)),
        )])
}

fn sprite_value(texture: ComponentValue) -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(16.0).unwrap()),
                ("y", ComponentValue::f64(16.0).unwrap()),
            ]),
        ),
        (
            "color",
            ComponentValue::map([
                ("r", ComponentValue::f64(1.0).unwrap()),
                ("g", ComponentValue::f64(1.0).unwrap()),
                ("b", ComponentValue::f64(1.0).unwrap()),
                ("a", ComponentValue::f64(1.0).unwrap()),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
        ("texture", texture),
    ])
}

fn asset_ref_value(kind: &str, value: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_string())),
        ("value", ComponentValue::String(value.to_string())),
    ])
}

fn tilemap_value(tileset: ComponentValue) -> ComponentValue {
    ComponentValue::map([
        (
            "tile_size",
            ComponentValue::map([
                ("x", ComponentValue::f64(16.0).unwrap()),
                ("y", ComponentValue::f64(16.0).unwrap()),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
        ("tileset", tileset),
        ("cells", ComponentValue::List(Vec::new())),
    ])
}

fn exported_sprite_texture(document: &SceneDocument) -> (&str, &str) {
    let entity = document
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "player")
        .unwrap();
    let component = entity
        .components
        .get(&ComponentTypeId::new("nara.sprite.Sprite"))
        .unwrap();
    let texture = component.value.field("texture").unwrap();
    (
        texture.field_str("kind").unwrap(),
        texture.field_str("value").unwrap(),
    )
}
