use super::*;

#[test]
fn frozen_complete_input_moves_to_a_worker_and_uses_the_direct_preparation_kernel() {
    let fixture = Fixture::new();
    let (mut workspace, mut authority, documents) = fixture.open();
    let before = snapshot(&workspace);
    let mut direct_work = AuthoredSchemaUpgradeWork::default();
    let direct = fixture.prepare(&workspace, &mut direct_work);
    let mut work = AuthoredSchemaUpgradeWork::default();
    let input = workspace
        .capture_authored_schema_upgrade_input(&fixture.previous, &mut work)
        .unwrap();
    assert_eq!(input.documents().collect::<Vec<_>>(), documents);
    assert_eq!(input.documents().len(), workspace.len());
    assert_eq!(input.schema_basis(), &fixture.previous);
    assert_eq!(input.active, workspace.active_document());
    for document in documents {
        assert_eq!(
            input.document_revision(document),
            workspace.document_revision(document)
        );
        assert_eq!(
            input.document_source_binding(document),
            workspace.document_source_binding(document)
        );
        match &workspace.documents[&document] {
            EditorDocumentSlot::Scene(slot) => {
                assert!(std::ptr::eq(
                    input.scene_document(document).unwrap(),
                    slot.session().document()
                ));
                assert!(input.prefab_document(document).is_none());
                assert!(std::ptr::eq(
                    input.document_source(document).unwrap(),
                    slot.session().authoring_source().unwrap()
                ));
            }
            EditorDocumentSlot::Prefab(slot) => {
                assert!(std::ptr::eq(
                    input.prefab_document(document).unwrap(),
                    slot.session().document()
                ));
                assert!(input.scene_document(document).is_none());
                assert!(std::ptr::eq(
                    input.document_source(document).unwrap(),
                    slot.session().authoring_source().unwrap()
                ));
            }
            EditorDocumentSlot::Extension(_) => unreachable!(),
        }
    }
    let missing = EditorDocumentId::from_raw(9001);
    assert!(input.scene_document(missing).is_none());
    assert!(input.prefab_document(missing).is_none());
    assert!(input.document_revision(missing).is_none());
    assert!(input.document_source_binding(missing).is_none());
    assert!(input.document_source(missing).is_none());

    let (input, prepared, mut work) = std::thread::spawn(move || {
        let mut proposed = frozen_proposals(&input);
        proposed.reverse();
        let prepared = input
            .prepare(&fixture.upgrade, proposed, &mut work)
            .unwrap();
        (input, prepared, work)
    })
    .join()
    .unwrap();
    assert_eq!(work.usage(), direct_work.usage());
    assert_eq!(snapshot(&workspace), before);
    for document in documents {
        assert_eq!(
            prepared.source_binding(document),
            direct.source_binding(document)
        );
        if let Some(source) = prepared.scene(document) {
            assert_eq!(
                source.document(),
                direct.scene(document).unwrap().document()
            );
        } else {
            assert_eq!(
                prepared.prefab(document).unwrap().document(),
                direct.prefab(document).unwrap().document()
            );
        }
    }
    prepared.validate(&workspace, &mut work).unwrap();
    let saved = observations(&prepared);
    authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
        .unwrap()
        .commit();
    // The retained input still borrows the original dirty sources after actual installation.
    for (document, original) in input.documents().zip(&before) {
        assert_eq!(
            input.document_revision(document),
            Some(original.tab.revision)
        );
        assert_ne!(
            input.document_revision(document),
            workspace.document_revision(document)
        );
        assert_eq!(workspace.document_is_dirty(document), Some(false));
    }
}

