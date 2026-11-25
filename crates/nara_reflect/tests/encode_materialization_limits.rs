use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::identity::{
    AuthoredEntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId,
    PersistentRuntimeReference, SceneEntityId,
};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentEncodeContext, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathSegment, ComponentFieldSchema, ComponentRegistryBuilder,
    ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueShape,
    EntityReference, PreparedComponentCandidate,
};

const COMPONENT_ID: &str = "nara.test.EncodeMaterialization";

#[derive(Component)]
struct Encoded(ComponentValue);

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn schema(field: ComponentFieldSchema) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new(COMPONENT_ID),
        "Encode materialization",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([field.with_capability(ComponentCapability::Scene)])
}

fn registry(field: ComponentFieldSchema) -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_with_codec::<Encoded, _, _>(
            schema(field),
            |value| Ok(Encoded(value.to_owned())),
            |component| Ok(component.0.clone()),
        )
        .unwrap();
    builder.freeze().unwrap()
}

#[test]
fn omitted_and_explicit_nested_values_obey_context_node_byte_and_depth_limits() {
    let default = ComponentValue::List(vec![ComponentValue::map([(
        "text",
        ComponentValue::String("x".repeat(4096)),
    )])]);
    let expanded = ComponentValue::map([(
        "settings",
        ComponentValue::map([("payload", default.clone())]),
    )]);
    let registry = registry(ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("payload"),
        "Payload",
        ComponentFieldPath::from_fields(["settings", "payload"]),
        ComponentValueKind::List,
        default,
    ));
    let component_id = ComponentTypeId::new(COMPONENT_ID);
    let cost = expanded.cost();
    let nodes = cost.nodes();
    let bytes = cost.logical_bytes();
    let exact = limits(nodes, bytes, 5);
    let rejected = [
        (
            limits(nodes - 1, bytes, 5),
            ComponentCodecError::MaterializationNodeLimit {
                observed: nodes,
                maximum: nodes - 1,
            },
        ),
        (
            limits(nodes, bytes - 1, 5),
            ComponentCodecError::MaterializationByteLimit {
                observed: bytes,
                maximum: bytes - 1,
            },
        ),
        (
            limits(nodes, bytes, 4),
            ComponentCodecError::MaterializationDepthLimit {
                observed: 5,
                maximum: 4,
            },
        ),
    ];
    let mut world = World::new();
    for encoded in [ComponentValue::Map(Default::default()), expanded] {
        let entity = world.spawn(Encoded(encoded.clone())).id();
        for (selected, expected) in &rejected {
            let context = ComponentEncodeContext::new().with_materialization_limits(*selected);
            assert_eq!(context.materialization_limits(), *selected);
            assert_eq!(
                registry.encode_component_with_context(&component_id, &world, entity, &context),
                Err(expected.clone()),
            );
        }
        let context = ComponentEncodeContext::new().with_materialization_limits(exact);
        assert_eq!(
            registry.encode_component_with_context(&component_id, &world, entity, &context),
            Ok(Some(encoded.clone())),
        );
        assert_eq!(world.get::<Encoded>(entity).unwrap().0, encoded);
    }
}

#[test]
fn default_context_retains_default_limits_and_omitted_encoding() {
    let registry = registry(ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("amount"),
        "Amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValueKind::U64,
        ComponentValue::U64(7),
    ));
    let component_id = ComponentTypeId::new(COMPONENT_ID);
    let encoded = ComponentValue::Map(Default::default());
    let mut world = World::new();
    let entity = world.spawn(Encoded(encoded.clone())).id();
    for context in [
        ComponentEncodeContext::new(),
        ComponentEncodeContext::default(),
    ] {
        assert_eq!(
            context.materialization_limits(),
            ComponentValueMaterializationLimits::default(),
        );
        assert_eq!(
            registry.encode_component_with_context(&component_id, &world, entity, &context),
            Ok(Some(encoded.clone())),
        );
    }
    assert_eq!(
        registry.encode_component(&component_id, &world, entity),
        Ok(Some(encoded)),
    );
}

