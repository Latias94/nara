use std::{
    borrow::Cow,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentDecodeContext, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathSegment, ComponentFieldSchema, ComponentRegistry,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, ComponentValueShape, DeclaredAssetReferenceError,
    EntityReferenceTraversalLimits, collect_declared_asset_references,
    collect_declared_asset_references_from_schema_with_limits, effective_component_value,
    effective_component_value_owned, validate_component_value,
    validate_component_value_with_limits,
};

#[derive(Debug, PartialEq, Component)]
struct DefaultAmount(u64);

#[derive(Component)]
struct DefaultImage;

#[test]
fn required_default_is_effective_before_native_preflight() {
    let component = ComponentTypeId::new("nara.test.DefaultAmount");
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("amount"),
        "Amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    field.default_value = Some(ComponentValue::U64(7));
    let schema = ComponentSchema::new(
        component.clone(),
        "Default amount",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field]);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<DefaultAmount, _, _>(
            schema.clone(),
            |value| Ok(DefaultAmount(value.field_u64("amount")?)),
            |amount| {
                Ok(ComponentValue::map([(
                    "amount",
                    ComponentValue::U64(amount.0),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    let omitted = ComponentValue::Map(Default::default());
    let source = omitted.clone();
    assert!(validate_component_value(&schema, &omitted).is_ok());

    let prepared = snapshot
        .preflight_component(&component, &omitted)
        .expect("native preflight must receive the admitted schema default");
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    prepared.apply(&mut world, entity).unwrap();

    assert_eq!(world.get::<DefaultAmount>(entity), Some(&DefaultAmount(7)));
    assert_eq!(omitted, source);
}

#[test]
fn whole_component_asset_collection_includes_omitted_default() {
    let component = ComponentTypeId::new("nara.test.DefaultImage");
    let default = ComponentValue::map([
        ("kind", ComponentValue::String("path".to_owned())),
        (
            "value",
            ComponentValue::String("textures/default.png".to_owned()),
        ),
    ]);
    let schema = ComponentSchema::new(
        component.clone(),
        "Default image",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::from_fields(["image"]),
        ComponentValueKind::AssetRef,
        default.clone(),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef)]);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<DefaultImage, _, _>(
            schema,
            |_| Ok(DefaultImage),
            |_| Ok(ComponentValue::Map(Default::default())),
        )
        .unwrap();
    registry.freeze().unwrap();
    let explicit = ComponentValue::map([("image", default)]);
    let omitted = ComponentValue::Map(Default::default());
    let source = omitted.clone();
    let explicit_references = collect_declared_asset_references(
        &registry,
        &component,
        ComponentSchemaVersion::ONE,
        &explicit,
    )
    .unwrap();
    assert_eq!(explicit_references.len(), 1);
    assert_eq!(
        explicit_references[0].asset_ref(),
        &AssetRef::path("textures/default.png").unwrap(),
    );

    let default_references = collect_declared_asset_references(
        &registry,
        &component,
        ComponentSchemaVersion::ONE,
        &omitted,
    )
    .unwrap();

    assert_eq!(default_references, explicit_references);
    assert_eq!(omitted, source);
}

fn materialization_limits(
    nodes: usize,
    bytes: usize,
    depth: usize,
) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn default_field(
    id: &str,
    path: ComponentFieldPath,
    default: ComponentValue,
) -> ComponentFieldSchema {
    let mut field =
        ComponentFieldSchema::required(ComponentFieldId::new(id), id, path, default.kind());
    field.default_value = Some(default);
    field
}

fn materialization_schema(
    fields: impl IntoIterator<Item = ComponentFieldSchema>,
) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.EffectiveValue"),
        "Effective value",
        ComponentSchemaVersion::ONE,
    )
    .with_fields(fields)
}

#[test]
fn primitive_materializes_defaults_without_validating_unrelated_fields() {
    let schema = materialization_schema([
        default_field(
            "required",
            ComponentFieldPath::from_fields(["required"]),
            ComponentValue::U64(7),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("optional"),
            "Optional",
            ComponentFieldPath::from_fields(["optional"]),
            ComponentValueKind::U64,
            ComponentValue::U64(9),
        ),
        ComponentFieldSchema::required(
            ComponentFieldId::new("missing"),
            "Missing",
            ComponentFieldPath::from_fields(["missing"]),
            ComponentValueKind::U64,
        ),
        ComponentFieldSchema::optional(
            ComponentFieldId::new("absent"),
            "Absent",
            ComponentFieldPath::from_fields(["absent"]),
            ComponentValueKind::U64,
        ),
    ]);
    let raw = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let effective = effective_component_value(
        &schema,
        &raw,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap();
    assert_eq!(
        effective.as_ref(),
        &ComponentValue::map([
            ("required", ComponentValue::U64(7)),
            ("optional", ComponentValue::U64(9)),
        ])
    );
    assert!(raw.get("required").is_none());
    assert!(effective.get("missing").is_none());
    assert!(effective.get("absent").is_none());
    assert!(validate_component_value(&schema, &effective).is_err());
}

#[test]
fn explicit_null_and_explicit_values_are_never_replaced() {
    let schema = materialization_schema([
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("nullable"),
            "Nullable",
            ComponentFieldPath::from_fields(["nullable"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        ),
        default_field(
            "explicit",
            ComponentFieldPath::from_fields(["explicit"]),
            ComponentValue::U64(9),
        ),
    ]);
    let raw = ComponentValue::map([
        ("nullable", ComponentValue::Null),
        ("explicit", ComponentValue::Bool(false)),
    ]);
    let effective =
        effective_component_value(&schema, &raw, materialization_limits(1, 1, 1)).unwrap();
    assert!(matches!(effective, Cow::Borrowed(_)));
    assert_eq!(effective.as_ref(), &raw);
    assert!(validate_component_value(&schema, &raw).is_err());
}

#[test]
fn missing_map_ancestors_are_shared_and_field_order_is_canonical() {
    let fields = [
        default_field(
            "right",
            ComponentFieldPath::from_fields(["settings", "right"]),
            ComponentValue::U64(2),
        ),
        default_field(
            "left",
            ComponentFieldPath::from_fields(["settings", "left"]),
            ComponentValue::U64(1),
        ),
    ];
    let schema = materialization_schema(fields.clone());
    let reversed = materialization_schema(fields.into_iter().rev());
    let raw = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let expected = ComponentValue::map([(
        "settings",
        ComponentValue::map([
            ("left", ComponentValue::U64(1)),
            ("right", ComponentValue::U64(2)),
        ]),
    )]);
    let limits = materialization_limits(4, 33, 3);
    assert_eq!(
        effective_component_value(&schema, &raw, limits)
            .unwrap()
            .as_ref(),
        &expected
    );
    assert_eq!(
        effective_component_value(&reversed, &raw, limits)
            .unwrap()
            .as_ref(),
        &expected
    );
    for (limits, expected) in [
        (
            materialization_limits(3, 33, 3),
            ComponentCodecError::MaterializationNodeLimit {
                observed: 4,
                maximum: 3,
            },
        ),
        (
            materialization_limits(4, 32, 3),
            ComponentCodecError::MaterializationByteLimit {
                observed: 33,
                maximum: 32,
            },
        ),
        (
            materialization_limits(4, 33, 2),
            ComponentCodecError::MaterializationDepthLimit {
                observed: 3,
                maximum: 2,
            },
        ),
    ] {
        assert_eq!(
            effective_component_value(&schema, &raw, limits).unwrap_err(),
            expected,
        );
        assert_eq!(
            effective_component_value_owned(&schema, raw.clone(), limits).unwrap_err(),
            expected,
        );
    }
    assert_eq!(raw, ComponentValue::map([] as [(&str, ComponentValue); 0]));
}

#[test]
fn explicit_null_or_wrong_container_ancestors_reject_without_overwrite() {
    let schema = materialization_schema([default_field(
        "amount",
        ComponentFieldPath::from_fields(["settings", "amount"]),
        ComponentValue::U64(7),
    )]);
    for parent in [
        ComponentValue::Null,
        ComponentValue::Bool(true),
        ComponentValue::List(Vec::new()),
    ] {
        let raw = ComponentValue::map([("settings", parent)]);
        let original = raw.clone();
        assert!(matches!(
            effective_component_value(
                &schema,
                &raw,
                ComponentValueMaterializationLimits::default()
            ),
            Err(ComponentCodecError::InvalidField { .. } | ComponentCodecError::Message(_)),
        ));
        assert_eq!(raw, original);
    }
}

#[test]
fn list_defaults_construct_contiguous_suffixes_and_nested_containers() {
    let path = |index, field| {
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("rows"),
            ComponentFieldPathSegment::index(index),
            ComponentFieldPathSegment::field(field),
        ])
    };
    let schema = materialization_schema([
        default_field("second", path(1, "amount"), ComponentValue::U64(2)),
        default_field("first", path(0, "amount"), ComponentValue::U64(1)),
        default_field(
            "label",
            path(0, "label"),
            ComponentValue::String("first".to_owned()),
        ),
    ]);
    let empty = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let expected = ComponentValue::map([(
        "rows",
        ComponentValue::List(vec![
            ComponentValue::map([
                ("amount", ComponentValue::U64(1)),
                ("label", ComponentValue::String("first".to_owned())),
            ]),
            ComponentValue::map([("amount", ComponentValue::U64(2))]),
        ]),
    )]);
    assert_eq!(
        effective_component_value(
            &schema,
            &empty,
            ComponentValueMaterializationLimits::default()
        )
        .unwrap()
        .as_ref(),
        &expected
    );
    let existing = ComponentValue::map([(
        "rows",
        ComponentValue::List(vec![ComponentValue::map([
            ("amount", ComponentValue::U64(99)),
            ("label", ComponentValue::String("authored".to_owned())),
        ])]),
    )]);
    let result = effective_component_value_owned(
        &schema,
        existing,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap();
    let rows = result
        .get_path(&ComponentFieldPath::from_fields(["rows"]))
        .unwrap();
    let ComponentValue::List(rows) = rows else {
        panic!("rows is a List")
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("amount"), Some(&ComponentValue::U64(99)));
    assert_eq!(rows[1].get("amount"), Some(&ComponentValue::U64(2)));
}

#[test]
fn sparse_list_default_holes_are_rejected() {
    let schema = materialization_schema([default_field(
        "second",
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("rows"),
            ComponentFieldPathSegment::index(1),
        ]),
        ComponentValue::U64(2),
    )]);
    for raw in [
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
        ComponentValue::map([("rows", ComponentValue::List(Vec::new()))]),
    ] {
        let error = effective_component_value(
            &schema,
            &raw,
            ComponentValueMaterializationLimits::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("sparse hole"));
    }
}

