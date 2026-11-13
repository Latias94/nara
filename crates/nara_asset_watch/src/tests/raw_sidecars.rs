use super::*;
use nara_asset::{
    Asset, AssetDependencyGraph, AssetEvents, AssetLoadGenerations, AssetReloadDiagnostics,
    AssetServer, AssetStates, Assets, SourceSubscriptionKind, register_source_reload_consumer,
};
use nara_ecs::Res;
use std::panic::resume_unwind;

#[derive(Default)]
struct RawSourceProduct;

#[derive(Default)]
struct RawMetadataProduct;

fn install_raw_consumer<T: Asset + Default>(
    app: &mut App,
    kind: &str,
    paths: Vec<AssetSourcePath>,
    subscription_kind: SourceSubscriptionKind,
) -> Arc<Mutex<Vec<AssetReloadRequest>>> {
    let consumer =
        register_source_reload_consumer::<T>(app, AssetSourceKind::Other(kind.to_owned())).unwrap();
    app.init_resource::<Assets<T>>().unwrap();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::clone(&captured);
    app.add_systems(
        CoreStage::TaskUpdate,
        (move |server: Res<AssetServer>,
               generations: Res<AssetLoadGenerations>,
               mut requests: ResMut<AssetReloadRequests>,
               mut dependencies: ResMut<AssetDependencyGraph>,
               mut assets: ResMut<Assets<T>>,
               mut states: ResMut<AssetStates>,
               mut events: ResMut<AssetEvents>| {
            for request in requests.drain_source(&consumer).unwrap() {
                let handle = server.get_handle::<T>(request.asset_id()).unwrap();
                let prepared = dependencies
                    .prepare_source_subscriptions(
                        &consumer,
                        &request,
                        &generations,
                        paths.clone(),
                        subscription_kind,
                    )
                    .unwrap();
                let baseline = assets.mutation_baseline(handle, &states).unwrap();
                prepared
                    .commit_with(
                        &mut dependencies,
                        &generations,
                        || match subscription_kind {
                            SourceSubscriptionKind::Published => baseline.commit_loaded(
                                T::default(),
                                &mut assets,
                                &mut states,
                                &mut events,
                                None,
                                None,
                            ),
                            SourceSubscriptionKind::Retry => baseline.record_failure(
                                &mut assets,
                                &mut states,
                                &mut events,
                                "asset.test.raw-input-build-failed",
                            ),
                        },
                    )
                    .unwrap();
                output.lock().unwrap().push(request);
            }
        })
        .in_set(AssetTaskUpdateSet::SpawnJobs),
    )
    .unwrap();
    captured
}

fn raw_record(id: &str, path: &str, kind: &str) -> AssetRecord {
    AssetRecord::new(
        StableAssetId::parse_str(id).unwrap(),
        AssetSourcePath::new(path).unwrap(),
        AssetSourceKind::Other(kind.to_owned()),
    )
}

