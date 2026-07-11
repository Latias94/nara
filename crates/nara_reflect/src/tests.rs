use std::{cell::Cell, collections::BTreeMap};

use nara_asset::{AssetRef, AssetServer};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::{Component, Resource, World};
use nara_identity::{
    PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference, SceneEntityId,
    WorldIdentityDomain, WorldIdentityDomainSettings, spawn_identity_entity,
};

use super::*;

#[derive(Clone, Component, Reflect)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Component, Reflect)]
struct Velocity {
    dx: f32,
    dy: f32,
}

#[derive(Debug, PartialEq, Eq, Resource)]
struct ExistingApplyState(u32);

struct TestAsset;

#[test]
fn registers_component_schema_by_stable_id_and_rust_type() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");

    registry
        .register_component::<Position>(id.clone(), ComponentSchemaVersion(1))
        .unwrap();

    assert_eq!(
        registry.schema(&id).unwrap().version,
        ComponentSchemaVersion(1)
    );
    assert!(
        !registry
            .schema(&id)
            .unwrap()
            .has_capability(ComponentCapability::Scene)
    );
    assert_eq!(
        registry.schema_for_type::<Position>().unwrap().id.as_str(),
        "nara.test.Position"
    );
}

#[test]
fn rejects_duplicate_component_ids() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");

    registry
        .register_component::<Position>(id.clone(), ComponentSchemaVersion(1))
        .unwrap();
    let result = registry.register_component::<Position>(id.clone(), ComponentSchemaVersion(1));
    assert!(matches!(
        result,
        Err(ComponentRegistryError::DuplicateComponentId(duplicate)) if duplicate == id
    ));
}

#[test]
fn rejects_duplicate_component_rust_types() {
    let mut registry = ComponentRegistry::new();
    let position_id = ComponentTypeId::new("nara.test.Position");
    let alias_id = ComponentTypeId::new("nara.test.PositionAlias");

    registry
        .register_component::<Position>(position_id.clone(), ComponentSchemaVersion(1))
        .unwrap();

    let result =
        registry.register_component::<Position>(alias_id.clone(), ComponentSchemaVersion(1));

    assert!(matches!(
        result,
        Err(ComponentRegistryError::DuplicateComponentRustType {
            existing_component_id,
            requested_component_id,
            ..
        }) if existing_component_id == position_id && requested_component_id == alias_id
    ));
}

#[test]
fn component_registration_validation_is_read_only_and_matches_commit_checks() {
    let mut registry = ComponentRegistry::new();
    let position_id = ComponentTypeId::new("nara.test.Position");
    let alias_id = ComponentTypeId::new("nara.test.PositionAlias");

    registry
        .validate_component_registration::<Position>(&position_id)
        .unwrap();
    assert!(registry.schema(&position_id).is_none());

    registry
        .register_component::<Position>(position_id.clone(), ComponentSchemaVersion(1))
        .unwrap();
    assert!(matches!(
        registry.validate_component_registration::<Velocity>(&position_id),
        Err(ComponentRegistryError::DuplicateComponentId(duplicate)) if duplicate == position_id
    ));
    assert!(matches!(
        registry.validate_component_registration::<Position>(&alias_id),
        Err(ComponentRegistryError::DuplicateComponentRustType {
            existing_component_id,
            requested_component_id,
            ..
        }) if existing_component_id == position_id && requested_component_id == alias_id
    ));
    assert_eq!(registry.schemas().count(), 1);
}

#[test]
fn scene_components_require_fields() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");

    let result = registry.register_scene_component_with_fields::<Position, _, _>(
        id.clone(),
        ComponentSchemaVersion(1),
        [],
        |_value| Ok(Position { x: 0.0, y: 0.0 }),
        |_position| Ok(ComponentValue::Map(Default::default())),
    );

    assert!(matches!(
        result,
        Err(ComponentRegistryError::MissingSceneComponentFields { component_id })
            if component_id == id
    ));
}

