use super::*;
use crate::reload::{
    AssetLoadGeneration, AssetReloadRequest, AssetReloadRequestId, AssetReloadRequestLimits,
    ImageReloadRegistrationError, SourceReloadConsumer, SourceReloadDrainError,
    SourceReloadRegistrationError, register_image_reload_consumer, register_source_reload_consumer,
};
use crate::source_subscriptions::SourceSubscriptionKind;
use crate::{Assets, LoadState};
use nara_app::App;
use nara_core::{ByteLimit, ItemLimit};

struct SourceProduct;
struct OtherProduct;

fn source_kind() -> AssetSourceKind {
    AssetSourceKind::Other("dialogue".to_owned())
}

fn record(id: u8, path: &str, kind: AssetSourceKind) -> AssetRecord {
    AssetRecord::new(
        StableAssetId::parse_str(format!("00000000-0000-0000-0000-{id:012x}")).unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn source_record(id: u8, path: &str) -> AssetRecord {
    record(id, path, source_kind())
}

struct RoutingFixture {
    database: ProjectAssetDatabase,
    dependencies: AssetDependencyGraph,
    server: AssetServer,
    assets: Assets<SourceProduct>,
    states: AssetStates,
    generations: AssetLoadGenerations,
    requests: AssetReloadRequests,
    registrations: SourceReloadRegistrations,
    consumer: SourceReloadConsumer,
    events: AssetEvents,
    diagnostics: AssetReloadDiagnostics,
}

impl RoutingFixture {
    fn new() -> Self {
        let mut app = App::new();
        app.init_resource::<AssetReloadRequests>().unwrap();
        let consumer =
            register_source_reload_consumer::<SourceProduct>(&mut app, source_kind()).unwrap();
        let registrations = app
            .world_mut()
            .unwrap()
            .remove_resource::<SourceReloadRegistrations>()
            .unwrap();
        Self {
            database: ProjectAssetDatabase::default(),
            dependencies: AssetDependencyGraph::default(),
            server: AssetServer::default(),
            assets: Assets::default(),
            states: AssetStates::default(),
            generations: AssetLoadGenerations::default(),
            requests: AssetReloadRequests::default(),
            registrations,
            consumer,
            events: AssetEvents::default(),
            diagnostics: AssetReloadDiagnostics::default(),
        }
    }

    fn resolver(&mut self) -> AssetReloadResolutionContext<'_> {
        AssetReloadResolutionContext {
            database: &self.database,
            dependencies: &self.dependencies,
            asset_server: &mut self.server,
            states: &mut self.states,
            generations: &mut self.generations,
            requests: &mut self.requests,
            registrations: Some(&self.registrations),
            events: &mut self.events,
            diagnostics: &mut self.diagnostics,
        }
    }

    fn subscribe(
        &mut self,
        record: &AssetRecord,
        paths: Vec<AssetSourcePath>,
        kind: SourceSubscriptionKind,
    ) {
        self.resolver()
            .resolve_record(record.clone(), AssetSourceChangeKind::Modified);
        let request = self
            .requests
            .drain_source(&self.consumer)
            .unwrap()
            .remove(0);
        let handle = self
            .server
            .get_handle::<SourceProduct>(request.asset_id())
            .unwrap();
        let prepared = self
            .dependencies
            .prepare_source_subscriptions(&self.consumer, &request, &self.generations, paths, kind)
            .unwrap();
        let baseline = self.assets.mutation_baseline(handle, &self.states).unwrap();
        prepared
            .commit_with(&mut self.dependencies, &self.generations, || match kind {
                SourceSubscriptionKind::Published => baseline.commit_loaded(
                    SourceProduct,
                    &mut self.assets,
                    &mut self.states,
                    &mut self.events,
                    None,
                    None,
                ),
                SourceSubscriptionKind::Retry => baseline.record_failure(
                    &mut self.assets,
                    &mut self.states,
                    &mut self.events,
                    "asset.test.source-build-failed",
                ),
            })
            .unwrap();
        self.events.drain();
    }
}

#[test]
fn source_registration_requires_source_resources_without_installing_routing() {
    let mut app = App::new();
    assert!(matches!(
        register_source_reload_consumer::<SourceProduct>(&mut app, source_kind()),
        Err(SourceReloadRegistrationError::AssetSourcePluginMissing)
    ));
    assert!(!app.world().contains_resource::<SourceReloadRegistrations>());
}

#[test]
fn source_registration_rejects_duplicate_kind_and_type_claims_without_replacing_authority() {
    let mut app = App::new();
    app.init_resource::<AssetReloadRequests>().unwrap();
    let consumer =
        register_source_reload_consumer::<SourceProduct>(&mut app, source_kind()).unwrap();
    let authority = Arc::clone(
        &app.world()
            .resource::<SourceReloadRegistrations>()
            .for_kind(&source_kind())
            .unwrap()
            .authority,
    );

    assert!(matches!(
        register_source_reload_consumer::<SourceProduct>(&mut app, source_kind()),
        Err(SourceReloadRegistrationError::AlreadyRegistered { source_kind: kind, .. })
            if kind == source_kind()
    ));
    assert!(matches!(
        register_source_reload_consumer::<OtherProduct>(&mut app, source_kind()),
        Err(SourceReloadRegistrationError::SourceKindConflict { source_kind: kind, .. })
            if kind == source_kind()
    ));
    let registrations = app.world().resource::<SourceReloadRegistrations>();
    assert!(Arc::ptr_eq(
        &authority,
        &registrations
            .for_kind(consumer.source_kind())
            .unwrap()
            .authority,
    ));
    assert!(registrations.for_kind(&AssetSourceKind::Scene).is_none());
    register_source_reload_consumer::<OtherProduct>(&mut app, AssetSourceKind::Scene).unwrap();
    assert!(app.world().resource::<AssetReloadRequests>().is_empty());
    assert!(!app.world().contains_resource::<AssetLoadGenerations>());
    assert!(!app.world().contains_resource::<AssetStates>());
}

#[test]
fn image_and_source_registration_share_kind_exclusivity_and_allow_a_shared_type() {
    let mut app = App::new();
    app.init_resource::<AssetReloadRequests>().unwrap();
    register_image_reload_consumer::<SourceProduct>(&mut app).unwrap();
    assert!(matches!(
        register_source_reload_consumer::<OtherProduct>(&mut app, AssetSourceKind::Image),
        Err(SourceReloadRegistrationError::SourceKindConflict { .. })
    ));
    register_source_reload_consumer::<SourceProduct>(&mut app, source_kind()).unwrap();
    assert!(
        app.world()
            .resource::<SourceReloadRegistrations>()
            .for_kind(&AssetSourceKind::Image)
            .unwrap()
            .image_recovery
    );

    let mut app = App::new();
    app.init_resource::<AssetReloadRequests>().unwrap();
    register_source_reload_consumer::<SourceProduct>(&mut app, AssetSourceKind::Image).unwrap();
    assert!(matches!(
        register_image_reload_consumer::<SourceProduct>(&mut app),
        Err(ImageReloadRegistrationError::AlreadyRegistered)
    ));
    assert!(
        !app.world()
            .resource::<SourceReloadRegistrations>()
            .for_kind(&AssetSourceKind::Image)
            .unwrap()
            .image_recovery
    );

    let mut app = App::new();
    app.init_resource::<AssetReloadRequests>().unwrap();
    register_source_reload_consumer::<SourceProduct>(&mut app, source_kind()).unwrap();
    register_image_reload_consumer::<SourceProduct>(&mut app).unwrap();
}

#[test]
fn distinct_source_kinds_sharing_a_type_drain_and_publish_independently() {
    let mut fixture = RoutingFixture::new();
    let mut app = App::new();
    app.init_resource::<AssetReloadRequests>().unwrap();
    fixture.consumer =
        register_source_reload_consumer::<SourceProduct>(&mut app, source_kind()).unwrap();
    let scene_consumer =
        register_source_reload_consumer::<SourceProduct>(&mut app, AssetSourceKind::Scene).unwrap();
    fixture.registrations = app
        .world_mut()
        .unwrap()
        .remove_resource::<SourceReloadRegistrations>()
        .unwrap();
    let source = source_record(1, "dialogue/root.json");
    let scene = record(2, "scenes/root.scene", AssetSourceKind::Scene);
    fixture
        .resolver()
        .resolve_record(source.clone(), AssetSourceChangeKind::Modified);
    fixture
        .resolver()
        .resolve_record(scene.clone(), AssetSourceChangeKind::Modified);
    let source_request = fixture
        .requests
        .drain_source(&fixture.consumer)
        .unwrap()
        .remove(0);
    let scene_request = fixture.requests.iter().next().unwrap();
    let source_handle = fixture
        .server
        .get_handle::<SourceProduct>(source_request.asset_id())
        .unwrap();
    let scene_handle = fixture
        .server
        .get_handle::<SourceProduct>(scene_request.asset_id())
        .unwrap();
    assert_ne!(source_handle.id(), scene_handle.id());
    assert!(fixture.consumer.owns_request(&source_request));
    assert!(!scene_consumer.owns_request(&source_request));
    assert!(!fixture.consumer.owns_request(scene_request));
    assert!(scene_consumer.owns_request(scene_request));
    assert!(
        fixture
            .requests
            .drain_source(&fixture.consumer)
            .unwrap()
            .is_empty()
    );
    assert_eq!(fixture.requests.len(), 1);
    assert!(matches!(
        fixture.dependencies.prepare_source_subscriptions(
            &scene_consumer,
            &source_request,
            &fixture.generations,
            vec![source.path().clone()],
            SourceSubscriptionKind::Published,
        ),
        Err(crate::SourceSubscriptionError::AuthorityMismatch)
    ));
    let prepared = fixture
        .dependencies
        .prepare_source_subscriptions(
            &fixture.consumer,
            &source_request,
            &fixture.generations,
            vec![source.path().clone()],
            SourceSubscriptionKind::Published,
        )
        .unwrap();
    let baseline = fixture
        .assets
        .mutation_baseline(source_handle, &fixture.states)
        .unwrap();
    prepared
        .commit_with(&mut fixture.dependencies, &fixture.generations, || {
            baseline.commit_loaded(
                SourceProduct,
                &mut fixture.assets,
                &mut fixture.states,
                &mut fixture.events,
                None,
                None,
            )
        })
        .unwrap();
    assert!(fixture.assets.get(source_handle).is_some());
    assert!(fixture.assets.get(scene_handle).is_none());
    assert_eq!(
        fixture.states.version(source_handle.id()),
        Some(AssetVersion::from_raw(1))
    );
    assert_eq!(
        fixture.states.version(scene_handle.id()),
        Some(AssetVersion::ZERO)
    );
    let scene_request = fixture
        .requests
        .drain_source(&scene_consumer)
        .unwrap()
        .remove(0);
    let prepared = fixture
        .dependencies
        .prepare_source_subscriptions(
            &scene_consumer,
            &scene_request,
            &fixture.generations,
            vec![scene.path().clone()],
            SourceSubscriptionKind::Published,
        )
        .unwrap();
    let baseline = fixture
        .assets
        .mutation_baseline(scene_handle, &fixture.states)
        .unwrap();
    prepared
        .commit_with(&mut fixture.dependencies, &fixture.generations, || {
            baseline.commit_loaded(
                SourceProduct,
                &mut fixture.assets,
                &mut fixture.states,
                &mut fixture.events,
                None,
                None,
            )
        })
        .unwrap();
    assert!(fixture.assets.get(source_handle).is_some());
    assert!(fixture.assets.get(scene_handle).is_some());
    assert_eq!(
        fixture.states.version(source_handle.id()),
        Some(AssetVersion::from_raw(1))
    );
    assert_eq!(
        fixture.states.version(scene_handle.id()),
        Some(AssetVersion::from_raw(1))
    );
    assert_eq!(
        fixture
            .dependencies
            .raw_dependents_for_path(source.path())
            .collect::<Vec<_>>(),
        vec![source.stable_id()]
    );
    assert_eq!(
        fixture
            .dependencies
            .raw_dependents_for_path(scene.path())
            .collect::<Vec<_>>(),
        vec![scene.stable_id()]
    );
    assert!(fixture.requests.is_empty());
    assert_eq!(fixture.requests.retained_payload_bytes(), 0);
    assert!(fixture.diagnostics.is_empty());
}

#[test]
fn admitted_request_retains_its_original_generation_store_before_any_consumer_preparation() {
    let mut fixture = RoutingFixture::new();
    let record = source_record(1, "dialogue/root.json");
    fixture
        .resolver()
        .resolve_record(record.clone(), AssetSourceChangeKind::Modified);
    let old_request = fixture
        .requests
        .drain_source(&fixture.consumer)
        .unwrap()
        .remove(0);
    assert!(fixture.generations.is_current_request(&old_request));
    assert!(fixture.generations.is_current_request(&old_request.clone()));
    let original = std::mem::take(&mut fixture.generations);
    for _ in 0..old_request.generation().raw() {
        fixture.generations.begin_request(old_request.asset_id());
    }
    assert!(
        fixture
            .generations
            .is_current(old_request.asset_id(), old_request.generation())
    );
    assert!(!fixture.generations.is_current_request(&old_request));
    assert!(original.is_current_request(&old_request));
    let fabricated = AssetReloadRequest::new(
        old_request.id(),
        old_request.asset_id(),
        old_request.record(),
        old_request.request_kind(),
        old_request.source_change_kind(),
        old_request.expected_version(),
        old_request.generation(),
        Vec::new(),
    );
    assert!(!original.is_current_request(&fabricated));
    assert!(!fixture.generations.is_current_request(&fabricated));
    fixture
        .resolver()
        .resolve_record(record, AssetSourceChangeKind::Modified);
    let new_request = fixture
        .requests
        .drain_source(&fixture.consumer)
        .unwrap()
        .remove(0);
    assert!(fixture.generations.is_current_request(&new_request));
    assert!(!fixture.generations.is_current_request(&old_request));
    assert!(!original.is_current_request(&new_request));
}

#[test]
fn generic_resolver_claims_the_concrete_type_and_captures_exact_state_before_draining() {
    let mut fixture = RoutingFixture::new();
    let record = source_record(1, "dialogue/root.json");
    let id = fixture.server.reserve_record_id(&record).unwrap();
    fixture
        .states
        .set_loaded_at(id, AssetVersion::from_raw(3), None, None);
    let previous = fixture.states.state_revision(id);
    fixture
        .resolver()
        .resolve_record(record.clone(), AssetSourceChangeKind::Modified);

    assert!(fixture.server.get_handle::<SourceProduct>(id).is_some());
    assert!(matches!(
        fixture.server.claim::<OtherProduct>(id),
        Err(AssetError::AssetTypeMismatch { .. })
    ));
    assert_eq!(fixture.requests.len(), 1);
    let request = fixture.requests.iter().next().unwrap();
    assert!(fixture.consumer.owns_request(request));
    assert_eq!(fixture.consumer.source_kind(), &source_kind());
    assert_eq!(request.record(), record);
    assert_eq!(request.previous_state_revision(), Some(&previous));
    assert_eq!(
        request.previous_usable_version(),
        Some(AssetVersion::from_raw(3))
    );
    assert_eq!(request.expected_version(), AssetVersion::from_raw(3));
    assert_eq!(
        request.resolved_state_revision(),
        Some(&fixture.states.state_revision(id))
    );
    assert_eq!(request.generation(), fixture.generations.current(id));
    assert_eq!(
        fixture.states.state(id).unwrap().load_state(),
        &LoadState::Loading
    );
    assert!(fixture.diagnostics.is_empty());
    assert!(fixture.events.is_empty());

    let drained = fixture.requests.drain_source(&fixture.consumer).unwrap();
    assert_eq!(drained.len(), 1);
    assert!(fixture.consumer.owns_request(&drained[0]));
    assert!(fixture.requests.is_empty());
    assert_eq!(fixture.requests.retained_payload_bytes(), 0);
}

#[test]
fn wrong_concrete_type_rejects_before_identity_state_generation_or_observation_mutation() {
    for kind in [
        AssetSourceChangeKind::Modified,
        AssetSourceChangeKind::MetaModified,
        AssetSourceChangeKind::Removed,
    ] {
        for binding in [0, 1, 2] {
            let mut fixture = RoutingFixture::new();
            let record = source_record(1, "dialogue/root.json");
            let handle = match binding {
                0 => fixture
                    .server
                    .reserve_record::<OtherProduct>(&record)
                    .unwrap(),
                1 => fixture
                    .server
                    .reserve_record::<OtherProduct>(&source_record(1, "dialogue/original.json"))
                    .unwrap(),
                _ => fixture
                    .server
                    .reserve::<OtherProduct>("dialogue/root.json")
                    .unwrap(),
            };
            let id = handle.id();
            fixture
                .states
                .set_loaded_at(id, AssetVersion::from_raw(7), None, None);
            fixture.generations.begin_request(id);
            let server = fixture.server.clone();
            let state = fixture.states.state(id).cloned();
            let revision = fixture.states.state_revision(id);
            let generation = fixture.generations.current(id);

            fixture
                .resolver()
                .resolve_record_observed(record, kind, true, true);

            assert_eq!(fixture.server, server);
            assert_eq!(fixture.states.state(id), state.as_ref());
            assert_eq!(fixture.states.state_revision(id), revision);
            assert_eq!(fixture.generations.current(id), generation);
            assert!(!fixture.states.metadata_change_pending(id));
            assert!(!fixture.states.source_removal_pending(id));
            assert!(fixture.requests.is_empty());
            assert!(fixture.events.is_empty());
            assert_eq!(fixture.diagnostics.iter().count(), 1);
            assert_eq!(
                fixture.diagnostics.iter().next().unwrap().code().as_str(),
                "asset.reload-source-change-resolve-failed"
            );
        }
    }
}

#[test]
fn generic_principal_removal_revokes_loaded_value_when_request_admission_is_full() {
    for limit in [
        AssetReloadRequestLimitKind::Items,
        AssetReloadRequestLimitKind::RetainedBytes,
    ] {
        let mut fixture = RoutingFixture::new();
        let record = source_record(1, "dialogue/root.json");
        fixture.database.insert(record.clone()).unwrap();
        let handle = fixture
            .server
            .reserve_record::<SourceProduct>(&record)
            .unwrap();
        fixture
            .assets
            .commit_loaded(
                handle,
                SourceProduct,
                &mut fixture.states,
                &mut fixture.events,
                None,
                None,
            )
            .unwrap();
        let baseline = fixture
            .assets
            .mutation_baseline(handle, &fixture.states)
            .unwrap();
        let generation = fixture.generations.begin_request(handle.id());
        fixture.events.drain();
        fixture.requests = AssetReloadRequests::with_limits(AssetReloadRequestLimits::new(
            ItemLimit::ONE,
            if limit == AssetReloadRequestLimitKind::Items {
                ByteLimit::new(4096).unwrap()
            } else {
                ByteLimit::ONE
            },
        ));
        if limit == AssetReloadRequestLimitKind::Items {
            fixture.resolver().resolve_record(
                source_record(2, "dialogue/blocker.json"),
                AssetSourceChangeKind::Modified,
            );
        }
        let queued = fixture.requests.iter().cloned().collect::<Vec<_>>();
        fixture
            .resolver()
            .resolve_changes(vec![AssetSourceChange::new(
                record.path().clone(),
                AssetSourceChangeKind::Removed,
            )]);

        assert_eq!(
            fixture.states.state(handle.id()).unwrap().usable_version(),
            None
        );
        assert!(fixture.states.source_removal_pending(handle.id()));
        assert!(!fixture.generations.is_current(handle.id(), generation));
        assert!(baseline.validate(&fixture.assets, &fixture.states).is_err());
        assert!(matches!(
            fixture.states.state(handle.id()).unwrap().load_state(),
            LoadState::Failed { .. }
        ));
        assert_eq!(fixture.requests.iter().cloned().collect::<Vec<_>>(), queued);
        assert!(
            fixture
                .events
                .iter()
                .any(|event| event.kind() == AssetEventKind::ReloadRejected)
        );
        fixture.resolver().resolve_changes(Vec::new());
        assert_eq!(
            fixture.states.state(handle.id()).unwrap().usable_version(),
            None
        );
        assert!(fixture.states.source_removal_pending(handle.id()));
    }
}

#[test]
fn generic_capacity_refusal_preserves_unreserved_identity_and_invalidates_known_attempts() {
    for limit in [
        AssetReloadRequestLimitKind::Items,
        AssetReloadRequestLimitKind::RetainedBytes,
    ] {
        for existing in [false, true] {
            for kind in [
                AssetSourceChangeKind::Modified,
                AssetSourceChangeKind::MetaModified,
                AssetSourceChangeKind::Removed,
            ] {
                let mut fixture = RoutingFixture::new();
                fixture.requests = AssetReloadRequests::with_limits(AssetReloadRequestLimits::new(
                    ItemLimit::ONE,
                    if limit == AssetReloadRequestLimitKind::Items {
                        ByteLimit::new(4096).unwrap()
                    } else {
                        ByteLimit::ONE
                    },
                ));
                if limit == AssetReloadRequestLimitKind::Items {
                    fixture.resolver().resolve_record(
                        source_record(2, "dialogue/blocker.json"),
                        AssetSourceChangeKind::Modified,
                    );
                }
                let record = source_record(1, "dialogue/root.json");
                let id = existing.then(|| {
                    let id = fixture.server.reserve_record_id(&record).unwrap();
                    fixture
                        .states
                        .set_loaded_at(id, AssetVersion::from_raw(5), None, None);
                    fixture.generations.begin_request(id);
                    id
                });
                let server = fixture.server.clone();
                let state = id.and_then(|id| fixture.states.state(id).cloned());
                let revision = id.map(|id| fixture.states.state_revision(id));
                let generation = id.map(|id| fixture.generations.current(id));
                let requests = fixture.requests.iter().cloned().collect::<Vec<_>>();
                let retained_bytes = fixture.requests.retained_payload_bytes();

                fixture
                    .resolver()
                    .resolve_record_observed(record, kind, true, true);

                assert_eq!(fixture.server, server);
                if let Some(id) = id {
                    assert!(matches!(
                        fixture.states.state(id).unwrap().load_state(),
                        LoadState::Failed { .. }
                    ));
                    assert_eq!(
                        fixture.states.version(id),
                        state.as_ref().map(|state| state.version())
                    );
                    assert_ne!(Some(fixture.states.state_revision(id)), revision);
                    assert_ne!(Some(fixture.generations.current(id)), generation);
                    assert!(fixture.states.metadata_change_pending(id));
                    assert_eq!(
                        fixture.states.source_removal_pending(id),
                        kind == AssetSourceChangeKind::Removed
                    );
                }
                assert_eq!(
                    fixture.requests.iter().cloned().collect::<Vec<_>>(),
                    requests
                );
                assert_eq!(fixture.requests.retained_payload_bytes(), retained_bytes);
                assert_eq!(fixture.events.is_empty(), !existing);
                assert_eq!(fixture.diagnostics.iter().count(), 1);
                assert_eq!(
                    fixture.diagnostics.iter().next().unwrap().code().as_str(),
                    limit.diagnostic_code()
                );
            }
        }
    }
}

#[test]
fn source_drain_rejects_foreign_app_authority_atomically_and_preserves_other_kinds() {
    let mut fixture = RoutingFixture::new();
    fixture.resolver().resolve_record(
        source_record(1, "dialogue/root.json"),
        AssetSourceChangeKind::Modified,
    );
    let mut foreign = RoutingFixture::new();
    foreign.resolver().resolve_record(
        source_record(2, "dialogue/foreign.json"),
        AssetSourceChangeKind::Modified,
    );
    let request = fixture.requests.iter().next().unwrap().clone();
    assert!(!foreign.consumer.owns_request(&request));
    let bytes = fixture.requests.retained_payload_bytes();
    assert_eq!(
        fixture.requests.drain_source(&foreign.consumer),
        Err(SourceReloadDrainError::AuthorityMismatch)
    );
    assert_eq!(fixture.requests.len(), 1);
    assert_eq!(fixture.requests.retained_payload_bytes(), bytes);

    let mut other_app = App::new();
    other_app.init_resource::<AssetReloadRequests>().unwrap();
    let other_consumer =
        register_source_reload_consumer::<OtherProduct>(&mut other_app, AssetSourceKind::Scene)
            .unwrap();
    assert!(!other_consumer.owns_request(&request));
    assert!(
        fixture
            .requests
            .drain_source(&other_consumer)
            .unwrap()
            .is_empty()
    );
    assert_eq!(fixture.requests.retained_payload_bytes(), bytes);

    let mut foreign_requests = foreign.requests.drain_source(&foreign.consumer).unwrap();
    let request = foreign_requests.pop().unwrap();
    let authority = Arc::clone(
        &foreign
            .registrations
            .for_kind(&source_kind())
            .unwrap()
            .authority,
    );
    fixture
        .requests
        .try_push_resolved(
            request.asset_id(),
            request.record(),
            request.request_kind(),
            request.source_change_kind(),
            request.expected_version(),
            request.generation(),
            Arc::clone(foreign.generations.store_identity()),
            Vec::new(),
            authority,
            None,
        )
        .unwrap();
    let mixed_bytes = fixture.requests.retained_payload_bytes();
    assert_eq!(
        fixture.requests.drain_source(&fixture.consumer),
        Err(SourceReloadDrainError::AuthorityMismatch)
    );
    assert_eq!(fixture.requests.len(), 2);
    assert_eq!(fixture.requests.retained_payload_bytes(), mixed_bytes);
}

#[test]
fn public_request_construction_cannot_forge_registered_source_authority() {
    let mut fixture = RoutingFixture::new();
    let record = source_record(1, "dialogue/root.json");
    let id = fixture.server.reserve_record_id(&record).unwrap();
    let request = AssetReloadRequest::new(
        AssetReloadRequestId::from_raw(0),
        id,
        record,
        AssetReloadRequestKind::LoadOrReload,
        AssetSourceChangeKind::Modified,
        AssetVersion::ZERO,
        AssetLoadGeneration::ZERO,
        Vec::new(),
    );
    assert!(!fixture.consumer.owns_request(&request));
}

#[test]
fn source_drain_partitions_kinds_and_reports_the_actual_payload_after_handoff() {
    let mut fixture = RoutingFixture::new();
    let mut app = App::new();
    app.init_resource::<AssetReloadRequests>().unwrap();
    fixture.consumer =
        register_source_reload_consumer::<SourceProduct>(&mut app, source_kind()).unwrap();
    let scene_consumer =
        register_source_reload_consumer::<OtherProduct>(&mut app, AssetSourceKind::Scene).unwrap();
    fixture.registrations = app
        .world_mut()
        .unwrap()
        .remove_resource::<SourceReloadRegistrations>()
        .unwrap();
    let mut path = String::with_capacity(4096);
    path.push_str("dialogue/root.json");
    let mut kind = String::with_capacity(1024);
    kind.push_str("dialogue");
    let source = AssetRecord::new(
        StableAssetId::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        AssetSourcePath::new(path).unwrap(),
        AssetSourceKind::Other(kind),
    );
    let scene = record(2, "scenes/root.scene", AssetSourceKind::Scene);
    fixture
        .resolver()
        .resolve_record(source, AssetSourceChangeKind::Modified);
    fixture
        .resolver()
        .resolve_record(scene, AssetSourceChangeKind::Modified);
    assert_eq!(fixture.requests.len(), 2);
    let queue_bytes = fixture.requests.retained_payload_bytes();
    let source_bytes = fixture
        .requests
        .iter()
        .next()
        .unwrap()
        .retained_payload_bytes()
        .unwrap();

    let drained = fixture.requests.drain_source(&fixture.consumer).unwrap();

    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].retained_payload_bytes(), Some(source_bytes));
    assert!(fixture.consumer.owns_request(&drained[0]));
    assert!(!scene_consumer.owns_request(&drained[0]));
    assert_eq!(fixture.requests.len(), 1);
    assert_eq!(
        fixture.requests.retained_payload_bytes(),
        queue_bytes - source_bytes
    );
    assert!(scene_consumer.owns_request(fixture.requests.iter().next().unwrap()));
    assert_eq!(
        fixture
            .requests
            .drain_source(&scene_consumer)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(fixture.requests.retained_payload_bytes(), 0);
    assert_eq!(drained[0].retained_payload_bytes(), Some(source_bytes));
}

