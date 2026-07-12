use nara::{advanced_prelude::*, diagnostic::DiagnosticValueRef};

const PLAYER_STABLE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const TILESET_STABLE_ID: &str = "b73f0f16-09e8-4265-b090-b689b41c197e";
const UNKNOWN_STABLE_ID: &str = "4bf6d3ff-f6c6-47fb-9a39-4ab27598094f";

fn diagnostic_has_field(
    diagnostic: &Diagnostic,
    key: &str,
    class: DiagnosticFieldClass,
    value: DiagnosticValueRef<'_>,
) -> bool {
    diagnostic.fields().iter().any(|field| {
        field.key().as_str() == key && field.class() == class && field.value() == value
    })
}

#[test]
fn sprite_stable_asset_id_resolves_before_world_spawn_and_exports_by_policy() {
    let registry = component_registry();
    let database = asset_database(PLAYER_STABLE_ID, "textures/player.png");
    let document = sprite_scene(asset_ref_value("stable_id", PLAYER_STABLE_ID));
    let mut world = World::new();

    let report = spawn_scene_with_asset_database(&mut world, &registry, &document, &database);

    assert!(!report.diagnostics.has_errors());
    let entity = spawned_entity(&report, &world, &scene_id("player"));
    let sprite = world.get::<Sprite>(entity).unwrap();
    let texture = sprite
        .material
        .image
        .expect("sprite texture should resolve");
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(asset_server.path(texture.id()), Some("textures/player.png"));
    assert_eq!(
        asset_server.stable_id(texture.id()),
        Some(stable_id(PLAYER_STABLE_ID))
    );

    let path_export = export_scene(&world, &registry);
    assert!(!path_export.diagnostics.has_errors());
    let path_document = &path_export.output().unwrap().document;
    assert_eq!(
        exported_sprite_texture(path_document),
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
    let stable_document = &stable_export.output().unwrap().document;
    assert_eq!(
        exported_sprite_texture(stable_document),
        ("stable_id", PLAYER_STABLE_ID)
    );

    #[cfg(feature = "serde")]
    {
        let json = stable_document.to_json_string().unwrap();
        let ron = stable_document.to_ron_string().unwrap();
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
    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(world.get_resource::<AssetServer>().is_none());
    assert!(report.instance.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.invalid-component-payload"
            && diagnostic_has_field(
                diagnostic,
                "entity-id",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("player"),
            )
            && diagnostic_has_field(
                diagnostic,
                "component-id",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("nara.sprite.Sprite"),
            )
            && diagnostic_has_field(
                diagnostic,
                "field-path",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("material.image.value"),
            )
            && diagnostic_has_field(
                diagnostic,
                "asset-ref",
                DiagnosticFieldClass::Sensitive,
                DiagnosticValueRef::Redacted,
            )
    }));
}

#[test]
fn sprite_path_asset_ref_still_resolves_without_project_database() {
    let registry = component_registry();
    let document = sprite_scene(asset_ref_value("path", "textures/player.png"));
    let mut world = World::new();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(!report.diagnostics.has_errors());
    let entity = spawned_entity(&report, &world, &scene_id("player"));
    let sprite = world.get::<Sprite>(entity).unwrap();
    let texture = sprite
        .material
        .image
        .expect("sprite texture should resolve");
    assert_eq!(
        world.resource::<AssetServer>().path(texture.id()),
        Some("textures/player.png")
    );
}

#[test]
fn sprite_codec_round_trip_preserves_anchor() {
    let registry = component_registry();
    let sprite_id = ComponentTypeId::new("nara.sprite.Sprite");
    let expected_anchor = SpriteAnchor {
        normalized: Vec2::new(-0.5, 0.75),
    };
    let mut source_world = World::new();
    let mut sprite = Sprite::from_color(Vec2::new(16.0, 24.0), Color::WHITE);
    sprite.anchor = expected_anchor;
    let source_entity = source_world.spawn(sprite).id();

    let encoded = registry
        .encode_component(&sprite_id, &source_world, source_entity)
        .unwrap()
        .unwrap()
        .unwrap();
    let prepared = registry
        .preflight_component(&sprite_id, &encoded)
        .unwrap()
        .unwrap();
    let mut target_world = World::new();
    let target_entity = target_world.spawn_empty().id();
    prepared.apply(&mut target_world, target_entity).unwrap();

    assert_eq!(
        target_world.get::<Sprite>(target_entity).unwrap().anchor,
        expected_anchor
    );
}

#[test]
fn unknown_sprite_path_asset_ref_fails_with_project_database_without_world_mutation() {
    let registry = component_registry();
    let database = asset_database(PLAYER_STABLE_ID, "textures/player.png");
    let document = sprite_scene(asset_ref_value("path", "textures/missing.png"));
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_scene_with_asset_database(&mut world, &registry, &document, &database);

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(world.get_resource::<AssetServer>().is_none());
    assert!(report.instance.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.invalid-component-payload"
            && diagnostic_has_field(
                diagnostic,
                "entity-id",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("player"),
            )
            && diagnostic_has_field(
                diagnostic,
                "component-id",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("nara.sprite.Sprite"),
            )
            && diagnostic_has_field(
                diagnostic,
                "field-path",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("material.image.value"),
            )
            && diagnostic_has_field(
                diagnostic,
                "asset-ref",
                DiagnosticFieldClass::Sensitive,
                DiagnosticValueRef::Redacted,
            )
    }));
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
    let entity = spawned_entity(&report, &world, &scene_id("player"));
    let sprite = world.get::<Sprite>(entity).unwrap();
    assert!(sprite.material.image.is_some());
}

