//! Independent fresh-basis admission of fixed stored inputs and outputs, without reconstruction.

use super::*;
use nara_identity::ReferenceInputId;

struct Stored {
    request: PrefabExtractionRequest,
    outputs: Vec<CapturedExtractionSource>,
    q: ResolvedPrefabSource,
    disk: Vec<CapturedExtractionSource>,
    assets: [AssetRecord; 3],
    q_asset: AssetRecord,
    work: PrefabExtractionWork,
}

fn wire(source: &CapturedExtractionSource, encoding: SceneFileEncoding) -> Vec<u8> {
    match (source, encoding) {
        (CapturedExtractionSource::Scene { document, .. }, SceneFileEncoding::Json) => {
            document.to_json_string().unwrap()
        }
        (CapturedExtractionSource::Scene { document, .. }, SceneFileEncoding::Ron) => {
            document.to_ron_string().unwrap()
        }
        (CapturedExtractionSource::Prefab { source }, SceneFileEncoding::Json) => {
            source.document().to_json_string().unwrap()
        }
        (CapturedExtractionSource::Prefab { source }, SceneFileEncoding::Ron) => {
            source.document().to_ron_string().unwrap()
        }
    }
    .into_bytes()
}

fn decode(
    bytes: &[u8],
    key: &SourceDocumentKey,
    basis: &AuthoringSchemaBasis,
    asset: Option<&AssetRecord>,
    encoding: SceneFileEncoding,
    reverse: bool,
) -> CapturedExtractionSource {
    match key.kind() {
        SourceDocumentKind::Scene => {
            let mut document = match encoding {
                SceneFileEncoding::Json => SceneDocumentCandidate::decode_json_bytes(bytes),
                SceneFileEncoding::Ron => SceneDocumentCandidate::decode_ron_bytes(bytes),
            }
            .unwrap()
            .into_composition_source()
            .unwrap();
            if reverse {
                document.entities.reverse();
            }
            CapturedExtractionSource::Scene {
                key: key.clone(),
                document: Arc::new(document),
                basis: basis.clone(),
            }
        }
        SourceDocumentKind::Prefab => {
            let mut document = match encoding {
                SceneFileEncoding::Json => PrefabDocumentCandidate::decode_json_bytes(bytes),
                SceneFileEncoding::Ron => PrefabDocumentCandidate::decode_ron_bytes(bytes),
            }
            .unwrap()
            .into_composition_source()
            .unwrap();
            if reverse {
                document.entities.reverse();
            }
            CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(document),
                    basis.clone(),
                    key.clone(),
                )
                .with_source_asset(asset.unwrap())
                .unwrap(),
            }
        }
    }
}

fn stored(encoding: SceneFileEncoding, reverse: bool, limits: SceneCompilationLimits) -> Stored {
    stored_with_unavailable(encoding, reverse, limits, false)
}

