use super::*;
use crate::{
    AssetMutationBaseline, AssetReloadRequestLimits, AssetStateError, Assets, Handle,
    ImageRecoveryContinuity, ImageReloadConsumer, LoadState, register_image_reload_consumer,
};
use nara_app::App;
use nara_core::{ByteLimit, ItemLimit};

struct RecoveryFixture {
    record: AssetRecord,
    handle: Handle<String>,
    database: ProjectAssetDatabase,
    dependencies: AssetDependencyGraph,
    server: AssetServer,
    assets: Assets<String>,
    states: AssetStates,
    generations: AssetLoadGenerations,
    requests: AssetReloadRequests,
    registration: ImageReloadRegistration,
    consumer: ImageReloadConsumer,
    events: AssetEvents,
    diagnostics: AssetReloadDiagnostics,
}

impl RecoveryFixture {
    fn new() -> Self {
        let mut app = App::new();
        app.init_resource::<AssetReloadRequests>().unwrap();
        let consumer = register_image_reload_consumer::<String>(&mut app).unwrap();
        let registration = app
            .world_mut()
            .unwrap()
            .remove_resource::<ImageReloadRegistration>()
            .unwrap();
        let record = AssetRecord::new(
            StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
            AssetPath::new("textures/recovery.png").unwrap(),
            AssetSourceKind::Image,
        );
        let mut database = ProjectAssetDatabase::default();
        database.insert(record.clone()).unwrap();
        let mut server = AssetServer::default();
        let handle = server.reserve_record::<String>(&record).unwrap();
        let mut assets = Assets::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        assets
            .commit_loaded(
                handle,
                "deleted pixels".to_owned(),
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap();
        assert_eq!(
            assets
                .remove_with_state(handle, &mut states, &mut events)
                .unwrap()
                .as_deref(),
            Some("deleted pixels")
        );
        events.drain();
        states.bind_image_reload_authority(&registration.authority);
        Self {
            record,
            handle,
            database,
            dependencies: AssetDependencyGraph::default(),
            server,
            assets,
            states,
            generations: AssetLoadGenerations::default(),
            requests: AssetReloadRequests::default(),
            registration,
            consumer,
            events,
            diagnostics: AssetReloadDiagnostics::default(),
        }
    }

    fn continuity(&self) -> ImageRecoveryContinuity {
        self.consumer
            .recovery_continuity(&self.states, self.handle.id())
    }

    fn baseline(&self) -> AssetMutationBaseline<String> {
        self.assets
            .mutation_baseline(self.handle, &self.states)
            .unwrap()
    }

    fn resolver(&mut self) -> AssetReloadResolutionContext<'_> {
        AssetReloadResolutionContext {
            database: &self.database,
            dependencies: &self.dependencies,
            asset_server: &mut self.server,
            states: &mut self.states,
            generations: &mut self.generations,
            requests: &mut self.requests,
            image_registration: Some(&self.registration),
            events: &mut self.events,
            diagnostics: &mut self.diagnostics,
        }
    }

    fn resolve(&mut self, kind: AssetSourceChangeKind) {
        let record = self.record.clone();
        self.resolver().resolve_record(record, kind);
    }

    fn reject_capacity(&mut self, kind: AssetReloadRequestLimitKind) {
        match kind {
            AssetReloadRequestLimitKind::Items => {
                self.requests = AssetReloadRequests::with_limits(AssetReloadRequestLimits::new(
                    ItemLimit::new(1).unwrap(),
                    ByteLimit::new(1_048_576).unwrap(),
                ));
                let blocker = AssetRecord::new(
                    StableAssetId::parse_str("b73f0f16-09e8-4265-b090-b689b41c197e").unwrap(),
                    AssetPath::new("textures/blocker.png").unwrap(),
                    AssetSourceKind::Image,
                );
                self.resolver()
                    .resolve_record(blocker, AssetSourceChangeKind::Modified);
                assert_eq!(self.requests.len(), 1);
            }
            AssetReloadRequestLimitKind::RetainedBytes => {
                self.requests = AssetReloadRequests::with_limits(AssetReloadRequestLimits::new(
                    ItemLimit::new(8).unwrap(),
                    ByteLimit::new(1).unwrap(),
                ));
            }
        }
    }
}