#[test]
fn failed_frozen_preparation_keeps_original_tokens_for_an_exact_capacity_retry() {
    let fixture = Fixture::new();
    let (workspace, _, documents) = fixture.open();
    let before = snapshot(&workspace);
    let mut measured = AuthoredSchemaUpgradeWork::default();
    drop(fixture.prepare(&workspace, &mut measured));
    let required = measured.usage();
    let mut work = work_with_limits(required.work_bytes * 4, required.peak_retained_bytes);
    let input = workspace
        .capture_authored_schema_upgrade_input(&fixture.previous, &mut work)
        .unwrap();
    let entries = input.sources.as_ptr();
    let original_prefab = input.prefab_document(documents[2]).unwrap() as *const PrefabDocument;
    let capture_usage = work.usage();
    let mut proposed = frozen_proposals(&input);
    // Two Scene successors have already been retained when the final Prefab refuses its old
    // record version. No output or token from this attempt may remain charged after refusal.
    proposed[2].candidate = ProposalCandidate::Prefab(
        PrefabDocumentCandidate::decode_json_str(
            &input
                .prefab_document(documents[2])
                .unwrap()
                .to_json_string()
                .unwrap(),
        )
        .unwrap(),
    );
    assert_refused(
        &input
            .prepare(&fixture.upgrade, proposed, &mut work)
            .unwrap_err(),
    );
    assert!(work.usage().work_bytes > capture_usage.work_bytes);
    assert_eq!(input.sources.as_ptr(), entries);
    assert_eq!(
        input.prefab_document(documents[2]).unwrap() as *const PrefabDocument,
        original_prefab
    );
    let prepared = input
        .prepare(&fixture.upgrade, frozen_proposals(&input), &mut work)
        .unwrap();
    assert!(work.usage().work_bytes > required.work_bytes);
    assert_eq!(
        work.usage().peak_retained_bytes,
        required.peak_retained_bytes
    );
    assert_eq!(prepared.documents().collect::<Vec<_>>(), documents);
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn frozen_input_refuses_foreign_nominal_basis_and_work_without_losing_originals() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    let (workspace, _, _) = fixture.open();
    let before = snapshot(&workspace);
    let mut work = AuthoredSchemaUpgradeWork::default();
    let input = workspace
        .capture_authored_schema_upgrade_input(&fixture.previous, &mut work)
        .unwrap();
    let captured = work.usage();
    assert_refused(
        &input
            .prepare(&foreign.upgrade, frozen_proposals(&input), &mut work)
            .unwrap_err(),
    );
    assert_eq!(work.usage(), captured);
    let mut other_work = AuthoredSchemaUpgradeWork::default();
    let report = input
        .prepare(&fixture.upgrade, frozen_proposals(&input), &mut other_work)
        .unwrap_err();
    assert_refused(&report);
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authored-schema-upgrade-stale"
        })
    );
    assert_eq!(work.usage(), captured);
    let prepared = input
        .prepare(&fixture.upgrade, frozen_proposals(&input), &mut work)
        .unwrap();
    drop(prepared);
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn capture_preadmits_storage_and_settles_a_partial_capture_before_retry() {
    const PRIOR_STORAGE: usize = 64;
    let fixture = Fixture::new();
    let (mut workspace, _, documents) = fixture.open();
    let before = snapshot(&workspace);
    for (maximum_work, maximum_retained) in [(1, 256 * 1024 * 1024), (512 * 1024 * 1024, 1)] {
        let mut work = work_with_limits(maximum_work, maximum_retained);
        let report = workspace
            .capture_authored_schema_upgrade_input(&fixture.previous, &mut work)
            .unwrap_err();
        assert_refused(&report);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.fields().iter().any(|field| {
                field.key().as_str() == "preparation-stage"
                    && field.display_value() == "workspace-source-retention-validation"
            })
        }));
        assert_eq!(work.usage().work_bytes, 0);
    }
    let mut measured = AuthoredSchemaUpgradeWork::default();
    measured.retain_preparation_storage(PRIOR_STORAGE).unwrap();
    drop(
        workspace
            .capture_authored_schema_upgrade_input(&fixture.previous, &mut measured)
            .unwrap(),
    );
    let required = measured.usage();
    let mut source = document();
    source.entities[1]
        .components
        .get_mut(&ComponentTypeId::new("missing.Opaque"))
        .unwrap()
        .value = ComponentValue::String("x".repeat(required.peak_retained_bytes));
    let path = "scenes/extra.scene.json";
    let published = SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path(path).unwrap()),
            &fixture.previous,
        )
        .unwrap()
        .publish();
    let extra = workspace
        .open_authoring_scene_document("Extra", EditorDocumentSourceBinding::new(path), published)
        .unwrap()
        .opened_document
        .unwrap();
    let mut work = work_with_limits(required.work_bytes * 8, required.peak_retained_bytes);
    work.retain_preparation_storage(PRIOR_STORAGE).unwrap();
    let report = workspace
        .capture_authored_schema_upgrade_input(&fixture.previous, &mut work)
        .unwrap_err();
    assert_refused(&report);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.authored-schema-upgrade-limit-exceeded"
    }));
    assert!(work.usage().work_bytes > PRIOR_STORAGE);
    assert!(workspace.__discard_document(extra).applied);
    assert!(workspace.activate_document(documents[0]).applied);
    let input = workspace
        .capture_authored_schema_upgrade_input(&fixture.previous, &mut work)
        .unwrap();
    assert_eq!(input.documents().collect::<Vec<_>>(), documents);
    assert!(work.usage().work_bytes > required.work_bytes);
    assert_eq!(
        work.usage().peak_retained_bytes,
        required.peak_retained_bytes
    );
    assert_eq!(snapshot(&workspace), before);
}