#[test]
fn raw_auxiliary_changes_reload_published_and_retry_subscribers_without_source_records() {
    for subscription_kind in [
        SourceSubscriptionKind::Published,
        SourceSubscriptionKind::Retry,
    ] {
        for change_kind in [
            AssetSourceChangeKind::Modified,
            AssetSourceChangeKind::MetaModified,
            AssetSourceChangeKind::Removed,
        ] {
            let mut fixture = RoutingFixture::new();
            let root = source_record(1, "dialogue/root.json");
            let auxiliary = AssetSourcePath::new("dialogue/optional-voice.bin").unwrap();
            let raw = if change_kind == AssetSourceChangeKind::MetaModified {
                AssetSourcePath::new(format!("{}.meta", auxiliary.as_str())).unwrap()
            } else {
                auxiliary.clone()
            };
            fixture.database.insert(root.clone()).unwrap();
            fixture.subscribe(&root, vec![raw], subscription_kind);
            let id = fixture.server.id_for_stable_id(root.stable_id()).unwrap();
            let expected_version = fixture.states.version(id).unwrap();
            let previous = fixture.states.state_revision(id);

            fixture
                .resolver()
                .resolve_changes(vec![AssetSourceChange::new(auxiliary.clone(), change_kind)]);

            assert!(fixture.diagnostics.is_empty());
            assert!(fixture.events.is_empty());
            assert_eq!(fixture.requests.len(), 1);
            let request = fixture.requests.iter().next().unwrap();
            assert_eq!(request.path(), root.path());
            assert_eq!(
                request.source_change_kind(),
                AssetSourceChangeKind::Modified
            );
            assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
            assert_eq!(request.expected_version(), expected_version);
            assert_eq!(request.previous_state_revision(), Some(&previous));
            assert_eq!(
                request.resolved_state_revision(),
                Some(&fixture.states.state_revision(id))
            );
            assert_eq!(request.generation().raw(), 2);
            assert!(fixture.consumer.owns_request(request));
            assert!(fixture.server.id_for_path(&auxiliary).is_none());
            assert!(!fixture.states.metadata_change_pending(id));
            assert!(!fixture.states.source_removal_pending(id));
        }
    }
}