fn stored_with_unavailable(
    encoding: SceneFileEncoding,
    reverse: bool,
    limits: SceneCompilationLimits,
    unavailable: bool,
) -> Stored {
    let old_basis = basis();
    let mut fixture = Fixture::new(&old_basis);
    let mut p = fixture.prefabs[P - 1].document().clone();
    p.placements.push(SceneEntityPlacement::root(id("z")));
    p.entities.push(SceneEntityRecord::new(id("z")));
    p.entities
        .iter_mut()
        .find(|record| record.id == id("k"))
        .unwrap()
        .components
        .insert(
            component(),
            link(EntityReference::SceneLocal { entity: id("z") }),
        );
    // This is initial saved source setup. Acquire it through the ordinary decoder and basis
    // admission, rather than requesting a composition edit from a field-only authoring schema.
    fixture.prefabs[P - 1] = open_prefab(&p, "p", &old_basis);
    if unavailable {
        let u = PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("zz")),
            SceneEntityRecord::new(id("u")).with_component(
                ComponentTypeId::new("missing.StoredOpaque"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("preserve this unavailable contributor".into()),
                ),
            ),
        ]);
        fixture.prefabs[U - 1] = open_prefab(&u, "u", &old_basis);
    }
    let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
    let capture = fixture.capture(vec![environment], Default::default());
    let prepared = capture.request.prepare(capture.work).unwrap();
    assert_eq!(prepared.extracted().document().inputs.len(), 2);
    let before = prepared
        .original_sources()
        .iter()
        .map(|source| wire(source, encoding))
        .collect::<Vec<_>>();
    let after = prepared
        .sources()
        .iter()
        .map(|source| wire(source, encoding))
        .collect::<Vec<_>>();
    let disk = capture
        .disk
        .iter()
        .map(|source| wire(source, encoding))
        .collect::<Vec<_>>();
    let q_bytes = match encoding {
        SceneFileEncoding::Json => prepared.extracted().document().to_json_string(),
        SceneFileEncoding::Ron => prepared.extracted().document().to_ron_string(),
    }
    .unwrap()
    .into_bytes();
    let fresh_basis = basis();
    assert_ne!(fresh_basis, old_basis);
    let mut work = PrefabExtractionWork::new(limits).unwrap();
    let encoded = before
        .iter()
        .chain(&after)
        .chain(&disk)
        .map(Vec::len)
        .sum::<usize>()
        + q_bytes.len();
    work.admit_capture_storage(Some(encoded * 32 + 32_768))
        .unwrap();
    let decode_all = |blobs: &[Vec<u8>], reverse| {
        blobs
            .iter()
            .enumerate()
            .map(|(index, bytes)| {
                decode(
                    bytes,
                    &fixture.keys[index],
                    &fresh_basis,
                    index.checked_sub(1).map(|index| &fixture.assets[index]),
                    encoding,
                    reverse,
                )
            })
            .collect::<Vec<_>>()
    };
    let originals = decode_all(&before, reverse);
    let outputs = decode_all(&after, false);
    let disk = decode_all(&disk, false);
    let CapturedExtractionSource::Prefab { source: q } = decode(
        &q_bytes,
        &fixture.q_key,
        &fresh_basis,
        Some(&fixture.q),
        encoding,
        false,
    ) else {
        unreachable!()
    };
    let request = PrefabExtractionRequest::new(
        originals,
        vec![fixture.environment("desktop", &[R, P, U, GUARD])],
        0,
        fresh_basis,
        fixture.keys[P].clone(),
        id("k"),
        PrefabExtractionDestination {
            key: fixture.q_key.clone(),
            reference: AssetRef::StableId(fixture.q.stable_id()),
            metadata: fixture.q.clone(),
        },
    )
    .with_max_depth(16);
    Stored {
        request,
        outputs,
        q,
        disk,
        assets: fixture.assets.clone(),
        q_asset: fixture.q.clone(),
        work,
    }
}

#[test]
fn unavailable_scene_contributor_survives_different_original_and_output_record_positions() {
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        let Stored {
            request,
            outputs,
            q,
            disk,
            work,
            ..
        } = stored_with_unavailable(encoding, true, Default::default(), true);
        let original = prefab_source(&request.sources()[U])
            .document()
            .entities
            .iter()
            .position(|record| record.id == id("u"))
            .unwrap();
        let output = prefab_source(&outputs[U])
            .document()
            .entities
            .iter()
            .position(|record| record.id == id("u"))
            .unwrap();
        assert_ne!(original, output);
        let before = compile(
            request.sources(),
            &request.environments()[0],
            None,
            match &request.sources()[R] {
                CapturedExtractionSource::Scene { basis, .. } => basis,
                _ => unreachable!(),
            },
        );
        assert!(
            !before.diagnostics().has_errors(),
            "{:?}",
            before.diagnostics()
        );
        assert!(before.failure().is_none());
        assert_eq!(
            before
                .omissions()
                .iter()
                .filter(|omission| omission.component.as_str() == "missing.StoredOpaque")
                .count(),
            1
        );
        let mut verified = request.admit_outputs(outputs, q, work).unwrap();
        assert_eq!(verified.source_changed(U), Some(false));
        let proof = verified
            .prepare_saved_generation(&disk, &[true; 4])
            .unwrap();
        verified.require_saved_generation(&proof).unwrap();
        let after = compile(
            proof.sources(),
            &verified.environments()[0],
            Some(proof.extracted()),
            verified.schema_basis(),
        );
        assert!(
            !after.diagnostics().has_errors(),
            "{:?}",
            after.diagnostics()
        );
        assert!(after.failure().is_none());
        assert_eq!(
            after
                .omissions()
                .iter()
                .filter(|omission| omission.component.as_str() == "missing.StoredOpaque")
                .count(),
            1
        );
    }
}

