use super::*;

fn prepared_checkpoints(
    prepared: &PreparedEditorWorkspaceAuthoredSchemaUpgrade,
) -> Vec<(
    EditorDocumentId,
    usize,
    SceneAuthoringRevision,
    SceneAuthoringContentState,
)> {
    prepared
        .sources
        .iter()
        .map(|source| {
            let address = match &source.source {
                PreparedSourceKind::Scene(source) => {
                    source.document() as *const SceneDocument as usize
                }
                PreparedSourceKind::Prefab(source) => {
                    source.document() as *const PrefabDocument as usize
                }
            };
            (
                source.document,
                address,
                source.source.revision(),
                source.source.content_state(),
            )
        })
        .collect()
}

#[test]
fn late_saved_observation_refusal_returns_original_owners_and_order_for_corrected_retry() {
    let fixture = Fixture::new();
    let (mut workspace, mut authority, documents) = fixture.open();
    let before = snapshot(&workspace);
    let mut work = AuthoredSchemaUpgradeWork::default();
    let prepared = fixture.prepare(&workspace, &mut work);
    let checkpoints = prepared_checkpoints(&prepared);
    let source_allocation = (prepared.sources.as_ptr(), prepared.sources.capacity());
    let owner = Arc::as_ptr(&prepared.owner);
    let preparation_usage = prepared.usage();
    let mut saved = observations(&prepared);
    saved.reverse();
    let saved_allocation = (saved.as_ptr(), saved.capacity());
    let last = saved
        .iter_mut()
        .find(|saved| saved.document == documents[2])
        .unwrap();
    let actual_digest = last.digest;
    last.digest = EditorDocumentDigest::new(u64::MAX, [0x51; 32]);
    let saved_values = saved
        .iter()
        .map(|saved| {
            (
                saved.document,
                saved.source_key.clone(),
                saved.source_binding.clone(),
                saved.revision,
                saved.content_state,
                saved.digest,
            )
        })
        .collect::<Vec<_>>();
    let usage = work.usage();
    let failure = authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
        .unwrap_err();
    assert_refused(failure.report());
    assert_eq!(failure.report().document, Some(documents[2]));
    let (prepared, mut saved, report) = failure.into_parts();
    assert_refused(&report);
    assert_eq!(snapshot(&workspace), before);
    assert_eq!(prepared_checkpoints(&prepared), checkpoints);
    assert_eq!(
        (prepared.sources.as_ptr(), prepared.sources.capacity()),
        source_allocation
    );
    assert_eq!(Arc::as_ptr(&prepared.owner), owner);
    assert_eq!(prepared.previous, fixture.previous);
    assert_eq!(prepared.successor, fixture.successor);
    assert_eq!(prepared.usage(), preparation_usage);
    assert_eq!((saved.as_ptr(), saved.capacity()), saved_allocation);
    assert_eq!(
        saved
            .iter()
            .map(|saved| {
                (
                    saved.document,
                    saved.source_key.clone(),
                    saved.source_binding.clone(),
                    saved.revision,
                    saved.content_state,
                    saved.digest,
                )
            })
            .collect::<Vec<_>>(),
        saved_values
    );
    assert!(work.usage().work_bytes > usage.work_bytes);
    let refused_usage = work.usage();
    saved
        .iter_mut()
        .find(|saved| saved.document == documents[2])
        .unwrap()
        .digest = actual_digest;
    authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
        .unwrap()
        .commit();
    assert!(work.usage().work_bytes > refused_usage.work_bytes);
    for ((document, address, revision, content), after) in
        checkpoints.iter().zip(snapshot(&workspace))
    {
        assert_eq!(*document, after.tab.document);
        assert_eq!(*address, after.address);
        assert_eq!(*revision, after.tab.revision);
        assert_eq!(*content, after.content);
        assert_eq!(after.tab.history, Default::default());
        assert!(!after.tab.dirty);
    }
}

#[test]
fn late_live_binding_refusal_retains_every_candidate_until_the_original_binding_is_restored() {
    let fixture = Fixture::new();
    let (mut workspace, mut authority, documents) = fixture.open();
    let before = snapshot(&workspace);
    let mut work = AuthoredSchemaUpgradeWork::default();
    let prepared = fixture.prepare(&workspace, &mut work);
    let checkpoints = prepared_checkpoints(&prepared);
    let source_allocation = (prepared.sources.as_ptr(), prepared.sources.capacity());
    let saved = observations(&prepared);
    let saved_allocation = (saved.as_ptr(), saved.capacity());
    let original_binding = match workspace.documents.get_mut(&documents[2]).unwrap() {
        EditorDocumentSlot::Prefab(slot) => std::mem::replace(
            &mut slot.source_binding,
            EditorDocumentSourceBinding::new("prefabs/moved.prefab.json"),
        ),
        _ => unreachable!(),
    };
    let changed = snapshot(&workspace);
    let failure = authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
        .unwrap_err();
    assert_refused(failure.report());
    assert_eq!(failure.report().document, Some(documents[2]));
    let (prepared, saved, _) = failure.into_parts();
    assert_eq!(snapshot(&workspace), changed);
    assert_eq!(prepared_checkpoints(&prepared), checkpoints);
    assert_eq!(
        (prepared.sources.as_ptr(), prepared.sources.capacity()),
        source_allocation
    );
    assert_eq!((saved.as_ptr(), saved.capacity()), saved_allocation);
    if let EditorDocumentSlot::Prefab(slot) = workspace.documents.get_mut(&documents[2]).unwrap() {
        slot.source_binding = original_binding;
    }
    assert_eq!(snapshot(&workspace), before);
    authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
        .unwrap()
        .commit();
    for ((_, address, revision, _), after) in checkpoints.iter().zip(snapshot(&workspace)) {
        assert_eq!(*address, after.address);
        assert_eq!(*revision, after.tab.revision);
        assert_eq!(after.tab.history, Default::default());
        assert!(!after.tab.dirty);
    }
}

