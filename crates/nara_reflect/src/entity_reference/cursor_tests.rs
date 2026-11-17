use std::collections::BTreeMap;

use nara_identity::{
    AssetSourcePath, AuthoredEntityReference, AuthoredReferenceTarget, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, PrefabEntityOrigin,
    PrefabInstanceHop, ReferenceInputId, SceneEntityId, SourceDocumentKey, SourceDocumentKind,
    SourceDocumentOrigin,
};

use super::*;
use crate::{
    __private::{FrozenComponentDefaults, freeze_component_defaults},
    ComponentFieldPathSegment as Segment, ComponentSchemaVersion, ComponentTypeId,
    ComponentValueMaterializationLimits,
};

fn limits(nodes: usize, bytes: usize, depth: usize) -> EntityReferenceTraversalLimits {
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes.max(1)).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn local(target: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: SceneEntityId::new(target).unwrap(),
    })
}

/// Computes complete raw work by recursion over owned values, independently of cursor frames.
fn oracle(
    value: &ComponentValue,
    root: &ComponentFieldPath,
    declared: &BTreeSet<ComponentFieldPath>,
) -> ReferencePathScan {
    fn walk(
        value: &ComponentValue,
        path: &mut Vec<Segment>,
        declared: &BTreeSet<ComponentFieldPath>,
        scan: &mut ReferencePathScan,
    ) {
        assert!(path.len() < 70);
        scan.observed_nodes += 1;
        match value {
            ComponentValue::Map(fields) => {
                for (name, child) in fields {
                    scan.observed_bytes += name.len();
                    path.push(Segment::field(name.clone()));
                    walk(child, path, declared, scan);
                    path.pop();
                }
            }
            ComponentValue::List(items) => {
                for (index, child) in items.iter().enumerate() {
                    path.push(Segment::index(u32::try_from(index).unwrap()));
                    walk(child, path, declared, scan);
                    path.pop();
                }
            }
            ComponentValue::String(text) => scan.observed_bytes += text.len(),
            ComponentValue::EntityReference(reference) => {
                match reference {
                    EntityReference::SceneLocal { entity } => {
                        scan.observed_bytes += entity.as_str().len();
                    }
                    EntityReference::Persistent { entity } => {
                        scan.observed_bytes +=
                            entity.namespace.as_str().len() + entity.entity.to_string().len();
                    }
                    EntityReference::Authored { reference } => {
                        scan.observed_nodes += 1 + 2 * reference.through.len();
                        scan.observed_bytes += match &reference.target {
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
                            scan.observed_bytes +=
                                hop.anchor.entity().as_str().len() + 36 + 1 + text;
                        }
                    }
                }
                let candidate = ComponentFieldPath::new(path.iter().cloned());
                if scan.first_undeclared_path.is_none() && !declared.contains(&candidate) {
                    scan.first_undeclared_path = Some(candidate);
                }
            }
            _ => {}
        }
    }
    let mut scan = ReferencePathScan {
        first_undeclared_path: None,
        observed_nodes: 0,
        observed_bytes: 0,
    };
    walk(value, &mut root.segments().to_vec(), declared, &mut scan);
    scan
}