#[test]
fn stored_json_and_ron_use_fresh_basis_fixed_names_and_actual_output_allocations() {
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        for reverse in [false, true] {
            let Stored {
                request,
                outputs,
                q,
                disk,
                work,
                ..
            } = stored(encoding, reverse, Default::default());
            let original = request.sources().as_ptr();
            let output_table = outputs.as_ptr();
            let allocations = outputs.iter().map(allocation).collect::<Vec<_>>();
            let q_allocation = q.document() as *const PrefabDocument;
            let arguments = prefab_source(&outputs[P])
                .document()
                .entities
                .iter()
                .find(|record| record.id == id("k"))
                .unwrap()
                .prefab
                .as_ref()
                .unwrap()
                .arguments
                .as_ref()
                .unwrap()
                .values
                .clone();
            let mut verified = request.admit_outputs(outputs, q, work).unwrap();
            assert_eq!(verified.original_sources().as_ptr(), original);
            assert_eq!(verified.sources().as_ptr(), output_table);
            assert_eq!(
                verified
                    .sources()
                    .iter()
                    .map(allocation)
                    .collect::<Vec<_>>(),
                allocations
            );
            assert_eq!(verified.extracted().document() as *const _, q_allocation);
            assert_eq!(
                prefab_source(&verified.sources()[P])
                    .document()
                    .entities
                    .iter()
                    .find(|record| record.id == id("k"))
                    .unwrap()
                    .prefab
                    .as_ref()
                    .unwrap()
                    .arguments
                    .as_ref()
                    .unwrap()
                    .values,
                arguments
            );
            assert_eq!(verified.source_changed(P), Some(true));
            assert_eq!(verified.source_changed(R), Some(true));
            assert_eq!(verified.source_changed(U), Some(false));
            let proof = verified
                .prepare_saved_generation(&disk, &[true; 4])
                .unwrap();
            verified.require_saved_generation(&proof).unwrap();
            successful(compile(
                proof.sources(),
                &verified.environments()[0],
                Some(proof.extracted()),
                verified.schema_basis(),
            ));
        }
    }
}

#[test]
fn stored_unused_input_refusal_returns_exact_sources_and_work_for_corrected_retry() {
    let Stored {
        request,
        outputs,
        q,
        q_asset,
        work,
        ..
    } = stored(SceneFileEncoding::Json, true, Default::default());
    let source_table = request.sources().as_ptr();
    let output_table = outputs.as_ptr();
    let mut document = q.document().clone();
    document
        .inputs
        .insert(ReferenceInputId::new("unused.stored").unwrap());
    let bad = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(document),
        request
            .sources()
            .iter()
            .find_map(|source| match source {
                CapturedExtractionSource::Scene { basis, .. } => Some(basis.clone()),
                _ => None,
            })
            .unwrap(),
        q.key().clone(),
    )
    .with_source_asset(&q_asset)
    .unwrap();
    let bad_allocation = bad.document() as *const PrefabDocument;
    let before = work.usage();
    let failure = request.admit_outputs(outputs, bad, work).unwrap_err();
    assert_code(failure.report(), "scene.prefab-extraction-invalid-source");
    let (request, outputs, bad, work, _) = failure.into_parts();
    assert_eq!(request.sources().as_ptr(), source_table);
    assert_eq!(outputs.as_ptr(), output_table);
    assert_eq!(bad.document() as *const _, bad_allocation);
    assert!(work.usage().work_bytes > before.work_bytes);
    let spent = work.usage();
    let verified = request.admit_outputs(outputs, q, work).unwrap();
    assert_eq!(verified.sources().as_ptr(), output_table);
    assert!(verified.usage().work_bytes > spent.work_bytes);
}

