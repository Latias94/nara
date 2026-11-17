use std::{collections::BTreeMap, convert::Infallible, mem::size_of};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::{
    AssetSourcePath, AuthoredEntityReference, AuthoredReferenceTarget, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, PrefabEntityOrigin,
    PrefabInstanceHop, ReferenceInputId, SceneEntityId, SourceDocumentKey, SourceDocumentKind,
    SourceDocumentOrigin, StableAssetId,
};

use super::{
    ComponentValueMaterializationLimits, EffectiveValueAdmissionError, FrozenComponentDefaults,
    MaterializationBudget, NativeValueMetrics, ReferencePolicy,
    component_field_default_requires_context_with_admission, freeze_component_defaults,
    validate_native_component_value_references_with_admission,
    validate_value_materialization_budget_with_policy_and_admission,
};
use crate::{
    ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldPathSegment as Segment, ComponentFieldSchema, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueRef,
    ComponentValueShape, EntityReference,
};

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes.max(1)).unwrap(),
        ByteLimit::new(bytes.max(1)).unwrap(),
        DepthLimit::new(depth.max(1)).unwrap(),
    )
    .remaining(usize::from(nodes == 0), usize::from(bytes == 0))
}

#[derive(Debug, Default)]
struct Oracle {
    nodes: usize,
    bytes: usize,
    depth: usize,
    retained_bytes: usize,
    requires_context: bool,
    work: Vec<(usize, usize)>,
}

impl Oracle {
    fn charge(&mut self, nodes: usize, bytes: usize) {
        self.nodes += nodes;
        self.bytes += bytes;
        self.work.push((nodes, bytes));
    }

    /// Recurses over finite owned fixtures without using a borrowed walker or its cost helpers.
    fn visit(&mut self, value: &ComponentValue, depth: usize) {
        assert!(
            depth <= 65,
            "fixtures must keep the recursive oracle bounded"
        );
        self.depth = self.depth.max(depth);
        self.charge(1, 0);
        let bytes = match value {
            ComponentValue::Null => return,
            ComponentValue::Bool(_) => 1,
            ComponentValue::I64(_) | ComponentValue::U64(_) | ComponentValue::F64(_) => 8,
            ComponentValue::String(text) => {
                self.retained_bytes += text.len();
                text.len()
            }
            ComponentValue::List(items) => {
                self.retained_bytes += items.len() * size_of::<ComponentValue>();
                for child in items {
                    self.visit(child, depth + 1);
                }
                return;
            }
            ComponentValue::Map(fields) => {
                self.retained_bytes +=
                    fields.len() * (size_of::<(String, ComponentValue)>() + 6 * size_of::<usize>());
                for (name, child) in fields {
                    self.charge(0, name.len());
                    self.retained_bytes += name.len();
                    self.visit(child, depth + 1);
                }
                return;
            }
            ComponentValue::EntityReference(reference) => {
                let bytes = match reference {
                    EntityReference::SceneLocal { entity } => {
                        self.requires_context = true;
                        entity.as_str().len()
                    }
                    EntityReference::Persistent { entity } => entity.namespace.as_str().len() + 16,
                    EntityReference::Authored { reference } => {
                        self.requires_context = true;
                        self.charge(1 + 2 * reference.through.len(), 0);
                        self.retained_bytes +=
                            reference.through.len() * size_of::<PrefabInstanceHop>();
                        let mut bytes = match &reference.target {
                            AuthoredReferenceTarget::Entity(origin) => {
                                origin.entity().as_str().len()
                            }
                            AuthoredReferenceTarget::Input(input) => input.as_str().len(),
                        };
                        for hop in &reference.through {
                            let text = match hop.expected_base.origin() {
                                SourceDocumentOrigin::Named { name, .. } => name.len(),
                                SourceDocumentOrigin::ProjectLocation { path, .. } => {
                                    path.as_str().len()
                                }
                                SourceDocumentOrigin::StableAsset { .. } => 0,
                            };
                            bytes += hop.anchor.entity().as_str().len() + 36 + 1 + text;
                        }
                        bytes
                    }
                };
                self.retained_bytes += bytes;
                bytes
            }
        };
        self.charge(0, bytes);
    }
}