#[test]
fn nested_cursor_matches_recursive_oracle_and_does_not_leak_sibling_paths() {
    let long_name = "branch-枝🌿".repeat(128);
    let root = ComponentFieldPath::new([
        Segment::field("owner-🧭".repeat(64)),
        Segment::index(7),
        Segment::field("payload"),
    ]);
    let rooted =
        |suffix: &[Segment]| ComponentFieldPath::new(root.segments().iter().chain(suffix).cloned());
    let before = rooted(&[
        Segment::field("a_before"),
        Segment::field("deep"),
        Segment::index(0),
        Segment::index(0),
        Segment::field("target"),
    ]);
    let hidden = rooted(&[
        Segment::field("m_hidden"),
        Segment::index(1),
        Segment::field(&long_name),
        Segment::index(1),
        Segment::field("target"),
    ]);
    let sibling = rooted(&[Segment::field("y_sibling"), Segment::field("target")]);
    let after = rooted(&[
        Segment::field("z_tail"),
        Segment::index(0),
        Segment::field("after"),
    ]);
    let value = ComponentValue::map([
        (
            "z_tail",
            ComponentValue::List(vec![
                ComponentValue::map([("after", local("declared-after"))]),
                ComponentValue::String("complete尾".into()),
            ]),
        ),
        (
            "m_hidden",
            ComponentValue::List(vec![
                ComponentValue::Null,
                ComponentValue::map([(
                    long_name.as_str(),
                    ComponentValue::List(vec![
                        ComponentValue::List(Vec::new()),
                        ComponentValue::map([("target", local("hidden"))]),
                    ]),
                )]),
                ComponentValue::Bool(true),
            ]),
        ),
        (
            "a_before",
            ComponentValue::map([
                (
                    "deep",
                    ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::map([
                        ("target", local("declared-before")),
                    ])])]),
                ),
                ("empty", ComponentValue::Map(BTreeMap::new())),
            ]),
        ),
        (
            "y_sibling",
            ComponentValue::map([("target", local("next-hidden"))]),
        ),
    ]);
    let original = value.clone();
    let mut declared = BTreeSet::from([before, after]);
    for first in [Some(hidden), Some(sibling), None] {
        let expected = oracle(&value, &root, &declared);
        assert_eq!(expected.first_undeclared_path, first);
        let finite = limits(expected.observed_nodes + 3, expected.observed_bytes + 9, 64);
        let mut charged = (0, 0);
        let mut callback = |nodes, bytes, _| {
            charged.0 += nodes;
            charged.1 += bytes;
            Ok::<_, ()>(())
        };
        let mut admission = ReferenceAdmission {
            callback: &mut callback,
            maximum_bytes: finite.bytes().get(),
        };
        let scan = find_undeclared_reference_path::<(), _>(
            (&value).into(),
            finite,
            &declared,
            3,
            9,
            &root,
            &mut admission,
        )
        .unwrap_or_else(|_| panic!("nested raw scan must fit its exact limits"));
        assert_eq!(scan.first_undeclared_path, first);
        assert_eq!(scan.observed_nodes, expected.observed_nodes + 3);
        assert_eq!(scan.observed_bytes, expected.observed_bytes + 9);
        assert_eq!(charged, (expected.observed_nodes, expected.observed_bytes));
        if let Some(first) = first {
            declared.insert(first);
        }
    }
    assert_eq!(value, original);
}

#[test]
fn frozen_cursor_matches_owned_oracle_for_merged_maps_and_list_suffixes() {
    let row_path = |index, name| {
        ComponentFieldPath::new([
            Segment::field("rows"),
            Segment::index(index),
            Segment::field(name),
        ])
    };
    let field = |id: &str, path, default: ComponentValue| {
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new(id),
            id,
            path,
            default.kind(),
            default,
        )
    };
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.ReferenceCursor"),
        "Reference cursor",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        field("target", row_path(0, "m_inserted"), local("default-hidden")),
        field(
            "suffix",
            ComponentFieldPath::new([Segment::field("rows"), Segment::index(1)]),
            ComponentValue::map([
                ("after", local("default-after")),
                ("empty", ComponentValue::List(Vec::new())),
            ]),
        ),
        field("nullable", row_path(0, "nullable"), local("masked")),
        field(
            "label",
            ComponentFieldPath::from_fields(["a_label"]),
            ComponentValue::String("overlaid🌿".into()),
        ),
    ]);
    let source = ComponentValue::map([
        (
            "rows",
            ComponentValue::List(vec![ComponentValue::map([
                ("before", local("source-before")),
                ("nullable", ComponentValue::Null),
            ])]),
        ),
        ("z_tail", ComponentValue::String("retained尾".into())),
    ]);
    let original = source.clone();
    let frozen: FrozenComponentDefaults = freeze_component_defaults(
        &schema,
        &source,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap()
    .unwrap();
    let read = frozen.read(&source);
    assert!(read.as_value().is_none());
    let owned = ComponentValue::map([
        ("a_label", ComponentValue::String("overlaid🌿".into())),
        (
            "rows",
            ComponentValue::List(vec![
                ComponentValue::map([
                    ("before", local("source-before")),
                    ("m_inserted", local("default-hidden")),
                    ("nullable", ComponentValue::Null),
                ]),
                ComponentValue::map([
                    ("after", local("default-after")),
                    ("empty", ComponentValue::List(Vec::new())),
                ]),
            ]),
        ),
        ("z_tail", ComponentValue::String("retained尾".into())),
    ]);
    let root = ComponentFieldPath::new([
        Segment::index(2),
        Segment::field("component"),
        Segment::index(4),
    ]);
    let rooted = |path: ComponentFieldPath| {
        ComponentFieldPath::new(root.segments().iter().chain(path.segments()).cloned())
    };
    let hidden = rooted(row_path(0, "m_inserted"));
    let mut declared =
        BTreeSet::from([rooted(row_path(0, "before")), rooted(row_path(1, "after"))]);
    for first in [Some(hidden.clone()), None] {
        let expected = oracle(&owned, &root, &declared);
        assert_eq!(expected.first_undeclared_path, first);
        let finite = limits(expected.observed_nodes, expected.observed_bytes, 64);
        let mut charged = (0, 0);
        let mut callback = |nodes, bytes, _| {
            charged.0 += nodes;
            charged.1 += bytes;
            Ok::<_, ()>(())
        };
        let mut admission = ReferenceAdmission {
            callback: &mut callback,
            maximum_bytes: finite.bytes().get(),
        };
        let scan = find_undeclared_reference_path::<(), _>(
            read,
            finite,
            &declared,
            0,
            0,
            &root,
            &mut admission,
        )
        .unwrap_or_else(|_| panic!("frozen raw scan must match the complete effective value"));
        assert_eq!(scan.first_undeclared_path, first);
        assert_eq!(scan.observed_nodes, expected.observed_nodes);
        assert_eq!(scan.observed_bytes, expected.observed_bytes);
        assert_eq!(charged, (expected.observed_nodes, expected.observed_bytes));
        declared.insert(hidden.clone());
    }
    assert_eq!(source, original);
}