#[test]
fn recovery_continuity_is_bound_to_one_asset_and_state_store() {
    let mut fixture = RecoveryFixture::new();
    let id = fixture.handle.id();
    let token = fixture.continuity();
    assert_eq!(token.clone(), fixture.continuity());
    assert!(token.is_current(&fixture.states, id));

    let other = fixture
        .server
        .reserve::<String>("textures/other.png")
        .unwrap();
    assert!(!token.is_current(&fixture.states, other.id()));
    assert_ne!(
        token,
        fixture
            .consumer
            .recovery_continuity(&fixture.states, other.id())
    );
    assert!(!token.is_current(&AssetStates::default(), id));

    fixture.states.set_loading(other.id());
    assert!(token.is_current(&fixture.states, id));
}

#[test]
fn a_foreign_reload_consumer_cannot_issue_recovery_authority_for_this_state_store() {
    let mut fixture = RecoveryFixture::new();
    let mut foreign_app = App::new();
    foreign_app.init_resource::<AssetReloadRequests>().unwrap();
    let foreign = register_image_reload_consumer::<String>(&mut foreign_app).unwrap();
    let token = foreign.recovery_continuity(&fixture.states, fixture.handle.id());
    let own = fixture.continuity();
    let previous = fixture.states.state_revision(fixture.handle.id());
    assert_ne!(own, token);
    assert!(!token.is_current(&fixture.states, fixture.handle.id()));
    assert!(
        token
            .record_failure(
                fixture.baseline(),
                &mut fixture.assets,
                &mut fixture.states,
                &mut fixture.events,
                "foreign writer"
            )
            .is_err()
    );
    assert_eq!(previous, fixture.states.state_revision(fixture.handle.id()));
    assert!(own.is_current(&fixture.states, fixture.handle.id()));
    assert!(fixture.events.is_empty());
}

#[test]
fn repeated_capacity_rejection_preserves_recovery_but_requires_a_fresh_attempt_baseline() {
    for kind in [
        AssetReloadRequestLimitKind::Items,
        AssetReloadRequestLimitKind::RetainedBytes,
    ] {
        let mut fixture = RecoveryFixture::new();
        fixture.reject_capacity(kind);
        let id = fixture.handle.id();
        let token = fixture.continuity();
        let baseline = fixture.baseline();
        let version = fixture.states.version(id).unwrap();
        let slot = fixture.assets.slot_revision(fixture.handle);

        for expected_generation in 1..=3 {
            let previous = fixture.states.state_revision(id);
            fixture.resolve(AssetSourceChangeKind::Modified);

            assert!(token.is_current(&fixture.states, id), "{kind:?}");
            assert_ne!(fixture.states.state_revision(id), previous);
            assert_eq!(fixture.states.version(id), Some(version));
            assert_eq!(fixture.states.state(id).unwrap().usable_version(), None);
            assert_eq!(fixture.assets.slot_revision(fixture.handle), slot);
            assert_eq!(fixture.assets.get(fixture.handle), None);
            assert_eq!(fixture.generations.current(id).raw(), expected_generation);
            assert!(matches!(
                fixture.states.state(id).unwrap().load_state(),
                LoadState::Failed { message } if message == kind.diagnostic_code()
            ));
            assert!(matches!(
                baseline.validate(&fixture.assets, &fixture.states),
                Err(AssetStateError::MutationSuperseded { .. })
            ));
        }
        assert_eq!(fixture.diagnostics.iter().count(), 3);
        assert!(
            fixture
                .diagnostics
                .iter()
                .all(|diagnostic| { diagnostic.code().as_str() == kind.diagnostic_code() })
        );
        assert_eq!(fixture.events.len(), 3);
        assert!(fixture.events.iter().all(|event| {
            event.id() == id
                && event.version() == version
                && event.kind() == AssetEventKind::ReloadRejected
        }));

        let before = fixture.states.state_revision(id);
        fixture.requests = AssetReloadRequests::default();
        fixture.resolve(AssetSourceChangeKind::Modified);
        assert!(token.is_current(&fixture.states, id));
        assert_ne!(fixture.states.state_revision(id), before);
        assert_eq!(fixture.generations.current(id).raw(), 4);
        let request = fixture.requests.iter().next().unwrap();
        assert_eq!(request.previous_state_revision(), Some(&before));
        assert_eq!(
            request.resolved_state_revision(),
            Some(&fixture.states.state_revision(id))
        );
        assert_eq!(request.expected_version(), version);
        assert_eq!(
            fixture.states.state(id).unwrap().load_state(),
            &LoadState::Loading
        );
        assert!(
            fixture
                .baseline()
                .validate(&fixture.assets, &fixture.states)
                .is_ok()
        );
    }
}