#[test]
fn rejects_component_field_default_kind_mismatch() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");
    let path = ComponentFieldPath::from_fields(["x"]);

    let result = registry.register_scene_component_with_fields::<Position, _, _>(
        id.clone(),
        ComponentSchemaVersion(1),
        [ComponentFieldSchema::optional_with_default(
            path.clone(),
            ComponentValueKind::F64,
            ComponentValue::String("wrong".to_string()),
        )],
        |_value| Ok(Position { x: 0.0, y: 0.0 }),
        |_position| Ok(ComponentValue::Map(Default::default())),
    );

    assert!(matches!(
        result,
        Err(ComponentRegistryError::InvalidComponentFieldDefault {
            component_id,
            path: failed_path,
            expected: ComponentValueKind::F64,
            actual: ComponentValueKind::String,
        }) if component_id == id && failed_path == path
    ));
}

#[test]
fn exports_schema_catalog_in_deterministic_order() {
    let mut registry = ComponentRegistry::new();
    let position_id = ComponentTypeId::new("nara.test.Position");
    let velocity_id = ComponentTypeId::new("nara.test.Velocity");

    registry
        .register_component::<Position>(position_id.clone(), ComponentSchemaVersion(1))
        .unwrap()
        .register_component::<Velocity>(velocity_id.clone(), ComponentSchemaVersion(1))
        .unwrap()
        .register_component_fields(
            &position_id,
            [
                ComponentFieldSchema::required(
                    ComponentFieldPath::from_fields(["y"]),
                    ComponentValueKind::F64,
                ),
                ComponentFieldSchema::required(
                    ComponentFieldPath::from_fields(["x"]),
                    ComponentValueKind::F64,
                ),
            ],
        )
        .unwrap();

    let catalog = registry.schema_catalog();

    assert_eq!(
        catalog
            .components
            .iter()
            .map(|schema| schema.id.as_str())
            .collect::<Vec<_>>(),
        vec!["nara.test.Position", "nara.test.Velocity"]
    );
    assert_eq!(
        registry
            .schema(&position_id)
            .unwrap()
            .fields
            .iter()
            .map(|field| field.path.to_string())
            .collect::<Vec<_>>(),
        vec!["x", "y"]
    );
}

#[test]
fn migrates_component_values_to_current_schema_version() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");
    registry
        .register_component::<Position>(id.clone(), ComponentSchemaVersion(3))
        .unwrap()
        .register_component_migration(
            &id,
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
        .unwrap()
        .register_component_migration(
            &id,
            ComponentSchemaVersion(2),
            ComponentSchemaVersion(3),
            |value| {
                let ComponentValue::Map(mut fields) = value else {
                    return Err(ComponentCodecError::invalid_field("<root>", "map"));
                };
                let x = fields
                    .remove("x2")
                    .ok_or_else(|| ComponentCodecError::missing_field("x2"))?;
                fields.insert("x3".to_string(), x);
                Ok(ComponentValue::Map(fields))
            },
        )
        .unwrap();

    let migrated = registry
        .migrate_component_value(
            &id,
            ComponentSchemaVersion(1),
            &ComponentValue::map([("x", ComponentValue::I64(5))]),
        )
        .unwrap();

    assert_eq!(migrated.version, ComponentSchemaVersion(3));
    assert_eq!(migrated.value.field_i64("x3").unwrap(), 5);
}

#[test]
fn reports_missing_component_migration_chain() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");
    registry
        .register_component::<Position>(id.clone(), ComponentSchemaVersion(2))
        .unwrap();

    let error = registry
        .migrate_component_value(
            &id,
            ComponentSchemaVersion(1),
            &ComponentValue::Map(BTreeMap::new()),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        ComponentMigrationError::MissingMigration {
            from_version: ComponentSchemaVersion(1),
            target_version: ComponentSchemaVersion(2),
            ..
        }
    ));
}

#[test]
fn rejects_non_finite_component_floats() {
    assert_eq!(
        ComponentValue::f64(f64::NAN),
        Err(ComponentValueError::NonFiniteFloat)
    );
}