fn oracle(value: &ComponentValue, depth: usize) -> Oracle {
    let mut expected = Oracle::default();
    expected.visit(value, depth);
    expected
}

fn persistent() -> EntityReference {
    EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save.cursor").unwrap(),
            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ),
    }
}

fn references() -> [EntityReference; 5] {
    let universe = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let through = vec![
        PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("outer").unwrap()),
            expected_base: SourceDocumentKey::named(
                universe,
                "reusable/雪",
                SourceDocumentKind::Prefab,
            )
            .unwrap(),
        },
        PrefabInstanceHop {
            anchor: PrefabEntityOrigin::InstanceAddition(SceneEntityId::new("inner").unwrap()),
            expected_base: SourceDocumentKey::project_location(
                universe,
                AssetSourcePath::new("parts/é.prefab").unwrap(),
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
    ];
    let authored = |target| EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: through.clone(),
            target,
        },
    };
    [
        persistent(),
        EntityReference::SceneLocal {
            entity: SceneEntityId::new("local/target").unwrap(),
        },
        authored(AuthoredReferenceTarget::Input(
            ReferenceInputId::new("socket").unwrap(),
        )),
        authored(AuthoredReferenceTarget::Entity(
            PrefabEntityOrigin::SourceEntity(SceneEntityId::new("source/target").unwrap()),
        )),
        authored(AuthoredReferenceTarget::Entity(
            PrefabEntityOrigin::InstanceAddition(SceneEntityId::new("added/target").unwrap()),
        )),
    ]
}

fn mixed_value(reference: EntityReference) -> ComponentValue {
    ComponentValue::map([
        ("a-empty", ComponentValue::Map(BTreeMap::new())),
        (
            "b-values",
            ComponentValue::List(vec![
                ComponentValue::Null,
                ComponentValue::Bool(false),
                ComponentValue::I64(i64::MIN),
                ComponentValue::U64(u64::MAX),
                ComponentValue::f64(-0.0).unwrap(),
                ComponentValue::String("雪é🧭".into()),
                ComponentValue::String(String::new()),
                ComponentValue::List(Vec::new()),
                ComponentValue::map([(
                    "deep",
                    ComponentValue::List(vec![ComponentValue::map([
                        ("reference", ComponentValue::EntityReference(reference)),
                        (
                            "z-sibling",
                            ComponentValue::String("after reference 尾".into()),
                        ),
                    ])]),
                )]),
            ]),
        ),
        (
            "y-persistent",
            ComponentValue::EntityReference(persistent()),
        ),
        ("z-tail", ComponentValue::String("complete 🌱".into())),
    ])
}