#[test]
fn rejected_successor_supersedes_loading_without_revoking_recovery() {
    for kind in [
        AssetReloadRequestLimitKind::Items,
        AssetReloadRequestLimitKind::RetainedBytes,
    ] {
        let mut fixture = RecoveryFixture::new();
        let id = fixture.handle.id();
        let token = fixture.continuity();
        fixture.resolve(AssetSourceChangeKind::Modified);
        let first = fixture
            .requests
            .drain_images(&fixture.consumer)
            .unwrap()
            .remove(0);
        let baseline = fixture.baseline();
        assert!(token.is_current(&fixture.states, id));

        fixture.reject_capacity(kind);
        fixture.resolve(AssetSourceChangeKind::Modified);

        assert!(!fixture.generations.is_current(id, first.generation()));
        assert!(token.is_current(&fixture.states, id));
        assert!(matches!(
            fixture.states.state(id).unwrap().load_state(),
            LoadState::Failed { message } if message == kind.diagnostic_code()
        ));
        let before = fixture.states.state_revision(id);
        let event_count = fixture.events.len();
        assert!(matches!(
            token.record_failure(
                baseline,
                &mut fixture.assets,
                &mut fixture.states,
                &mut fixture.events,
                "late attempt",
            ),
            Err(AssetStateError::MutationSuperseded { .. })
        ));
        assert_eq!(fixture.states.state_revision(id), before);
        assert_eq!(fixture.events.len(), event_count);
        assert!(token.is_current(&fixture.states, id));
    }
}

#[derive(Debug, Clone, Copy)]
enum OrdinaryWrite {
    Loading,
    LoadFailure,
    ReloadFailure,
    RequestRejection,
    Loaded,
    Removed,
    MetadataObservation,
    SourceRemoval,
    PublicationInvalidation,
    MetadataAcknowledgement,
}

impl OrdinaryWrite {
    fn apply(self, fixture: &mut RecoveryFixture) {
        let id = fixture.handle.id();
        match self {
            Self::Loading => {
                fixture.states.set_loading(id);
            }
            Self::LoadFailure => {
                fixture
                    .assets
                    .record_load_failure(
                        fixture.handle,
                        &mut fixture.states,
                        &mut fixture.events,
                        "asset.reload-request-item-limit-exceeded",
                    )
                    .unwrap();
            }
            Self::ReloadFailure => {
                fixture
                    .assets
                    .record_reload_failure(
                        fixture.handle,
                        &mut fixture.states,
                        &mut fixture.events,
                        "asset.reload-request-byte-limit-exceeded",
                    )
                    .unwrap();
            }
            Self::RequestRejection => {
                fixture
                    .states
                    .reject_reload_request(id, "asset.reload-request-item-limit-exceeded");
            }
            Self::Loaded => {
                fixture
                    .assets
                    .commit_loaded(
                        fixture.handle,
                        "new pixels".to_owned(),
                        &mut fixture.states,
                        &mut fixture.events,
                        None,
                        None,
                    )
                    .unwrap();
            }
            Self::Removed => {
                fixture
                    .assets
                    .remove_with_state(fixture.handle, &mut fixture.states, &mut fixture.events)
                    .unwrap();
            }
            Self::MetadataObservation => fixture.states.observe_metadata_change(id),
            Self::SourceRemoval => fixture.states.observe_source_removal(id),
            Self::PublicationInvalidation => fixture.states.invalidate_publication(id),
            Self::MetadataAcknowledgement => fixture.states.acknowledge_metadata_publication(id),
        }
    }
}