#[test]
fn cursor_counts_persistent_uuid_text_and_complete_authored_route_work() {
    let universe = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save-slot").unwrap(),
            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ),
    });
    let authored = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("outer").unwrap()),
                    expected_base: SourceDocumentKey::named(
                        universe,
                        "reusable枝",
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
            ],
            target: AuthoredReferenceTarget::Input(ReferenceInputId::new("destination").unwrap()),
        },
    });
    let value = ComponentValue::List(vec![persistent, authored, local("tail")]);
    let root = ComponentFieldPath::empty();
    let declared = BTreeSet::from([ComponentFieldPath::new([Segment::index(0)])]);
    let expected = oracle(&value, &root, &declared);
    assert_eq!(expected.observed_nodes, 1 + 1 + 6 + 1);
    assert_eq!(
        expected.observed_bytes,
        "save-slot".len()
            + 36
            + "destination".len()
            + "outer".len()
            + 37
            + "reusable枝".len()
            + "inner".len()
            + 37
            + "parts/socket.prefab".len()
            + "tail".len(),
    );
    let finite = limits(expected.observed_nodes, expected.observed_bytes, 2);
    let mut charged = (0, 0);
    let mut callback = |nodes, bytes, _| {
        charged.0 += nodes;
        charged.1 += bytes;
        Ok::<_, ()>(())
    };
    let mut admission = ReferenceAdmission {
        callback: &mut callback,
        maximum_bytes: finite.bytes().get(),
    };
    let scan = find_undeclared_reference_path::<(), _>(
        (&value).into(),
        finite,
        &declared,
        0,
        0,
        &root,
        &mut admission,
    )
    .unwrap_or_else(|_| panic!("reference payload work must fit its exact limits"));
    assert_eq!(
        scan.first_undeclared_path,
        Some(ComponentFieldPath::new([Segment::index(1)])),
    );
    assert_eq!(scan.observed_nodes, expected.observed_nodes);
    assert_eq!(scan.observed_bytes, expected.observed_bytes);
    assert_eq!(charged, (expected.observed_nodes, expected.observed_bytes));
}

