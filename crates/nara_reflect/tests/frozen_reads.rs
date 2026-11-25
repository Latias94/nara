use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::identity::{EntityReference, SceneEntityId};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentDecodeContext, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathSegment as Segment, ComponentFieldSchema,
    ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits,
    ComponentValueRef, ComponentValueShape, PersistentComponent,
    effective::{
        EffectiveValueAdmissionError, effective_component_value_with_default_overrides,
        freeze_component_defaults, freeze_component_defaults_with_admission,
        validate_native_component_value_references_with_limits,
        validate_value_materialization_budget,
    },
    rewrite_declared_entity_references, validate_component_value,
    validate_component_value_with_limits,
};

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

#[test]
fn reference_copy_rewrites_frozen_defaults_without_retaining_or_changing_the_input() {
    let before = EntityReference::SceneLocal {
        entity: SceneEntityId::new("before").unwrap(),
    };
    let after = EntityReference::SceneLocal {
        entity: SceneEntityId::new("after").unwrap(),
    };
    let schema = schema([
        default_field(
            "target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValue::EntityReference(before.clone()),
        )
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::EntityRef]),
        ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueShape::list(ComponentValueKind::String.into()),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ]);
    let source = ComponentValue::map([(
        "payload",
        ComponentValue::List(vec![ComponentValue::String("unchanged".repeat(32)); 64]),
    )]);
    let original = source.clone();
    let frozen = freeze_component_defaults(
        &schema,
        &source,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap()
    .unwrap();
    let read = frozen.read(&source);
    let mut calls = 0;
    let rewritten =
        rewrite_declared_entity_references(&schema, read, Default::default(), |path, reference| {
            calls += 1;
            assert_eq!(path, &ComponentFieldPath::from_fields(["target"]));
            assert_eq!(reference, &before);
            Ok::<_, ()>(after.clone())
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(rewritten.field_entity_reference("target").unwrap(), &after);
    assert_eq!(read.field_entity_reference("target").unwrap(), &before);
    assert!(!std::ptr::eq(
        rewritten.field("payload").unwrap(),
        read.field("payload").unwrap().as_value().unwrap(),
    ));
    let mut refused_calls = 0;
    let refused = rewrite_declared_entity_references(
        &schema,
        read,
        nara_reflect::EntityReferenceTraversalLimits::new(
            ItemLimit::ONE,
            ByteLimit::new(1024 * 1024).unwrap(),
            DepthLimit::new(16).unwrap(),
        ),
        |_, reference| {
            refused_calls += 1;
            Ok::<_, ()>(reference.clone())
        },
    );
    assert!(refused.is_err());
    assert_eq!(refused_calls, 0);
    assert_eq!(source, original);
    drop(frozen);
    drop(source);
    assert_eq!(
        rewritten.field("payload").unwrap(),
        original.field("payload").unwrap()
    );
    assert_eq!(rewritten.field_entity_reference("target").unwrap(), &after);
}

fn default_field(
    id: &str,
    path: ComponentFieldPath,
    value: ComponentValue,
) -> ComponentFieldSchema {
    let mut field =
        ComponentFieldSchema::required(ComponentFieldId::new(id), id, path, value.kind())
            .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    field.default_value = Some(value);
    field
}

fn schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.FrozenRead"),
        "Frozen read",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(fields)
}

fn row_path(index: u32) -> ComponentFieldPath {
    ComponentFieldPath::new([
        Segment::field("rows"),
        Segment::index(index),
        Segment::field("amount"),
    ])
}

fn nested_fixture() -> (
    ComponentSchema,
    ComponentValue,
    BTreeMap<ComponentFieldId, ComponentValue>,
) {
    let schema = schema([
        default_field("second", row_path(1), ComponentValue::U64(2)),
        default_field("first", row_path(0), ComponentValue::U64(1)),
        default_field(
            "label",
            ComponentFieldPath::from_fields(["settings", "label"]),
            ComponentValue::String("schema".into()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("nullable"),
            "Nullable",
            ComponentFieldPath::from_fields(["nullable"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        ),
        ComponentFieldSchema::optional(
            ComponentFieldId::new("absent"),
            "Absent",
            ComponentFieldPath::from_fields(["absent"]),
            ComponentValueKind::U64,
        ),
    ]);
    let raw = ComponentValue::map([
        (
            "rows",
            ComponentValue::List(vec![ComponentValue::map([(
                "amount",
                ComponentValue::U64(99),
            )])]),
        ),
        ("nullable", ComponentValue::Null),
    ]);
    let overrides = BTreeMap::from([
        (
            ComponentFieldId::new("label"),
            ComponentValue::String("context".into()),
        ),
        (ComponentFieldId::new("first"), ComponentValue::U64(44)),
        (ComponentFieldId::new("nullable"), ComponentValue::U64(88)),
    ]);
    (schema, raw, overrides)
}

#[test]
fn borrowed_scalar_accessors_preserve_values_and_report_field_errors() {
    let reference = EntityReference::SceneLocal {
        entity: SceneEntityId::new("target").unwrap(),
    };
    let raw = ComponentValue::map([
        ("bool", ComponentValue::Bool(true)),
        ("signed", ComponentValue::I64(-2)),
        ("unsigned", ComponentValue::U64(3)),
        ("float", ComponentValue::f64(0.5).unwrap()),
        ("text", ComponentValue::String("borrowed".into())),
        (
            "reference",
            ComponentValue::EntityReference(reference.clone()),
        ),
    ]);
    let read = ComponentValueRef::from(&raw);
    let copied = read;
    assert!(std::ptr::eq(copied.as_value().unwrap(), &raw));
    assert!(read.field_bool("bool").unwrap());
    assert_eq!(read.field_i64("signed").unwrap(), -2);
    assert_eq!(read.field_u64("unsigned").unwrap(), 3);
    assert_eq!(read.field_f64("float").unwrap(), 0.5);
    assert_eq!(read.field_str("text").unwrap(), "borrowed");
    assert_eq!(
        read.field_entity_reference("reference").unwrap(),
        &reference
    );
    assert_eq!(
        read.field_u64("text"),
        Err(ComponentCodecError::invalid_field("text", "u64"))
    );
    assert_eq!(read.cost(), raw.cost());
}

#[test]
fn frozen_nested_maps_and_list_suffixes_match_the_ordinary_materializer() {
    let (schema, raw, overrides) = nested_fixture();
    let original = raw.clone();
    let allowance = ComponentValueMaterializationLimits::default();
    let (ordinary, cost) =
        effective_component_value_with_default_overrides(&schema, &raw, &overrides, allowance)
            .unwrap();
    let frozen = freeze_component_defaults(&schema, &raw, &overrides, allowance)
        .unwrap()
        .unwrap();
    let read = frozen.read(&raw);
    assert!(read.as_value().is_none());
    assert!(read.field("settings").unwrap().as_value().is_none());
    assert!(read.field("rows").unwrap().as_value().is_none());
    assert_eq!(read, ordinary.as_ref());
    assert_eq!(Some(frozen.cost()), cost);
    assert_eq!(read.cost(), ordinary.cost());
    assert_eq!(
        read.cost_up_to(frozen.cost().nodes(), frozen.cost().logical_bytes()),
        Some(frozen.cost())
    );
    let fields = read.as_map().unwrap();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields.iter().len(), fields.len());
    assert_eq!(
        fields.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        ["nullable", "rows", "settings"]
    );
    let rows = read.field("rows").unwrap().as_list().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().len(), rows.len());
    assert!(rows.get(2).is_none());
    assert_eq!(
        rows.iter()
            .map(|row| row.field_u64("amount").unwrap())
            .collect::<Vec<_>>(),
        [99, 2]
    );
    assert_eq!(read.get_path(&row_path(1)).unwrap().as_u64(), Some(2));
    assert_eq!(
        read.get("nullable").unwrap().kind(),
        ComponentValueKind::Null
    );
    assert!(read.get("absent").is_none());
    assert_eq!(
        read.field("absent"),
        Err(ComponentCodecError::missing_field("absent"))
    );
    assert_eq!(
        read.field("settings").unwrap().field_str("label").unwrap(),
        "context"
    );
    let mut owned = read.to_owned();
    owned
        .replace_path(&row_path(1), ComponentValue::U64(300))
        .unwrap();
    let label_path = ComponentFieldPath::from_fields(["settings", "label"]);
    let ComponentValue::String(label) = owned.get_path_mut(&label_path).unwrap() else {
        panic!("label is a String")
    };
    label.push_str("-edited");
    assert_eq!(
        owned.get_path(&label_path).unwrap().as_str(),
        Some("context-edited")
    );
    assert_eq!(owned.get_path(&row_path(1)).unwrap().as_u64(), Some(300));
    assert_eq!(read.to_owned(), *ordinary);
    assert_eq!(raw, original);
    assert!(
        freeze_component_defaults(&schema, ordinary.as_ref(), &overrides, limits(1, 1, 1))
            .unwrap()
            .is_none()
    );
    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&read).unwrap();
        assert_eq!(json, serde_json::to_string(ordinary.as_ref()).unwrap());
        assert_eq!(
            serde_json::from_str::<ComponentValue>(&json).unwrap(),
            read.to_owned()
        );
        let ron = ron::to_string(&read).unwrap();
        assert_eq!(ron, ron::to_string(ordinary.as_ref()).unwrap());
        assert_eq!(
            ron::from_str::<ComponentValue>(&ron).unwrap(),
            read.to_owned()
        );
    }
}

