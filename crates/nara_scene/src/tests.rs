use std::collections::BTreeMap;

use super::*;
use nara_asset::{
    AssetId, AssetPath, AssetRecord, AssetRef, AssetRefError, AssetServer, AssetSourceKind, Handle,
    ProjectAssetDatabase, StableAssetId,
};
use nara_ecs::{Component, World};
use nara_reflect::bevy_reflect;
use nara_reflect::{
    ComponentCodecError, ComponentDecodeContext, ComponentRegistry, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, PreparedComponent, Reflect,
};
#[derive(Clone, Debug, PartialEq, Component, Reflect)]
struct TestPosition {
    x: i32,
}

#[derive(Clone, Debug, PartialEq, Component)]
struct TestAssetLink {
    handle: Handle<TestAsset>,
}

#[derive(Debug)]
struct TestAsset;

#[test]
fn syncs_parent_child_links() {
    let mut world = World::new();
    let parent = world.spawn((Name::new("parent"),)).id();
    let child = spawn_child(&mut world, parent, (Name::new("child"),));

    sync_children(&mut world);

    let parent_ref = world.get_entity(parent).unwrap();
    let children = parent_ref.get::<Children>().unwrap();
    assert_eq!(children.as_slice(), &[child]);
}

#[test]
fn validates_scene_entity_id_shape() {
    assert_eq!(SceneEntityId::new(""), Err(SceneEntityIdError::Empty));
    assert_eq!(
        SceneEntityId::new("/player"),
        Err(SceneEntityIdError::LeadingSlash)
    );
    assert_eq!(
        SceneEntityId::new("root//player"),
        Err(SceneEntityIdError::EmptySegment)
    );
    assert_eq!(
        SceneEntityId::new("root/../player"),
        Err(SceneEntityIdError::ParentDirectorySegment)
    );
    assert!(SceneEntityId::new("root/player-1").is_ok());
}

#[test]
fn validation_reports_duplicate_missing_parent_cycle_and_unknown_component() {
    let registry = test_registry();
    let id = scene_id("player");
    let missing_parent = scene_id("missing");
    let cycle_a = scene_id("cycle_a");
    let cycle_b = scene_id("cycle_b");
    let unknown_component = ComponentTypeId::new("nara.test.Unknown");
    let document = SceneDocument {
        format_version: SceneDocument::CURRENT_FORMAT_VERSION,
        entities: vec![
            SceneEntityRecord::new(id.clone()),
            SceneEntityRecord::new(id),
            SceneEntityRecord::new(scene_id("orphan")).with_parent(missing_parent),
            SceneEntityRecord::new(cycle_a.clone()).with_parent(cycle_b.clone()),
            SceneEntityRecord::new(cycle_b).with_parent(cycle_a),
            SceneEntityRecord::new(scene_id("unknown")).with_component(
                unknown_component,
                SceneComponentRecord::new(ComponentSchemaVersion(1), ComponentValue::Null),
            ),
        ],
    };

    let report = document.validate(&registry);
    let codes = report
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();

    assert!(codes.contains(&"scene.duplicate-entity-id"));
    assert!(codes.contains(&"scene.missing-parent"));
    assert!(codes.contains(&"scene.parent-cycle"));
    assert!(codes.contains(&"scene.unknown-component"));
    assert!(report.has_errors());
}

#[test]
fn validates_document_format_version() {
    let registry = test_registry();
    let document = SceneDocument {
        format_version: SceneDocument::CURRENT_FORMAT_VERSION + 1,
        entities: vec![SceneEntityRecord::new(scene_id("player"))],
    };

    let report = document.validate(&registry);

    assert!(report.has_errors());
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "scene.unsupported-format-version")
    );
}

#[test]
fn prefab_default_uses_current_format_version() {
    assert_eq!(
        PrefabDocument::default().format_version,
        PrefabDocument::CURRENT_FORMAT_VERSION
    );
}

#[test]
fn unsupported_prefab_instance_prevents_world_mutation() {
    let registry = test_registry();
    let document = SceneDocument::new([SceneEntityRecord {
        id: scene_id("enemy"),
        parent: None,
        components: BTreeMap::new(),
        prefab: Some(PrefabInstance {
            source: AssetRef::path("prefabs/enemy.ron").unwrap(),
            overrides: BTreeMap::new(),
        }),
    }]);
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "scene.prefab-instance-unsupported"
            && diagnostic.context.field_path.as_deref() == Some("prefab.source")
            && diagnostic.context.asset_ref.as_deref() == Some("prefabs/enemy.ron")
    }));
}

