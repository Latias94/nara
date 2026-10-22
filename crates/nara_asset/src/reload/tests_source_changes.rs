use super::*;

#[test]
fn source_changes_coalesce_duplicate_frame_events() {
    let mut changes = AssetSourceChanges::new();
    let path = AssetPath::new("textures/player.png").unwrap();
    changes.meta_modified(path.clone()).unwrap();
    changes.modified(path.clone()).unwrap();
    changes.removed(path.clone()).unwrap();

    let coalesced = changes.drain_coalesced();

    assert_eq!(coalesced.len(), 1);
    assert_eq!(coalesced[0].path(), &path);
    assert_eq!(coalesced[0].kind(), AssetSourceChangeKind::Removed);
    assert!(coalesced[0].metadata_dirty());
    assert!(changes.is_empty());
}

#[test]
fn source_changes_keep_last_semantic_event_for_atomic_save_sequences() {
    let mut changes = AssetSourceChanges::new();
    let path = AssetPath::new("textures/player.png").unwrap();
    changes.removed(path.clone()).unwrap();
    changes.modified(path.clone()).unwrap();

    let coalesced = changes.drain_coalesced();

    assert_eq!(coalesced.len(), 1);
    assert_eq!(coalesced[0].path(), &path);
    assert_eq!(coalesced[0].kind(), AssetSourceChangeKind::Modified);
    assert!(!coalesced[0].metadata_dirty());
}

#[test]
fn source_changes_preserve_metadata_dirtiness_across_source_events_and_batches() {
    let path = AssetPath::new("textures/player.png").unwrap();
    for kinds in [
        vec![
            AssetSourceChangeKind::MetaModified,
            AssetSourceChangeKind::Modified,
        ],
        vec![
            AssetSourceChangeKind::Modified,
            AssetSourceChangeKind::MetaModified,
        ],
        vec![
            AssetSourceChangeKind::Removed,
            AssetSourceChangeKind::MetaModified,
            AssetSourceChangeKind::Modified,
        ],
    ] {
        for batch in [false, true] {
            let mut changes = AssetSourceChanges::new();
            for kind in kinds.iter().copied() {
                let change = AssetSourceChange::new(path.clone(), kind);
                if batch {
                    changes.try_extend([change]).unwrap();
                } else {
                    changes.push(change).unwrap();
                }
            }
            let coalesced = changes.drain_coalesced();
            assert_eq!(coalesced.len(), 1);
            assert_eq!(coalesced[0].path(), &path);
            assert_eq!(
                coalesced[0].kind(),
                AssetSourceChangeKind::MetaModified,
                "source and metadata facts must survive {kinds:?}, batch={batch}"
            );
            assert!(coalesced[0].metadata_dirty());
        }
    }
}

#[test]
fn source_changes_metadata_does_not_resurrect_a_removed_source() {
    let path = AssetPath::new("textures/player.png").unwrap();
    for batch in [false, true] {
        let mut changes = AssetSourceChanges::new();
        changes.removed(path.clone()).unwrap();
        let metadata = AssetSourceChange::new(path.clone(), AssetSourceChangeKind::MetaModified);
        if batch {
            changes.try_extend([metadata]).unwrap();
        } else {
            changes.push(metadata).unwrap();
        }
        let coalesced = changes.drain_coalesced();
        assert_eq!(coalesced.len(), 1);
        assert_eq!(coalesced[0].path(), &path);
        assert_eq!(coalesced[0].kind(), AssetSourceChangeKind::Removed);
        assert!(coalesced[0].metadata_dirty());
    }
}