#[test]
fn permitted_resolver_transitions_never_bless_an_earlier_external_write() {
    for write in [
        OrdinaryWrite::Loading,
        OrdinaryWrite::LoadFailure,
        OrdinaryWrite::ReloadFailure,
        OrdinaryWrite::RequestRejection,
        OrdinaryWrite::Loaded,
        OrdinaryWrite::Removed,
        OrdinaryWrite::MetadataObservation,
        OrdinaryWrite::SourceRemoval,
        OrdinaryWrite::PublicationInvalidation,
        OrdinaryWrite::MetadataAcknowledgement,
    ] {
        for capacity in [
            None,
            Some(AssetReloadRequestLimitKind::Items),
            Some(AssetReloadRequestLimitKind::RetainedBytes),
        ] {
            let mut fixture = RecoveryFixture::new();
            if let Some(kind) = capacity {
                fixture.reject_capacity(kind);
            }
            let id = fixture.handle.id();
            if matches!(write, OrdinaryWrite::MetadataAcknowledgement) {
                fixture.states.observe_metadata_change(id);
            }
            let old = fixture.continuity();
            write.apply(&mut fixture);
            assert!(!old.is_current(&fixture.states, id), "{write:?}");
            let current = fixture.continuity();
            fixture.resolve(AssetSourceChangeKind::Modified);
            assert!(
                !old.is_current(&fixture.states, id),
                "{write:?}, {capacity:?}"
            );
            assert!(
                current.is_current(&fixture.states, id),
                "{write:?}, {capacity:?}"
            );
        }
    }
}

#[test]
fn validated_image_failure_carries_recovery_and_retires_the_exact_baseline() {
    for last_good in [false, true] {
        let mut fixture = RecoveryFixture::new();
        let id = fixture.handle.id();
        if last_good {
            fixture
                .assets
                .commit_loaded(
                    fixture.handle,
                    "last good".to_owned(),
                    &mut fixture.states,
                    &mut fixture.events,
                    None,
                    None,
                )
                .unwrap();
            fixture.events.drain();
        }
        let token = fixture.continuity();
        let version = fixture.states.version(id).unwrap();
        for _ in 0..2 {
            fixture.resolve(AssetSourceChangeKind::Modified);
            fixture.requests.drain_images(&fixture.consumer).unwrap();
            let baseline = fixture.baseline();
            let previous = fixture.states.state_revision(id);
            assert_eq!(
                token.record_failure(
                    baseline.clone(),
                    &mut fixture.assets,
                    &mut fixture.states,
                    &mut fixture.events,
                    "decode failed",
                ),
                Ok(version)
            );
            assert!(token.is_current(&fixture.states, id));
            assert_ne!(fixture.states.state_revision(id), previous);
            assert!(matches!(
                baseline.validate(&fixture.assets, &fixture.states),
                Err(AssetStateError::MutationSuperseded { .. })
            ));
            assert_eq!(
                fixture.assets.get(fixture.handle).map(String::as_str),
                last_good.then_some("last good")
            );
            assert_eq!(
                fixture.states.state(id).unwrap().usable_version(),
                last_good.then_some(version)
            );
            let event_kind = if last_good {
                AssetEventKind::ReloadFailed
            } else {
                AssetEventKind::LoadFailed
            };
            assert_eq!(
                fixture.events.drain(),
                vec![crate::AssetEvent::new(id, version, event_kind)]
            );
        }
    }
}