#[test]
fn invalid_component_payload_does_not_mutate_world() {
    let registry = test_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("bad")).with_component(
        position_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([("x", ComponentValue::String("not a number".to_string()))]),
        ),
    )]);
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(report.entity_map.is_empty());
}

#[test]
fn component_migration_runs_before_scene_preflight_without_mutating_document() {
    let registry = migrated_position_registry();
    let id = scene_id("player");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone()).with_component(
        position_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([("x", ComponentValue::I64(8))]),
        ),
    )]);
    let mut world = World::new();

    let validation = document.validate(&registry);
    let report = spawn_scene(&mut world, &registry, &document);

    assert!(!validation.has_errors());
    assert!(!report.diagnostics.has_errors());
    let entity = report.entity_map.get(&id).unwrap();
    assert_eq!(world.get::<TestPosition>(entity).unwrap().x, 8);
    let source_component = document.entities[0]
        .components
        .get(&position_type_id())
        .unwrap();
    assert_eq!(source_component.version, ComponentSchemaVersion(1));
    assert!(source_component.value.get("x").is_some());
    assert!(source_component.value.get("x2").is_none());
}

#[test]
fn missing_component_migration_reports_unsupported_version() {
    let mut registry = ComponentRegistry::new();
    registry
        .register_serializable_component::<TestPosition, _, _>(
            position_type_id(),
            ComponentSchemaVersion(2),
            |value| {
                let x = value.field_i64("x2")?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x2", "i32"))?,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x2",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("bad")).with_component(
        position_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([("x", ComponentValue::I64(8))]),
        ),
    )]);

    let report = document.validate(&registry);

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "scene.unsupported-component-version"
            && diagnostic.context.component_id.as_deref() == Some(position_type_id().as_str())
    }));
}

#[test]
fn path_asset_ref_resolves_before_scene_spawn_without_database() {
    let registry = test_asset_registry();
    let id = scene_id("player");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone()).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::path("textures/player.png").unwrap()),
    )]);
    let mut world = World::new();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(!report.diagnostics.has_errors());
    let entity = report.entity_map.get(&id).unwrap();
    let link = world.get::<TestAssetLink>(entity).unwrap();
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(link.handle.id()),
        Some("textures/player.png")
    );
}

#[test]
fn stable_asset_ref_resolves_with_database_before_scene_spawn() {
    let registry = test_asset_registry();
    let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let database = test_database(stable_id, "textures/player.png");
    let id = scene_id("player");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone()).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::StableId(stable_id)),
    )]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    let validation = document.validate_with_asset_database(&registry, &database);
    let report = spawner.spawn_with_asset_database(&mut world, &registry, &document, &database);

    assert!(!validation.has_errors());
    assert!(!report.diagnostics.has_errors());
    let entity = report.entity_map.get(&id).unwrap();
    let link = world.get::<TestAssetLink>(entity).unwrap();
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(link.handle.id()),
        Some("textures/player.png")
    );
    assert_eq!(asset_server.stable_id(link.handle.id()), Some(stable_id));
}

#[test]
fn unknown_stable_asset_ref_does_not_mutate_world_or_asset_server() {
    let registry = test_asset_registry();
    let known_stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let unknown_stable_id = stable_id("b73f0f16-09e8-4265-b090-b689b41c197e");
    let database = test_database(known_stable_id, "textures/player.png");
    let id = scene_id("player");
    let document = SceneDocument::new([SceneEntityRecord::new(id).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::StableId(unknown_stable_id)),
    )]);
    let expected_component_id = asset_link_type_id();
    let mut existing_asset_server = AssetServer::new();
    let existing_handle = existing_asset_server
        .reserve::<TestAsset>("textures/existing.png")
        .unwrap();
    let mut world = World::new();
    world.insert_resource(existing_asset_server);
    let before_entities = world.iter_entities().count();

    let report = spawn_scene_with_asset_database(&mut world, &registry, &document, &database);
    let expected_asset_ref = format!("stable_id:{unknown_stable_id}");

    assert!(report.diagnostics.has_errors());
    assert!(report.entity_map.is_empty());
    assert_eq!(world.iter_entities().count(), before_entities);
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(existing_handle.id()),
        Some("textures/existing.png")
    );
    assert_eq!(asset_server.path(AssetId::from_raw(2)), None);
    assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "scene.invalid-component-payload"
            && diagnostic.context.entity_id.as_deref() == Some("player")
            && diagnostic.context.component_id.as_deref() == Some(expected_component_id.as_str())
            && diagnostic.context.field_path.as_deref() == Some("asset.value")
            && diagnostic.context.asset_ref.as_deref() == Some(expected_asset_ref.as_str())
    }));
}

