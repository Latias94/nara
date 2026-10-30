use super::*;

fn refresh_catalogue(
    project: &mut AuthoringProjectSessionSnapshot,
    runtime: &ImageRuntime,
) -> Arc<EditorImageCatalogSnapshot> {
    project.request_source_index_refresh().unwrap();
    indexed_catalogue(project, runtime)
}

fn wait_for_physical_return(host: &EditorImageHydrationHost) {
    let deadline = Instant::now() + WAIT;
    while host.pending.as_ref().unwrap().return_state() == TaskReturnState::Pending {
        assert!(
            Instant::now() < deadline,
            "the held proof did not physically return"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn replace_source(fixture: &CaptureImages) {
    fs::rename(
        fixture.path.join("assets/image.png"),
        fixture.path.join("previous-image.png"),
    )
    .unwrap();
    fs::copy(
        fixture.path.join("previous-image.png"),
        fixture.path.join("assets/image.png"),
    )
    .unwrap();
}

fn write_competing_metadata(fixture: &CaptureImages, stable_id: StableAssetId) {
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            stable_id,
            AssetPath::new("other.png").unwrap(),
            AssetSourceKind::Image,
        ),
        ImageImportSettings::default(),
    );
    fs::write(
        fixture.path.join("assets/other.png.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
}

#[test]
fn later_global_identity_conflict_revokes_captured_pixels_with_worker_evidence() {
    let fixture = CaptureImages::new();
    let mut project = fixture.project();
    let mut runtime = ImageRuntime::new(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    host.initialize(&project, &mut store);
    let source = fixture.source(&project);
    host.enqueue_source(source.clone(), &mut store);
    settle(&mut host, &project, &mut runtime, &mut store);
    let accepted = store.get(source.asset_path()).unwrap().clone();
    fs::copy(
        fixture.path.join("assets/image.png"),
        fixture.path.join("assets/other.png"),
    )
    .unwrap();
    write_competing_metadata(&fixture, source.stable_id());
    let catalog = refresh_catalogue(&mut project, &runtime);
    assert!(matches!(
        catalog.entry("assets/image.png").unwrap().metadata(),
        EditorImageMetadataStatus::IdentityConflict { .. }
    ));
    assert!(
        host.observe_catalog(catalog, &store)
            .contains(source.asset_path())
    );

    let (started, observed) = std::sync::mpsc::sync_channel(1);
    host.next_validation_gate = Some(Box::new(move || {
        started.send(thread::current().id()).unwrap();
    }));
    host.advance(&project, &mut runtime.0, &mut store);
    assert_ne!(observed.recv_timeout(WAIT).unwrap(), thread::current().id());
    settle(&mut host, &project, &mut runtime, &mut store);
    assert_eq!(
        host.take_failure().unwrap().kind(),
        EditorImageHydrationFailureKind::MetadataChanged
    );
    assert!(store.get(source.asset_path()).is_none());
    assert!(
        runtime
            .0
            .image_import_asset(accepted.handle(), accepted.version())
            .is_none()
    );
}

#[test]
fn obsolete_conflict_members_are_re_admitted_without_rebuilding_the_catalogue() {
    for remove in [true, false] {
        let fixture = CaptureImages::new();
        let mut project = fixture.project();
        let mut runtime = ImageRuntime::new(&project);
        let source = fixture.source(&project);
        let mut store = EditorImageLastGoodStore::default();
        let mut host = EditorImageHydrationHost::default();
        host.initialize(&project, &mut store);
        host.enqueue_source(source.clone(), &mut store);
        settle(&mut host, &project, &mut runtime, &mut store);
        let accepted = store.get(source.asset_path()).unwrap().clone();
        write_competing_metadata(&fixture, source.stable_id());
        let catalog = refresh_catalogue(&mut project, &runtime);
        if remove {
            fs::remove_file(fixture.path.join("assets/other.png.meta")).unwrap();
        } else {
            write_competing_metadata(
                &fixture,
                StableAssetId::parse_str("a65bb47a-31d5-42ee-97ca-81ae0ed404fc").unwrap(),
            );
        }
        host.observe_catalog(catalog, &store);
        settle(&mut host, &project, &mut runtime, &mut store);
        assert!(host.take_failure().is_none());
        assert_eq!(store.get(source.asset_path()), Some(&accepted));
    }
}

#[test]
fn a_newer_deletion_retains_one_successor_after_an_older_recheck_has_returned() {
    let fixture = CaptureImages::new();
    let mut project = fixture.project();
    let mut runtime = ImageRuntime::new(&project);
    let older = indexed_catalogue(&mut project, &runtime);
    replace_source(&fixture);
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    host.initialize(&project, &mut store);
    host.enqueue_source(source.clone(), &mut store);
    settle(&mut host, &project, &mut runtime, &mut store);
    let accepted = store.get(source.asset_path()).unwrap().clone();
    host.observe_catalog(older, &store);
    host.advance(&project, &mut runtime.0, &mut store);
    wait_for_physical_return(&host);

    // Do not advance hydration while the newer catalogue is built: this is the same
    // publication pause imposed by an unrelated explicit import.
    fs::remove_file(fixture.path.join("assets/image.png")).unwrap();
    let newer = refresh_catalogue(&mut project, &runtime);
    host.observe_catalog(Arc::clone(&newer), &store);
    host.observe_catalog(newer, &store);
    assert_eq!(host.rechecks.len(), 1);
    host.advance(&project, &mut runtime.0, &mut store);
    assert_eq!(host.rechecks.len(), 1);
    settle(&mut host, &project, &mut runtime, &mut store);
    assert_eq!(
        host.take_failure().unwrap().kind(),
        EditorImageHydrationFailureKind::SourceUnavailable
    );
    assert!(store.get(source.asset_path()).is_none());
    assert!(
        runtime
            .0
            .image_import_asset(accepted.handle(), accepted.version())
            .is_none()
    );
}

#[test]
fn validation_cannot_publish_after_a_newer_catalogue_observes_deletion() {
    let fixture = CaptureImages::new();
    let mut project = fixture.project();
    let mut runtime = ImageRuntime::new(&project);
    let catalog = indexed_catalogue(&mut project, &runtime);
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    host.initialize(&project, &mut store);
    host.observe_catalog(catalog, &store);
    host.enqueue_source(source.clone(), &mut store);
    let deadline = Instant::now() + WAIT;
    loop {
        host.advance(&project, &mut runtime.0, &mut store);
        if host
            .pending
            .as_ref()
            .is_some_and(|pending| matches!(pending.work, ImageHydrationWork::Validate { .. }))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "post-import validation did not start"
        );
        thread::sleep(Duration::from_millis(1));
    }
    wait_for_physical_return(&host);
    let reservations = runtime
        .0
        .image_import_budget_host()
        .snapshot()
        .accepted_reservations();
    fs::remove_file(fixture.path.join("assets/image.png")).unwrap();
    let newer = refresh_catalogue(&mut project, &runtime);
    host.observe_catalog(newer, &store);
    host.advance(&project, &mut runtime.0, &mut store);
    assert!(store.get(source.asset_path()).is_none());
    settle(&mut host, &project, &mut runtime, &mut store);
    assert_eq!(
        host.take_failure().unwrap().kind(),
        EditorImageHydrationFailureKind::SourceUnavailable
    );
    assert!(store.get(source.asset_path()).is_none());
    assert_eq!(
        runtime
            .0
            .image_import_budget_host()
            .snapshot()
            .accepted_reservations(),
        reservations,
        "a successor proof must reuse the decoded candidate",
    );
}

#[test]
fn equivalent_metadata_capture_refreshes_the_receipt_without_redecoding_pixels() {
    for queued_replacement in [false, true] {
        let fixture = CaptureImages::new();
        let mut project = fixture.project();
        let mut runtime = ImageRuntime::new(&project);
        let older = indexed_catalogue(&mut project, &runtime);
        replace_source(&fixture);
        let source = fixture.source(&project);
        let mut store = EditorImageLastGoodStore::default();
        let mut host = EditorImageHydrationHost::default();
        host.initialize(&project, &mut store);
        host.enqueue_source(source.clone(), &mut store);
        settle(&mut host, &project, &mut runtime, &mut store);
        let accepted = store.get(source.asset_path()).unwrap().clone();
        let reservations = runtime
            .0
            .image_import_budget_host()
            .snapshot()
            .accepted_reservations();
        if queued_replacement {
            let mut superseded = source.clone();
            superseded.source_digest = Some(ContentDigest::of_bytes(b"superseded bytes"));
            host.enqueue_source(superseded, &mut store);
        }
        let metadata_path = fixture.path.join("assets/image.png.meta");
        let metadata = fs::read(&metadata_path).unwrap();
        fs::rename(&metadata_path, fixture.path.join("previous.meta")).unwrap();
        let mut rewritten = metadata;
        rewritten.push(b'\n');
        fs::write(&metadata_path, rewritten).unwrap();
        let refreshed = fixture.source(&project);
        assert!(refreshed.matches_last_good(&store));
        assert_ne!(source.metadata_digest, refreshed.metadata_digest);
        assert_ne!(source.metadata_identity, refreshed.metadata_identity);
        host.enqueue_source(refreshed.clone(), &mut store);
        assert_eq!(
            store.get(source.asset_path()).unwrap().capture_source(),
            Some(&refreshed)
        );
        assert!(!host.has_pending_work());
        host.observe_catalog(older, &store);
        settle(&mut host, &project, &mut runtime, &mut store);
        assert!(host.take_failure().is_none());
        let current = store.get(source.asset_path()).unwrap();
        assert_eq!(current.handle(), accepted.handle());
        assert_eq!(current.version(), accepted.version());
        assert_eq!(current.capture_source(), Some(&refreshed));
        assert_eq!(
            runtime
                .0
                .image_import_budget_host()
                .snapshot()
                .accepted_reservations(),
            reservations,
        );
    }
}

#[test]
fn cancelled_work_without_a_successor_is_idle_but_keeps_physical_custody() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let mut runtime = ImageRuntime::new(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    host.initialize(&project, &mut store);
    host.enqueue_source(fixture.source(&project), &mut store);
    let held = runtime.0.hold_next_image_import_through_shutdown();
    host.advance(&project, &mut runtime.0, &mut store);
    assert!(held.wait_until_started());
    host.cancel_pending();
    assert!(!host.has_pending_work());
    assert_eq!(
        host.pending.as_ref().unwrap().return_state(),
        TaskReturnState::Pending
    );
    assert!(held.release());
    wait_for_physical_return(&host);
    host.advance(&project, &mut runtime.0, &mut store);
    assert!(host.pending.is_none());
    assert!(
        store
            .get(&AssetSourcePath::new("assets/image.png").unwrap())
            .is_none()
    );
}

#[cfg(all(feature = "runtime-2d", feature = "presentation-2d"))]
#[test]
fn failed_recheck_revocation_retires_choices_and_latches_the_transaction_failure() {
    use super::super::super::transaction::{EditorImageImportFailure, EditorImageImportHost};

    let fixture = CaptureImages::new();
    let mut project = fixture.project();
    let mut runtime = ImageRuntime::new(&project);
    let mut host = EditorImageImportHost::default();
    let capture = newly_selected_image_capture(&project);
    host.enqueue_authoring_capture(&project, &capture);
    let deadline = Instant::now() + WAIT;
    while host.has_pending_work() || project.source_index_has_pending_work() {
        project.advance_source_index(runtime.0.persistence_tasks());
        assert!(host.advance(&mut project, &mut runtime.0).is_none());
        assert!(
            Instant::now() < deadline,
            "initial image publication did not settle"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let path = AssetSourcePath::new("assets/image.png").unwrap();
    assert!(host.last_good(&path).is_some());
    runtime.0.test_exhaust_image_publication_revision();
    fs::write(
        fixture.path.join("assets/image.png.meta"),
        b"invalid metadata",
    )
    .unwrap();
    project.request_source_index_refresh().unwrap();
    let deadline = Instant::now() + WAIT;
    let mut failures = Vec::new();
    while host.has_pending_work() || project.source_index_has_pending_work() {
        project.advance_source_index(runtime.0.persistence_tasks());
        if let Some(failure) = host.advance(&mut project, &mut runtime.0) {
            failures.push(failure);
        }
        assert!(
            Instant::now() < deadline,
            "terminal revocation failure must settle"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(failures, vec![EditorImageRuntimeFailure::Publication]);
    assert!(host.last_good(&path).is_none());
    assert!(
        host.metadata_snapshot(&project.source_index_snapshot().unwrap())
            .is_some()
    );
    assert_eq!(
        host.request_blocker(),
        Some(EditorImageImportFailure::Runtime(
            EditorImageRuntimeFailure::Publication
        )),
    );
    for _ in 0..3 {
        assert!(host.advance(&mut project, &mut runtime.0).is_none());
        assert!(!host.has_pending_work());
    }
}
