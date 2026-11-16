use std::collections::BTreeMap;

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SourceUniverseId};
use nara_identity::{
    AssetSourcePath, AuthoredEntityReference, AuthoredReferenceTarget, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, PrefabEntityOrigin,
    PrefabInstanceHop, ReferenceInputId, SceneEntityId, SourceDocumentKey, SourceDocumentKind,
    StableAssetId,
};

use super::{
    effective_component_value_in_place_with_default_overrides, effective_component_value_owned,
    effective_component_value_owned_with_default_overrides,
    effective_component_value_with_default_overrides,
    validate_native_component_value_references_with_limits, validate_value_materialization_budget,
};
use crate::{
    ComponentCodecError, ComponentFieldId, ComponentFieldPath, ComponentFieldPathSegment,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueShape, EntityReference,
};

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes.max(1)).unwrap(),
        ByteLimit::new(bytes.max(1)).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
    .remaining(usize::from(nodes == 0), usize::from(bytes == 0))
}

#[test]
fn raw_cost_matches_value_cost_for_scalars_and_containers() {
    for (value, bytes) in [
        (ComponentValue::Null, 0),
        (ComponentValue::Bool(false), 1),
        (ComponentValue::Bool(true), 1),
        (ComponentValue::I64(i64::MIN), 8),
        (ComponentValue::U64(u64::MAX), 8),
        (ComponentValue::f64(-0.0).unwrap(), 8),
        (ComponentValue::String(String::new()), 0),
        (ComponentValue::String("snow: \u{96ea}".into()), 9),
        (ComponentValue::List(Vec::new()), 0),
        (ComponentValue::Map(BTreeMap::new()), 0),
        (
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: SceneEntityId::new("local").unwrap(),
            }),
            5,
        ),
        (
            ComponentValue::EntityReference(EntityReference::Persistent {
                entity: PersistentRuntimeReference::new(
                    PersistentRuntimeNamespaceId::new("save").unwrap(),
                    PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
                ),
            }),
            20,
        ),
    ] {
        let expected = value.cost();
        assert_eq!(expected.nodes(), 1);
        assert_eq!(expected.logical_bytes(), bytes);
        assert_eq!(
            validate_value_materialization_budget(&value, limits(1, bytes, 1)),
            Ok(expected),
        );
    }

    let value = ComponentValue::map([
        ("empty", ComponentValue::Map(BTreeMap::new())),
        (
            "\u{e9}",
            ComponentValue::List(vec![
                ComponentValue::Null,
                ComponentValue::Bool(true),
                ComponentValue::I64(-7),
                ComponentValue::U64(8),
                ComponentValue::f64(1.5).unwrap(),
                ComponentValue::String("\u{96ea}".into()),
            ]),
        ),
    ]);
    let expected = value.cost();
    assert_eq!(expected.nodes(), 9);
    assert_eq!(expected.logical_bytes(), 35);
    assert_eq!(
        validate_value_materialization_budget(&value, limits(9, 35, 3)),
        Ok(expected),
    );
}

#[test]
fn source_cost_counts_complete_authored_routes_without_native_rejection() {
    let universe: SourceUniverseId = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let value = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("outer").unwrap()),
                    expected_base: SourceDocumentKey::named(
                        universe,
                        "reusable",
                        SourceDocumentKind::Prefab,
                    )
                    .unwrap(),
                },
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::InstanceAddition(
                        SceneEntityId::new("inner").unwrap(),
                    ),
                    expected_base: SourceDocumentKey::project_location(
                        universe,
                        AssetSourcePath::new("parts/socket.prefab").unwrap(),
                        SourceDocumentKind::Prefab,
                    ),
                },
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("leaf").unwrap()),
                    expected_base: SourceDocumentKey::stable_asset(
                        StableAssetId::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
                        SourceDocumentKind::Prefab,
                    ),
                },
            ],
            target: AuthoredReferenceTarget::Input(ReferenceInputId::new("socket").unwrap()),
        },
    });
    let bytes = "socket".len()
        + "outer".len()
        + "inner".len()
        + "leaf".len()
        + "reusable".len()
        + "parts/socket.prefab".len()
        + 3 * (36 + 1);
    let expected = value.cost();
    assert_eq!(expected.nodes(), 8);
    assert_eq!(expected.logical_bytes(), bytes);
    assert_eq!(
        validate_value_materialization_budget(&value, limits(8, bytes, 1)),
        Ok(expected),
    );
    assert_eq!(
        validate_native_component_value_references_with_limits(&value, limits(8, bytes, 1)),
        Err(ComponentCodecError::UnresolvedAuthoredReference),
    );
    assert_eq!(
        validate_value_materialization_budget(&value, limits(7, 1, 1)),
        Err(ComponentCodecError::MaterializationNodeLimit {
            observed: 8,
            maximum: 7,
        }),
    );
    assert_eq!(
        validate_value_materialization_budget(&value, limits(8, bytes - 1, 1)),
        Err(ComponentCodecError::MaterializationByteLimit {
            observed: bytes,
            maximum: bytes - 1,
        }),
    );

    let nested = ComponentValue::map([("routes", ComponentValue::List(vec![value]))]);
    let expected = nested.cost();
    assert_eq!(
        validate_value_materialization_budget(
            &nested,
            limits(expected.nodes(), expected.logical_bytes(), 3),
        ),
        Ok(expected),
    );
}