#[test]
fn component_field_path_display_keeps_structured_segments() {
    let path = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("sprite"),
        ComponentFieldPathSegment::field("color"),
        ComponentFieldPathSegment::field("r"),
        ComponentFieldPathSegment::index(0),
    ]);

    assert_eq!(path.to_string(), "sprite.color.r[0]");
    assert_eq!(
        path.segments(),
        &[
            ComponentFieldPathSegment::Field("sprite".to_string()),
            ComponentFieldPathSegment::Field("color".to_string()),
            ComponentFieldPathSegment::Field("r".to_string()),
            ComponentFieldPathSegment::Index(0),
        ]
    );
}

#[test]
fn component_value_sets_nested_map_field() {
    let mut value = ComponentValue::map([(
        "sprite",
        ComponentValue::map([(
            "color",
            ComponentValue::map([("r", ComponentValue::I64(1))]),
        )]),
    )]);
    let path = ComponentFieldPath::from_fields(["sprite", "color", "r"]);

    let previous = value.set_path(&path, ComponentValue::I64(9)).unwrap();

    assert_eq!(previous, Some(ComponentValue::I64(1)));
    assert_eq!(value.get_path(&path).unwrap().as_i64(), Some(9));
}

#[test]
fn component_value_reports_missing_field_with_path_context() {
    let value = ComponentValue::map([("sprite", ComponentValue::Map(BTreeMap::new()))]);
    let path = ComponentFieldPath::from_fields(["sprite", "color", "r"]);

    let error = value.get_path(&path).unwrap_err();

    assert_eq!(
        error,
        ComponentFieldPathError::MissingField {
            path: ComponentFieldPath::from_fields(["sprite", "color"]),
            field: "color".to_string(),
        }
    );
}

#[test]
fn component_value_reports_wrong_container_kind() {
    let value = ComponentValue::map([("sprite", ComponentValue::Bool(true))]);
    let path = ComponentFieldPath::from_fields(["sprite", "color"]);

    let error = value.get_path(&path).unwrap_err();

    assert_eq!(
        error,
        ComponentFieldPathError::ExpectedMap {
            path: ComponentFieldPath::from_fields(["sprite"]),
        }
    );
}

#[test]
fn component_value_gets_list_indices_and_reports_bounds() {
    let value = ComponentValue::map([(
        "cells",
        ComponentValue::List(vec![ComponentValue::map([(
            "tile",
            ComponentValue::U64(7),
        )])]),
    )]);
    let valid_path = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("cells"),
        ComponentFieldPathSegment::index(0),
        ComponentFieldPathSegment::field("tile"),
    ]);
    let invalid_path = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("cells"),
        ComponentFieldPathSegment::index(1),
    ]);

    assert_eq!(value.get_path(&valid_path).unwrap().as_u64(), Some(7));
    assert_eq!(
        value.get_path(&invalid_path).unwrap_err(),
        ComponentFieldPathError::IndexOutOfBounds {
            path: invalid_path,
            index: 1,
            len: 1,
        }
    );
}

#[test]
fn invalid_component_value_path_does_not_mutate_original() {
    let mut value = ComponentValue::map([("sprite", ComponentValue::Bool(true))]);
    let original = value.clone();
    let path = ComponentFieldPath::from_fields(["sprite", "color"]);

    let error = value
        .set_path(&path, ComponentValue::String("red".to_string()))
        .unwrap_err();

    assert_eq!(
        error,
        ComponentFieldPathError::ExpectedMap {
            path: ComponentFieldPath::from_fields(["sprite"]),
        }
    );
    assert_eq!(value, original);
}