#[test]
fn spawns_hierarchy_records_source_and_exports_stable_document() {
    let registry = test_registry();
    let parent_id = scene_id("parent");
    let child_id = scene_id("parent/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(parent_id.clone())
            .with_component(position_type_id(), position_record(1)),
        SceneEntityRecord::new(child_id.clone())
            .with_parent(parent_id.clone())
            .with_component(position_type_id(), position_record(2)),
    ]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    let report = spawner.spawn(&mut world, &registry, &document);

    assert!(!report.diagnostics.has_errors());
    assert_eq!(report.entity_map.len(), 2);
    let parent = report.entity_map.get(&parent_id).unwrap();
    let child = report.entity_map.get(&child_id).unwrap();
    assert_eq!(
        world
            .get::<Children>(parent)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![child]
    );
    assert_eq!(
        world.get::<SceneEntitySource>(child).unwrap().entity_id,
        child_id
    );

    let export = export_scene(&world, &registry);

    assert!(!export.diagnostics.has_errors());
    assert_eq!(export.document.entities.len(), 2);
    assert_eq!(
        export
            .document
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        vec!["parent", "parent/child"]
    );
    assert_eq!(
        export.document.entities[1]
            .parent
            .as_ref()
            .unwrap()
            .as_str(),
        "parent"
    );
}

#[test]
fn repeated_prefab_spawns_export_with_instance_namespaces() {
    let registry = test_registry();
    let id = scene_id("enemy");
    let document =
        SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(7))]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    assert!(
        !spawner
            .spawn(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );
    assert!(
        !spawner
            .spawn(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );

    let export = export_scene(&world, &registry);
    let ids = export
        .document
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["enemy", "instance_2/enemy"]);
}

#[test]
fn direct_prefab_spawn_supports_whole_component_overrides() {
    let registry = test_registry();
    let id = scene_id("enemy");
    let prefab =
        PrefabDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]);
    let mut overrides = PrefabComponentOverrides::new();
    overrides.insert(
        id.clone(),
        BTreeMap::from([(position_type_id(), position_record(9))]),
    );
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    let report = spawner.spawn_prefab_with_overrides(&mut world, &registry, &prefab, &overrides);

    assert!(!report.diagnostics.has_errors());
    let entity = report.entity_map.get(&id).unwrap();
    assert_eq!(world.get::<TestPosition>(entity).unwrap().x, 9);
}

#[test]
fn unknown_prefab_override_entity_prevents_world_mutation() {
    let registry = test_registry();
    let prefab = PrefabDocument::new([SceneEntityRecord::new(scene_id("enemy"))
        .with_component(position_type_id(), position_record(1))]);
    let mut overrides = PrefabComponentOverrides::new();
    overrides.insert(scene_id("missing"), BTreeMap::new());
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_prefab_with_overrides(&mut world, &registry, &prefab, &overrides);

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "scene.unknown-prefab-override-entity"
            && diagnostic.context.entity_id.as_deref() == Some("missing")
    }));
}

#[test]
fn export_drops_parent_that_is_not_in_document() {
    let registry = test_registry();
    let mut world = World::new();
    let parent = world.spawn_empty().id();
    let child = world
        .spawn((
            Parent(parent),
            SceneEntitySource {
                instance_id: SceneInstanceId::from_raw(1),
                entity_id: scene_id("child"),
            },
            TestPosition { x: 3 },
        ))
        .id();

    let export = export_scene(&world, &registry);

    assert_eq!(export.document.entities.len(), 1);
    assert_eq!(export.document.entities[0].id.as_str(), "child");
    assert_eq!(export.document.entities[0].parent, None);
    assert_eq!(world.get::<Parent>(child).unwrap().0, parent);
    assert!(
        export
            .diagnostics
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "scene.export-parent-skipped")
    );
}

#[cfg(feature = "serde")]
#[test]
fn scene_entity_id_deserialization_validates_shape() {
    let error = serde_json::from_str::<SceneDocument>(
        r#"{"format_version":1,"entities":[{"id":"root/../player","components":{}}]}"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains(".."));
}

#[test]
fn scene_component_schemas_expose_scalar_fields() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry);

    let name_schema = registry
        .schema(&ComponentTypeId::new("nara.scene.Name"))
        .unwrap();
    let visibility_schema = registry
        .schema(&ComponentTypeId::new("nara.scene.Visibility"))
        .unwrap();

    assert_eq!(
        name_schema
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind, field.required))
            .collect::<Vec<_>>(),
        vec![("<root>".to_string(), ComponentValueKind::String, true)]
    );
    assert_eq!(
        visibility_schema
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind, field.required))
            .collect::<Vec<_>>(),
        vec![("<root>".to_string(), ComponentValueKind::String, true)]
    );
}