#[test]
fn source_changes_preserve_metadata_removal_order_across_batches_and_roundtrips() {
    let path = AssetPath::new("textures/player.png").unwrap();
    let kinds = [
        AssetSourceChangeKind::Modified,
        AssetSourceChangeKind::MetaModified,
        AssetSourceChangeKind::Removed,
    ];
    for first in kinds {
        for second in kinds {
            for third in kinds {
                let sequence = [first, second, third];
                let last_source = sequence
                    .iter()
                    .rfind(|kind| **kind != AssetSourceChangeKind::MetaModified);
                let metadata = sequence.contains(&AssetSourceChangeKind::MetaModified);
                let last_invalidation = sequence
                    .iter()
                    .rfind(|kind| **kind != AssetSourceChangeKind::Modified);
                let expected_kind = match last_source {
                    Some(AssetSourceChangeKind::Removed) => AssetSourceChangeKind::Removed,
                    _ if metadata => AssetSourceChangeKind::MetaModified,
                    _ => AssetSourceChangeKind::Modified,
                };
                let mut direct = AssetSourceChanges::new();
                for kind in sequence {
                    direct
                        .push(AssetSourceChange::new(path.clone(), kind))
                        .unwrap();
                }
                let expected = direct.drain_coalesced();
                assert_eq!(expected[0].kind(), expected_kind, "{sequence:?}");
                assert_eq!(expected[0].metadata_dirty(), metadata, "{sequence:?}");
                assert_eq!(
                    expected[0].metadata_after_removal(),
                    last_invalidation == Some(&AssetSourceChangeKind::MetaModified),
                    "{sequence:?}"
                );
                let mut batch = AssetSourceChanges::new();
                batch
                    .try_extend(sequence.map(|kind| AssetSourceChange::new(path.clone(), kind)))
                    .unwrap();
                assert_eq!(batch.drain_coalesced(), expected, "one batch: {sequence:?}");
                for split in 1..3 {
                    for use_batch in [false, true] {
                        let mut prefix = AssetSourceChanges::new();
                        prefix
                            .try_extend(
                                sequence[..split]
                                    .iter()
                                    .map(|kind| AssetSourceChange::new(path.clone(), *kind)),
                            )
                            .unwrap();
                        let mut roundtrip = AssetSourceChanges::new();
                        for change in prefix.drain_coalesced() {
                            if use_batch {
                                roundtrip.try_extend([change]).unwrap();
                            } else {
                                roundtrip.push(change).unwrap();
                            }
                        }
                        roundtrip
                            .try_extend(
                                sequence[split..]
                                    .iter()
                                    .map(|kind| AssetSourceChange::new(path.clone(), *kind)),
                            )
                            .unwrap();
                        assert_eq!(
                            roundtrip.drain_coalesced(),
                            expected,
                            "split={split}, batch={use_batch}: {sequence:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn source_changes_are_bounded_and_duplicate_paths_do_not_consume_capacity() {
    let mut changes = AssetSourceChanges::with_limits(AssetSourceChangeLimits::new(
        ItemLimit::ONE,
        ByteLimit::new(4 * 1024).unwrap(),
    ));
    let path = AssetPath::new("textures/player.png").unwrap();
    changes.modified(path.clone()).unwrap();
    let retained = changes.retained_payload_bytes();
    changes.removed(path.clone()).unwrap();

    let error = changes
        .modified(AssetPath::new("textures/other.png").unwrap())
        .unwrap_err();

    assert_eq!(error.kind(), AssetSourceChangeLimitKind::Items);
    assert_eq!(error.limit(), 1);
    assert_eq!(error.attempted(), Some(2));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes.retained_payload_bytes(), retained);
    assert_eq!(
        changes.drain_coalesced(),
        vec![AssetSourceChange::new(path, AssetSourceChangeKind::Removed)]
    );
    assert_eq!(changes.retained_payload_bytes(), 0);
}

#[test]
fn source_changes_bound_atomic_input_before_coalescing() {
    let mut changes = AssetSourceChanges::with_limits(AssetSourceChangeLimits::new(
        ItemLimit::ONE,
        ByteLimit::new(4 * 1024).unwrap(),
    ));
    let path = AssetPath::new("textures/player.png").unwrap();

    let error = changes
        .try_extend([
            AssetSourceChange::new(path.clone(), AssetSourceChangeKind::Modified),
            AssetSourceChange::new(path, AssetSourceChangeKind::Removed),
        ])
        .unwrap_err();

    assert_eq!(error.kind(), AssetSourceChangeLimitKind::Items);
    assert_eq!(error.attempted(), Some(2));
    assert!(changes.is_empty());
    assert_eq!(changes.retained_payload_bytes(), 0);
}

#[test]
fn source_changes_charge_owned_path_capacity_before_retention() {
    let mut changes = AssetSourceChanges::with_limits(AssetSourceChangeLimits::new(
        ItemLimit::new(8).unwrap(),
        ByteLimit::new(64).unwrap(),
    ));
    let mut path = String::with_capacity(4_096);
    path.push_str("textures/player.png");

    let error = changes.modified(AssetPath::new(path).unwrap()).unwrap_err();

    assert_eq!(error.kind(), AssetSourceChangeLimitKind::RetainedBytes);
    assert_eq!(error.limit(), 64);
    assert_eq!(error.attempted(), Some(4_096));
    assert!(changes.is_empty());
    assert_eq!(changes.retained_payload_bytes(), 0);
}

#[test]
fn asset_events_require_a_rescan_after_bounded_retention_overflows() {
    let mut events = AssetEvents::with_limits(AssetEventLimits::new(ItemLimit::ONE));
    let mut states = AssetStates::new();
    for raw_id in 1..=4 {
        states.set_loading(AssetId::from_raw(raw_id));
    }

    assert_eq!(
        events.push(
            AssetId::from_raw(1),
            AssetVersion::ZERO,
            AssetEventKind::Modified
        ),
        AssetEventPushOutcome::Accepted
    );
    assert_eq!(
        events.push(
            AssetId::from_raw(2),
            AssetVersion::ZERO,
            AssetEventKind::ReloadRejected
        ),
        AssetEventPushOutcome::RescanRequired
    );
    assert_eq!(
        events.push(
            AssetId::from_raw(3),
            AssetVersion::ZERO,
            AssetEventKind::ReloadRejected
        ),
        AssetEventPushOutcome::RescanRequired
    );

    assert_eq!(events.len(), 1);
    assert_eq!(events.discarded_events(), 2);
    assert!(events.requires_rescan());
    assert_eq!(events.drain().len(), 1);
    assert_eq!(
        events.push(
            AssetId::from_raw(4),
            AssetVersion::ZERO,
            AssetEventKind::Modified
        ),
        AssetEventPushOutcome::RescanRequired
    );

    let rebuilt = states
        .iter()
        .map(|(id, state)| (id.raw(), state.load_state().clone()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(rebuilt.len(), 4);
    assert!(rebuilt.values().all(|state| state == &LoadState::Loading));

    events.complete_rescan();
    assert_eq!(events.discarded_events(), 0);
    assert!(!events.requires_rescan());
    assert_eq!(
        events.push(
            AssetId::from_raw(5),
            AssetVersion::ZERO,
            AssetEventKind::Modified
        ),
        AssetEventPushOutcome::Accepted
    );
}