#[test]
fn scalar_and_empty_cursors_allocate_nothing_and_flat_scalars_use_bounded_scratch() {
    let root = ComponentFieldPath::new([
        Segment::field("borrowed-根🌿".repeat(256)),
        Segment::index(3),
    ]);
    let declared = BTreeSet::new();
    for value in [
        ComponentValue::Null,
        ComponentValue::Bool(true),
        ComponentValue::I64(-1),
        ComponentValue::U64(7),
        ComponentValue::f64(1.25).unwrap(),
        ComponentValue::String("payload🌿".into()),
        ComponentValue::List(Vec::new()),
        ComponentValue::Map(BTreeMap::new()),
    ] {
        let expected = oracle(&value, &root, &declared);
        let finite = limits(1, expected.observed_bytes, root.segments().len() + 1);
        let mut charged = (0, 0, 0);
        let mut callback = |nodes, bytes, allocation| {
            charged.0 += nodes;
            charged.1 += bytes;
            charged.2 += allocation;
            assert_eq!(
                allocation, 0,
                "a scalar or empty root needs no cursor/path storage",
            );
            Ok::<_, ()>(())
        };
        let mut admission = ReferenceAdmission {
            callback: &mut callback,
            maximum_bytes: finite.bytes().get(),
        };
        let scan = find_undeclared_reference_path::<(), _>(
            (&value).into(),
            finite,
            &declared,
            0,
            0,
            &root,
            &mut admission,
        )
        .unwrap_or_else(|_| panic!("scalar and empty roots must require no scratch"));
        assert!(scan.first_undeclared_path.is_none());
        assert_eq!(
            (scan.observed_nodes, scan.observed_bytes),
            (1, expected.observed_bytes),
        );
        assert_eq!(charged, (1, expected.observed_bytes, 0));
    }
    let mut scratch = None;
    for population in [1, 16_384] {
        let value = ComponentValue::List(vec![ComponentValue::U64(7); population]);
        let finite = limits(population + 1, 1, root.segments().len() + 2);
        let mut charged = (0, 0, 0);
        let mut callback = |nodes, bytes, allocation| {
            charged.0 += nodes;
            charged.1 += bytes;
            charged.2 += allocation;
            Ok::<_, ()>(())
        };
        let mut admission = ReferenceAdmission {
            callback: &mut callback,
            maximum_bytes: finite.bytes().get(),
        };
        let scan = find_undeclared_reference_path::<(), _>(
            (&value).into(),
            finite,
            &declared,
            0,
            0,
            &root,
            &mut admission,
        )
        .unwrap_or_else(|_| panic!("flat scalar traversal must fit its exact node limit"));
        assert!(scan.first_undeclared_path.is_none());
        assert_eq!(
            (scan.observed_nodes, scan.observed_bytes),
            (population + 1, 0),
        );
        assert_eq!((charged.0, charged.1), (population + 1, 0));
        if let Some(scratch) = scratch {
            assert_eq!(
                charged.2, scratch,
                "flat siblings must reuse cursor storage"
            );
        }
        scratch = Some(charged.2);
    }
}

#[test]
fn cursor_growth_preserves_deep_paths_and_refuses_depth_sixty_five() {
    let root = ComponentFieldPath::empty();
    let mut shallow_allocation = None;
    let mut valid_allocation = None;
    for depth in [8, 64, 65] {
        let mut branch = local("deep");
        for level in (0..depth - 2).rev() {
            branch = if level % 2 == 0 {
                ComponentValue::map([(format!("branch-{level}-枝"), branch)])
            } else {
                ComponentValue::List(vec![branch])
            };
        }
        let value = ComponentValue::List(vec![
            branch,
            local("after-deep"),
            ComponentValue::Map(BTreeMap::new()),
        ]);
        let mut declared = BTreeSet::new();
        for pass in 0..2 {
            let expected = oracle(&value, &root, &declared);
            let finite = limits(expected.observed_nodes, expected.observed_bytes, 64);
            let mut charged = (0, 0, 0);
            let mut callback = |nodes, bytes, allocation| {
                charged.0 += nodes;
                charged.1 += bytes;
                charged.2 += allocation;
                Ok::<_, ()>(())
            };
            let mut admission = ReferenceAdmission {
                callback: &mut callback,
                maximum_bytes: finite.bytes().get(),
            };
            let result = find_undeclared_reference_path::<(), _>(
                (&value).into(),
                finite,
                &declared,
                0,
                0,
                &root,
                &mut admission,
            );
            if depth == 65 {
                assert!(matches!(
                    result,
                    Err(ReferenceTraversalError::Reference(
                        ComponentEntityReferenceRewriteError::DepthLimit { maximum: 64 }
                    )),
                ));
                assert_eq!(
                    (charged.0, charged.1),
                    (65, expected.observed_bytes - "after-deep".len()),
                );
                assert!(charged.2 <= valid_allocation.unwrap());
                break;
            }
            let scan = result.unwrap_or_else(|_| panic!("depth {depth} must be admitted"));
            assert_eq!(scan.first_undeclared_path, expected.first_undeclared_path);
            assert_eq!(
                (scan.observed_nodes, scan.observed_bytes),
                (expected.observed_nodes, expected.observed_bytes),
            );
            assert_eq!(
                (charged.0, charged.1),
                (expected.observed_nodes, expected.observed_bytes),
            );
            let first = scan.first_undeclared_path.unwrap();
            if pass == 0 {
                assert_eq!(first.segments().len() + 1, depth);
                if depth == 8 {
                    shallow_allocation = Some(charged.2);
                } else {
                    assert!(charged.2 > shallow_allocation.unwrap());
                    valid_allocation = Some(charged.2);
                }
            } else {
                assert_eq!(first, ComponentFieldPath::new([Segment::index(1)]));
            }
            declared.insert(first);
        }
    }
}