fn check_exact_raw_sidecar_routing(
    subscription_kind: SourceSubscriptionKind,
    image_principal: bool,
) {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let source = root.join("player.png");
    let metadata = root.join("player.png.meta");
    fs::write(&source, b"initial source").unwrap();
    fs::write(&metadata, b"initial metadata").unwrap();
    let bound = AssetSourceRoot::new(
        &root,
        AssetPathScope::under(AssetSourcePath::new("content/selected").unwrap()),
    );
    let raw_source = AssetSourcePath::new("content/selected/player.png").unwrap();
    let raw_metadata = AssetSourcePath::new("content/selected/player.png.meta").unwrap();
    let source_owner = raw_record(
        "00000000-0000-0000-0000-000000000001",
        "content/selected/source-owner.json",
        "raw-source-only",
    );
    let metadata_owner = raw_record(
        "00000000-0000-0000-0000-000000000002",
        "content/selected/metadata-owner.json",
        "raw-metadata-only",
    );
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.insert_resource(bound).unwrap();
    let source_calls = install_raw_consumer::<RawSourceProduct>(
        &mut app,
        "raw-source-only",
        vec![raw_source.clone()],
        subscription_kind,
    );
    let metadata_calls = install_raw_consumer::<RawMetadataProduct>(
        &mut app,
        "raw-metadata-only",
        vec![raw_metadata.clone()],
        subscription_kind,
    );
    for record in [&source_owner, &metadata_owner] {
        app.world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>()
            .insert(record.clone())
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<AssetSourceChanges>()
            .modified(record.path().clone())
            .unwrap();
    }
    if image_principal {
        app.world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>()
            .insert(AssetRecord::new(
                stable_id(),
                raw_source.clone(),
                AssetSourceKind::Image,
            ))
            .unwrap();
        install_reload_capture(&mut app);
    }
    let queue = AssetWatchEventQueue::new();
    let sender = queue.sender();
    install_watch_runtime(&mut app, queue);
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(source_calls.lock().unwrap().len(), 1);
        assert_eq!(metadata_calls.lock().unwrap().len(), 1);
        source_calls.lock().unwrap().clear();
        metadata_calls.lock().unwrap().clear();
        for (record, path) in [
            (&source_owner, &raw_source),
            (&metadata_owner, &raw_metadata),
        ] {
            assert_eq!(
                runtime
                    .world()
                    .resource::<AssetDependencyGraph>()
                    .source_subscription_paths(record.stable_id(), subscription_kind),
                std::slice::from_ref(path)
            );
        }

        let mut image_observations = 0;
        for (file, owner, calls, unrelated) in [
            (&metadata, &metadata_owner, &metadata_calls, &source_calls),
            (&source, &source_owner, &source_calls, &metadata_calls),
        ] {
            for (generation, removed) in (2_u64..).zip([false, true, false]) {
                let event = if removed {
                    fs::remove_file(file).unwrap();
                    AssetWatchEvent::removed(file)
                } else {
                    fs::write(file, b"modified or recreated raw input").unwrap();
                    AssetWatchEvent::modified(file)
                };
                sender.try_send(event).unwrap();
                runtime.drive(Duration::ZERO).unwrap();

                assert!(
                    unrelated.lock().unwrap().is_empty(),
                    "unrelated consumer ran for {} ({subscription_kind:?}, removed={removed})",
                    file.display()
                );
                let captured = std::mem::take(&mut *calls.lock().unwrap());
                assert_eq!(
                    captured.len(),
                    1,
                    "exact raw subscriber missed {} ({subscription_kind:?}, removed={removed})",
                    file.display()
                );
                let request = &captured[0];
                assert_eq!(request.stable_id(), owner.stable_id());
                assert_eq!(request.path(), owner.path());
                assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
                assert_eq!(
                    request.source_change_kind(),
                    AssetSourceChangeKind::Modified
                );
                assert_eq!(request.generation().raw(), generation);
                let states = runtime.world().resource::<AssetStates>();
                assert!(!states.metadata_change_pending(request.asset_id()));
                assert!(!states.source_removal_pending(request.asset_id()));
                assert!(
                    runtime
                        .world()
                        .resource::<AssetReloadDiagnostics>()
                        .is_empty()
                );
                assert!(runtime.world().resource::<AssetReloadRequests>().is_empty());
                if image_principal {
                    image_observations += 1;
                    let image_requests = runtime.world().resource::<WatchCapturedReloadRequests>();
                    assert_eq!(image_requests.0.len(), image_observations);
                    let image_request = image_requests.0.last().unwrap();
                    assert_eq!(image_request.path(), &raw_source);
                    assert_eq!(
                        image_request.source_change_kind(),
                        if removed {
                            AssetSourceChangeKind::Removed
                        } else if file == &metadata {
                            AssetSourceChangeKind::MetaModified
                        } else {
                            AssetSourceChangeKind::Modified
                        }
                    );
                    assert_eq!(
                        image_request.request_kind(),
                        if removed {
                            AssetReloadRequestKind::Remove
                        } else {
                            AssetReloadRequestKind::LoadOrReload
                        }
                    );
                    if removed {
                        assert!(states.source_removal_pending(image_request.asset_id()));
                        assert!(
                            !states.metadata_change_after_source_removal(image_request.asset_id())
                        );
                    } else if file == &metadata {
                        assert!(states.metadata_change_pending(image_request.asset_id()));
                        assert_eq!(
                            states.metadata_change_after_source_removal(image_request.asset_id()),
                            image_observations == 3
                        );
                    }
                }
                assert!(
                    !runtime
                        .world()
                        .resource::<AssetWatchRuntimeStatus>()
                        .requires_rescan()
                );
            }
        }
        for (generation, reverse) in (5_u64..).zip([false, true]) {
            fs::write(&source, b"source changed in the same frame").unwrap();
            fs::write(&metadata, b"metadata changed in the same frame").unwrap();
            let mut events = vec![
                AssetWatchEvent::modified(&source),
                AssetWatchEvent::modified(&metadata),
            ];
            if reverse {
                events.reverse();
            }
            sender.try_send_batch(events).unwrap();
            runtime.drive(Duration::ZERO).unwrap();
            for (owner, calls) in [
                (&source_owner, &source_calls),
                (&metadata_owner, &metadata_calls),
            ] {
                let captured = std::mem::take(&mut *calls.lock().unwrap());
                assert_eq!(captured.len(), 1, "coalesced inputs, reverse={reverse}");
                let request = &captured[0];
                assert_eq!(request.stable_id(), owner.stable_id());
                assert_eq!(
                    request.source_change_kind(),
                    AssetSourceChangeKind::Modified
                );
                assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
                assert_eq!(request.generation().raw(), generation);
            }
            if image_principal {
                image_observations += 1;
                let image_requests = runtime.world().resource::<WatchCapturedReloadRequests>();
                assert_eq!(image_requests.0.len(), image_observations);
                let request = image_requests.0.last().unwrap();
                assert_eq!(request.path(), &raw_source);
                assert_eq!(
                    request.source_change_kind(),
                    AssetSourceChangeKind::MetaModified
                );
                assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
            }
            assert!(
                runtime
                    .world()
                    .resource::<AssetReloadDiagnostics>()
                    .is_empty()
            );
            assert!(runtime.world().resource::<AssetReloadRequests>().is_empty());
        }
        let database = runtime.world().resource::<ProjectAssetDatabase>();
        assert_eq!(
            database.record_for_path(&raw_source).is_some(),
            image_principal
        );
        assert!(database.record_for_path(&raw_metadata).is_none());
        assert_eq!(
            runtime.world().resource::<AssetStates>().len(),
            if image_principal { 3 } else { 2 }
        );
    }));
    let closed = runtime.finish().map_err(|owner| {
        let error = owner.to_string();
        drop(owner);
        error
    });
    remove_temp_root(&root);
    match result {
        Ok(()) => {
            closed.expect("the raw sidecar consumer's real Runtime must close physically");
        }
        Err(failure) => {
            if let Err(error) = closed {
                eprintln!("raw sidecar consumer Runtime cleanup failed: {error}");
            }
            resume_unwind(failure);
        }
    }
}

#[test]
fn watch_translator_routes_published_exact_raw_sidecar_changes_without_cross_notifications() {
    check_exact_raw_sidecar_routing(SourceSubscriptionKind::Published, false);
}

#[test]
fn watch_translator_routes_retry_exact_raw_sidecar_changes_without_cross_notifications() {
    check_exact_raw_sidecar_routing(SourceSubscriptionKind::Retry, false);
}

#[test]
fn watch_translator_keeps_image_principal_semantics_with_exact_raw_sidecar_subscribers() {
    for subscription_kind in [
        SourceSubscriptionKind::Published,
        SourceSubscriptionKind::Retry,
    ] {
        check_exact_raw_sidecar_routing(subscription_kind, true);
    }
}