#[test]
fn ready_refusal_unbind_reuses_the_exact_preparation_and_retention_reservation() {
    const RETRIES: usize = 4;
    let fixture = Fixture::new();
    let (mut workspace, mut authority, _) = fixture.open();
    let before = snapshot(&workspace);
    let mut measured = AuthoredSchemaUpgradeWork::default();
    let prepared = fixture.prepare(&workspace, &mut measured);
    let preparation_work = measured.usage().work_bytes;
    let saved = observations(&prepared);
    let (prepared, saved) = authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut measured)
        .unwrap()
        .into_prepared();
    let binding_work = measured.usage().work_bytes - preparation_work;
    let peak = measured.usage().peak_retained_bytes;
    assert!(binding_work > 0);
    drop((prepared, saved));
    let mut work = work_with_limits(preparation_work + (RETRIES + 1) * binding_work, peak);
    let mut prepared = fixture.prepare(&workspace, &mut work);
    let mut saved = observations(&prepared);
    saved.reverse();
    let source_allocation = (prepared.sources.as_ptr(), prepared.sources.capacity());
    let saved_allocation = (saved.as_ptr(), saved.capacity());
    let saved_order = saved.iter().map(|saved| saved.document).collect::<Vec<_>>();
    let checkpoints = prepared_checkpoints(&prepared);
    let owner = Arc::as_ptr(&prepared.owner);
    let preparation_usage = prepared.usage();
    for retry in 0..RETRIES {
        let commit = authority
            .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
            .unwrap();
        let bound_usage = work.usage();
        (prepared, saved) = commit.into_prepared();
        assert_eq!(work.usage(), bound_usage);
        assert_eq!(
            work.usage().work_bytes,
            preparation_work + (retry + 1) * binding_work
        );
        assert_eq!(work.usage().peak_retained_bytes, peak);
        assert_eq!(snapshot(&workspace), before);
        assert_eq!(prepared_checkpoints(&prepared), checkpoints);
        assert_eq!(
            (prepared.sources.as_ptr(), prepared.sources.capacity()),
            source_allocation
        );
        assert_eq!(Arc::as_ptr(&prepared.owner), owner);
        assert_eq!(prepared.previous, fixture.previous);
        assert_eq!(prepared.successor, fixture.successor);
        assert_eq!(prepared.usage(), preparation_usage);
        assert_eq!((saved.as_ptr(), saved.capacity()), saved_allocation);
        assert_eq!(
            saved.iter().map(|saved| saved.document).collect::<Vec<_>>(),
            saved_order
        );
    }
    authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
        .unwrap()
        .commit();
    assert_eq!(
        work.usage().work_bytes,
        preparation_work + (RETRIES + 1) * binding_work
    );
    assert_eq!(work.usage().peak_retained_bytes, peak);
    for ((_, address, revision, content), after) in checkpoints.iter().zip(snapshot(&workspace)) {
        assert_eq!(*address, after.address);
        assert_eq!(*revision, after.tab.saved_revision);
        assert_eq!(*content, after.saved_content);
        assert_eq!(after.tab.history, Default::default());
        assert!(!after.tab.dirty);
    }
}

#[test]
fn binding_capacity_refusal_returns_both_original_allocations_without_refunding_work() {
    let fixture = Fixture::new();
    let (mut workspace, mut authority, _) = fixture.open();
    let before = snapshot(&workspace);
    let mut measured = AuthoredSchemaUpgradeWork::default();
    let prepared = fixture.prepare(&workspace, &mut measured);
    // Keep an unrelated live-storage reservation so final binding, rather than an earlier
    // preparation step, determines the exact retained ceiling in the one-short control.
    let held_storage = measured.usage().peak_retained_bytes;
    measured.retain_preparation_storage(held_storage).unwrap();
    let preparation_work = measured.usage().work_bytes;
    let saved = observations(&prepared);
    let (prepared, saved) = authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut measured)
        .unwrap()
        .into_prepared();
    let binding_work = measured.usage().work_bytes - preparation_work;
    let peak = measured.usage().peak_retained_bytes;
    drop((prepared, saved));
    for (work_limit, retained_limit) in [
        (preparation_work + binding_work - 1, peak),
        (preparation_work + binding_work, peak - 1),
    ] {
        let mut work = work_with_limits(work_limit, retained_limit);
        let prepared = fixture.prepare(&workspace, &mut work);
        work.retain_preparation_storage(held_storage).unwrap();
        let checkpoints = prepared_checkpoints(&prepared);
        let source_allocation = (prepared.sources.as_ptr(), prepared.sources.capacity());
        let mut saved = observations(&prepared);
        saved.reverse();
        let saved_allocation = (saved.as_ptr(), saved.capacity());
        let order = saved.iter().map(|saved| saved.document).collect::<Vec<_>>();
        let failure = authority
            .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
            .unwrap_err();
        assert_refused(failure.report());
        let usage = work.usage();
        assert!(usage.work_bytes > preparation_work);
        let (prepared, saved, _) = failure.into_parts();
        assert_eq!(work.usage(), usage);
        assert_eq!(prepared_checkpoints(&prepared), checkpoints);
        assert_eq!(
            (prepared.sources.as_ptr(), prepared.sources.capacity()),
            source_allocation
        );
        assert_eq!((saved.as_ptr(), saved.capacity()), saved_allocation);
        assert_eq!(
            saved.iter().map(|saved| saved.document).collect::<Vec<_>>(),
            order
        );
        assert_eq!(snapshot(&workspace), before);
    }
}