#[test]
fn preflights_applies_and_encodes_scene_component() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");
    registry
        .register_scene_component_with_fields::<Position, _, _>(
            id.clone(),
            ComponentSchemaVersion(1),
            [
                ComponentFieldSchema::required(
                    ComponentFieldPath::from_fields(["x"]),
                    ComponentValueKind::F64,
                ),
                ComponentFieldSchema::required(
                    ComponentFieldPath::from_fields(["y"]),
                    ComponentValueKind::F64,
                ),
            ],
            |value| {
                let x = value
                    .get("x")
                    .and_then(ComponentValue::as_f64)
                    .ok_or_else(|| ComponentCodecError::invalid_field("x", "finite float"))?
                    as f32;
                let y = value
                    .get("y")
                    .and_then(ComponentValue::as_f64)
                    .ok_or_else(|| ComponentCodecError::invalid_field("y", "finite float"))?
                    as f32;
                Ok(Position { x, y })
            },
            |position| {
                Ok(ComponentValue::map([
                    ("x", ComponentValue::f64(f64::from(position.x))?),
                    ("y", ComponentValue::f64(f64::from(position.y))?),
                ]))
            },
        )
        .unwrap();

    let schema = registry.schema(&id).unwrap();
    assert!(schema.has_capability(ComponentCapability::Scene));
    assert!(schema.has_capability(ComponentCapability::Inspect));
    assert!(schema.has_capability(ComponentCapability::Edit));
    assert!(
        schema
            .fields
            .iter()
            .all(|field| field.has_capability(ComponentCapability::Edit))
    );

    let value = ComponentValue::map([
        ("x", ComponentValue::f64(2.0).unwrap()),
        ("y", ComponentValue::f64(3.0).unwrap()),
    ]);
    let prepared = registry.preflight_component(&id, &value).unwrap().unwrap();
    let mut world = World::new();
    let entity = world.spawn_empty().id();

    prepared.apply(&mut world, entity).unwrap();

    assert_eq!(world.get::<Position>(entity).unwrap().x, 2.0);
    assert_eq!(
        registry
            .encode_component(&id, &world, entity)
            .unwrap()
            .unwrap(),
        Some(value)
    );
}

#[test]
fn failed_prepared_component_does_not_mutate_existing_world_state() {
    let mut world = World::new();
    world.insert_resource(ExistingApplyState(1));
    let entity = world.spawn_empty().id();
    let prepared = PreparedComponent::new(|_context| {
        Err::<Position, _>(ComponentCodecError::invalid_field("apply", "success"))
    });

    let result = prepared.apply(&mut world, entity);

    assert!(result.is_err());
    assert_eq!(world.resource::<ExistingApplyState>().0, 1);
    assert!(world.get::<Position>(entity).is_none());
}

#[test]
fn component_batch_rejects_a_different_world_with_equal_entity_bits() {
    let mut source = World::new();
    let source_entity = source.spawn_empty().id();
    let batch = ComponentApplyBatch::from_world(&source)
        .stage(
            source_entity,
            PreparedComponent::insert(Position { x: 1.0, y: 2.0 }),
        )
        .unwrap();
    let mut target = World::new();
    let target_entity = target.spawn_empty().id();
    assert_eq!(source_entity, target_entity);

    let result = batch.commit(&mut target);

    assert!(matches!(result, Err(ComponentCodecError::WrongWorld)));
    assert!(source.get::<Position>(source_entity).is_none());
    assert!(target.get::<Position>(target_entity).is_none());
}

#[test]
fn missing_batch_target_rejects_every_component_and_scratch_asset() {
    let mut world = World::new();
    let first = world.spawn_empty().id();
    let missing = world.spawn_empty().id();
    let mut batch = ComponentApplyBatch::from_world(&world);
    let asset_ref = AssetRef::path("textures/staged.png").unwrap();
    batch.with_decode_context(None, |context| {
        context
            .resolve_asset_ref::<TestAsset>(&asset_ref)
            .unwrap()
            .unwrap();
    });
    let batch = batch
        .stage(
            first,
            PreparedComponent::insert(Position { x: 1.0, y: 2.0 }),
        )
        .unwrap()
        .stage(
            missing,
            PreparedComponent::insert(Velocity { dx: 3.0, dy: 4.0 }),
        )
        .unwrap();
    world.despawn(missing);

    let result = batch.commit(&mut world);

    assert!(matches!(result, Err(ComponentCodecError::EntityMissing)));
    assert!(world.get::<Position>(first).is_none());
    assert!(world.get_resource::<AssetServer>().is_none());
}