#[test]
fn direct_raw_and_transitive_roots_are_deduplicated_with_principal_observations_preserved() {
    let mut fixture = RoutingFixture::new();
    let principal = source_record(1, "dialogue/a.json");
    let removed = source_record(2, "dialogue/b.json");
    let transitive = source_record(3, "dialogue/c.json");
    for record in [&principal, &removed, &transitive] {
        fixture.database.insert(record.clone()).unwrap();
    }
    let first_auxiliary = AssetSourcePath::new("dialogue/voice.bin").unwrap();
    let second_auxiliary = AssetSourcePath::new("dialogue/speakers.bin").unwrap();
    for record in [&principal, &removed] {
        fixture.subscribe(
            record,
            vec![
                first_auxiliary.clone(),
                AssetSourcePath::new(format!("{}.meta", second_auxiliary.as_str())).unwrap(),
            ],
            SourceSubscriptionKind::Published,
        );
    }
    fixture
        .dependencies
        .add_source_dependency(principal.stable_id(), removed.stable_id());
    fixture
        .dependencies
        .add_source_dependency(removed.stable_id(), transitive.stable_id());
    fixture
        .dependencies
        .add_source_dependency(transitive.stable_id(), principal.stable_id());

    fixture.resolver().resolve_changes(vec![
        AssetSourceChange::new(first_auxiliary, AssetSourceChangeKind::Removed),
        AssetSourceChange::new(second_auxiliary, AssetSourceChangeKind::MetaModified),
        AssetSourceChange::new(
            principal.path().clone(),
            AssetSourceChangeKind::MetaModified,
        ),
        AssetSourceChange::new(removed.path().clone(), AssetSourceChangeKind::Removed),
    ]);

    assert!(fixture.diagnostics.is_empty());
    assert_eq!(fixture.requests.len(), 3);
    let requests = fixture
        .requests
        .iter()
        .map(|request| (request.stable_id(), request))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(requests.len(), 3);
    let principal_request = requests[&principal.stable_id()];
    let removed_request = requests[&removed.stable_id()];
    let transitive_request = requests[&transitive.stable_id()];
    assert_eq!(
        principal_request.source_change_kind(),
        AssetSourceChangeKind::MetaModified
    );
    assert_eq!(principal_request.generation().raw(), 2);
    assert!(
        fixture
            .states
            .metadata_change_pending(principal_request.asset_id())
    );
    assert_eq!(
        removed_request.request_kind(),
        AssetReloadRequestKind::Remove
    );
    assert_eq!(
        removed_request.source_change_kind(),
        AssetSourceChangeKind::Removed
    );
    assert_eq!(removed_request.generation().raw(), 2);
    assert!(
        fixture
            .states
            .source_removal_pending(removed_request.asset_id())
    );
    assert!(
        !fixture
            .states
            .metadata_change_pending(removed_request.asset_id())
    );
    assert_eq!(
        transitive_request.request_kind(),
        AssetReloadRequestKind::LoadOrReload
    );
    assert_eq!(
        transitive_request.source_change_kind(),
        AssetSourceChangeKind::Modified
    );
    assert_eq!(transitive_request.generation().raw(), 1);
    assert!(
        !fixture
            .states
            .metadata_change_pending(transitive_request.asset_id())
    );
    assert!(
        !fixture
            .states
            .source_removal_pending(transitive_request.asset_id())
    );
}