fn frozen_fixture(
    branch: ComponentValue,
) -> (ComponentValue, ComponentValue, FrozenComponentDefaults) {
    let field = |id: &str, path, default: ComponentValue| {
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new(id),
            id,
            path,
            default.kind(),
            default,
        )
    };
    let row_path = |index, name| {
        ComponentFieldPath::new([
            Segment::field("rows"),
            Segment::index(index),
            Segment::field(name),
        ])
    };
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.CursorDefaults"),
        "Cursor defaults",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        field(
            "created",
            ComponentFieldPath::new([
                Segment::field("b-created"),
                Segment::field("items"),
                Segment::index(0),
                Segment::field("message"),
            ]),
            ComponentValue::String("created 🌱".into()),
        ),
        field(
            "hidden",
            ComponentFieldPath::from_fields(["m-null"]),
            ComponentValue::String("hidden canonical".into()),
        ),
        field(
            "label",
            row_path(0, "é-label"),
            ComponentValue::String("canonical".into()),
        ),
        field(
            "nullable",
            row_path(0, "nullable"),
            ComponentValue::String("hidden row".into()),
        ),
        field(
            "branch",
            ComponentFieldPath::new([Segment::field("rows"), Segment::index(1)]),
            branch.clone(),
        ),
        field("count", row_path(2, "count"), ComponentValue::U64(7)),
    ]);
    let source = ComponentValue::map([
        ("a-原", ComponentValue::String("retained 🧭".into())),
        ("m-null", ComponentValue::Null),
        (
            "rows",
            ComponentValue::List(vec![ComponentValue::map([
                ("a-keep", ComponentValue::U64(11)),
                ("nullable", ComponentValue::Null),
            ])]),
        ),
        ("z-tail", ComponentValue::Bool(true)),
    ]);
    let overrides = BTreeMap::from([
        (
            ComponentFieldId::new("label"),
            ComponentValue::String("selected é🌱".into()),
        ),
        (
            ComponentFieldId::new("nullable"),
            ComponentValue::String("hidden override".into()),
        ),
    ]);
    let expected = ComponentValue::map([
        ("a-原", ComponentValue::String("retained 🧭".into())),
        (
            "b-created",
            ComponentValue::map([(
                "items",
                ComponentValue::List(vec![ComponentValue::map([(
                    "message",
                    ComponentValue::String("created 🌱".into()),
                )])]),
            )]),
        ),
        ("m-null", ComponentValue::Null),
        (
            "rows",
            ComponentValue::List(vec![
                ComponentValue::map([
                    ("a-keep", ComponentValue::U64(11)),
                    ("nullable", ComponentValue::Null),
                    ("é-label", ComponentValue::String("selected é🌱".into())),
                ]),
                branch,
                ComponentValue::map([("count", ComponentValue::U64(7))]),
            ]),
        ),
        ("z-tail", ComponentValue::Bool(true)),
    ]);
    let cost = oracle(&expected, 1);
    let frozen = freeze_component_defaults(
        &schema,
        &source,
        &overrides,
        limits(cost.nodes, cost.bytes, cost.depth),
    )
    .unwrap()
    .unwrap();
    (source, expected, frozen)
}

fn logical_work(requests: &[(usize, usize, usize)]) -> Vec<(usize, usize)> {
    requests
        .iter()
        .filter(|(_, _, allocated)| *allocated == 0)
        .map(|(nodes, bytes, _)| (*nodes, *bytes))
        .collect()
}