#[test]
fn asset_server_drift_rejects_the_batch_without_overwriting_newer_state() {
    let mut world = World::new();
    let mut original = AssetServer::new();
    original
        .reserve::<TestAsset>("textures/existing.png")
        .unwrap();
    world.insert_resource(original);
    let entity = world.spawn_empty().id();
    let mut batch = ComponentApplyBatch::from_world(&world);
    let staged_ref = AssetRef::path("textures/staged.png").unwrap();
    let staged = batch.with_decode_context(None, |context| {
        context
            .resolve_asset_ref::<TestAsset>(&staged_ref)
            .unwrap()
            .unwrap()
    });
    let batch = batch
        .stage(
            entity,
            PreparedComponent::insert(Position { x: 1.0, y: 2.0 }),
        )
        .unwrap();
    let live = world
        .resource_mut::<AssetServer>()
        .reserve::<TestAsset>("textures/live.png")
        .unwrap();
    assert_eq!(staged.id(), live.id());

    let result = batch.commit(&mut world);

    assert!(matches!(
        result,
        Err(ComponentCodecError::AssetServerChanged)
    ));
    assert!(world.get::<Position>(entity).is_none());
    assert_eq!(
        world.resource::<AssetServer>().path(live.id()),
        Some("textures/live.png")
    );
}

#[test]
fn component_batch_publishes_components_and_scratch_assets_together() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let mut batch = ComponentApplyBatch::from_world(&world);
    let asset_ref = AssetRef::path("textures/staged.png").unwrap();
    let handle = batch.with_decode_context(None, |context| {
        context
            .resolve_asset_ref::<TestAsset>(&asset_ref)
            .unwrap()
            .unwrap()
    });
    let batch = batch
        .stage(
            entity,
            PreparedComponent::insert(Position { x: 1.0, y: 2.0 }),
        )
        .unwrap();

    batch.commit(&mut world).unwrap();

    assert_eq!(world.get::<Position>(entity).unwrap().x, 1.0);
    assert_eq!(
        world.resource::<AssetServer>().path(handle.id()),
        Some("textures/staged.png")
    );
}

fn entity_reference_schema(fields: Vec<ComponentFieldSchema>) -> ComponentSchema {
    ComponentSchema {
        id: ComponentTypeId::new("nara.test.EntityLinks"),
        version: ComponentSchemaVersion(1),
        rust_type_path: "nara_test::EntityLinks".to_owned(),
        capabilities: [ComponentCapability::Scene].into_iter().collect(),
        fields,
    }
}

fn scene_reference(value: &str) -> EntityReference {
    EntityReference::SceneLocal {
        entity: SceneEntityId::new(value).unwrap(),
    }
}