#[test]
fn recorded_auxiliary_removal_removes_only_its_principal_and_reloads_its_subscriber_once() {
    let mut fixture = RoutingFixture::new();
    let auxiliary = source_record(1, "dialogue/voice.json");
    let root = source_record(2, "dialogue/root.json");
    fixture.database.insert(auxiliary.clone()).unwrap();
    fixture.database.insert(root.clone()).unwrap();
    fixture.subscribe(
        &root,
        vec![auxiliary.path().clone()],
        SourceSubscriptionKind::Published,
    );
    fixture
        .dependencies
        .add_source_dependency(auxiliary.stable_id(), root.stable_id());

    fixture
        .resolver()
        .resolve_changes(vec![AssetSourceChange::new(
            auxiliary.path().clone(),
            AssetSourceChangeKind::Removed,
        )]);

    assert!(fixture.diagnostics.is_empty());
    assert_eq!(fixture.requests.len(), 2);
    let principal = fixture
        .requests
        .iter()
        .find(|request| request.stable_id() == auxiliary.stable_id())
        .unwrap();
    let subscriber = fixture
        .requests
        .iter()
        .find(|request| request.stable_id() == root.stable_id())
        .unwrap();
    assert_eq!(principal.request_kind(), AssetReloadRequestKind::Remove);
    assert!(fixture.states.source_removal_pending(principal.asset_id()));
    assert_eq!(
        subscriber.request_kind(),
        AssetReloadRequestKind::LoadOrReload
    );
    assert_eq!(
        subscriber.source_change_kind(),
        AssetSourceChangeKind::Modified
    );
    assert!(!fixture.states.source_removal_pending(subscriber.asset_id()));
    assert_eq!(subscriber.generation().raw(), 2);
}

