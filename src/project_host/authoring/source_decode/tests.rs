use super::*;

fn source(kind: SourceDocumentKind, encoding: SceneFileEncoding, invalid: bool) -> Vec<u8> {
    let mut document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(nara_scene::SceneEntityId::new("z").unwrap()),
        SceneEntityRecord::new(nara_scene::SceneEntityId::new("a").unwrap()),
    ]);
    if invalid {
        document.entities[1].id = document.entities[0].id.clone();
    }
    match (kind, encoding) {
        (SourceDocumentKind::Scene, SceneFileEncoding::Json) => document.to_json_string().unwrap(),
        (SourceDocumentKind::Scene, SceneFileEncoding::Ron) => document.to_ron_string().unwrap(),
        (SourceDocumentKind::Prefab, SceneFileEncoding::Json) => PrefabDocument {
            inputs: Default::default(),
            entities: document.entities,
            placements: document.placements,
        }
        .to_json_string()
        .unwrap(),
        (SourceDocumentKind::Prefab, SceneFileEncoding::Ron) => PrefabDocument {
            inputs: Default::default(),
            entities: document.entities,
            placements: document.placements,
        }
        .to_ron_string()
        .unwrap(),
    }
    .into_bytes()
}

#[test]
fn source_sorting_and_invalid_structure_are_admitted_before_they_run() {
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
            let bytes = source(kind, encoding, true);
            let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
            // This covers the existing wire passes, leaving no allowance for domain work.
            budget.limits.decode_work = nara_core::ItemLimit::new(bytes.len() * 3).unwrap();
            assert!(matches!(
                decode_source(
                    kind,
                    encoding,
                    &bytes,
                    SceneFileLimits::default(),
                    &mut budget
                ),
                Err(SourceDecodeError::Admission(
                    AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
                ))
            ));
            assert!(budget.decode_work > bytes.len() * 3);
            assert_eq!(budget.retained_candidate_bytes, 0);
            assert_eq!(budget.retained_open_handles, 0);

            let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
            assert!(matches!(
                decode_source(
                    kind,
                    encoding,
                    &bytes,
                    SceneFileLimits::default(),
                    &mut budget
                ),
                Err(SourceDecodeError::Structure(_))
            ));
            assert!(budget.decode_work > bytes.len() * 3);
            assert_eq!(budget.retained_candidate_bytes, 0);
            let spent = budget.decode_work;
            let corrected = source(kind, encoding, false);
            let tree = decode_source(
                kind,
                encoding,
                &corrected,
                SceneFileLimits::default(),
                &mut budget,
            )
            .unwrap();
            assert!(budget.decode_work > spent);
            release_tree(tree, &mut budget).unwrap();
            assert_eq!(budget.retained_candidate_bytes, 0);
        }
    }
}

#[test]
fn source_decode_exact_work_and_one_short_preserve_the_same_continuing_budget() {
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
            let bytes = source(kind, encoding, false);
            let mut measured = AuthoringProjectAdmissionBudget::new(Default::default());
            let tree = decode_source(
                kind,
                encoding,
                &bytes,
                SceneFileLimits::default(),
                &mut measured,
            )
            .unwrap();
            release_tree(tree, &mut measured).unwrap();
            for short in [false, true] {
                let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
                budget.limits.decode_work =
                    nara_core::ItemLimit::new(measured.decode_work - usize::from(short)).unwrap();
                let result = decode_source(
                    kind,
                    encoding,
                    &bytes,
                    SceneFileLimits::default(),
                    &mut budget,
                );
                if short {
                    assert!(matches!(
                        result,
                        Err(SourceDecodeError::Admission(
                            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
                        ))
                    ));
                } else {
                    release_tree(result.unwrap(), &mut budget).unwrap();
                }
                assert_eq!(budget.decode_work, measured.decode_work);
                assert_eq!(budget.retained_candidate_bytes, 0);
            }
        }
    }
}