#[test]
fn raw_cost_refusals_preserve_observed_prefix_and_order() {
    let list = ComponentValue::List(vec![ComponentValue::String("abc".into())]);
    let map = ComponentValue::map([("key", ComponentValue::Null)]);
    let wide = ComponentValue::List(vec![ComponentValue::Null; 4]);
    for (value, limits, error) in [
        (
            &list,
            limits(1, 2, 1),
            ComponentCodecError::MaterializationDepthLimit {
                observed: 2,
                maximum: 1,
            },
        ),
        (
            &list,
            limits(1, 2, 2),
            ComponentCodecError::MaterializationNodeLimit {
                observed: 2,
                maximum: 1,
            },
        ),
        (
            &list,
            limits(2, 2, 2),
            ComponentCodecError::MaterializationByteLimit {
                observed: 3,
                maximum: 2,
            },
        ),
        (
            &map,
            limits(1, 2, 1),
            ComponentCodecError::MaterializationByteLimit {
                observed: 3,
                maximum: 2,
            },
        ),
        (
            &wide,
            limits(2, 0, 2),
            ComponentCodecError::MaterializationNodeLimit {
                observed: 3,
                maximum: 2,
            },
        ),
        (
            &list,
            limits(0, 0, 1),
            ComponentCodecError::MaterializationNodeLimit {
                observed: 1,
                maximum: 0,
            },
        ),
    ] {
        assert_eq!(
            validate_value_materialization_budget(value, limits),
            Err(error)
        );
    }
}

#[test]
fn inserted_defaults_return_exact_cost_for_nested_containers_and_overrides() {
    let row_path = |index, name| {
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("rows"),
            ComponentFieldPathSegment::index(index),
            ComponentFieldPathSegment::field(name),
        ])
    };
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.DefaultCost"),
        "Default cost",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("retained"),
            "Retained",
            ComponentFieldPath::from_fields(["retained"]),
            ComponentValueKind::String,
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("label"),
            "Label",
            row_path(0, "label"),
            ComponentValueKind::String,
            ComponentValue::String("canonical".into()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("enabled"),
            "Enabled",
            row_path(1, "enabled"),
            ComponentValueKind::Bool,
            ComponentValue::Bool(true),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("mode"),
            "Mode",
            ComponentFieldPath::from_fields(["settings", "mode"]),
            ComponentValueKind::String,
            ComponentValue::String("auto".into()),
        ),
    ]);
    let overrides = BTreeMap::from([(
        ComponentFieldId::new("label"),
        ComponentValue::String("chosen".into()),
    )]);
    let retained_overrides = overrides.clone();
    let raw = ComponentValue::map([("retained", ComponentValue::String("kept".into()))]);
    let expected = ComponentValue::map([
        ("retained", ComponentValue::String("kept".into())),
        (
            "rows",
            ComponentValue::List(vec![
                ComponentValue::map([("label", ComponentValue::String("chosen".into()))]),
                ComponentValue::map([("enabled", ComponentValue::Bool(true))]),
            ]),
        ),
        (
            "settings",
            ComponentValue::map([("mode", ComponentValue::String("auto".into()))]),
        ),
    ]);
    let cost = expected.cost();
    assert_eq!(cost.nodes(), 9);
    assert_eq!(cost.logical_bytes(), 51);
    for (limits, error) in [
        (
            limits(8, 51, 4),
            ComponentCodecError::MaterializationNodeLimit {
                observed: 9,
                maximum: 8,
            },
        ),
        (
            limits(9, 50, 4),
            ComponentCodecError::MaterializationByteLimit {
                observed: 51,
                maximum: 50,
            },
        ),
        (
            limits(9, 51, 3),
            ComponentCodecError::MaterializationDepthLimit {
                observed: 4,
                maximum: 3,
            },
        ),
    ] {
        assert_eq!(
            effective_component_value_owned_with_default_overrides(
                &schema,
                raw.clone(),
                &overrides,
                limits,
            ),
            Err(error.clone()),
        );
        assert_eq!(
            effective_component_value_with_default_overrides(&schema, &raw, &overrides, limits),
            Err(error.clone()),
        );
        let mut candidate = raw.clone();
        assert_eq!(
            effective_component_value_in_place_with_default_overrides(
                &schema,
                &mut candidate,
                &overrides,
                limits,
            ),
            Err(error),
        );
        assert_eq!(candidate, raw, "preflight refusal precedes all mutation");
    }
    let (borrowed_result, borrowed_cost) = effective_component_value_with_default_overrides(
        &schema,
        &raw,
        &overrides,
        limits(9, 51, 4),
    )
    .unwrap();
    assert!(matches!(borrowed_result, std::borrow::Cow::Owned(_)));
    assert_eq!(borrowed_result.as_ref(), &expected);
    assert_eq!(borrowed_cost, Some(cost));
    let mut isolated = raw.clone();
    let retained = isolated.get("retained").unwrap().as_str().unwrap().as_ptr();
    assert_eq!(
        effective_component_value_in_place_with_default_overrides(
            &schema,
            &mut isolated,
            &overrides,
            limits(9, 51, 4),
        )
        .unwrap(),
        Some(cost),
    );
    assert_eq!(isolated, expected);
    assert_eq!(
        isolated.get("retained").unwrap().as_str().unwrap().as_ptr(),
        retained
    );
    let (value, observed) = effective_component_value_owned_with_default_overrides(
        &schema,
        raw,
        &overrides,
        limits(9, 51, 4),
    )
    .unwrap();
    assert_eq!(value, expected);
    assert_eq!(observed, Some(cost));
    assert_eq!(observed, Some(value.cost()));
    assert_eq!(overrides, retained_overrides);
}