#[test]
fn declared_entity_reference_rewrite_is_typed_and_failure_atomic() {
    let path = ComponentFieldPath::from_fields(["target"]);
    let schema = entity_reference_schema(vec![
        ComponentFieldSchema::required(path.clone(), ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
    ]);
    let original = ComponentValue::map([(
        "target",
        ComponentValue::EntityReference(scene_reference("old")),
    )]);

    let rewritten = rewrite_declared_entity_references(
        &schema,
        &original,
        EntityReferenceTraversalLimits::default(),
        |observed_path, reference| {
            assert_eq!(observed_path, &path);
            assert_eq!(reference, &scene_reference("old"));
            Ok::<_, ()>(scene_reference("new"))
        },
    )
    .unwrap();

    assert_eq!(
        original.field_entity_reference("target").unwrap(),
        &scene_reference("old")
    );
    assert_eq!(
        rewritten.field_entity_reference("target").unwrap(),
        &scene_reference("new")
    );

    let failed = rewrite_declared_entity_references(
        &schema,
        &original,
        EntityReferenceTraversalLimits::default(),
        |_path, _reference| Err::<EntityReference, _>("unresolved"),
    );
    assert!(matches!(
        failed,
        Err(ComponentEntityReferenceRewriteError::Rewrite { .. })
    ));
    assert_eq!(
        original.field_entity_reference("target").unwrap(),
        &scene_reference("old")
    );
}

#[test]
fn entity_reference_rewrite_validates_every_field_before_callbacks() {
    let first = ComponentFieldPath::from_fields(["first"]);
    let missing = ComponentFieldPath::from_fields(["missing"]);
    let schema = entity_reference_schema(vec![
        ComponentFieldSchema::required(first.clone(), ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::required(missing, ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
    ]);
    let value = ComponentValue::map([(
        "first",
        ComponentValue::EntityReference(scene_reference("target")),
    )]);
    let callbacks = Cell::new(0);

    let result = rewrite_declared_entity_references(
        &schema,
        &value,
        EntityReferenceTraversalLimits::default(),
        |_path, reference| {
            callbacks.set(callbacks.get() + 1);
            Ok::<_, ()>(reference.clone())
        },
    );

    assert!(result.is_err());
    assert_eq!(callbacks.get(), 0);
}

#[test]
fn entity_reference_rewrite_rejects_duplicate_schema_paths() {
    let path = ComponentFieldPath::from_fields(["target"]);
    let schema = entity_reference_schema(vec![
        ComponentFieldSchema::required(path.clone(), ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::optional(path, ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
    ]);
    let value = ComponentValue::map([(
        "target",
        ComponentValue::EntityReference(scene_reference("target")),
    )]);
    let callbacks = Cell::new(0);

    let result = rewrite_declared_entity_references(
        &schema,
        &value,
        EntityReferenceTraversalLimits::default(),
        |_path, reference| {
            callbacks.set(callbacks.get() + 1);
            Ok::<_, ()>(reference.clone())
        },
    );

    assert!(result.is_err());
    assert_eq!(callbacks.get(), 0);
}

#[test]
fn undeclared_nested_entity_reference_is_rejected() {
    let schema = entity_reference_schema(vec![ComponentFieldSchema::required(
        ComponentFieldPath::from_fields(["payload"]),
        ComponentValueKind::Map,
    )]);
    let value = ComponentValue::map([(
        "payload",
        ComponentValue::map([(
            "hidden",
            ComponentValue::List(vec![ComponentValue::EntityReference(scene_reference(
                "target",
            ))]),
        )]),
    )]);

    let result = rewrite_declared_entity_references(
        &schema,
        &value,
        EntityReferenceTraversalLimits::default(),
        |_path, reference| Ok::<_, ()>(reference.clone()),
    );

    assert!(matches!(
        result,
        Err(ComponentEntityReferenceRewriteError::UndeclaredReference { .. })
    ));
}

#[test]
fn entity_reference_rewrite_enforces_node_and_depth_limits() {
    let schema = entity_reference_schema(Vec::new());
    let value = ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::Null])]);

    let node_result = rewrite_declared_entity_references(
        &schema,
        &value,
        EntityReferenceTraversalLimits::new(
            ItemLimit::new(2).unwrap(),
            ByteLimit::new(64).unwrap(),
            DepthLimit::new(8).unwrap(),
        ),
        |_path, reference| Ok::<_, ()>(reference.clone()),
    );
    assert!(matches!(
        node_result,
        Err(ComponentEntityReferenceRewriteError::NodeLimit { maximum: 2 })
    ));

    let depth_result = rewrite_declared_entity_references(
        &schema,
        &value,
        EntityReferenceTraversalLimits::new(
            ItemLimit::new(8).unwrap(),
            ByteLimit::new(64).unwrap(),
            DepthLimit::new(2).unwrap(),
        ),
        |_path, reference| Ok::<_, ()>(reference.clone()),
    );
    assert!(matches!(
        depth_result,
        Err(ComponentEntityReferenceRewriteError::DepthLimit { maximum: 2 })
    ));
}

#[test]
fn root_entity_reference_rewrite_accepts_exact_traversal_limits() {
    let schema = entity_reference_schema(vec![
        ComponentFieldSchema::required(ComponentFieldPath::empty(), ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
    ]);
    let original = ComponentValue::EntityReference(scene_reference("target"));
    let exact = EntityReferenceTraversalLimits::new(
        ItemLimit::new(1).unwrap(),
        ByteLimit::new("target".len()).unwrap(),
        DepthLimit::new(1).unwrap(),
    );

    let rewritten =
        rewrite_declared_entity_references(&schema, &original, exact, |path, reference| {
            assert!(path.is_empty());
            assert_eq!(reference, &scene_reference("target"));
            Ok::<_, ()>(scene_reference("replacement"))
        })
        .unwrap();

    assert_eq!(
        rewritten.as_entity_reference(),
        Some(&scene_reference("replacement"))
    );
    assert_eq!(
        original.as_entity_reference(),
        Some(&scene_reference("target"))
    );

    let too_small = rewrite_declared_entity_references(
        &schema,
        &original,
        EntityReferenceTraversalLimits::new(
            ItemLimit::new(1).unwrap(),
            ByteLimit::new("target".len() - 1).unwrap(),
            DepthLimit::new(1).unwrap(),
        ),
        |_path, reference| Ok::<_, ()>(reference.clone()),
    );
    assert!(matches!(
        too_small,
        Err(ComponentEntityReferenceRewriteError::ByteLimit { maximum: 5 })
    ));
}

#[test]
fn entity_reference_rewrite_distinguishes_optional_and_invalid_fields() {
    let path = ComponentFieldPath::from_fields(["target"]);
    let optional = entity_reference_schema(vec![
        ComponentFieldSchema::optional(path.clone(), ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
    ]);
    for value in [
        ComponentValue::Map(BTreeMap::new()),
        ComponentValue::map([("target", ComponentValue::Null)]),
    ] {
        assert_eq!(
            rewrite_declared_entity_references(
                &optional,
                &value,
                EntityReferenceTraversalLimits::default(),
                |_path, reference| Ok::<_, ()>(reference.clone()),
            )
            .unwrap(),
            value
        );
    }

    let required = entity_reference_schema(vec![
        ComponentFieldSchema::required(path.clone(), ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
    ]);
    let null = rewrite_declared_entity_references(
        &required,
        &ComponentValue::map([("target", ComponentValue::Null)]),
        EntityReferenceTraversalLimits::default(),
        |_path, reference| Ok::<_, ()>(reference.clone()),
    );
    assert!(matches!(
        null,
        Err(ComponentEntityReferenceRewriteError::RequiredReferenceMissing { .. })
    ));

    let invalid = rewrite_declared_entity_references(
        &required,
        &ComponentValue::map([("target", ComponentValue::String("target".to_string()))]),
        EntityReferenceTraversalLimits::default(),
        |_path, reference| Ok::<_, ()>(reference.clone()),
    );
    assert!(matches!(
        invalid,
        Err(
            ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                actual: ComponentValueKind::String,
                ..
            }
        )
    ));

    let missing_capability = entity_reference_schema(vec![ComponentFieldSchema::required(
        path,
        ComponentValueKind::EntityRef,
    )]);
    let result = rewrite_declared_entity_references(
        &missing_capability,
        &ComponentValue::map([(
            "target",
            ComponentValue::EntityReference(scene_reference("target")),
        )]),
        EntityReferenceTraversalLimits::default(),
        |_path, reference| Ok::<_, ()>(reference.clone()),
    );
    assert!(matches!(
        result,
        Err(ComponentEntityReferenceRewriteError::MissingEntityRefCapability { .. })
    ));
}

#[test]
fn parallel_fork_remap_rewrites_reflected_reference_candidates_atomically() {
    fn persistent(value: &str) -> PersistentRuntimeReference {
        PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str(value).unwrap(),
        )
    }

    let source_persistent = persistent("11111111-1111-4111-8111-111111111111");
    let target_persistent = persistent("22222222-2222-4222-8222-222222222222");
    let entity_id = SceneEntityId::new("player").unwrap();
    let mut source_world = World::new();
    let source_domain =
        WorldIdentityDomain::new(&source_world, WorldIdentityDomainSettings::default()).unwrap();
    source_world.insert_resource(source_domain);
    let source_token = spawn_identity_entity(&mut source_world).unwrap();
    let mut source_domain = source_world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    let source_instance = source_domain
        .register_new_scene_instance(&source_world, [(entity_id.clone(), source_token)])
        .unwrap();
    source_domain
        .register_persistent(&source_world, source_token, source_persistent.clone())
        .unwrap();
    source_world.insert_resource(source_domain);
    let snapshot = source_world
        .resource::<WorldIdentityDomain>()
        .scene_identity_snapshot(&source_world, &source_instance)
        .unwrap();

    let mut target_world = World::new();
    let target_domain =
        WorldIdentityDomain::new(&target_world, WorldIdentityDomainSettings::default()).unwrap();
    target_world.insert_resource(target_domain);
    let target_token = spawn_identity_entity(&mut target_world).unwrap();
    let mut target_domain = target_world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    let (_, locator_remap) = target_domain
        .register_parallel_scene_fork(
            &target_world,
            &snapshot,
            [(
                entity_id.clone(),
                target_token,
                Some(target_persistent.clone()),
            )],
        )
        .unwrap();
    target_world.insert_resource(target_domain);

    let peer_path = ComponentFieldPath::from_fields(["peer"]);
    let persistent_path = ComponentFieldPath::from_fields(["persistent"]);
    let schema = entity_reference_schema(vec![
        ComponentFieldSchema::required(peer_path, ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::required(persistent_path, ComponentValueKind::EntityRef)
            .with_capability(ComponentCapability::EntityRef),
    ]);
    let original = ComponentValue::map([
        (
            "peer",
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: entity_id.clone(),
            }),
        ),
        (
            "persistent",
            ComponentValue::EntityReference(EntityReference::Persistent {
                entity: source_persistent.clone(),
            }),
        ),
    ]);

    let rewritten = remap_declared_entity_references(
        &schema,
        &original,
        EntityReferenceTraversalLimits::default(),
        locator_remap.references(),
    )
    .unwrap();

    assert_eq!(
        rewritten.field_entity_reference("peer").unwrap(),
        &EntityReference::SceneLocal { entity: entity_id }
    );
    assert_eq!(
        rewritten.field_entity_reference("persistent").unwrap(),
        &EntityReference::Persistent {
            entity: target_persistent,
        }
    );
    assert_eq!(
        original.field_entity_reference("persistent").unwrap(),
        &EntityReference::Persistent {
            entity: source_persistent,
        }
    );
}

#[cfg(feature = "serde")]
#[test]
fn entity_reference_component_values_have_canonical_serde_shapes() {
    use nara_identity::{
        PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
    };

    let scene = ComponentValue::EntityReference(scene_reference("root/player"));
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
        ),
    });

    let scene_json = serde_json::to_string(&scene).unwrap();
    let persistent_json = serde_json::to_string(&persistent).unwrap();
    assert_eq!(
        scene_json,
        r#"{"type":"entity_ref","value":{"kind":"scene_local","entity":"root/player"}}"#
    );
    assert_eq!(
        persistent_json,
        r#"{"type":"entity_ref","value":{"kind":"persistent","entity":{"namespace":"save","entity":"2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f"}}}"#
    );
    assert_eq!(
        serde_json::from_str::<ComponentValue>(&scene_json).unwrap(),
        scene
    );
    assert_eq!(
        serde_json::from_str::<ComponentValue>(&persistent_json).unwrap(),
        persistent
    );
    assert!(
        serde_json::from_str::<ComponentValue>(
            r#"{"type":"entity_ref","value":{"kind":"scene","instance":1,"entity":"root/player"}}"#,
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<ComponentValue>(
            r#"{"type":"entity_reference","value":{"kind":"scene_local","entity":"root/player"}}"#,
        )
        .is_err()
    );
}