#[test]
fn stored_anchor_component_and_moved_reference_forgery_are_not_valid_correspondence() {
    for defect in ["anchor", "reference", "extra-record"] {
        let Stored {
            request,
            mut outputs,
            q,
            q_asset,
            assets,
            work,
            ..
        } = stored(SceneFileEncoding::Ron, true, Default::default());
        let basis = request
            .sources()
            .iter()
            .find_map(|source| match source {
                CapturedExtractionSource::Scene { basis, .. } => Some(basis.clone()),
                _ => None,
            })
            .unwrap();
        let q = if defect == "anchor" {
            let mut document = prefab_source(&outputs[P]).document().clone();
            document
                .entities
                .iter_mut()
                .find(|record| record.id == id("k"))
                .unwrap()
                .components
                .insert(
                    component(),
                    link(EntityReference::SceneLocal { entity: id("x") }),
                );
            outputs[P] = CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(document),
                    basis.clone(),
                    request.sources()[P].key().clone(),
                )
                .with_source_asset(&assets[P - 1])
                .unwrap(),
            };
            q
        } else {
            let mut document = q.document().clone();
            if defect == "reference" {
                let target = document
                    .entities
                    .iter_mut()
                    .find(|record| record.id == id("k"))
                    .unwrap()
                    .components
                    .get_mut(&component())
                    .unwrap();
                target.value = link(EntityReference::Authored {
                    reference: AuthoredEntityReference::input(
                        ReferenceInputId::new("extracted.0").unwrap(),
                    ),
                })
                .value;
            } else {
                document
                    .entities
                    .push(SceneEntityRecord::new(id("unreviewed")));
                document
                    .placements
                    .push(SceneEntityPlacement::root(id("unreviewed")));
            }
            ResolvedPrefabSource::bound_authoring_source(Arc::new(document), basis, q.key().clone())
                .with_source_asset(&q_asset)
                .unwrap()
        };
        let failure = request.admit_outputs(outputs, q, work).unwrap_err();
        assert_code(failure.report(), "scene.prefab-extraction-invalid-source");
    }
}

#[test]
fn stored_admission_has_inclusive_cumulative_work_limits_and_refusal_keeps_the_supplied_generation()
{
    let Stored {
        request,
        outputs,
        q,
        work,
        ..
    } = stored(SceneFileEncoding::Json, true, Default::default());
    let measured = request.admit_outputs(outputs, q, work).unwrap().usage();
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(measured.work_bytes).unwrap())
        .with_reference_work_nodes(ItemLimit::new(measured.work_nodes).unwrap());
    let Stored {
        request,
        outputs,
        q,
        work,
        ..
    } = stored(SceneFileEncoding::Json, true, exact);
    assert_eq!(
        request.admit_outputs(outputs, q, work).unwrap().usage(),
        measured
    );
    for limits in [
        exact.with_working_bytes(ByteLimit::new(measured.work_bytes - 1).unwrap()),
        exact.with_reference_work_nodes(ItemLimit::new(measured.work_nodes - 1).unwrap()),
    ] {
        let Stored {
            request,
            outputs,
            q,
            work,
            ..
        } = stored(SceneFileEncoding::Json, true, limits);
        let original = request.sources().as_ptr();
        let output_table = outputs.as_ptr();
        let q_allocation = q.document() as *const PrefabDocument;
        let failure = request.admit_outputs(outputs, q, work).unwrap_err();
        assert_code(failure.report(), "scene.prefab-extraction-capacity");
        let (request, outputs, q, work, _) = failure.into_parts();
        assert_eq!(request.sources().as_ptr(), original);
        assert_eq!(outputs.as_ptr(), output_table);
        assert_eq!(q.document() as *const _, q_allocation);
        let spent = work.usage();
        let failure = request.admit_outputs(outputs, q, work).unwrap_err();
        assert_code(failure.report(), "scene.prefab-extraction-capacity");
        let (_, outputs, q, work, _) = failure.into_parts();
        assert_eq!(outputs.as_ptr(), output_table);
        assert_eq!(q.document() as *const _, q_allocation);
        assert!(work.usage().work_bytes >= spent.work_bytes);
        assert!(work.usage().work_nodes >= spent.work_nodes);
    }
}
