use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SourceUniverseId};
use nara_ecs::{Component, World};
use nara_identity::{
    AssetSourcePath, AuthoredEntityReference, AuthoredReferenceTarget, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, PrefabEntityOrigin,
    PrefabInstanceHop, ReferenceInputId, SceneEntityId, SourceDocumentKey, SourceDocumentKind,
    StableAssetId,
};

use crate::{
    __private::{
        effective_component_value_owned_with_default_overrides,
        effective_component_value_with_admission, effective_component_value_with_budget,
        effective_component_value_with_default_overrides_with_budget,
        plan_declared_entity_references, plan_declared_entity_references_with_admission,
        validate_native_component_value_references_with_limits,
    },
    *,
};

fn entity(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

fn universe() -> SourceUniverseId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}

fn authored() -> AuthoredEntityReference {
    AuthoredEntityReference {
        through: vec![
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(entity("outer")),
                expected_base: SourceDocumentKey::named(
                    universe(),
                    "reusable",
                    SourceDocumentKind::Prefab,
                )
                .unwrap(),
            },
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::InstanceAddition(entity("inner")),
                expected_base: SourceDocumentKey::project_location(
                    universe(),
                    AssetSourcePath::new("parts/socket.prefab").unwrap(),
                    SourceDocumentKind::Prefab,
                ),
            },
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(entity("leaf")),
                expected_base: SourceDocumentKey::stable_asset(
                    StableAssetId::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
                    SourceDocumentKind::Prefab,
                ),
            },
        ],
        target: AuthoredReferenceTarget::Input(ReferenceInputId::new("socket").unwrap()),
    }
}

fn authored_value() -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored {
        reference: authored(),
    })
}

fn payload() -> ComponentValue {
    ComponentValue::List(vec![ComponentValue::map([
        ("optional", ComponentValue::Null),
        ("peer", authored_value()),
    ])])
}

fn payload_shape() -> ComponentValueShape {
    ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::optional(
        ComponentValueShape::EntityRef,
    )))
}

fn default_schema(default: ComponentValue, shape: ComponentValueShape) -> ComponentSchema {
    let entity_refs = shape.contains_kind(ComponentValueKind::EntityRef);
    let mut field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("payload"),
        "Payload",
        ComponentFieldPath::from_fields(["payload"]),
        shape,
        default,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    if entity_refs {
        field = field.with_capability(ComponentCapability::EntityRef);
    }
    ComponentSchema::new(
        ComponentTypeId::new("test.InlineReference"),
        "Inline reference",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field])
}

fn limits(nodes: usize, bytes: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(16).unwrap(),
    )
}

#[test]
fn shared_reference_cost_preserves_native_and_counts_complete_authored_routes() {
    let local = EntityReference::SceneLocal {
        entity: entity("local"),
    };
    let persistent = EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ),
    };
    for (reference, bytes) in [(local, 5), (persistent, 20)] {
        assert_eq!(reference.cost().nodes(), 1);
        assert_eq!(reference.cost().logical_bytes(), bytes);
        let value = ComponentValue::EntityReference(reference);
        assert_eq!(value.cost().nodes(), 1);
        assert_eq!(value.cost().logical_bytes(), bytes);
        assert_eq!(value.cost_up_to(1, bytes), Some(value.cost()));
        assert_eq!(value.cost_up_to(1, bytes - 1), None);
        validate_native_component_value_references_with_limits(&value, limits(1, bytes)).unwrap();
        assert!(matches!(
            validate_native_component_value_references_with_limits(&value, limits(1, bytes - 1)),
            Err(ComponentCodecError::MaterializationByteLimit { .. })
        ));
    }

    let reference = EntityReference::Authored {
        reference: authored(),
    };
    let bytes = "socket".len()
        + "outer".len()
        + "inner".len()
        + "leaf".len()
        + "reusable".len()
        + "parts/socket.prefab".len()
        + 3 * (36 + 1);
    assert_eq!(reference.cost().nodes(), 8);
    assert_eq!(reference.cost().logical_bytes(), bytes);
    let value = ComponentValue::EntityReference(reference);
    assert_eq!(value.cost().nodes(), 8);
    assert_eq!(value.cost().logical_bytes(), bytes);
    assert_eq!(value.cost_up_to(8, bytes), Some(value.cost()));
    assert_eq!(value.cost_up_to(7, bytes), None);
    assert_eq!(value.cost_up_to(8, bytes - 1), None);
}