#[test]
fn native_reference_validation_includes_explicit_values_and_omitted_defaults() {
    let target = SceneEntityId::new("target").unwrap();
    let persistent = PersistentRuntimeReference::new(
        PersistentRuntimeNamespaceId::new("encode-materialization").unwrap(),
        PersistentRuntimeId::parse_str("31415926-5358-4979-8323-846264338327").unwrap(),
    );
    for (reference, accepted) in [
        (
            EntityReference::Authored {
                reference: AuthoredEntityReference::source(target.clone()),
            },
            false,
        ),
        (EntityReference::SceneLocal { entity: target }, true),
        (EntityReference::Persistent { entity: persistent }, true),
    ] {
        let default = ComponentValue::List(vec![ComponentValue::EntityReference(reference)]);
        let explicit = ComponentValue::map([("links", default.clone())]);
        let cost = explicit.cost();
        let registry = registry(
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("links"),
                "Links",
                ComponentFieldPath::from_fields(["links"]),
                ComponentValueShape::list(ComponentValueShape::EntityRef),
                default,
            )
            .with_capability(ComponentCapability::EntityRef),
        );
        let component_id = ComponentTypeId::new(COMPONENT_ID);
        let context = ComponentEncodeContext::new().with_materialization_limits(limits(
            cost.nodes(),
            cost.logical_bytes(),
            3,
        ));
        let mut world = World::new();
        for encoded in [ComponentValue::Map(Default::default()), explicit] {
            let entity = world.spawn(Encoded(encoded.clone())).id();
            let expected = if accepted {
                Ok(Some(encoded))
            } else {
                Err(ComponentCodecError::UnresolvedAuthoredReference)
            };
            assert_eq!(
                registry.encode_component_with_context(&component_id, &world, entity, &context),
                expected,
            );
            assert_eq!(
                registry.encode_component(&component_id, &world, entity),
                expected,
            );
        }
    }
}

#[test]
fn trusted_encoder_receives_selected_materialization_limits() {
    let selected = limits(2, 14, 2);
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_codec_with_context::<Encoded, _, _>(
            schema(ComponentFieldSchema::required(
                ComponentFieldId::new("amount"),
                "Amount",
                ComponentFieldPath::from_fields(["amount"]),
                ComponentValueKind::U64,
            )),
            |value, _| {
                Ok(PreparedComponentCandidate::insert(Encoded(
                    value.to_owned(),
                )))
            },
            move |component, _, context| {
                assert_eq!(context.materialization_limits(), selected);
                Ok(component.0.clone())
            },
        )
        .unwrap();
    let registry = builder.freeze().unwrap();
    let component_id = ComponentTypeId::new(COMPONENT_ID);
    let encoded = ComponentValue::map([("amount", ComponentValue::U64(7))]);
    let mut world = World::new();
    let entity = world.spawn(Encoded(encoded.clone())).id();
    let context = ComponentEncodeContext::new().with_materialization_limits(selected);
    assert_eq!(
        registry.encode_component_with_context(&component_id, &world, entity, &context),
        Ok(Some(encoded)),
    );
}

#[test]
fn zero_byte_context_admits_empty_defaults_and_refuses_positive_defaults() {
    let selected = limits(2, 1, 2).remaining(0, 1);
    assert_eq!(selected.nodes(), 2);
    assert_eq!(selected.bytes(), 0);
    assert_eq!(selected.depth(), DepthLimit::new(2).unwrap());
    let component_id = ComponentTypeId::new(COMPONENT_ID);
    for default in [String::new(), "x".to_owned()] {
        let registry = registry(ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::new([ComponentFieldPathSegment::index(0)]),
            ComponentValueKind::String,
            ComponentValue::String(default.clone()),
        ));
        let mut world = World::new();
        for encoded in [
            ComponentValue::List(Vec::new()),
            ComponentValue::List(vec![ComponentValue::String(default.clone())]),
        ] {
            let entity = world.spawn(Encoded(encoded.clone())).id();
            let context = ComponentEncodeContext::new().with_materialization_limits(selected);
            let expected = if default.is_empty() {
                Ok(Some(encoded.clone()))
            } else {
                Err(ComponentCodecError::MaterializationByteLimit {
                    observed: 1,
                    maximum: 0,
                })
            };
            assert_eq!(
                registry.encode_component_with_context(&component_id, &world, entity, &context),
                expected,
            );
            let exhausted = selected.remaining(usize::MAX, usize::MAX);
            assert_eq!(exhausted.nodes(), 0);
            assert_eq!(exhausted.bytes(), 0);
            let context = ComponentEncodeContext::new().with_materialization_limits(exhausted);
            assert_eq!(
                registry.encode_component_with_context(&component_id, &world, entity, &context),
                Err(ComponentCodecError::MaterializationNodeLimit {
                    observed: 1,
                    maximum: 0,
                }),
            );
            assert_eq!(world.get::<Encoded>(entity).unwrap().0, encoded);
        }
    }
}