#[test]
fn unsuccessful_failure_recording_does_not_restore_a_saved_continuity() {
    let mut fixture = RecoveryFixture::new();
    let fresh = fixture
        .server
        .reserve::<String>("textures/fresh.png")
        .unwrap();
    let token = fixture
        .consumer
        .recovery_continuity(&fixture.states, fresh.id());
    let baseline = fixture
        .assets
        .mutation_baseline(fresh, &fixture.states)
        .unwrap();
    assert_eq!(
        token.record_failure(
            baseline,
            &mut fixture.assets,
            &mut fixture.states,
            &mut fixture.events,
            "not yet loading",
        ),
        Err(AssetStateError::UnknownAsset { id: fresh.id() })
    );
    assert!(!token.is_current(&fixture.states, fresh.id()));
    assert!(fixture.states.state(fresh.id()).is_none());
    assert!(fixture.events.is_empty());
}

#[test]
fn recovery_failure_rejects_wrong_asset_and_external_state_write_without_effects() {
    let mut fixture = RecoveryFixture::new();
    let id = fixture.handle.id();
    let token = fixture.continuity();
    let other = fixture
        .server
        .reserve::<String>("textures/other.png")
        .unwrap();
    fixture.states.set_loading(other.id());
    let wrong_baseline = fixture
        .assets
        .mutation_baseline(other, &fixture.states)
        .unwrap();
    let before = fixture.states.state_revision(other.id());
    assert_eq!(
        token.record_failure(
            wrong_baseline,
            &mut fixture.assets,
            &mut fixture.states,
            &mut fixture.events,
            "wrong asset",
        ),
        Err(AssetStateError::MutationSuperseded { id: other.id() })
    );
    assert_eq!(fixture.states.state_revision(other.id()), before);
    assert!(fixture.events.is_empty());
    assert!(token.is_current(&fixture.states, id));

    fixture.states.set_loading(id);
    let baseline = fixture.baseline();
    assert!(baseline.validate(&fixture.assets, &fixture.states).is_ok());
    let before = fixture.states.state_revision(id);
    assert_eq!(
        token.record_failure(
            baseline,
            &mut fixture.assets,
            &mut fixture.states,
            &mut fixture.events,
            "external write",
        ),
        Err(AssetStateError::MutationSuperseded { id })
    );
    assert_eq!(fixture.states.state_revision(id), before);
    assert!(fixture.events.is_empty());
    assert!(!token.is_current(&fixture.states, id));
}

#[test]
fn recovery_failure_keeps_exact_empty_slot_and_replaced_store_guards() {
    for replace_values in [false, true] {
        let mut fixture = RecoveryFixture::new();
        let id = fixture.handle.id();
        let token = fixture.continuity();
        let baseline = fixture.baseline();
        if replace_values {
            fixture.assets = Assets::default();
        } else {
            fixture
                .assets
                .insert(fixture.handle, "raw pixels".to_owned());
            fixture.assets.remove(fixture.handle);
        }
        let before = fixture.states.state_revision(id);
        assert_eq!(
            token.record_failure(
                baseline,
                &mut fixture.assets,
                &mut fixture.states,
                &mut fixture.events,
                "slot interference",
            ),
            Err(AssetStateError::SlotSuperseded { id })
        );
        assert_eq!(fixture.states.state_revision(id), before);
        assert!(fixture.events.is_empty());
        assert!(
            fixture
                .assets
                .mutation_baseline(fixture.handle, &fixture.states)
                .is_err()
        );
    }

    let mut fixture = RecoveryFixture::new();
    let token = fixture.continuity();
    let baseline = fixture.baseline();
    fixture.states = AssetStates::default();
    assert_eq!(
        token.record_failure(
            baseline,
            &mut fixture.assets,
            &mut fixture.states,
            &mut fixture.events,
            "replaced states",
        ),
        Err(AssetStateError::MutationSuperseded {
            id: fixture.handle.id()
        })
    );
    assert!(fixture.states.is_empty());
    assert!(fixture.events.is_empty());
}