#[test]
fn source_decode_host_capacity_refusal_refunds_scratch_and_preserves_existing_usage() {
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
            let bytes = source(kind, encoding, false);
            let limits = SceneFileLimits::default();
            let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
            budget.charge_source_bytes(0, 0, 17, 31).unwrap();
            budget.retain_open_handles(2).unwrap();
            let tree = decode_source(kind, encoding, &bytes, limits, &mut budget).unwrap();
            assert_eq!(
                budget.retained_candidate_bytes,
                31 + tree.retained_bytes().unwrap()
            );
            release_tree(tree, &mut budget).unwrap();
            assert_eq!(budget.retained_candidate_bytes, 31);
            assert_eq!(budget.retained_open_handles, 2);

            let shape = limits.shape();
            let one_node = limits.with_shape(nara_core::SerdeShapeLimits::new(
                shape.depth(),
                nara_core::ItemLimit::new(1).unwrap(),
                shape.container_items(),
                shape.string_bytes(),
                shape.total_string_bytes(),
            ));
            let mut probe = budget;
            let scratch = reserve_source_decode(
                bytes.len(),
                limits.encoded_bytes().get(),
                0,
                one_node,
                &mut probe,
            )
            .unwrap()
            .bytes;
            let maximum = budget.retained_candidate_bytes + scratch;
            budget.limits = budget
                .limits
                .with_retained_candidate_bytes(nara_core::ByteLimit::new(maximum).unwrap());
            let mut probe = budget;
            let reservation = reserve_source_decode(
                bytes.len(),
                limits.encoded_bytes().get(),
                0,
                limits,
                &mut probe,
            )
            .unwrap();
            assert!(reservation.constrained_nodes);
            assert_eq!(reservation.limits.shape().nodes().get(), 1);
            assert_eq!(probe.retained_candidate_bytes, maximum);

            let work_before = budget.decode_work;
            assert!(matches!(
                decode_source(kind, encoding, &bytes, limits, &mut budget),
                Err(SourceDecodeError::Admission(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                        maximum: actual,
                    }
                )) if actual == maximum
            ));
            assert_eq!(budget.decode_work, work_before + bytes.len() * 3);
            assert_eq!(budget.retained_candidate_bytes, 31);
            assert_eq!(budget.retained_open_handles, 2);
        }
    }
}

#[test]
fn source_decode_explicit_shape_ceilings_remain_typed_format_errors() {
    let defaults = SceneFileLimits::default();
    let shape = defaults.shape();
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
            let bytes = source(kind, encoding, false);
            for (nodes, items, expected) in [
                (
                    nara_core::ItemLimit::new(1).unwrap(),
                    shape.container_items(),
                    nara_core::SerdeShapeError::NodeLimitExceeded { maximum: 1 },
                ),
                (
                    shape.nodes(),
                    nara_core::ItemLimit::new(1).unwrap(),
                    nara_core::SerdeShapeError::ContainerItemLimitExceeded { maximum: 1 },
                ),
            ] {
                let limits = defaults.with_shape(nara_core::SerdeShapeLimits::new(
                    shape.depth(),
                    nodes,
                    items,
                    shape.string_bytes(),
                    shape.total_string_bytes(),
                ));
                let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
                budget.charge_source_bytes(0, 0, 17, 31).unwrap();
                budget.retain_open_handles(2).unwrap();
                let mut probe = budget;
                let reservation = reserve_source_decode(
                    bytes.len(),
                    limits.encoded_bytes().get(),
                    0,
                    limits,
                    &mut probe,
                )
                .unwrap();
                assert!(!reservation.constrained_nodes);

                let result = decode_source(kind, encoding, &bytes, limits, &mut budget);
                let Err(SourceDecodeError::Format(error)) = result else {
                    panic!("an explicit shape ceiling must remain a format error")
                };
                assert!(matches!(
                    &error,
                    SceneFormatError::Shape { encoding: actual, .. } if *actual == encoding
                ));
                assert_eq!(error.shape_error(), Some(&expected));
                assert_eq!(budget.decode_work, 17 + bytes.len() * 3);
                assert_eq!(budget.retained_candidate_bytes, 31);
                assert_eq!(budget.retained_open_handles, 2);
            }
        }
    }
}