#[test]
fn native_persistent_materialization_keeps_exact_twenty_byte_value_cost() {
    let value = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ),
    });
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.PersistentCost"),
        "Persistent cost",
        ComponentSchemaVersion::ONE,
    );
    let (effective, cost) =
        effective_component_value_with_budget(&schema, &value, 1, 20, DepthLimit::ONE).unwrap();
    assert!(matches!(effective, Cow::Borrowed(_)));
    assert_eq!(cost, value.cost());
    assert_eq!(cost.logical_bytes(), 20);
    assert_eq!(value.cost_up_to(1, 20), Some(cost));
    assert_eq!(value.cost_up_to(1, 19), None);
    assert_eq!(
        effective_component_value_with_budget(&schema, &value, 1, 19, DepthLimit::ONE),
        Err(ComponentCodecError::MaterializationByteLimit {
            observed: 20,
            maximum: 19,
        }),
    );
}

#[test]
fn native_persistent_reference_work_retains_text_width_uuid_cost() {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.PersistentReferenceWork"),
        "Persistent reference work",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("root"),
        "Root",
        ComponentFieldPath::empty(),
        ComponentValueShape::EntityRef,
    )
    .with_capability(ComponentCapability::EntityRef)]);
    let value = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ),
    });
    assert_eq!(value.cost().logical_bytes(), 20);
    let work_bytes = "root".len() + "save".len() + 36;
    let exact = EntityReferenceTraversalLimits::new(
        ItemLimit::new(2).unwrap(),
        ByteLimit::new(work_bytes).unwrap(),
        DepthLimit::ONE,
    );
    plan_declared_entity_references::<()>(&schema, &value, exact).unwrap();
    let short = EntityReferenceTraversalLimits::new(
        exact.nodes(),
        ByteLimit::new(work_bytes - 1).unwrap(),
        exact.depth(),
    );
    assert_eq!(
        plan_declared_entity_references::<()>(&schema, &value, short),
        Err(ComponentEntityReferenceRewriteError::ByteLimit {
            maximum: work_bytes - 1,
        }),
    );
    let result = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        exact,
        |nodes, bytes, _| {
            if nodes == 1 && bytes == "save".len() + 36 {
                Err("persistent work refused")
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(
        result,
        Err(__private::EntityReferencePlanError::Admission(
            "persistent work refused"
        )),
    );
}

#[test]
fn source_values_and_defaults_materialize_at_exact_inclusive_route_cost() {
    let schema = default_schema(payload(), payload_shape());
    let source = ComponentValue::Map(Default::default());
    let expected = ComponentValue::map([("payload", payload())]);
    let cost = expected.cost();
    assert_eq!(cost.nodes(), 12);
    assert_eq!(
        cost.logical_bytes(),
        authored_value().cost().logical_bytes() + 19,
    );
    let exact = limits(cost.nodes(), cost.logical_bytes());
    let defaulted = effective_component_value(&schema, &source, exact).unwrap();
    assert!(matches!(defaulted, Cow::Owned(_)));
    assert_eq!(defaulted.as_ref(), &expected);
    validate_component_value_with_limits(&schema, &source, exact).unwrap();
    assert_eq!(
        effective_component_value_owned(&schema, source.clone(), exact).unwrap(),
        expected,
    );
    let admitted = effective_component_value_with_admission(&schema, &source, exact, |_, _, _| {
        Ok::<(), ()>(())
    })
    .unwrap();
    assert_eq!(admitted, defaulted);

    for value in [&source, &expected] {
        let (effective, observed) = effective_component_value_with_budget(
            &schema,
            value,
            cost.nodes(),
            cost.logical_bytes(),
            exact.depth(),
        )
        .unwrap();
        assert_eq!(effective.as_ref(), &expected);
        assert_eq!(observed, cost);
        assert!(matches!(
            effective_component_value_with_budget(
                &schema,
                value,
                cost.nodes() - 1,
                cost.logical_bytes(),
                exact.depth(),
            ),
            Err(ComponentCodecError::MaterializationNodeLimit { .. })
        ));
        assert!(matches!(
            effective_component_value_with_budget(
                &schema,
                value,
                cost.nodes(),
                cost.logical_bytes() - 1,
                exact.depth(),
            ),
            Err(ComponentCodecError::MaterializationByteLimit { .. })
        ));
    }
    for short in [
        limits(cost.nodes() - 1, cost.logical_bytes()),
        limits(cost.nodes(), cost.logical_bytes() - 1),
    ] {
        assert!(effective_component_value(&schema, &source, short).is_err());
    }
    let plan = plan_declared_entity_references::<()>(
        &schema,
        &expected,
        EntityReferenceTraversalLimits::default(),
    )
    .unwrap();
    let mut visited = 0;
    plan.visit(&expected, |_, reference| {
        assert_eq!(reference, authored_value().as_entity_reference().unwrap());
        visited += 1;
        Ok::<(), ()>(())
    })
    .unwrap();
    assert_eq!(visited, 1);
    assert!(source.get("payload").is_none());
}

#[test]
fn contextual_defaults_select_field_ids_and_charge_actual_payloads_with_canonical_fallback() {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.ContextualDefaults"),
        "Contextual defaults",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("routed"),
            "Routed",
            ComponentFieldPath::from_fields(["nested", "payload"]),
            payload_shape(),
            ComponentValue::List(Vec::new()),
        )
        .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("count"),
            "Count",
            ComponentFieldPath::from_fields(["fallback"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        ),
    ]);
    let overrides = BTreeMap::from([(ComponentFieldId::new("routed"), payload())]);
    let retained = overrides.clone();
    let source = ComponentValue::Map(Default::default());
    let expected = ComponentValue::map([
        ("nested", ComponentValue::map([("payload", payload())])),
        ("fallback", ComponentValue::U64(7)),
    ]);
    let cost = expected.cost();
    let exact = limits(cost.nodes(), cost.logical_bytes());
    let (effective, observed) = effective_component_value_with_default_overrides_with_budget(
        &schema,
        &source,
        &overrides,
        cost.nodes(),
        cost.logical_bytes(),
        exact.depth(),
    )
    .unwrap();
    assert!(matches!(effective, Cow::Owned(_)));
    assert_eq!(effective.as_ref(), &expected);
    assert_eq!(observed, cost);
    assert_eq!(
        effective_component_value_owned_with_default_overrides(
            &schema,
            source.clone(),
            &overrides,
            exact,
        )
        .unwrap(),
        expected,
    );
    assert!(matches!(
        effective_component_value_with_default_overrides_with_budget(
            &schema,
            &source,
            &overrides,
            cost.nodes() - 1,
            cost.logical_bytes(),
            exact.depth(),
        ),
        Err(ComponentCodecError::MaterializationNodeLimit { .. })
    ));
    assert!(matches!(
        effective_component_value_with_default_overrides_with_budget(
            &schema,
            &source,
            &overrides,
            cost.nodes(),
            cost.logical_bytes() - 1,
            exact.depth(),
        ),
        Err(ComponentCodecError::MaterializationByteLimit { .. })
    ));
    assert!(matches!(
        effective_component_value_owned_with_default_overrides(
            &schema,
            source.clone(),
            &overrides,
            limits(cost.nodes() - 1, cost.logical_bytes()),
        ),
        Err(ComponentCodecError::MaterializationNodeLimit { .. })
    ));
    assert!(matches!(
        effective_component_value_owned_with_default_overrides(
            &schema,
            source.clone(),
            &overrides,
            limits(cost.nodes(), cost.logical_bytes() - 1),
        ),
        Err(ComponentCodecError::MaterializationByteLimit { .. })
    ));
    let canonical = ComponentValue::map([
        (
            "nested",
            ComponentValue::map([("payload", ComponentValue::List(Vec::new()))]),
        ),
        ("fallback", ComponentValue::U64(7)),
    ]);
    let (ordinary, ordinary_cost) = effective_component_value_with_budget(
        &schema,
        &source,
        cost.nodes(),
        cost.logical_bytes(),
        exact.depth(),
    )
    .unwrap();
    assert_eq!(ordinary.as_ref(), &canonical);
    assert_eq!(ordinary_cost, canonical.cost());
    assert_eq!(
        effective_component_value_owned(&schema, source.clone(), exact).unwrap(),
        canonical,
    );
    assert_eq!(overrides, retained);
}