#[test]
fn materialization_charges_raw_values_and_default_subtree_depth_before_clone() {
    let schema = materialization_schema([default_field(
        "addition",
        ComponentFieldPath::from_fields(["addition"]),
        ComponentValue::List(vec![ComponentValue::String("abc".to_owned())]),
    )]);
    let raw = ComponentValue::map([("kept", ComponentValue::String("hello".to_owned()))]);
    let exact = materialization_limits(4, 20, 3);
    let effective = effective_component_value(&schema, &raw, exact).unwrap();
    assert_eq!(effective.cost().nodes(), 4);
    assert_eq!(effective.cost().logical_bytes(), 20);
    for (limits, expected) in [
        (
            materialization_limits(3, 20, 3),
            ComponentCodecError::MaterializationNodeLimit {
                observed: 4,
                maximum: 3,
            },
        ),
        (
            materialization_limits(4, 19, 3),
            ComponentCodecError::MaterializationByteLimit {
                observed: 20,
                maximum: 19,
            },
        ),
        (
            materialization_limits(4, 20, 2),
            ComponentCodecError::MaterializationDepthLimit {
                observed: 3,
                maximum: 2,
            },
        ),
    ] {
        assert_eq!(
            effective_component_value(&schema, &raw, limits).unwrap_err(),
            expected,
        );
        assert_eq!(
            effective_component_value_owned(&schema, raw.clone(), limits).unwrap_err(),
            expected,
        );
    }
    assert!(raw.get("addition").is_none());
}

