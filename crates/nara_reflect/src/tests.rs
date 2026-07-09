use std::collections::BTreeMap;

use nara_ecs::{Component, World};

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
    assert!(!registry.schema(&id).unwrap().serializable);
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
fn serializable_components_require_fields() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");

    let result = registry.register_serializable_component_with_fields::<Position, _, _>(
        id.clone(),
        ComponentSchemaVersion(1),
        [],
        |_value| Ok(Position { x: 0.0, y: 0.0 }),
        |_position| Ok(ComponentValue::Map(Default::default())),
    );

    assert!(matches!(
        result,
        Err(ComponentRegistryError::MissingSerializableComponentFields { component_id })
            if component_id == id
    ));
}

#[test]
fn rejects_component_field_default_kind_mismatch() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");
    let path = ComponentFieldPath::from_fields(["x"]);

    let result = registry.register_serializable_component_with_fields::<Position, _, _>(
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
fn preflights_applies_and_encodes_serializable_component() {
    let mut registry = ComponentRegistry::new();
    let id = ComponentTypeId::new("nara.test.Position");
    registry
        .register_serializable_component_with_fields::<Position, _, _>(
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

    assert!(registry.schema(&id).unwrap().serializable);

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