#[test]
fn explicit_null_hides_retained_contextual_defaults_and_reset_exposes_their_full_cost() {
    let schema = default_schema(ComponentValue::List(Vec::new()), payload_shape());
    let overrides = BTreeMap::from([(ComponentFieldId::new("payload"), payload())]);
    let retained = overrides.clone();
    let explicit = ComponentValue::map([("payload", ComponentValue::Null)]);
    let hidden_cost = explicit.cost();
    let (hidden, observed) = effective_component_value_with_default_overrides_with_budget(
        &schema,
        &explicit,
        &overrides,
        hidden_cost.nodes(),
        hidden_cost.logical_bytes(),
        DepthLimit::new(16).unwrap(),
    )
    .unwrap();
    assert!(matches!(hidden, Cow::Borrowed(_)));
    assert!(std::ptr::eq(hidden.as_ref(), &explicit));
    assert_eq!(observed, hidden_cost);
    assert_eq!(
        effective_component_value_owned_with_default_overrides(
            &schema,
            explicit.clone(),
            &overrides,
            limits(hidden_cost.nodes(), hidden_cost.logical_bytes()),
        )
        .unwrap(),
        explicit,
    );

    // Reset removes the explicit slot while retaining the source's contextual declaration.
    let reset = ComponentValue::Map(Default::default());
    assert!(matches!(
        effective_component_value_with_default_overrides_with_budget(
            &schema,
            &reset,
            &overrides,
            hidden_cost.nodes(),
            hidden_cost.logical_bytes(),
            DepthLimit::new(16).unwrap(),
        ),
        Err(ComponentCodecError::MaterializationNodeLimit { .. })
    ));
    let expected = ComponentValue::map([("payload", payload())]);
    let cost = expected.cost();
    let (restored, observed) = effective_component_value_with_default_overrides_with_budget(
        &schema,
        &reset,
        &overrides,
        cost.nodes(),
        cost.logical_bytes(),
        DepthLimit::new(16).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.as_ref(), &expected);
    assert_eq!(observed, cost);
    assert_eq!(overrides, retained);
}