fn trace(
    value: ComponentValueRef<'_>,
    allowance: ComponentValueMaterializationLimits,
    policy: ReferencePolicy,
) -> (NativeValueMetrics, Vec<(usize, usize, usize)>) {
    let mut requests = Vec::new();
    let metrics = validate_value_materialization_budget_with_policy_and_admission(
        value,
        allowance,
        policy,
        |nodes, bytes, allocated| {
            requests.push((nodes, bytes, allocated));
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
    (metrics, requests)
}

fn assert_matches_oracle(value: ComponentValueRef<'_>, expected: &Oracle, policy: ReferencePolicy) {
    let allowance = limits(expected.nodes, expected.bytes, expected.depth);
    let (metrics, requests) = trace(value, allowance, policy);
    assert_eq!(metrics.cost().nodes(), expected.nodes);
    assert_eq!(metrics.cost().logical_bytes(), expected.bytes);
    assert_eq!(metrics.depth(), expected.depth);
    assert_eq!(logical_work(&requests), expected.work);

    let mut budget = MaterializationBudget {
        maximum_nodes: allowance.nodes(),
        maximum_bytes: allowance.bytes(),
        maximum_depth: allowance.depth(),
        nodes: 0,
        bytes: 0,
        observed_depth: 0,
        admission: None,
        reference_policy: policy,
        retained_bytes: Some(0),
    };
    budget.observe_tree(value, 1).unwrap();
    assert_eq!(budget.retained_bytes, Some(expected.retained_bytes));
    if let ReferencePolicy::SourceContext { required } = policy {
        assert!(matches!(
            budget.reference_policy,
            ReferencePolicy::SourceContext { required: actual }
                if actual == (required || expected.requires_context)
        ));
    }
}

#[test]
fn mixed_raw_values_match_recursive_work_depth_and_retained_storage() {
    for reference in references() {
        let authored = matches!(reference, EntityReference::Authored { .. });
        let value = mixed_value(reference);
        let original = value.clone();
        let expected = oracle(&value, 1);
        for policy in [
            ReferencePolicy::Source,
            ReferencePolicy::Native,
            ReferencePolicy::SourceContext { required: false },
            ReferencePolicy::SourceContext { required: true },
        ] {
            if authored && matches!(policy, ReferencePolicy::Native) {
                continue;
            }
            assert_matches_oracle((&value).into(), &expected, policy);
        }
        assert_eq!(value, original);
    }
}

#[test]
fn frozen_insertions_match_owned_preorder_and_retain_explicit_nulls() {
    let (_, baseline_value, baseline_frozen) = frozen_fixture(ComponentValue::Map(BTreeMap::new()));
    let baseline_storage = oracle(&baseline_value, 1).retained_bytes;
    for reference in references() {
        let authored = matches!(reference, EntityReference::Authored { .. });
        let (source, owned, frozen) = frozen_fixture(mixed_value(reference));
        let original = source.clone();
        let expected = oracle(&owned, 1);
        let read = frozen.read(&source);
        assert_eq!(read, owned);
        assert_eq!(frozen.cost().nodes(), expected.nodes);
        assert_eq!(frozen.cost().logical_bytes(), expected.bytes);
        assert_eq!(
            read.field("m-null").unwrap().kind(),
            ComponentValueKind::Null
        );
        assert_eq!(
            read.get_path(&ComponentFieldPath::new([
                Segment::field("rows"),
                Segment::index(0),
                Segment::field("nullable"),
            ]))
            .unwrap()
            .kind(),
            ComponentValueKind::Null,
        );
        assert_eq!(
            frozen.retained_bytes() + baseline_storage,
            baseline_frozen.retained_bytes() + expected.retained_bytes,
            "the retained storage delta must cover the complete selected default branch",
        );
        for policy in [
            ReferencePolicy::Source,
            ReferencePolicy::Native,
            ReferencePolicy::SourceContext { required: false },
            ReferencePolicy::SourceContext { required: true },
        ] {
            if authored && matches!(policy, ReferencePolicy::Native) {
                continue;
            }
            assert_matches_oracle(read, &expected, policy);
        }
        assert_eq!(source, original);
    }
}

#[test]
fn alternating_containers_accept_depth_64_and_reject_depth_65() {
    let local = EntityReference::SceneLocal {
        entity: SceneEntityId::new("depth/target").unwrap(),
    };
    for depth in [64, 65] {
        let mut value = ComponentValue::String("leaf 雪".into());
        for level in 1..depth {
            value = if level % 2 == 0 {
                ComponentValue::map([
                    ("a-child", value),
                    ("z-sibling", ComponentValue::EntityReference(local.clone())),
                ])
            } else {
                ComponentValue::List(vec![value, ComponentValue::U64(9)])
            };
        }
        let expected = oracle(&value, 1);
        assert_eq!(expected.depth, depth);
        for policy in [
            ReferencePolicy::Source,
            ReferencePolicy::Native,
            ReferencePolicy::SourceContext { required: false },
        ] {
            let result = validate_value_materialization_budget_with_policy_and_admission(
                (&value).into(),
                limits(expected.nodes, expected.bytes, 64),
                policy,
                |_, _, _| Ok::<(), Infallible>(()),
            );
            if depth == 65 {
                assert_eq!(
                    result,
                    Err(EffectiveValueAdmissionError::Value(
                        ComponentCodecError::MaterializationDepthLimit {
                            observed: 65,
                            maximum: 64,
                        },
                    )),
                );
            } else {
                let metrics = result.unwrap();
                assert_eq!(metrics.depth(), 64);
                assert_eq!(metrics.cost().nodes(), expected.nodes);
                assert_eq!(metrics.cost().logical_bytes(), expected.bytes);
            }
            assert_matches_oracle((&value).into(), &expected, policy);
        }
        if depth == 65 {
            let shape = ComponentValueShape::record([
                (
                    "a-child".into(),
                    ComponentValueShape::opaque(ComponentValueKind::List),
                ),
                ("z-sibling".into(), ComponentValueShape::EntityRef),
            ]);
            let context_field = |shape, default| {
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("deep"),
                    "Deep",
                    ComponentFieldPath::from_fields(["deep"]),
                    shape,
                    default,
                )
            };
            let field = context_field(shape.clone(), value.clone());
            field.shape().validate_value(&value).unwrap();
            let mut requests = Vec::new();
            assert_eq!(
                component_field_default_requires_context_with_admission(
                    &field,
                    |nodes, bytes, allocated| {
                        requests.push((nodes, bytes, allocated));
                        Ok::<(), Infallible>(())
                    },
                ),
                Ok(true),
                "a 65-level value has depth 64 when a Schema default starts at zero",
            );
            assert!(logical_work(&requests).ends_with(&oracle(&value, 0).work));
            let too_deep = context_field(
                ComponentValueShape::list(shape),
                ComponentValue::List(vec![value]),
            );
            assert_eq!(
                component_field_default_requires_context_with_admission(&too_deep, |_, _, _| {
                    Ok::<(), Infallible>(())
                }),
                Err(EffectiveValueAdmissionError::Value(
                    ComponentCodecError::MaterializationDepthLimit {
                        observed: 65,
                        maximum: 64,
                    },
                )),
            );
        }
    }
}

#[test]
fn native_authored_rejection_follows_key_and_root_gates_before_route_cost() {
    let reference = references().into_iter().nth(2).unwrap();
    let value = ComponentValue::map([
        ("a-route", ComponentValue::EntityReference(reference)),
        (
            "z-tail",
            ComponentValue::String("must remain unvisited".into()),
        ),
    ]);
    let prefix = oracle(&ComponentValue::map([("a-route", ComponentValue::Null)]), 1);
    let mut requests = Vec::new();
    let result = validate_native_component_value_references_with_admission(
        &value,
        limits(prefix.nodes, prefix.bytes, 2),
        |nodes, bytes, allocated| {
            requests.push((nodes, bytes, allocated));
            Ok::<(), Infallible>(())
        },
    );
    assert_eq!(
        result,
        Err(EffectiveValueAdmissionError::Value(
            ComponentCodecError::UnresolvedAuthoredReference,
        )),
    );
    assert_eq!(logical_work(&requests), prefix.work);
    for (allowance, error) in [
        (
            limits(1, prefix.bytes, 2),
            ComponentCodecError::MaterializationNodeLimit {
                observed: 2,
                maximum: 1,
            },
        ),
        (
            limits(prefix.nodes, prefix.bytes - 1, 2),
            ComponentCodecError::MaterializationByteLimit {
                observed: prefix.bytes,
                maximum: prefix.bytes - 1,
            },
        ),
        (
            limits(1, prefix.bytes, 1),
            ComponentCodecError::MaterializationDepthLimit {
                observed: 2,
                maximum: 1,
            },
        ),
    ] {
        assert_eq!(
            validate_native_component_value_references_with_admission(
                &value,
                allowance,
                |_, _, _| Ok::<(), Infallible>(()),
            ),
            Err(EffectiveValueAdmissionError::Value(error)),
        );
    }
}

#[test]
fn source_context_classification_checks_the_complete_tail_after_its_first_reference() {
    let value = ComponentValue::map([
        (
            "a-reference",
            ComponentValue::EntityReference(references()[1].clone()),
        ),
        ("z-tail", mixed_value(references()[2].clone())),
    ]);
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("payload"),
        "Payload",
        ComponentFieldPath::from_fields(["payload"]),
        ComponentValueShape::record([
            ("a-reference".into(), ComponentValueShape::EntityRef),
            (
                "z-tail".into(),
                ComponentValueShape::opaque(ComponentValueKind::Map),
            ),
        ]),
        value.clone(),
    );
    field.shape().validate().unwrap();
    field.shape().validate_value(&value).unwrap();
    let expected = oracle(&value, 0);
    let mut requests = Vec::new();
    assert_eq!(
        component_field_default_requires_context_with_admission(
            &field,
            |nodes, bytes, allocated| {
                requests.push((nodes, bytes, allocated));
                Ok::<(), Infallible>(())
            }
        ),
        Ok(true),
    );
    assert!(logical_work(&requests).ends_with(&expected.work));

    let expected = oracle(&value, 1);
    for (allowance, error) in [
        (
            limits(expected.nodes - 1, expected.bytes, expected.depth),
            ComponentCodecError::MaterializationNodeLimit {
                observed: expected.nodes,
                maximum: expected.nodes - 1,
            },
        ),
        (
            limits(expected.nodes, expected.bytes - 1, expected.depth),
            ComponentCodecError::MaterializationByteLimit {
                observed: expected.bytes,
                maximum: expected.bytes - 1,
            },
        ),
        (
            limits(expected.nodes, expected.bytes, expected.depth - 1),
            ComponentCodecError::MaterializationDepthLimit {
                observed: expected.depth,
                maximum: expected.depth - 1,
            },
        ),
    ] {
        assert_eq!(
            validate_value_materialization_budget_with_policy_and_admission(
                (&value).into(),
                allowance,
                ReferencePolicy::SourceContext { required: false },
                |_, _, _| Ok::<(), Infallible>(()),
            ),
            Err(EffectiveValueAdmissionError::Value(error)),
        );
    }
}