#[test]
fn incompatible_ancestors_and_sparse_defaults_share_ordinary_refusals() {
    let (component_schema, _, overrides) = nested_fixture();
    let allowance = ComponentValueMaterializationLimits::default();
    for raw in [
        ComponentValue::map([("settings", ComponentValue::Null)]),
        ComponentValue::map([("settings", ComponentValue::Bool(true))]),
        ComponentValue::map([("rows", ComponentValue::Bool(false))]),
    ] {
        let original = raw.clone();
        let error = effective_component_value_with_default_overrides(
            &component_schema,
            &raw,
            &overrides,
            allowance,
        )
        .unwrap_err();
        assert!(matches!(error, ComponentCodecError::InvalidField { .. }));
        assert_eq!(
            freeze_component_defaults(&component_schema, &raw, &overrides, allowance).unwrap_err(),
            error
        );
        assert_eq!(raw, original);
    }
    let sparse = schema([default_field("third", row_path(2), ComponentValue::U64(3))]);
    let raw = ComponentValue::map([("rows", ComponentValue::List(Vec::new()))]);
    let error = effective_component_value_with_default_overrides(
        &sparse,
        &raw,
        &BTreeMap::new(),
        allowance,
    )
    .unwrap_err();
    assert!(
        matches!(&error, ComponentCodecError::Message(message) if message.contains("sparse hole"))
    );
    assert_eq!(
        freeze_component_defaults(&sparse, &raw, &BTreeMap::new(), allowance).unwrap_err(),
        error
    );
    assert!(
        ComponentValueRef::from(&raw)
            .field("rows")
            .unwrap()
            .as_list()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn retained_default_admission_preserves_the_callers_exact_refusal() {
    let (schema, raw, overrides) = nested_fixture();
    let original = raw.clone();
    let allowance = ComponentValueMaterializationLimits::default();
    let mut requests = Vec::new();
    let frozen = freeze_component_defaults_with_admission(
        &schema,
        &raw,
        &overrides,
        allowance,
        |nodes, bytes, allocated| {
            requests.push((nodes, bytes, allocated));
            Ok::<(), &'static str>(())
        },
    )
    .unwrap()
    .unwrap();
    let refusal_at = requests
        .iter()
        .rposition(|(_, _, allocated)| *allocated > 0)
        .unwrap()
        + 1;
    let mut visited = 0;
    let error = freeze_component_defaults_with_admission(
        &schema,
        &raw,
        &overrides,
        allowance,
        |_, _, _| {
            visited += 1;
            if visited == refusal_at {
                Err("retained-default-refused")
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(
        error,
        EffectiveValueAdmissionError::Admission("retained-default-refused")
    );
    assert_eq!(visited, refusal_at);
    assert!(frozen.retained_bytes() > 0);
    assert_eq!(raw, original);
}

#[derive(Component, Debug, PartialEq)]
struct NativeRead {
    amount: u64,
    payload: Vec<u64>,
}

fn native_schema() -> ComponentSchema {
    schema([
        default_field(
            "amount",
            ComponentFieldPath::from_fields(["amount"]),
            ComponentValue::U64(7),
        ),
        ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueKind::List,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn native_registry(raw: &ComponentValue, calls: Arc<AtomicUsize>) -> ComponentRegistrySnapshot {
    let address = std::ptr::from_ref(raw.get("payload").unwrap()) as usize;
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_with_codec::<NativeRead, _, _>(
            native_schema(),
            move |value| {
                calls.fetch_add(1, Ordering::SeqCst);
                assert!(value.as_value().is_none());
                let payload = value.field("payload")?;
                assert_eq!(
                    std::ptr::from_ref(payload.as_value().unwrap()) as usize,
                    address
                );
                Ok(NativeRead {
                    amount: value.field_u64("amount")?,
                    payload: payload
                        .as_list()
                        .unwrap()
                        .iter()
                        .map(|item| item.as_u64().unwrap())
                        .collect(),
                })
            },
            |value| {
                Ok(ComponentValue::map([
                    ("amount", ComponentValue::U64(value.amount)),
                    (
                        "payload",
                        ComponentValue::List(
                            value
                                .payload
                                .iter()
                                .copied()
                                .map(ComponentValue::U64)
                                .collect(),
                        ),
                    ),
                ]))
            },
        )
        .unwrap();
    builder.freeze().unwrap()
}

#[test]
fn actual_native_callback_borrows_large_unchanged_payload_and_returns_owned_components() {
    let raw = ComponentValue::map([(
        "payload",
        ComponentValue::List((0..4096).map(ComponentValue::U64).collect()),
    )]);
    let original = raw.clone();
    let frozen = {
        let mut compiled_schema = native_schema();
        let hidden = ComponentValue::List(vec![ComponentValue::U64(9); 4096]);
        compiled_schema.fields[1].default_value = Some(hidden.clone());
        let overrides = BTreeMap::from([(ComponentFieldId::new("payload"), hidden)]);
        let allowance = ComponentValueMaterializationLimits::default();
        let frozen = freeze_component_defaults(&compiled_schema, &raw, &overrides, allowance)
            .unwrap()
            .unwrap();
        let small = ComponentValue::map([("payload", ComponentValue::List(Vec::new()))]);
        let lean = freeze_component_defaults(&native_schema(), &small, &BTreeMap::new(), allowance)
            .unwrap()
            .unwrap();
        assert_eq!(frozen.retained_bytes(), lean.retained_bytes());
        frozen
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = native_registry(&raw, Arc::clone(&calls));
    let id = native_schema().id().clone();
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    registry
        .preflight_component(&id, frozen.read(&raw))
        .unwrap()
        .apply(&mut world, entity)
        .unwrap();
    let expected = NativeRead {
        amount: 7,
        payload: (0..4096).collect(),
    };
    let effective = frozen.read(&raw).to_owned();
    let mut edited = frozen.read(&raw).to_owned();
    let edit_path = ComponentFieldPath::new([Segment::field("payload"), Segment::index(0)]);
    edited
        .replace_path(&edit_path, ComponentValue::U64(999))
        .unwrap();
    assert_eq!(edited.get_path(&edit_path).unwrap().as_u64(), Some(999));
    assert_eq!(raw, original);
    assert_eq!(frozen.read(&raw), effective);
    assert_eq!(world.get::<NativeRead>(entity), Some(&expected));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(frozen);
    drop(raw);
    assert_eq!(
        registry.encode_component(&id, &world, entity).unwrap(),
        Some(effective)
    );
    assert_eq!(world.get::<NativeRead>(entity), Some(&expected));
}

#[derive(Component, PersistentComponent, Debug, PartialEq)]
#[nara(
    id = "nara.test.DerivedFrozenRead",
    version = 1,
    alias = "Derived frozen read",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct DerivedRead {
    #[nara(id = "rows", alias = "Rows")]
    rows: Vec<BTreeMap<String, u64>>,
    #[nara(id = "settings", alias = "Settings")]
    settings: BTreeMap<String, String>,
    #[nara(id = "nullable", alias = "Nullable")]
    nullable: Option<u64>,
}

#[test]
fn derived_nested_codecs_decode_frozen_containers_through_public_registry() {
    let (compiled_schema, raw, overrides) = nested_fixture();
    let frozen = freeze_component_defaults(
        &compiled_schema,
        &raw,
        &overrides,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap()
    .unwrap();
    drop(compiled_schema);
    drop(overrides);
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component::<DerivedRead>()
        .unwrap();
    let registry = builder.freeze().unwrap();
    let id = ComponentTypeId::new("nara.test.DerivedFrozenRead");
    let expected = DerivedRead {
        rows: vec![
            BTreeMap::from([("amount".into(), 99)]),
            BTreeMap::from([("amount".into(), 2)]),
        ],
        settings: BTreeMap::from([("label".into(), "context".into())]),
        nullable: None,
    };
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    validate_component_value(registry.schema(&id).unwrap(), frozen.read(&raw)).unwrap();
    registry
        .preflight_component(&id, frozen.read(&raw))
        .unwrap()
        .apply(&mut world, entity)
        .unwrap();
    assert_eq!(world.get::<DerivedRead>(entity), Some(&expected));
    let mut edited = frozen.read(&raw).to_owned();
    edited
        .replace_path(&row_path(0), ComponentValue::U64(1))
        .unwrap();
    let successor = world.spawn_empty().id();
    registry
        .preflight_component(&id, &edited)
        .unwrap()
        .apply(&mut world, successor)
        .unwrap();
    assert_eq!(
        world.get::<DerivedRead>(successor).unwrap().rows[0]["amount"],
        1
    );
    assert_eq!(world.get::<DerivedRead>(entity), Some(&expected));
    assert_eq!(
        frozen.read(&raw).get_path(&row_path(0)).unwrap().as_u64(),
        Some(99)
    );
}

#[test]
fn exact_and_one_short_budgets_cover_freezing_and_real_native_validation() {
    let schema = native_schema();
    let raw = ComponentValue::map([(
        "payload",
        ComponentValue::List(vec![ComponentValue::U64(1)]),
    )]);
    let original = raw.clone();
    let exact = limits(4, 29, 3);
    let frozen = freeze_component_defaults(&schema, &raw, &BTreeMap::new(), exact)
        .unwrap()
        .unwrap();
    assert_eq!(frozen.cost().nodes(), 4);
    assert_eq!(frozen.cost().logical_bytes(), 29);
    assert_eq!(
        validate_value_materialization_budget(frozen.read(&raw), exact).unwrap(),
        frozen.cost()
    );
    validate_component_value_with_limits(&schema, &raw, exact).unwrap();
    validate_component_value_with_limits(&schema, frozen.read(&raw), exact).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = native_registry(&raw, Arc::clone(&calls));
    for (allowance, expected) in [
        (
            limits(3, 29, 3),
            ComponentCodecError::MaterializationNodeLimit {
                observed: 4,
                maximum: 3,
            },
        ),
        (
            limits(4, 28, 3),
            ComponentCodecError::MaterializationByteLimit {
                observed: 29,
                maximum: 28,
            },
        ),
        (
            limits(4, 29, 2),
            ComponentCodecError::MaterializationDepthLimit {
                observed: 3,
                maximum: 2,
            },
        ),
    ] {
        assert_eq!(
            freeze_component_defaults(&schema, &raw, &BTreeMap::new(), allowance).unwrap_err(),
            expected
        );
        assert_eq!(
            validate_component_value_with_limits(&schema, &raw, allowance).unwrap_err(),
            expected
        );
        assert_eq!(
            validate_component_value_with_limits(&schema, frozen.read(&raw), allowance)
                .unwrap_err(),
            expected
        );
        assert_eq!(
            validate_value_materialization_budget(frozen.read(&raw), allowance).unwrap_err(),
            expected
        );
        assert_eq!(
            validate_native_component_value_references_with_limits(frozen.read(&raw), allowance)
                .unwrap_err(),
            expected
        );
        let mut context = ComponentDecodeContext::new().with_materialization_limits(allowance);
        assert_eq!(
            registry
                .preflight_component_with_context(schema.id(), &raw, &mut context)
                .err()
                .unwrap(),
            expected
        );
        assert_eq!(
            registry
                .preflight_component_with_context(schema.id(), frozen.read(&raw), &mut context)
                .err()
                .unwrap(),
            expected
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let mut world = World::new();
    for frozen_input in [false, true] {
        let read = if frozen_input {
            frozen.read(&raw)
        } else {
            ComponentValueRef::from(&raw)
        };
        let mut context = ComponentDecodeContext::new().with_materialization_limits(exact);
        let entity = world.spawn_empty().id();
        registry
            .preflight_component_with_context(schema.id(), read, &mut context)
            .unwrap()
            .apply(&mut world, entity)
            .unwrap();
        assert_eq!(
            world.get::<NativeRead>(entity),
            Some(&NativeRead {
                amount: 7,
                payload: vec![1]
            })
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(raw, original);
}