#[test]
fn metadata_acknowledgement_only_revokes_when_it_clears_an_observation() {
    let mut fixture = RecoveryFixture::new();
    let id = fixture.handle.id();
    let token = fixture.continuity();
    fixture.states.acknowledge_metadata_publication(id);
    assert!(token.is_current(&fixture.states, id));
    fixture.states.observe_metadata_change(id);
    let dirty = fixture.continuity();
    let baseline = fixture.baseline();
    fixture.states.acknowledge_metadata_publication(id);
    assert!(!dirty.is_current(&fixture.states, id));
    assert!(matches!(
        baseline.validate(&fixture.assets, &fixture.states),
        Err(AssetStateError::MutationSuperseded { .. })
    ));
    let clean = fixture.continuity();
    fixture.states.acknowledge_metadata_publication(id);
    assert!(clean.is_current(&fixture.states, id));
}

#[test]
fn unavailable_and_absent_publication_invalidation_always_revoke_recovery() {
    let mut fixture = RecoveryFixture::new();
    let id = fixture.handle.id();
    for _ in 0..2 {
        let token = fixture.continuity();
        fixture.states.invalidate_publication(id);
        assert!(!token.is_current(&fixture.states, id));
    }
    let absent = fixture
        .server
        .reserve::<String>("textures/absent.png")
        .unwrap()
        .id();
    let token = fixture
        .consumer
        .recovery_continuity(&fixture.states, absent);
    fixture.states.invalidate_publication(absent);
    assert!(!token.is_current(&fixture.states, absent));
    assert!(fixture.states.state(absent).is_none());
}

#[test]
fn metadata_and_missing_consumer_are_not_capacity_continuations() {
    for kind in [AssetSourceChangeKind::MetaModified] {
        let mut fixture = RecoveryFixture::new();
        fixture.reject_capacity(AssetReloadRequestLimitKind::RetainedBytes);
        let token = fixture.continuity();
        fixture.resolve(kind);
        assert!(!token.is_current(&fixture.states, fixture.handle.id()));
    }

    for metadata_after_removal in [false, true] {
        let mut fixture = RecoveryFixture::new();
        fixture.reject_capacity(AssetReloadRequestLimitKind::RetainedBytes);
        let token = fixture.continuity();
        let record = fixture.record.clone();
        fixture.resolver().resolve_record_observed(
            record,
            AssetSourceChangeKind::Removed,
            true,
            metadata_after_removal,
        );
        let id = fixture.handle.id();
        assert!(!token.is_current(&fixture.states, id));
        assert_eq!(
            fixture.states.metadata_change_after_source_removal(id),
            metadata_after_removal
        );
    }

    let mut fixture = RecoveryFixture::new();
    let token = fixture.continuity();
    let record = fixture.record.clone();
    let mut resolver = fixture.resolver();
    resolver.image_registration = None;
    resolver.resolve_record(record, AssetSourceChangeKind::Modified);
    assert!(!token.is_current(&fixture.states, fixture.handle.id()));
    assert_eq!(
        fixture.events.drain()[0].kind(),
        AssetEventKind::ReloadRejected
    );
}

#[test]
fn resolver_removal_preserves_only_the_current_continuity_for_image_owner_readmission() {
    for external_write in [false, true] {
        let mut fixture = RecoveryFixture::new();
        let id = fixture.handle.id();
        let token = fixture.continuity();
        if external_write {
            fixture.states.set_loading(id);
        }
        let current = fixture.continuity();
        fixture.resolve(AssetSourceChangeKind::Removed);
        assert_eq!(token.is_current(&fixture.states, id), !external_write);
        assert!(current.is_current(&fixture.states, id));
        assert!(fixture.states.source_removal_pending(id));
        assert_eq!(fixture.states.state(id).unwrap().usable_version(), None);
        // Continuity alone does not grant recovery: Image must still own its exact empty slot.
    }
}