#[test]
fn every_actual_raw_and_frozen_callback_boundary_preserves_exact_refusal_and_stops() {
    for reference in [persistent(), references()[2].clone()] {
        let authored = matches!(reference, EntityReference::Authored { .. });
        let (source, owned, frozen) = frozen_fixture(mixed_value(reference));
        let original = (source.clone(), owned.clone());
        let expected = oracle(&owned, 1);
        let allowance = limits(expected.nodes, expected.bytes, expected.depth);
        for value in [ComponentValueRef::from(&owned), frozen.read(&source)] {
            for policy in [
                ReferencePolicy::Source,
                ReferencePolicy::Native,
                ReferencePolicy::SourceContext { required: false },
                ReferencePolicy::SourceContext { required: true },
            ] {
                let mut requests = Vec::new();
                let result = validate_value_materialization_budget_with_policy_and_admission(
                    value,
                    allowance,
                    policy,
                    |nodes, bytes, allocated| {
                        requests.push((nodes, bytes, allocated));
                        Ok::<(), Infallible>(())
                    },
                );
                if authored && matches!(policy, ReferencePolicy::Native) {
                    assert_eq!(
                        result,
                        Err(EffectiveValueAdmissionError::Value(
                            ComponentCodecError::UnresolvedAuthoredReference,
                        )),
                    );
                } else {
                    assert!(result.is_ok());
                    assert!(
                        requests
                            .iter()
                            .filter(|(_, _, allocated)| *allocated > 0)
                            .count()
                            >= 2
                    );
                }
                for boundary in 0..requests.len() {
                    let marker = Box::new(std::io::Error::other("cursor observation refused"));
                    let pointer = std::ptr::from_ref(marker.as_ref());
                    let mut marker = Some(marker);
                    let mut calls = 0;
                    let result = validate_value_materialization_budget_with_policy_and_admission(
                        value,
                        allowance,
                        policy,
                        |nodes, bytes, allocated| {
                            assert_eq!((nodes, bytes, allocated), requests[calls]);
                            let current = calls;
                            calls += 1;
                            if current == boundary {
                                Err(marker.take().unwrap())
                            } else {
                                Ok(())
                            }
                        },
                    );
                    let Err(EffectiveValueAdmissionError::Admission(actual)) = result else {
                        panic!("boundary {boundary} must return the exact caller refusal");
                    };
                    assert!(std::ptr::eq(actual.as_ref(), pointer));
                    assert_eq!(calls, boundary + 1);
                }
            }
        }
        assert_eq!((&source, &owned), (&original.0, &original.1));
    }
}