#[test]
fn subscribed_paths_report_missing_owner_records_and_unsubscribed_paths_keep_real_diagnostics() {
    let mut fixture = RoutingFixture::new();
    let root = source_record(1, "dialogue/root.json");
    let auxiliary = AssetSourcePath::new("dialogue/voice.bin").unwrap();
    let unknown = AssetSourcePath::new("dialogue/unknown.bin").unwrap();
    fixture.database.insert(root.clone()).unwrap();
    fixture.subscribe(
        &root,
        vec![auxiliary.clone()],
        SourceSubscriptionKind::Published,
    );
    assert!(fixture.database.remove_exact(&root));
    let id = fixture.server.id_for_stable_id(root.stable_id()).unwrap();

    fixture.resolver().resolve_changes(vec![
        AssetSourceChange::new(auxiliary.clone(), AssetSourceChangeKind::Removed),
        AssetSourceChange::new(unknown.clone(), AssetSourceChangeKind::Modified),
    ]);

    assert!(fixture.requests.is_empty());
    let codes = fixture
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        vec![
            "asset.reload-record-missing",
            "asset.reload-dependent-record-missing"
        ]
    );
    assert_eq!(fixture.generations.current(id).raw(), 2);
    assert_eq!(fixture.states.state(id).unwrap().usable_version(), None);
    assert_eq!(fixture.events.len(), 1);
    assert_eq!(
        fixture.events.iter().next().unwrap().kind(),
        AssetEventKind::ReloadRejected
    );
    assert!(fixture.server.id_for_path(&auxiliary).is_none());
    assert!(fixture.server.id_for_path(&unknown).is_none());
}