#[test]
fn contextual_null_is_a_concrete_default_and_does_not_rewrite_the_schema_template() {
    let schema = default_schema(authored_value(), ComponentValueShape::EntityRef);
    let overrides = BTreeMap::from([(ComponentFieldId::new("payload"), ComponentValue::Null)]);
    let source = ComponentValue::Map(Default::default());
    let expected = ComponentValue::map([("payload", ComponentValue::Null)]);
    let cost = expected.cost();
    let (effective, observed) = effective_component_value_with_default_overrides_with_budget(
        &schema,
        &source,
        &overrides,
        cost.nodes(),
        cost.logical_bytes(),
        DepthLimit::new(16).unwrap(),
    )
    .unwrap();
    assert_eq!(effective.as_ref(), &expected);
    assert_eq!(observed, cost);
    validate_native_component_value_references_with_limits(
        effective.as_ref(),
        limits(cost.nodes(), cost.logical_bytes()),
    )
    .unwrap();
    let canonical = effective_component_value(
        &schema,
        &source,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap();
    assert_eq!(canonical.get("payload"), Some(&authored_value()));
    assert_eq!(
        validate_native_component_value_references_with_limits(
            canonical.as_ref(),
            ComponentValueMaterializationLimits::default(),
        ),
        Err(ComponentCodecError::UnresolvedAuthoredReference),
    );
}

#[test]
fn declared_planner_admits_full_route_before_owned_paths_and_keeps_exact_limits() {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.RootReference"),
        "Root reference",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("root"),
        "Root",
        ComponentFieldPath::empty(),
        ComponentValueShape::EntityRef,
    )
    .with_capability(ComponentCapability::EntityRef)]);
    let value = authored_value();
    let cost = value.cost();
    let exact = EntityReferenceTraversalLimits::new(
        ItemLimit::new(cost.nodes() + 1).unwrap(),
        ByteLimit::new(cost.logical_bytes() + "root".len()).unwrap(),
        DepthLimit::ONE,
    );
    plan_declared_entity_references::<()>(&schema, &value, exact).unwrap();
    for (nodes, bytes) in [
        (cost.nodes(), exact.bytes().get()),
        (exact.nodes().get(), exact.bytes().get() - 1),
    ] {
        assert!(
            plan_declared_entity_references::<()>(
                &schema,
                &value,
                EntityReferenceTraversalLimits::new(
                    ItemLimit::new(nodes).unwrap(),
                    ByteLimit::new(bytes).unwrap(),
                    DepthLimit::ONE,
                ),
            )
            .is_err()
        );
    }
    let result = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        exact,
        |nodes, bytes, _| {
            if bytes == cost.logical_bytes() {
                assert_eq!(nodes, cost.nodes());
                Err("route refused")
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(
        result,
        Err(__private::EntityReferencePlanError::Admission(
            "route refused"
        )),
    );
}

#[test]
fn native_raw_gate_rejects_authored_leaves_inside_opaque_and_undeclared_contents() {
    let schema = default_schema(
        payload(),
        ComponentValueShape::opaque(ComponentValueKind::List),
    );
    let opaque = ComponentValue::map([("payload", payload())]);
    validate_component_value(&schema, &opaque).unwrap();
    let undeclared =
        ComponentValue::map([("unregistered", ComponentValue::map([("nested", payload())]))]);
    for value in [authored_value(), opaque, undeclared] {
        assert_eq!(
            validate_native_component_value_references_with_limits(
                &value,
                ComponentValueMaterializationLimits::default(),
            ),
            Err(ComponentCodecError::UnresolvedAuthoredReference),
        );
    }
}

#[derive(Component)]
struct NativeProbe;

#[test]
fn handwritten_and_deferred_codecs_run_only_after_effective_native_reference_admission() {
    for deferred in [false, true] {
        for (shape, default, replacement) in [
            (
                ComponentValueShape::EntityRef,
                authored_value(),
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: entity("native"),
                }),
            ),
            (
                ComponentValueShape::opaque(ComponentValueKind::List),
                payload(),
                ComponentValue::List(Vec::new()),
            ),
        ] {
            let calls = Arc::new(AtomicUsize::new(0));
            let schema = default_schema(default.clone(), shape);
            let id = schema.id().clone();
            let mut builder = ComponentRegistryBuilder::new();
            let entered = Arc::clone(&calls);
            if deferred {
                builder
                    .register_persistent_component_codec::<NativeProbe, _, _>(
                        schema,
                        move |_| {
                            entered.fetch_add(1, Ordering::SeqCst);
                            Ok(PreparedComponentCandidate::with_asset_server(|_| {
                                Ok(NativeProbe)
                            }))
                        },
                        |_| Ok(ComponentValue::Map(Default::default())),
                    )
                    .unwrap();
            } else {
                builder
                    .register_persistent_component_with_codec::<NativeProbe, _, _>(
                        schema,
                        move |_| {
                            entered.fetch_add(1, Ordering::SeqCst);
                            Ok(NativeProbe)
                        },
                        |_| Ok(ComponentValue::Map(Default::default())),
                    )
                    .unwrap();
            }
            let registry = builder.freeze().unwrap();
            for value in [
                ComponentValue::map([("payload", default)]),
                ComponentValue::Map(Default::default()),
                ComponentValue::map([("payload", replacement.clone()), ("undeclared", payload())]),
            ] {
                assert!(matches!(
                    registry.preflight_component(&id, &value),
                    Err(ComponentCodecError::UnresolvedAuthoredReference)
                ));
                let mut context = ComponentDecodeContext::new();
                assert!(matches!(
                    registry.preflight_component_with_context(&id, &value, &mut context),
                    Err(ComponentCodecError::UnresolvedAuthoredReference)
                ));
                assert_eq!(calls.load(Ordering::SeqCst), 0);
            }
            registry
                .preflight_component(&id, &ComponentValue::map([("payload", replacement)]))
                .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            registry
                .preflight_component(
                    &id,
                    &ComponentValue::map([("payload", ComponentValue::Null)]),
                )
                .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 2);
        }
    }
}