#[test]
fn prefab_patch_field_override_preserves_inherited_sprite_data() {
    let registry = component_registry();
    let prefab = PrefabDocument::new([SceneEntityRecord::new(scene_id("player")).with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            sprite_value(asset_ref_value("path", "textures/player.png")),
        ),
    )]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: scene_id("player"),
        component: ComponentTypeId::new("nara.sprite.Sprite"),
        component_version: ComponentSchemaVersion(1),
        field: ComponentFieldId::new("material.tint.r"),
        value: ComponentValue::f64(0.25).unwrap(),
    }]);
    let mut world = World::new();

    let report = spawn_prefab_with_patch(&mut world, &registry, &prefab, &patch);

    assert!(!report.diagnostics.has_errors());
    let sprite = world
        .get::<Sprite>(spawned_entity(&report, &world, &scene_id("player")))
        .unwrap();
    assert_eq!(sprite.material.tint.r, 0.25);
    assert_eq!(sprite.material.tint.g, 1.0);
    assert_eq!(sprite.size, Vec2::new(16.0, 16.0));
    assert!(sprite.material.image.is_some());
}

#[test]
fn prefab_patch_invalid_asset_ref_fails_before_world_mutation() {
    let registry = component_registry();
    let prefab = PrefabDocument::new([SceneEntityRecord::new(scene_id("player")).with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            sprite_value(ComponentValue::Null),
        ),
    )]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetAssetRefField {
        entity: scene_id("player"),
        component: ComponentTypeId::new("nara.sprite.Sprite"),
        component_version: ComponentSchemaVersion(1),
        field: ComponentFieldId::new("material.image"),
        asset_ref: AssetRef::stable_id(UNKNOWN_STABLE_ID).unwrap(),
    }]);
    let database = ProjectAssetDatabase::default();
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_prefab_with_patch_and_asset_database(
        &mut world, &registry, &prefab, &patch, &database,
    );
    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(world.get_resource::<AssetServer>().is_none());
    assert!(report.instance.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.invalid-component-payload"
            && diagnostic_has_field(
                diagnostic,
                "operation-index",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(0),
            )
            && diagnostic_has_field(
                diagnostic,
                "entity-id",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("player"),
            )
            && diagnostic_has_field(
                diagnostic,
                "component-id",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("nara.sprite.Sprite"),
            )
            && diagnostic_has_field(
                diagnostic,
                "field-path",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("material.image.value"),
            )
            && diagnostic_has_field(
                diagnostic,
                "asset-ref",
                DiagnosticFieldClass::Sensitive,
                DiagnosticValueRef::Redacted,
            )
    }));
}

#[test]
fn tilemap_stable_tileset_id_resolves_before_world_spawn() {
    let registry = component_registry_with_tilemap();
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
    let entity = spawned_entity(&report, &world, &scene_id("tiles"));
    let tilemap = world.get::<Tilemap>(entity).unwrap();
    let tileset = tilemap.tileset.expect("tileset handle should resolve");
    assert_eq!(
        world.resource::<AssetServer>().stable_id(tileset.id()),
        Some(stable_id(TILESET_STABLE_ID))
    );
}

fn component_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_base_components(&mut registry);
    registry.freeze().unwrap();
    registry
}

fn component_registry_with_tilemap() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_base_components(&mut registry);
    nara::tilemap::register_tilemap_components(&mut registry)
        .expect("tilemap components should register once");
    registry.freeze().unwrap();
    registry
}

fn register_base_components(registry: &mut ComponentRegistry) {
    nara::scene::register_scene_components(registry)
        .expect("scene components should register once");
    nara::sprite::register_sprite_components(registry)
        .expect("sprite components should register once");
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

fn sprite_value(image: ComponentValue) -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(16.0).unwrap()),
                ("y", ComponentValue::f64(16.0).unwrap()),
            ]),
        ),
        (
            "material",
            ComponentValue::map([
                ("image", image),
                (
                    "tint",
                    ComponentValue::map([
                        ("r", ComponentValue::f64(1.0).unwrap()),
                        ("g", ComponentValue::f64(1.0).unwrap()),
                        ("b", ComponentValue::f64(1.0).unwrap()),
                        ("a", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
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
    let texture = component
        .value
        .field("material")
        .unwrap()
        .field("image")
        .unwrap();
    (
        texture.field_str("kind").unwrap(),
        texture.field_str("value").unwrap(),
    )
}

fn spawned_entity(report: &SceneSpawnReport, world: &World, id: &SceneEntityId) -> Entity {
    match report
        .instance
        .as_ref()
        .expect("successful scene spawn should publish an instance")
        .resolve(world, id)
    {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected resolved scene entity, got {lookup:?}"),
    }
}