#[test]
fn source_decode_explicit_container_ceiling_survives_host_restricted_nodes() {
    let defaults = SceneFileLimits::default();
    let shape = defaults.shape();
    let limits = defaults.with_shape(nara_core::SerdeShapeLimits::new(
        shape.depth(),
        shape.nodes(),
        nara_core::ItemLimit::ONE,
        shape.string_bytes(),
        shape.total_string_bytes(),
    ));
    let probe_limits = limits.with_shape(nara_core::SerdeShapeLimits::new(
        shape.depth(),
        nara_core::ItemLimit::new(64).unwrap(),
        nara_core::ItemLimit::ONE,
        shape.string_bytes(),
        shape.total_string_bytes(),
    ));
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
            let bytes = source(kind, encoding, false);
            let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
            budget.charge_source_bytes(0, 0, 17, 31).unwrap();
            budget.retain_open_handles(2).unwrap();
            let mut probe = budget;
            let scratch = reserve_source_decode(
                bytes.len(),
                limits.encoded_bytes().get(),
                0,
                probe_limits,
                &mut probe,
            )
            .unwrap()
            .bytes;
            budget.limits = budget
                .limits
                .with_retained_candidate_bytes(nara_core::ByteLimit::new(31 + scratch).unwrap());
            let mut probe = budget;
            let reservation = reserve_source_decode(
                bytes.len(),
                limits.encoded_bytes().get(),
                0,
                limits,
                &mut probe,
            )
            .unwrap();
            assert!(reservation.constrained_nodes);
            assert!(!reservation.constrained_container_items);
            assert_eq!(reservation.limits.shape().nodes().get(), 64);
            let Err(SourceDecodeError::Format(error)) =
                decode_source(kind, encoding, &bytes, limits, &mut budget)
            else {
                panic!("an unchanged explicit container ceiling must remain a format error")
            };
            assert_eq!(
                error.shape_error(),
                Some(&nara_core::SerdeShapeError::ContainerItemLimitExceeded { maximum: 1 })
            );
            assert_eq!(budget.decode_work, 17 + bytes.len() * 3);
            assert_eq!(budget.retained_candidate_bytes, 31);
            assert_eq!(budget.retained_open_handles, 2);
        }
    }
}

#[test]
fn canonical_comparison_admits_repeated_placement_keys_separately_from_scalar_payloads() {
    use nara_scene::{PrefabEntityTarget, SceneEntityId, ScenePlacement};
    let root = SceneEntityId::new("r".repeat(120)).unwrap();
    let entities = std::iter::once(SceneEntityRecord::new(root.clone()))
        .chain((0..64).map(|index| {
            SceneEntityRecord::new(SceneEntityId::new(format!("child-{index:03}")).unwrap())
        }))
        .collect::<Vec<_>>();
    let flat = SceneDocument::from_ordered_roots(entities);
    let mut nested = flat.clone();
    for placement in nested.placements.iter_mut().skip(1) {
        placement.placement = ScenePlacement::last(ScenePlacementParent::Entity(
            PrefabEntityTarget::source(root.clone()),
        ));
    }
    let encoded = flat.to_json_string().unwrap().len();
    let flat = SourceTree::Scene(flat.clone());
    let nested = SourceTree::Scene(nested);
    let mut flat_work = AuthoringProjectAdmissionBudget::new(Default::default());
    let mut nested_work = AuthoringProjectAdmissionBudget::new(Default::default());
    flat.admit_canonical_comparison(encoded, &mut flat_work)
        .unwrap();
    nested
        .admit_canonical_comparison(encoded, &mut nested_work)
        .unwrap();
    let flat_retained = flat.retained_bytes().unwrap();
    assert!(
        nested_work.decode_work - flat_work.decode_work
            > nested.retained_bytes().unwrap() - flat.retained_bytes().unwrap()
    );
    for short in [false, true] {
        let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
        budget.limits.decode_work =
            nara_core::ItemLimit::new(nested_work.decode_work - usize::from(short)).unwrap();
        let result = nested.admit_canonical_comparison(encoded, &mut budget);
        assert_eq!(result.is_ok(), !short);
        assert_eq!(budget.decode_work, nested_work.decode_work);
        assert_eq!(budget.retained_candidate_bytes, 0);
    }
    let (SourceTree::Scene(mut plain), SourceTree::Scene(_)) = (flat, nested) else {
        unreachable!()
    };
    plain.entities[0].components.insert(
        nara_reflect::ComponentTypeId::new("unknown.Notes"),
        nara_scene::SceneComponentRecord::new(
            nara_reflect::ComponentSchemaVersion::ONE,
            nara_reflect::ComponentValue::String("n".repeat(128 * 1024)),
        ),
    );
    let with_scalar = SourceTree::Scene(plain);
    let mut scalar_work = AuthoringProjectAdmissionBudget::new(Default::default());
    with_scalar
        .admit_canonical_comparison(encoded, &mut scalar_work)
        .unwrap();
    assert_eq!(
        scalar_work.decode_work - flat_work.decode_work,
        with_scalar.retained_bytes().unwrap() - flat_retained
    );
}