#[test]
fn native_encoding_refuses_explicit_and_omitted_authored_defaults() {
    let native = ComponentValue::map([("payload", ComponentValue::List(Vec::new()))]);
    for (encoded, expected) in [
        (
            ComponentValue::map([("payload", payload())]),
            Err(ComponentCodecError::UnresolvedAuthoredReference),
        ),
        (
            ComponentValue::Map(Default::default()),
            Err(ComponentCodecError::UnresolvedAuthoredReference),
        ),
        (native.clone(), Ok(Some(native))),
    ] {
        let schema = default_schema(payload(), payload_shape());
        let id = schema.id().clone();
        let mut builder = ComponentRegistryBuilder::new();
        builder
            .register_persistent_component_with_codec::<NativeProbe, _, _>(
                schema,
                |_| Ok(NativeProbe),
                move |_| Ok(encoded.clone()),
            )
            .unwrap();
        let registry = builder.freeze().unwrap();
        let mut world = World::new();
        let entity = world.spawn(NativeProbe).id();
        assert_eq!(registry.encode_component(&id, &world, entity), expected);
        assert_eq!(
            registry.encode_component_with_context(
                &id,
                &world,
                entity,
                &ComponentEncodeContext::new()
            ),
            expected,
        );
    }
}

#[test]
fn authored_default_fingerprints_include_every_route_key_origin_and_endpoint() {
    let original = authored();
    let mut variants = vec![original.clone()];
    for index in 0..original.through.len() {
        let mut changed = original.clone();
        changed.through[index].anchor = match &original.through[index].anchor {
            PrefabEntityOrigin::SourceEntity(_) => {
                PrefabEntityOrigin::SourceEntity(entity("other"))
            }
            PrefabEntityOrigin::InstanceAddition(_) => {
                PrefabEntityOrigin::InstanceAddition(entity("other"))
            }
        };
        variants.push(changed);
        let mut changed = original.clone();
        let anchor = &original.through[index].anchor;
        changed.through[index].anchor = match anchor {
            PrefabEntityOrigin::SourceEntity(id) => {
                PrefabEntityOrigin::InstanceAddition(id.clone())
            }
            PrefabEntityOrigin::InstanceAddition(id) => {
                PrefabEntityOrigin::SourceEntity(id.clone())
            }
        };
        variants.push(changed);
        let mut changed = original.clone();
        changed.through[index].expected_base =
            SourceDocumentKey::named(universe(), "other", SourceDocumentKind::Prefab).unwrap();
        variants.push(changed);
    }
    let mut changed = original.clone();
    changed.through[0].expected_base = SourceDocumentKey::named(
        "44444444-4444-4444-8444-444444444444".parse().unwrap(),
        "reusable",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    variants.push(changed);
    let mut changed = original.clone();
    changed.through[0].expected_base =
        SourceDocumentKey::named(universe(), "reusable", SourceDocumentKind::Scene).unwrap();
    variants.push(changed);
    let mut changed = original.clone();
    changed.through[1].expected_base = SourceDocumentKey::named(
        universe(),
        "parts/socket.prefab",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    variants.push(changed);
    let mut changed = original.clone();
    changed.through[1].expected_base = SourceDocumentKey::project_location(
        "44444444-4444-4444-8444-444444444444".parse().unwrap(),
        AssetSourcePath::new("parts/socket.prefab").unwrap(),
        SourceDocumentKind::Prefab,
    );
    variants.push(changed);
    let mut changed = original.clone();
    changed.through[1].expected_base = SourceDocumentKey::project_location(
        universe(),
        AssetSourcePath::new("parts/other.prefab").unwrap(),
        SourceDocumentKind::Prefab,
    );
    variants.push(changed);
    let mut changed = original.clone();
    changed.through[1].expected_base = SourceDocumentKey::project_location(
        universe(),
        AssetSourcePath::new("parts/socket.prefab").unwrap(),
        SourceDocumentKind::Scene,
    );
    variants.push(changed);
    let mut changed = original.clone();
    changed.through[2].expected_base = SourceDocumentKey::stable_asset(
        StableAssetId::parse_str("55555555-5555-4555-8555-555555555555").unwrap(),
        SourceDocumentKind::Prefab,
    );
    variants.push(changed);
    let mut changed = original.clone();
    changed.through[2].expected_base = SourceDocumentKey::stable_asset(
        StableAssetId::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
        SourceDocumentKind::Scene,
    );
    variants.push(changed);
    let mut changed = original.clone();
    changed.through.swap(0, 1);
    variants.push(changed);
    let mut changed = original.clone();
    changed.through.pop();
    variants.push(changed);
    for target in [
        AuthoredReferenceTarget::Input(ReferenceInputId::new("other").unwrap()),
        AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(entity("socket"))),
        AuthoredReferenceTarget::Entity(PrefabEntityOrigin::InstanceAddition(entity("socket"))),
    ] {
        let mut changed = original.clone();
        changed.target = target;
        variants.push(changed);
    }
    let mut fingerprints = BTreeSet::new();
    for reference in variants {
        let value = ComponentValue::EntityReference(EntityReference::Authored { reference });
        let catalog = ComponentSchemaCatalog {
            components: vec![default_schema(value, ComponentValueShape::EntityRef)],
            ..Default::default()
        };
        let fingerprint = catalog.fingerprint().to_string();
        assert_eq!(fingerprint, catalog.clone().fingerprint().to_string());
        assert!(fingerprints.insert(fingerprint));
    }
}