#[test]
fn owned_defaults_return_none_and_keep_allocations_without_insertions() {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.UnchangedCost"),
        "Unchanged cost",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueShape::opaque(ComponentValueKind::List),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("label"),
            "Label",
            ComponentFieldPath::from_fields(["label"]),
            ComponentValueKind::String,
            ComponentValue::String("canonical".into()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("optional"),
            "Optional",
            ComponentFieldPath::from_fields(["optional"]),
            ComponentValueShape::optional(ComponentValueKind::String.into()),
            ComponentValue::String("fallback".into()),
        ),
    ]);
    let overrides = BTreeMap::from([
        (
            ComponentFieldId::new("label"),
            ComponentValue::String("contextual".into()),
        ),
        (
            ComponentFieldId::new("optional"),
            ComponentValue::String("hidden".into()),
        ),
    ]);
    let retained_overrides = overrides.clone();
    let payload = vec![ComponentValue::U64(7); 17_000];
    let payload_pointer = payload.as_ptr();
    let label = "retained allocation".to_owned();
    let label_pointer = label.as_ptr();
    let mut raw = ComponentValue::map([
        ("payload", ComponentValue::List(payload)),
        ("label", ComponentValue::String(label)),
        ("optional", ComponentValue::Null),
    ]);
    let raw_cost = raw.cost();
    assert_eq!(
        validate_value_materialization_budget(
            &raw,
            limits(raw_cost.nodes(), raw_cost.logical_bytes(), 3),
        ),
        Ok(raw_cost),
    );
    for value in overrides.values() {
        let cost = value.cost();
        assert_eq!(
            validate_value_materialization_budget(
                value,
                limits(cost.nodes(), cost.logical_bytes(), 1),
            ),
            Ok(cost),
        );
    }
    let tiny = limits(1, 1, 1);
    assert_eq!(
        effective_component_value_in_place_with_default_overrides(
            &schema, &mut raw, &overrides, tiny
        )
        .unwrap(),
        None,
    );
    let (borrowed, cost) =
        effective_component_value_with_default_overrides(&schema, &raw, &overrides, tiny).unwrap();
    assert!(matches!(borrowed, std::borrow::Cow::Borrowed(_)));
    assert!(std::ptr::eq(borrowed.as_ref(), &raw));
    assert_eq!(cost, None);
    let (value, cost) =
        effective_component_value_owned_with_default_overrides(&schema, raw, &overrides, tiny)
            .unwrap();
    assert_eq!(cost, None);
    let value: ComponentValue = effective_component_value_owned(&schema, value, tiny).unwrap();
    let ComponentValue::List(payload) = value.get("payload").unwrap() else {
        panic!("payload is a List");
    };
    assert_eq!(payload.as_ptr(), payload_pointer);
    assert_eq!(payload.len(), 17_000);
    assert!(payload.iter().all(|item| item == &ComponentValue::U64(7)));
    let ComponentValue::String(label) = value.get("label").unwrap() else {
        panic!("label is a String");
    };
    assert_eq!(label.as_ptr(), label_pointer);
    assert_eq!(label, "retained allocation");
    assert_eq!(value.get("optional"), Some(&ComponentValue::Null));
    assert_eq!(overrides, retained_overrides);
}