fn test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_serializable_component::<TestPosition, _, _>(
            position_type_id(),
            ComponentSchemaVersion(1),
            |value| {
                let x = value
                    .get("x")
                    .and_then(ComponentValue::as_i64)
                    .ok_or_else(|| ComponentCodecError::invalid_field("x", "i64"))?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x", "i32"))?,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap();
    registry
}

fn migrated_position_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_serializable_component::<TestPosition, _, _>(
            position_type_id(),
            ComponentSchemaVersion(2),
            |value| {
                let x = value.field_i64("x2")?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x2", "i32"))?,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x2",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap()
        .register_component_migration(
            &position_type_id(),
            ComponentSchemaVersion(1),
            ComponentSchemaVersion(2),
            |value| {
                let ComponentValue::Map(mut fields) = value else {
                    return Err(ComponentCodecError::invalid_field("<root>", "map"));
                };
                let x = fields
                    .remove("x")
                    .ok_or_else(|| ComponentCodecError::missing_field("x"))?;
                fields.insert("x2".to_string(), x);
                Ok(ComponentValue::Map(fields))
            },
        )
        .unwrap();
    registry
}

fn test_asset_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_component_codec_with_context::<TestAssetLink, _, _>(
            asset_link_type_id(),
            ComponentSchemaVersion(1),
            |value, context| {
                let asset_ref = read_asset_ref(value.field("asset")?, "asset")?;
                let prepared = prepare_test_asset_handle(context, "asset.value", asset_ref)?;
                Ok(PreparedComponent::new(move |world, entity| {
                    let handle = match prepared {
                        PreparedTestAsset::Resolved(handle) => handle,
                        PreparedTestAsset::Deferred(asset_ref) => {
                            if world.get_resource::<AssetServer>().is_none() {
                                world.insert_resource(AssetServer::new());
                            }
                            asset_ref
                                .resolve::<TestAsset>(&mut world.resource_mut::<AssetServer>())
                                .map_err(|error| {
                                    ComponentCodecError::invalid_asset_ref(
                                        "asset.value",
                                        asset_ref.to_string(),
                                        error.to_string(),
                                    )
                                })?
                        }
                    };
                    let mut entity_mut = world
                        .get_entity_mut(entity)
                        .map_err(|_| ComponentCodecError::EntityMissing)?;
                    entity_mut.insert(TestAssetLink { handle });
                    Ok(())
                }))
            },
            |world, entity, _context| {
                let Some(link) = world.get::<TestAssetLink>(entity) else {
                    return Ok(None);
                };
                Ok(Some(ComponentValue::map([(
                    "handle",
                    ComponentValue::U64(link.handle.id().raw()),
                )])))
            },
        )
        .unwrap();
    registry
}

enum PreparedTestAsset {
    Resolved(Handle<TestAsset>),
    Deferred(AssetRef),
}

fn prepare_test_asset_handle(
    context: &mut ComponentDecodeContext<'_>,
    field: &str,
    asset_ref: AssetRef,
) -> Result<PreparedTestAsset, ComponentCodecError> {
    if let Some(result) = context.resolve_asset_ref::<TestAsset>(&asset_ref) {
        return result.map(PreparedTestAsset::Resolved).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(field, asset_ref.to_string(), error.to_string())
        });
    }

    if let Some(stable_id) = asset_ref.as_stable_id() {
        let Some(database) = context.project_asset_database() else {
            return Err(ComponentCodecError::invalid_asset_ref(
                field,
                asset_ref.to_string(),
                AssetRefError::MissingProjectDatabase(stable_id).to_string(),
            ));
        };
        database.resolve_ref(&asset_ref).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(field, asset_ref.to_string(), error.to_string())
        })?;
    }

    Ok(PreparedTestAsset::Deferred(asset_ref))
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

fn asset_link_record(asset_ref: AssetRef) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion(1),
        ComponentValue::map([("asset", asset_ref_value(&asset_ref))]),
    )
}

fn asset_ref_value(asset_ref: &AssetRef) -> ComponentValue {
    match asset_ref {
        AssetRef::Path(path) => ComponentValue::map([
            ("kind", ComponentValue::String("path".to_string())),
            ("value", ComponentValue::String(path.as_str().to_string())),
        ]),
        AssetRef::StableId(id) => ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_string())),
            ("value", ComponentValue::String(id.to_string())),
        ]),
    }
}

fn test_database(stable_id: StableAssetId, path: &str) -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            stable_id,
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Image,
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

fn asset_link_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.AssetLink")
}

fn position_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Position")
}

fn position_record(x: i32) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion(1),
        ComponentValue::map([("x", ComponentValue::I64(i64::from(x)))]),
    )
}
