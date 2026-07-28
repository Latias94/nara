use super::*;

#[test]
fn source_changes_coalesce_duplicate_frame_events() {
    let mut changes = AssetSourceChanges::new();
    let path = AssetPath::new("textures/player.png").unwrap();
    changes.meta_modified(path.clone()).unwrap();
    changes.modified(path.clone()).unwrap();
    changes.removed(path.clone()).unwrap();

    let coalesced = changes.drain_coalesced();

    assert_eq!(
        coalesced,
        vec![AssetSourceChange::new(path, AssetSourceChangeKind::Removed)]
    );
    assert!(changes.is_empty());
}

#[test]
fn source_changes_keep_last_semantic_event_for_atomic_save_sequences() {
    let mut changes = AssetSourceChanges::new();
    let path = AssetPath::new("textures/player.png").unwrap();
    changes.removed(path.clone()).unwrap();
    changes.modified(path.clone()).unwrap();

    let coalesced = changes.drain_coalesced();

    assert_eq!(
        coalesced,
        vec![AssetSourceChange::new(
            path,
            AssetSourceChangeKind::Modified
        )]
    );
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