#[test]
fn raw_subscribers_do_not_hide_a_missing_known_principal_record() {
    let mut fixture = RoutingFixture::new();
    let missing = source_record(1, "dialogue/voice.json");
    let root = source_record(2, "dialogue/root.json");
    fixture.database.insert(root.clone()).unwrap();
    fixture.subscribe(
        &root,
        vec![missing.path().clone()],
        SourceSubscriptionKind::Published,
    );
    let id = fixture
        .server
        .reserve_record::<SourceProduct>(&missing)
        .unwrap()
        .id();
    fixture
        .states
        .set_loaded_at(id, AssetVersion::from_raw(1), None, None);

    fixture
        .resolver()
        .resolve_changes(vec![AssetSourceChange::new(
            missing.path().clone(),
            AssetSourceChangeKind::Removed,
        )]);

    assert_eq!(fixture.requests.len(), 1);
    assert_eq!(
        fixture.requests.iter().next().unwrap().stable_id(),
        root.stable_id()
    );
    assert_eq!(fixture.diagnostics.iter().count(), 1);
    assert_eq!(
        fixture.diagnostics.iter().next().unwrap().code().as_str(),
        "asset.reload-record-missing"
    );
    assert_eq!(fixture.states.state(id).unwrap().usable_version(), None);
    assert_eq!(fixture.generations.current(id).raw(), 1);
    assert_eq!(fixture.events.len(), 1);
    assert_eq!(fixture.events.iter().next().unwrap().id(), id);
}