#[test]
fn scalar_and_empty_roots_need_no_modeled_traversal_allocation() {
    for value in [
        ComponentValue::Null,
        ComponentValue::Bool(true),
        ComponentValue::I64(-7),
        ComponentValue::U64(8),
        ComponentValue::f64(1.5).unwrap(),
        ComponentValue::String(String::new()),
        ComponentValue::String("雪🌱".into()),
        ComponentValue::List(Vec::new()),
        ComponentValue::Map(BTreeMap::new()),
        ComponentValue::EntityReference(persistent()),
    ] {
        let expected = oracle(&value, 1);
        for policy in [
            ReferencePolicy::Source,
            ReferencePolicy::Native,
            ReferencePolicy::SourceContext { required: false },
        ] {
            let (metrics, requests) = trace(
                (&value).into(),
                limits(expected.nodes, expected.bytes, 1),
                policy,
            );
            assert_eq!(metrics.depth(), 1);
            assert_eq!(logical_work(&requests), expected.work);
            assert!(requests.iter().all(|(_, _, allocated)| *allocated == 0));
        }
    }
}

#[test]
fn flat_16k_lists_and_maps_keep_scratch_independent_of_width() {
    const WIDTH: usize = 16_384;
    for leaf in [
        ComponentValue::U64(7),
        ComponentValue::List(Vec::new()),
        ComponentValue::Map(BTreeMap::new()),
    ] {
        for map in [false, true] {
            let make_value = |width| {
                if map {
                    ComponentValue::map(
                        (0..width).map(|index| (format!("cell-{index:05}-雪"), leaf.clone())),
                    )
                } else {
                    ComponentValue::List(vec![leaf.clone(); width])
                }
            };
            let narrow = make_value(1);
            let wide = make_value(WIDTH);
            let expected = oracle(&wide, 1);
            for policy in [
                ReferencePolicy::Source,
                ReferencePolicy::Native,
                ReferencePolicy::SourceContext { required: false },
            ] {
                let allowance = limits(expected.nodes, expected.bytes, 2);
                let (_, narrow_requests) = trace((&narrow).into(), allowance, policy);
                let (metrics, wide_requests) = trace((&wide).into(), allowance, policy);
                assert_eq!(metrics.cost().nodes(), WIDTH + 1);
                assert_eq!(metrics.cost().logical_bytes(), expected.bytes);
                assert_eq!(metrics.depth(), 2);
                assert_eq!(logical_work(&wide_requests), expected.work);
                assert_eq!(wide_requests.first(), Some(&(1, 0, 0)));
                let allocations = |requests: &[(usize, usize, usize)]| {
                    requests
                        .iter()
                        .filter_map(|(_, _, allocated)| (*allocated > 0).then_some(*allocated))
                        .collect::<Vec<_>>()
                };
                let narrow_scratch = allocations(&narrow_requests);
                let wide_scratch = allocations(&wide_requests);
                assert!(!wide_scratch.is_empty());
                assert_eq!(wide_scratch, narrow_scratch);
                assert!(wide_scratch.iter().sum::<usize>() <= 4 * 1024);
            }
        }
    }
}