#[test]
fn cursor_limits_and_every_admission_refusal_stop_at_the_exact_prefix() {
    let mut branch = local("hidden");
    for level in (0..9).rev() {
        branch = if level % 2 == 0 {
            ComponentValue::map([(format!("branch-{level}"), branch)])
        } else {
            ComponentValue::List(vec![branch])
        };
    }
    let tail_key = "tail-尾🌿".repeat(32);
    let value = ComponentValue::List(vec![
        branch,
        ComponentValue::map([(tail_key.as_str(), ComponentValue::String("tail🙂".into()))]),
        local("later"),
    ]);
    let root = ComponentFieldPath::empty();
    let declared = BTreeSet::new();
    let expected = oracle(&value, &root, &declared);
    let finite = limits(expected.observed_nodes + 2, expected.observed_bytes + 5, 64);
    let mut trace = Vec::new();
    let mut callback = |nodes, bytes, allocation| {
        trace.push((nodes, bytes, allocation));
        Ok::<_, ()>(())
    };
    let mut admission = ReferenceAdmission {
        callback: &mut callback,
        maximum_bytes: finite.bytes().get(),
    };
    let scan = find_undeclared_reference_path::<(), _>(
        (&value).into(),
        finite,
        &declared,
        2,
        5,
        &root,
        &mut admission,
    )
    .unwrap_or_else(|_| panic!("the baseline must fit inclusive combined limits"));
    assert_eq!(scan.first_undeclared_path, expected.first_undeclared_path);
    assert_eq!(
        (scan.observed_nodes, scan.observed_bytes),
        (finite.nodes().get(), finite.bytes().get()),
    );
    assert!(trace.len() < 64);
    assert_eq!(
        trace.iter().map(|delta| delta.0).sum::<usize>(),
        expected.observed_nodes,
    );
    assert_eq!(
        trace.iter().map(|delta| delta.1).sum::<usize>(),
        expected.observed_bytes,
    );

    for (node_limit, byte_limit, expected_error) in [
        (
            finite.nodes().get() - 1,
            finite.bytes().get(),
            ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: finite.nodes().get() - 1,
            },
        ),
        (
            finite.nodes().get(),
            finite.bytes().get() - 1,
            ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: finite.bytes().get() - 1,
            },
        ),
        (
            finite.nodes().get(),
            tail_key.len() - 1,
            ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: tail_key.len() - 1,
            },
        ),
    ] {
        let mut charged = (2, 5);
        let denied_at = trace
            .iter()
            .position(|&(nodes, bytes, _)| {
                charged.0 += nodes;
                charged.1 += bytes;
                charged.0 > node_limit || charged.1 > byte_limit
            })
            .unwrap();
        let mut observed = Vec::new();
        let mut callback = |nodes, bytes, allocation| {
            observed.push((nodes, bytes, allocation));
            Ok::<_, ()>(())
        };
        let mut admission = ReferenceAdmission {
            callback: &mut callback,
            maximum_bytes: byte_limit,
        };
        let result = find_undeclared_reference_path::<(), _>(
            (&value).into(),
            limits(node_limit, byte_limit, 64),
            &declared,
            2,
            5,
            &root,
            &mut admission,
        );
        match result {
            Err(ReferenceTraversalError::Reference(error)) => assert_eq!(error, expected_error),
            _ => {
                panic!("a raw limit must reject the visited tail after finding a hidden reference")
            }
        }
        assert_eq!(observed, trace[..=denied_at]);
    }

    let refusal = String::from("raw cursor admission refused");
    for denied_at in 0..trace.len() {
        let mut observed = Vec::new();
        let mut refused = false;
        let mut callback = |nodes, bytes, allocation| {
            assert!(
                !refused,
                "no callback or traversal work may follow admission refusal",
            );
            observed.push((nodes, bytes, allocation));
            if observed.len() == denied_at + 1 {
                refused = true;
                Err(&refusal)
            } else {
                Ok(())
            }
        };
        let mut admission = ReferenceAdmission {
            callback: &mut callback,
            maximum_bytes: finite.bytes().get(),
        };
        let result = find_undeclared_reference_path::<(), _>(
            (&value).into(),
            finite,
            &declared,
            2,
            5,
            &root,
            &mut admission,
        );
        match result {
            Err(ReferenceTraversalError::Admission(error)) => {
                assert!(std::ptr::eq(error, &refusal));
            }
            _ => panic!("admission must preserve the exact caller error"),
        }
        assert!(refused);
        assert_eq!(observed, trace[..=denied_at]);
    }
}
