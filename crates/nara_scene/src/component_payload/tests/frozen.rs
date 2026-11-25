use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use nara_reflect::{
    ComponentCapability, ComponentFieldPath, ComponentFieldSchema, ComponentSchema,
    ComponentValueKind, ComponentValueShape,
    effective::{effective_component_value_with_default_overrides, freeze_component_defaults},
};

use super::*;
use crate::effective_component_payload::EffectiveComponentPayload;

fn schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.FrozenPayload"),
        "Frozen payload",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("items"),
            "Items",
            ComponentFieldPath::from_fields(["items"]),
            ComponentValueShape::list(ComponentValueKind::String.into()),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("revision"),
            "Revision",
            ComponentFieldPath::from_fields(["revision"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn source_value() -> ComponentValue {
    ComponentValue::map([(
        "items",
        ComponentValue::List(vec![ComponentValue::String("cell".repeat(32)); 1_024]),
    )])
}

#[test]
fn frozen_payload_keeps_exact_backing_and_isolates_explicit_mutation() {
    let schema = schema();
    let limits = Default::default();
    let mut source = SceneComponentRecord::new(ComponentSchemaVersion::ONE, source_value());
    let raw = source.clone_for_compilation().value.raw().unwrap().clone();
    let original = Arc::downgrade(&raw.0);
    let (expected, cost) = effective_component_value_with_default_overrides(
        &schema,
        source.value(),
        &BTreeMap::new(),
        limits,
    )
    .unwrap();
    let expected = expected.into_owned();
    let defaults = freeze_component_defaults(&schema, source.value(), &BTreeMap::new(), limits)
        .unwrap()
        .unwrap();
    assert_eq!(Some(defaults.cost()), cost);
    let expected_storage = raw.retained_bytes()
        + EffectiveComponentPayload::defaulted_storage_bytes(defaults.retained_bytes());
    let frozen = EffectiveComponentPayload::with_defaults(raw, defaults);
    let mut isolated = frozen.clone();
    assert_eq!(frozen.read(), &expected);
    assert_eq!(frozen.read().cost(), expected.cost());
    assert!(frozen.read().as_value().is_none());
    assert!(ptr::eq(
        source.value().get("items").unwrap(),
        frozen.read().get("items").unwrap().as_value().unwrap(),
    ));
    assert_eq!(frozen.retained_bytes(), expected_storage);
    assert_eq!(isolated.retained_bytes(), expected_storage);
    isolated
        .materialize_mut()
        .set_path(
            &ComponentFieldPath::from_fields(["revision"]),
            ComponentValue::U64(9),
        )
        .unwrap();
    assert!(!ptr::eq(
        isolated.read().get("items").unwrap().as_value().unwrap(),
        frozen.read().get("items").unwrap().as_value().unwrap(),
    ));
    assert_eq!(isolated.read().field_u64("revision").unwrap(), 9);
    assert_eq!(frozen.read().field_u64("revision").unwrap(), 7);
    source
        .value_mut()
        .set_path(
            &ComponentFieldPath::from_fields(["items"]),
            ComponentValue::List(Vec::new()),
        )
        .unwrap();
    assert_eq!(frozen.read(), &expected);
    drop(source);
    assert!(original.upgrade().is_some());
    assert_eq!(frozen.read(), &expected);
    drop(frozen);
    assert!(original.upgrade().is_none());
    assert_eq!(isolated.read().field_u64("revision").unwrap(), 9);
}

#[derive(nara_ecs::Component)]
struct FrozenNative;

#[test]
fn actual_native_preflight_reads_frozen_defaults_and_original_list_backing() {
    let schema = schema();
    let component = schema.id().clone();
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("root").unwrap(),
    )
    .with_component(
        component.clone(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, source_value()),
    )]);
    let original_items = source.entities[0].components[&component]
        .value()
        .get("items")
        .unwrap() as *const ComponentValue as usize;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_with_codec::<FrozenNative, _, _>(
            schema,
            move |value| {
                assert!(value.as_value().is_none());
                assert_eq!(value.field_u64("revision")?, 7);
                assert_eq!(
                    value.field("items")?.as_value().unwrap() as *const ComponentValue as usize,
                    original_items,
                );
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(FrozenNative)
            },
            |_| Ok(source_value()),
        )
        .unwrap();
    let registry = registry.freeze().unwrap();
    for diagnostics in [
        source.validate(&registry),
        source.validate_authoring(&registry),
    ] {
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        source.entities[0].components[&component]
            .value()
            .get("revision")
            .is_none()
    );
}

#[cfg(feature = "serde")]
#[test]
fn frozen_reads_serialize_the_same_owned_wire_without_changing_source() {
    let schema = schema();
    let source = ComponentPayload::from(source_value());
    let expected = effective_component_value_with_default_overrides(
        &schema,
        &source,
        &BTreeMap::new(),
        Default::default(),
    )
    .unwrap()
    .0
    .into_owned();
    let defaults =
        freeze_component_defaults(&schema, &source, &BTreeMap::new(), Default::default())
            .unwrap()
            .unwrap();
    let frozen = EffectiveComponentPayload::with_defaults(source.clone(), defaults);
    assert_eq!(
        serde_json::to_string(&frozen.read()).unwrap(),
        serde_json::to_string(&expected).unwrap()
    );
    assert_eq!(
        ron::to_string(&frozen.read()).unwrap(),
        ron::to_string(&expected).unwrap()
    );
    assert_eq!(&*source, &source_value());
    let independent = frozen.read().to_owned();
    assert!(!ptr::eq(
        independent.get("items").unwrap(),
        frozen.read().get("items").unwrap().as_value().unwrap(),
    ));
    drop(frozen);
    drop(source);
    assert_eq!(independent, expected);
}