#[test]
fn default_free_large_opaque_values_are_borrowed_or_moved_without_scanning() {
    let schema = materialization_schema([ComponentFieldSchema::required(
        ComponentFieldId::new("payload"),
        "Payload",
        ComponentFieldPath::from_fields(["payload"]),
        ComponentValueShape::opaque(ComponentValueKind::List),
    )]);
    let raw = ComponentValue::map([(
        "payload",
        ComponentValue::List(vec![ComponentValue::U64(1); 17_000]),
    )]);
    let list_pointer = match raw.get("payload").unwrap() {
        ComponentValue::List(values) => values.as_ptr(),
        _ => unreachable!(),
    };
    let tiny = materialization_limits(1, 1, 1);
    let borrowed = effective_component_value(&schema, &raw, tiny).unwrap();
    assert!(matches!(borrowed, Cow::Borrowed(_)));
    let moved = effective_component_value_owned(&schema, raw, tiny).unwrap();
    let ComponentValue::List(values) = moved.get("payload").unwrap() else {
        panic!("payload is a List")
    };
    assert_eq!(values.as_ptr(), list_pointer);
}

#[test]
fn owned_materialization_keeps_existing_scalar_allocation() {
    let schema = materialization_schema([default_field(
        "amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValue::U64(7),
    )]);
    let raw = ComponentValue::map([(
        "label",
        ComponentValue::String("retained allocation".to_owned()),
    )]);
    let original_pointer = match raw.get("label").unwrap() {
        ComponentValue::String(label) => label.as_ptr(),
        _ => unreachable!(),
    };
    let effective = effective_component_value_owned(
        &schema,
        raw,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap();
    let ComponentValue::String(label) = effective.get("label").unwrap() else {
        panic!("label is a String")
    };
    assert_eq!(label.as_ptr(), original_pointer);
    assert_eq!(effective.get("amount"), Some(&ComponentValue::U64(7)));
}

#[test]
fn large_raw_opaque_payload_uses_the_selected_expansion_budget() {
    let schema = materialization_schema([default_field(
        "amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValue::U64(7),
    )]);
    let raw = ComponentValue::map([(
        "payload",
        ComponentValue::List(vec![ComponentValue::U64(1); 17_000]),
    )]);
    let expected_nodes = 17_003;
    let expected_bytes = 17_000 * 8 + 7 + 6 + 8;
    let exact = materialization_limits(expected_nodes, expected_bytes, 3);
    let effective = effective_component_value(&schema, &raw, exact).unwrap();
    assert_eq!(effective.cost().nodes(), expected_nodes);
    assert_eq!(effective.cost().logical_bytes(), expected_bytes);
    assert_eq!(
        effective_component_value(
            &schema,
            &raw,
            materialization_limits(16_384, expected_bytes, 3),
        )
        .unwrap_err(),
        ComponentCodecError::MaterializationNodeLimit {
            observed: 16_385,
            maximum: 16_384,
        },
    );
    assert!(raw.get("amount").is_none());
}

#[test]
fn deep_raw_values_are_rejected_iteratively_when_expansion_is_needed() {
    let schema = materialization_schema([default_field(
        "amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValue::U64(7),
    )]);
    let mut payload = ComponentValue::Null;
    for _ in 0..4096 {
        payload = ComponentValue::List(vec![payload]);
    }
    let mut raw = ComponentValue::map([("payload", payload)]);
    let error = effective_component_value(
        &schema,
        &raw,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        ComponentCodecError::MaterializationDepthLimit {
            observed: 65,
            maximum: 64,
        },
    );
    let ComponentValue::Map(fields) = &mut raw else {
        unreachable!()
    };
    let mut payload = fields.remove("payload").unwrap();
    while let ComponentValue::List(mut values) = payload {
        payload = values.pop().unwrap();
    }
}

#[test]
fn explicit_deep_opaque_values_keep_the_no_insertion_fast_path() {
    let schema = materialization_schema([default_field(
        "amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValue::U64(7),
    )]);
    let mut payload = ComponentValue::Null;
    for _ in 0..4096 {
        payload = ComponentValue::List(vec![payload]);
    }
    let mut raw = ComponentValue::map([("amount", ComponentValue::U64(8)), ("payload", payload)]);
    let effective =
        effective_component_value(&schema, &raw, materialization_limits(1, 1, 1)).unwrap();
    assert!(matches!(effective, Cow::Borrowed(_)));
    drop(effective);
    let ComponentValue::Map(fields) = &mut raw else {
        unreachable!()
    };
    let mut payload = fields.remove("payload").unwrap();
    while let ComponentValue::List(mut values) = payload {
        payload = values.pop().unwrap();
    }
}

#[test]
fn decode_context_budget_rejects_before_the_native_callback() {
    let component = ComponentTypeId::new("nara.test.BoundedDefaultAmount");
    let schema = ComponentSchema::new(
        component.clone(),
        "Bounded default",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([default_field(
        "amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValue::U64(7),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let calls = Arc::new(AtomicUsize::new(0));
    let decode_calls = Arc::clone(&calls);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<DefaultAmount, _, _>(
            schema.clone(),
            move |value| {
                decode_calls.fetch_add(1, Ordering::SeqCst);
                Ok(DefaultAmount(value.field_u64("amount")?))
            },
            |amount| {
                Ok(ComponentValue::map([(
                    "amount",
                    ComponentValue::U64(amount.0),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    let omitted = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let exact = materialization_limits(2, 14, 2);
    let small = materialization_limits(1, 14, 2);
    let mut context = ComponentDecodeContext::new().with_materialization_limits(small);
    assert_eq!(context.materialization_limits(), small);
    let expected = ComponentCodecError::MaterializationNodeLimit {
        observed: 2,
        maximum: 1,
    };
    assert_eq!(
        snapshot
            .preflight_component_with_context(&component, &omitted, &mut context)
            .err()
            .expect("materialization must reject before native decoding"),
        expected,
    );
    assert_eq!(
        validate_component_value_with_limits(&schema, &omitted, small).unwrap_err(),
        expected,
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    context.set_materialization_limits(exact);
    assert_eq!(context.materialization_limits(), exact);
    assert!(validate_component_value_with_limits(&schema, &omitted, exact).is_ok());
    assert!(
        snapshot
            .preflight_component_with_context(&component, &omitted, &mut context)
            .is_ok()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(omitted.get("amount").is_none());
}

#[test]
fn asset_defaults_keep_partial_record_semantics_and_separate_work_limits() {
    let asset_value = ComponentValue::map([
        ("kind", ComponentValue::String("path".to_owned())),
        (
            "value",
            ComponentValue::String("textures/default.png".to_owned()),
        ),
    ]);
    let schema = materialization_schema([
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("image"),
            "Image",
            ComponentFieldPath::from_fields(["image"]),
            ComponentValueKind::AssetRef,
            asset_value,
        )
        .with_capability(ComponentCapability::AssetRef),
        ComponentFieldSchema::required(
            ComponentFieldId::new("unrelated"),
            "Unrelated",
            ComponentFieldPath::from_fields(["unrelated"]),
            ComponentValueKind::U64,
        ),
        ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueKind::List,
        ),
    ]);
    let raw = ComponentValue::map([(
        "payload",
        ComponentValue::List(vec![ComponentValue::U64(1); 17_000]),
    )]);
    let materialization = materialization_limits(20_000, 200_000, 8);
    let traversal = EntityReferenceTraversalLimits::new(
        ItemLimit::new(100_000).unwrap(),
        ByteLimit::new(4 * 1024 * 1024).unwrap(),
        DepthLimit::new(8).unwrap(),
    );
    let references = collect_declared_asset_references_from_schema_with_limits(
        &schema,
        schema.version(),
        &raw,
        materialization,
        traversal,
    )
    .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(
        references[0].asset_ref(),
        &AssetRef::path("textures/default.png").unwrap()
    );
    assert!(validate_component_value_with_limits(&schema, &raw, materialization).is_err());
    assert!(matches!(
        collect_declared_asset_references_from_schema_with_limits(
            &schema,
            schema.version(),
            &raw,
            materialization_limits(16_384, 200_000, 8),
            traversal,
        ),
        Err(DeclaredAssetReferenceError::Materialization { .. }),
    ));
    assert!(matches!(
        collect_declared_asset_references_from_schema_with_limits(
            &schema,
            schema.version(),
            &raw,
            materialization,
            EntityReferenceTraversalLimits::default(),
        ),
        Err(DeclaredAssetReferenceError::Traversal { .. }),
    ));
    assert!(raw.get("image").is_none());
}
