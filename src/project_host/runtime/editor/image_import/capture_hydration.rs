use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;

use nara_asset::{AssetMeta, AssetMetaIdentity, AssetServer, AssetStates, AssetVersion, Assets};
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_image::{
    ImageAsset, ImageColorSpace, ImageFileImportRequest, ImageImportLimits, ImageImporter,
};
use nara_reflect::AuthoringSchemaBundle;

use super::*;
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionHost, PreparedAuthoringProject,
        first_party_authoring_preview_registry, project_authoring_adapters,
    },
    composition::built_in_schema_providers,
    runtime::editor::authoring_runtime::EditorAuthoringRuntimeCloseProgress,
};

#[path = "capture_hydration_observations.rs"]
mod observations;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
const WAIT: Duration = Duration::from_secs(5);

struct CaptureImages {
    path: PathBuf,
}

impl CaptureImages {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nara_capture_hydration_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(path.join(directory)).unwrap();
        }
        fs::write(
            path.join("nara.toml"),
            r#"schema_version = 1
[project]
name = "capture-hydration"
source_universe = 'c60a6da4-b6a7-5028-bc53-39111af1a2ba'
native_document_identity = 'project_location'
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
"#,
        )
        .unwrap();
        fs::write(
            path.join("schema/authoring.bundle.json"),
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            path.join("scenes/startup.scene.json"),
            nara_scene::SceneDocument::default()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/images/valid-rgba-1x1.png"),
            path.join("assets/image.png"),
        )
        .unwrap();
        let fixture = Self { path };
        fixture.write_metadata(ImageImportSettings::default());
        fixture
    }

    fn write_metadata(&self, settings: ImageImportSettings) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("3c7c5be4-fd4e-4b65-b8d4-c671f5982186").unwrap(),
                AssetPath::new("image.png").unwrap(),
                AssetSourceKind::Image,
            ),
            settings,
        );
        fs::write(
            self.path.join("assets/image.png.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }

    fn project(&self) -> AuthoringProjectSessionSnapshot {
        #[cfg(windows)]
        let root = {
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .share_mode(0x0000_0001 | 0x0000_0002 | 0x0000_0004)
                .open(&self.path)
                .unwrap()
        };
        #[cfg(target_os = "linux")]
        let root = File::open(&self.path).unwrap();
        let root = DirectoryCapability::from_host_handle(
            root,
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
        )
        .unwrap();
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(root, None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters = project_authoring_adapters(schema.bundle()).unwrap();
        PreparedAuthoringProject::new(schema, adapters)
            .admit_startup_closure()
            .unwrap()
            .publish()
            .into_session_parts()
            .unwrap()
            .snapshot
    }

    fn source(&self, project: &AuthoringProjectSessionSnapshot) -> EditorImageHydrationSource {
        let source_path = RelativePath::new("assets/image.png").unwrap();
        let metadata_path = RelativePath::new("assets/image.png.meta").unwrap();
        let root = project.project_root();
        let source = root.open_file(&source_path).unwrap();
        let metadata = root.open_file(&metadata_path).unwrap();
        let bytes = metadata
            .read_to_end_bounded(super::super::metadata_byte_limit())
            .unwrap();
        let candidate =
            nara_asset::AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(&bytes)
                .unwrap();
        EditorImageHydrationSource {
            attempt_generation: project.attempt_generation(),
            project_root_identity: root.identity(),
            record: AssetRecord::new(
                candidate.identity().stable_id(),
                candidate.identity().path().clone(),
                AssetSourceKind::Image,
            ),
            source_path,
            source_identity: source.identity(),
            source_digest: Some(ContentDigest::of_bytes(
                &fs::read(self.path.join("assets/image.png")).unwrap(),
            )),
            metadata_path,
            metadata_identity: metadata.identity(),
            metadata_digest: ContentDigest::of_bytes(&bytes),
            settings: *candidate.settings(),
        }
    }
}

impl Drop for CaptureImages {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct ImageRuntime(EditorAuthoringRuntime);

impl ImageRuntime {
    fn new(project: &AuthoringProjectSessionSnapshot) -> Self {
        let registry = first_party_authoring_preview_registry(built_in_schema_providers()).unwrap();
        Self(EditorAuthoringRuntime::new_authoring(project, &registry).unwrap())
    }
}

impl Drop for ImageRuntime {
    fn drop(&mut self) {
        let deadline = Instant::now() + WAIT;
        loop {
            if let EditorAuthoringRuntimeCloseProgress::Complete(report) = self.0.shutdown_tasks() {
                assert!(report.is_complete());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the image runtime did not retire"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
}

fn settle(
    host: &mut EditorImageHydrationHost,
    project: &AuthoringProjectSessionSnapshot,
    runtime: &mut ImageRuntime,
    store: &mut EditorImageLastGoodStore,
) {
    let deadline = Instant::now() + WAIT;
    while host.has_pending_work() {
        host.advance(project, &mut runtime.0, store);
        assert!(Instant::now() < deadline, "image hydration did not finish");
        thread::sleep(Duration::from_millis(1));
    }
}

fn indexed_catalogue(
    project: &mut AuthoringProjectSessionSnapshot,
    runtime: &ImageRuntime,
) -> Arc<EditorImageCatalogSnapshot> {
    let deadline = Instant::now() + WAIT;
    while project.source_index_has_pending_work() {
        project.advance_source_index(runtime.0.persistence_tasks());
        assert!(Instant::now() < deadline, "the source index did not finish");
        thread::sleep(Duration::from_millis(1));
    }
    let mut attempt = super::super::catalog::EditorImageCatalogAttempt::begin(
        project,
        project.source_index_snapshot().unwrap(),
    )
    .unwrap();
    loop {
        if let Some(catalogue) = attempt.advance(project).unwrap() {
            return Arc::new(catalogue);
        }
        assert!(
            Instant::now() < deadline,
            "the image catalogue did not finish"
        );
    }
}

#[test]
fn queued_captures_deduplicate_exact_receipts_and_replace_changes_at_the_same_path() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    host.initialize(&project, &mut store);
    host.enqueue_source(source.clone(), &mut store);
    host.enqueue_source(source.clone(), &mut store);
    assert_eq!(host.queued.len(), 1);
    assert_eq!(host.queued.front(), Some(&source));

    let mut changed_digest = source.clone();
    changed_digest.source_digest = Some(ContentDigest::of_bytes(b"a newer source"));
    let mut changed_metadata = source.clone();
    changed_metadata.metadata_digest = ContentDigest::of_bytes(b"a newer sidecar");
    let mut changed_settings = source.clone();
    changed_settings.settings = ImageImportSettings::new(ImageColorSpace::Linear);
    let mut changed_identity = source.clone();
    changed_identity.source_identity = project.startup_source().identity();
    let mut changed_record = source.clone();
    changed_record.record = AssetRecord::new(
        StableAssetId::parse_str("a65bb47a-31d5-42ee-97ca-81ae0ed404fc").unwrap(),
        source.asset_path().clone(),
        AssetSourceKind::Image,
    );
    for changed in [
        changed_digest,
        changed_metadata,
        changed_settings,
        changed_identity,
        changed_record,
    ] {
        host.enqueue_source(changed.clone(), &mut store);
        assert_eq!(host.queued.len(), 1);
        assert_eq!(host.queued.front(), Some(&changed));
    }
    assert!(host.take_failure().is_none());
}

#[test]
fn exact_last_good_reuses_pixels_and_removes_a_queued_superseded_receipt() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    let mut runtime = ImageRuntime::new(&project);
    host.initialize(&project, &mut store);
    host.enqueue_source(source.clone(), &mut store);
    settle(&mut host, &project, &mut runtime, &mut store);
    assert!(host.take_failure().is_none());
    let accepted = store.get(source.asset_path()).unwrap().clone();
    assert!(
        runtime
            .0
            .image_import_asset(accepted.handle(), accepted.version())
            .is_some()
    );
    host.enqueue_source(source.clone(), &mut store);
    assert!(!host.has_pending_work());

    let mut changed = source.clone();
    changed.source_digest = Some(ContentDigest::of_bytes(b"different image bytes"));
    host.enqueue_source(changed, &mut store);
    assert!(host.has_pending_work());
    host.enqueue_source(source, &mut store);
    assert!(!host.has_pending_work());
    assert_eq!(
        store.get(&AssetPath::new("image.png").unwrap()),
        Some(&accepted)
    );
}

#[test]
fn pending_capture_is_reused_only_for_the_same_receipt_and_cannot_publish_after_replacement() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    let mut runtime = ImageRuntime::new(&project);
    host.initialize(&project, &mut store);
    let held = runtime.0.hold_next_image_import();
    host.enqueue_source(source.clone(), &mut store);
    host.advance(&project, &mut runtime.0, &mut store);
    assert!(held.wait_until_started());
    let next_tick = host.next_admission_tick;
    host.enqueue_source(source.clone(), &mut store);
    assert!(host.pending.is_some());
    assert!(host.queued.is_empty());
    assert_eq!(host.next_admission_tick, next_tick);

    fixture.write_metadata(ImageImportSettings::new(ImageColorSpace::Linear));
    let replacement = fixture.source(&project);
    host.enqueue_source(replacement.clone(), &mut store);
    assert!(host.pending.as_ref().unwrap().cancelled);
    assert_eq!(host.queued.front(), Some(&replacement));
    let _ = held.release();
    settle(&mut host, &project, &mut runtime, &mut store);
    assert!(host.take_failure().is_none());
    let accepted = store.get(source.asset_path()).unwrap();
    assert_eq!(
        accepted.settings_hash(),
        replacement.settings.settings_hash()
    );
    assert_eq!(accepted.version(), AssetVersion::from_raw(1));
}

#[test]
fn successor_waits_for_cancelled_physical_return_under_a_tight_shared_budget() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    let mut runtime = ImageRuntime::new(&project);
    host.initialize(&project, &mut store);
    let held = runtime.0.hold_next_image_import_through_shutdown();
    host.enqueue_source(source.clone(), &mut store);
    host.advance(&project, &mut runtime.0, &mut store);
    assert!(held.wait_until_started());
    let returned = match &host.pending.as_ref().unwrap().work {
        ImageHydrationWork::Import(task) => task.return_observer(),
        _ => panic!("the held task is the original image import"),
    };

    // Reserve quota without reading or allocating encoded image buffers. Only one new file
    // admission fits after the held import's retained pixels have physically been released.
    let limits = ImageImportLimits::default();
    let budget = runtime.0.image_import_budget_host();
    let importer = ImageImporter::with_budget_host(limits, budget.clone()).unwrap();
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&source.record).unwrap();
    let count = budget.limit().get() / limits.max_encoded_bytes().get() - 1;
    let blockers = (0..count)
        .map(|_| {
            importer
                .admit_file(
                    ImageFileImportRequest::new(
                        source.record.clone(),
                        project
                            .project_root()
                            .open_file_exact(&source.source_path)
                            .unwrap(),
                        source.settings,
                    ),
                    handle,
                    AssetVersion::ZERO,
                    &server,
                    &Assets::default(),
                    &AssetStates::default(),
                )
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(
        budget.snapshot().active_bytes() + limits.max_encoded_bytes().get() > budget.limit().get()
    );
    let before = budget.snapshot();
    let next_tick = host.next_admission_tick;
    fixture.write_metadata(ImageImportSettings::new(ImageColorSpace::Linear));
    let replacement = fixture.source(&project);
    host.enqueue_source(replacement.clone(), &mut store);
    for _ in 0..4 {
        host.advance(&project, &mut runtime.0, &mut store);
        assert_eq!(returned.state(), TaskReturnState::Pending);
        assert_eq!(host.next_admission_tick, next_tick);
        assert_eq!(
            budget.snapshot().accepted_reservations(),
            before.accepted_reservations()
        );
        assert_eq!(
            budget.snapshot().rejected_reservations(),
            before.rejected_reservations()
        );
        assert!(store.get(source.asset_path()).is_none());
    }
    assert!(held.release());
    settle(&mut host, &project, &mut runtime, &mut store);
    assert!(host.take_failure().is_none());
    let accepted = store.get(source.asset_path()).unwrap();
    assert_eq!(
        accepted.settings_hash(),
        replacement.settings.settings_hash()
    );
    assert_eq!(accepted.version(), AssetVersion::from_raw(1));
    assert_eq!(
        budget.snapshot().rejected_reservations(),
        before.rejected_reservations()
    );
    drop(blockers);
}

#[test]
fn late_file_verification_runs_off_thread_and_rejects_the_original_imported_result() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    let mut runtime = ImageRuntime::new(&project);
    host.initialize(&project, &mut store);
    let (started, observed) = std::sync::mpsc::sync_channel(1);
    let (release, gate) = std::sync::mpsc::channel();
    host.next_validation_gate = Some(Box::new(move || {
        started.send(thread::current().id()).unwrap();
        gate.recv_timeout(WAIT).unwrap();
    }));
    host.enqueue_source(source.clone(), &mut store);
    let deadline = Instant::now() + WAIT;
    let worker = loop {
        host.advance(&project, &mut runtime.0, &mut store);
        if let Ok(worker) = observed.try_recv() {
            break worker;
        }
        assert!(
            Instant::now() < deadline,
            "the asynchronous proof did not start"
        );
        thread::sleep(Duration::from_millis(1));
    };
    assert_ne!(worker, thread::current().id());
    assert!(store.get(source.asset_path()).is_none());
    assert!(matches!(
        &host.pending.as_ref().unwrap().work,
        ImageHydrationWork::Validate { .. }
    ));
    fs::write(
        fixture.path.join("assets/image.png"),
        b"changed after decode",
    )
    .unwrap();
    release.send(()).unwrap();
    settle(&mut host, &project, &mut runtime, &mut store);
    assert_eq!(
        host.take_failure().unwrap().kind(),
        EditorImageHydrationFailureKind::SourceChanged
    );
    assert!(store.get(source.asset_path()).is_none());
}

#[test]
fn older_catalogue_identity_cannot_cancel_a_new_capture_or_revoke_its_pixels() {
    let fixture = CaptureImages::new();
    let mut project = fixture.project();
    let mut runtime = ImageRuntime::new(&project);
    let catalogue = indexed_catalogue(&mut project, &runtime);
    let old = fixture.source(&project);
    fs::rename(
        fixture.path.join("assets/image.png"),
        fixture.path.join("old-image.png"),
    )
    .unwrap();
    fs::copy(
        fixture.path.join("old-image.png"),
        fixture.path.join("assets/image.png"),
    )
    .unwrap();
    let source = fixture.source(&project);
    assert_ne!(old.source_identity, source.source_identity);
    assert!(!catalogue.retains_imported_image(
        source.asset_path(),
        source.stable_id(),
        source.source_identity
    ));
    let mut host = EditorImageHydrationHost::default();
    let mut store = EditorImageLastGoodStore::default();
    host.initialize(&project, &mut store);
    host.enqueue_source(source.clone(), &mut store);
    host.observe_catalog(Arc::clone(&catalogue), &store);
    assert_eq!(host.queued.front(), Some(&source));
    let held = runtime.0.hold_next_image_import();
    host.advance(&project, &mut runtime.0, &mut store);
    assert!(held.wait_until_started());
    host.observe_catalog(Arc::clone(&catalogue), &store);
    assert!(!host.pending.as_ref().unwrap().cancelled);
    assert!(held.release());
    settle(&mut host, &project, &mut runtime, &mut store);
    let accepted = store.get(source.asset_path()).unwrap().clone();
    assert!(
        host.observe_catalog(Arc::clone(&catalogue), &store)
            .contains(source.asset_path())
    );
    settle(&mut host, &project, &mut runtime, &mut store);
    assert!(host.take_failure().is_none());
    assert_eq!(store.get(source.asset_path()), Some(&accepted));
    assert!(
        runtime
            .0
            .image_import_asset(accepted.handle(), accepted.version())
            .is_some()
    );
}

#[test]
fn missing_catalogue_entry_defers_to_capture_but_later_real_deletion_revokes_it() {
    let fixture = CaptureImages::new();
    fs::rename(
        fixture.path.join("assets/image.png"),
        fixture.path.join("stashed.png"),
    )
    .unwrap();
    fs::rename(
        fixture.path.join("assets/image.png.meta"),
        fixture.path.join("stashed.meta"),
    )
    .unwrap();
    let mut project = fixture.project();
    let mut runtime = ImageRuntime::new(&project);
    let catalogue = indexed_catalogue(&mut project, &runtime);
    fs::rename(
        fixture.path.join("stashed.png"),
        fixture.path.join("assets/image.png"),
    )
    .unwrap();
    fs::rename(
        fixture.path.join("stashed.meta"),
        fixture.path.join("assets/image.png.meta"),
    )
    .unwrap();
    let source = fixture.source(&project);
    let retain = |path: &AssetPath, stable_id, identity| {
        catalogue.retains_imported_image(path, stable_id, identity)
    };
    assert!(!retain(
        source.asset_path(),
        source.stable_id(),
        source.source_identity
    ));
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    host.initialize(&project, &mut store);
    host.enqueue_source(source.clone(), &mut store);
    host.observe_catalog(Arc::clone(&catalogue), &store);
    settle(&mut host, &project, &mut runtime, &mut store);
    let accepted = store.get(source.asset_path()).unwrap().clone();
    assert!(
        host.observe_catalog(Arc::clone(&catalogue), &store)
            .contains(source.asset_path())
    );
    settle(&mut host, &project, &mut runtime, &mut store);
    assert_eq!(store.get(source.asset_path()), Some(&accepted));

    fs::remove_file(fixture.path.join("assets/image.png")).unwrap();
    assert!(
        host.observe_catalog(Arc::clone(&catalogue), &store)
            .contains(source.asset_path())
    );
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
fn late_metadata_change_rejects_hydration_without_overwriting_last_good() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    let mut runtime = ImageRuntime::new(&project);
    host.initialize(&project, &mut store);
    host.enqueue_source(fixture.source(&project), &mut store);
    settle(&mut host, &project, &mut runtime, &mut store);
    let path = AssetPath::new("image.png").unwrap();
    let accepted = store.get(&path).unwrap().clone();

    fixture.write_metadata(ImageImportSettings::new(ImageColorSpace::Linear));
    let held = runtime.0.hold_next_image_import();
    host.enqueue_source(fixture.source(&project), &mut store);
    host.advance(&project, &mut runtime.0, &mut store);
    assert!(held.wait_until_started());
    fixture.write_metadata(ImageImportSettings::default());
    assert!(held.release());
    settle(&mut host, &project, &mut runtime, &mut store);
    assert_eq!(
        host.take_failure().unwrap().kind(),
        EditorImageHydrationFailureKind::MetadataChanged,
    );
    assert_eq!(store.get(&path), Some(&accepted));
    assert!(
        runtime
            .0
            .image_import_asset(accepted.handle(), accepted.version())
            .is_some()
    );
}

#[test]
fn completion_rechecks_source_bytes_after_the_worker_has_proven_its_input() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let source = fixture.source(&project);
    let digest = source.source_digest.unwrap();
    assert_eq!(
        source.validate_current_files(&project.project_root(), digest),
        Ok(())
    );
    fs::write(fixture.path.join("assets/image.png"), b"changed source").unwrap();
    assert_eq!(
        source.validate_current_files(&project.project_root(), digest),
        Err(EditorImageHydrationFailureKind::SourceChanged),
    );
}

#[test]
fn queued_receipt_replacement_does_not_consume_a_second_capacity_slot() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let mut host = EditorImageHydrationHost::default();
    for index in 0..MAX_LAST_GOOD_IMAGES {
        let mut queued = source.clone();
        queued.record = AssetRecord::new(
            source.stable_id(),
            AssetPath::new(format!("image-{index}.png")).unwrap(),
            AssetSourceKind::Image,
        );
        host.enqueue_source(queued, &mut store);
    }
    let mut replacement = host.queued.front().unwrap().clone();
    replacement.metadata_digest = ContentDigest::of_bytes(b"replacement receipt");
    host.enqueue_source(replacement.clone(), &mut store);
    assert_eq!(host.queued.len(), MAX_LAST_GOOD_IMAGES);
    assert_eq!(host.queued.front(), Some(&replacement));
    assert!(host.take_failure().is_none());
    host.enqueue_source(source.clone(), &mut store);
    assert_eq!(host.queued.len(), MAX_LAST_GOOD_IMAGES);
    let rejected = host.take_failure().unwrap();
    assert_eq!(rejected.asset_path(), source.asset_path());
    assert_eq!(
        rejected.kind(),
        EditorImageHydrationFailureKind::CapacityExceeded
    );
}

#[test]
fn last_good_matching_includes_identity_digest_settings_and_stable_record() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    let source = fixture.source(&project);
    let mut store = EditorImageLastGoodStore::default();
    let handle = AssetServer::new()
        .reserve_record::<ImageAsset>(&source.record)
        .unwrap();
    assert!(store.insert(
        source.asset_path().clone(),
        EditorImageLastGood::new(
            source.stable_id(),
            handle,
            source.source_identity,
            EditorImageSourceProof::AuthoringClosure {
                attempt_generation: project.attempt_generation(),
            },
            source.source_digest.unwrap(),
            source.settings.settings_hash(),
            AssetVersion::from_raw(1),
        ),
    ));
    assert!(source.matches_last_good(&store));
    let mut changed = source.clone();
    changed.source_digest = Some(ContentDigest::of_bytes(b"changed source"));
    assert!(!changed.matches_last_good(&store));
    changed = source.clone();
    changed.source_identity = project.startup_source().identity();
    assert!(!changed.matches_last_good(&store));
    changed = source.clone();
    changed.settings = ImageImportSettings::new(ImageColorSpace::Linear);
    assert!(!changed.matches_last_good(&store));
    changed = source;
    changed.record = AssetRecord::new(
        StableAssetId::parse_str("a65bb47a-31d5-42ee-97ca-81ae0ed404fc").unwrap(),
        changed.asset_path().clone(),
        AssetSourceKind::Image,
    );
    assert!(!changed.matches_last_good(&store));
}

#[cfg(feature = "runtime-2d")]
fn newly_selected_image_capture(
    project: &AuthoringProjectSessionSnapshot,
) -> AuthoringDocumentCapture {
    use crate::project_host::authoring::document::AuthoringDocumentSourceFact;
    use nara_reflect::{ComponentFieldId, ComponentPropertyPath, ComponentTypeId, ComponentValue};
    use nara_scene::{SceneAuthoringSession, ScenePlacement, ScenePlacementOwner};

    let source = AuthoringDocumentSourceFact::from_captured_source(
        project.startup_source(),
        project.startup_native_source(),
    )
    .unwrap();
    let mut session =
        SceneAuthoringSession::from_published_authoring_document(project.fork_startup_scene());
    let mut draft = session
        .begin_authoring_construction(session.revision())
        .unwrap();
    let entity = draft
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    let component = ComponentTypeId::new("nara.sprite.Sprite");
    draft.add_component(&entity, component.clone()).unwrap();
    draft
        .set_property(
            &entity,
            component,
            ComponentPropertyPath::new(ComponentFieldId::new("material.image")),
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                ("value", ComponentValue::String("image.png".into())),
            ]),
        )
        .unwrap();
    let prepared = session.prepare_authoring_construction(&draft).unwrap();
    session.commit_authoring_construction(prepared).unwrap();
    project
        .capture_scene_document(&source, &session, &[])
        .unwrap()
}

#[cfg(feature = "runtime-2d")]
#[test]
fn actual_capture_enqueues_only_the_newly_selected_image_not_the_startup_inventory() {
    let fixture = CaptureImages::new();
    let project = fixture.project();
    assert_eq!(project.image_asset_sources().count(), 0);
    let capture = newly_selected_image_capture(&project);
    assert_eq!(capture.assets().len(), 1);
    let expected = fixture.source(&project);
    let mut host = EditorImageHydrationHost::default();
    let mut store = EditorImageLastGoodStore::default();
    host.enqueue_authoring_capture(&project, &capture, &mut store);
    host.enqueue_authoring_capture(&project, &capture, &mut store);
    assert_eq!(host.queued.len(), 1);
    assert_eq!(host.queued.front(), Some(&expected));
    assert!(host.take_failure().is_none());
    assert_eq!(project.image_asset_sources().count(), 0);
}

#[cfg(feature = "runtime-2d")]
#[test]
fn catalogue_finishing_after_actual_capture_enqueue_does_not_erase_the_import() {
    use super::super::transaction::EditorImageImportHost;

    let fixture = CaptureImages::new();
    fs::rename(
        fixture.path.join("assets/image.png"),
        fixture.path.join("stashed.png"),
    )
    .unwrap();
    fs::rename(
        fixture.path.join("assets/image.png.meta"),
        fixture.path.join("stashed.meta"),
    )
    .unwrap();
    for index in 0..96 {
        fs::write(
            fixture.path.join(format!("assets/padding-{index}.txt")),
            b"padding",
        )
        .unwrap();
    }
    let mut project = fixture.project();
    let mut runtime = ImageRuntime::new(&project);
    let _ = indexed_catalogue(&mut project, &runtime);
    assert!(project.source_index_snapshot().unwrap().entries().len() >= 96);
    let mut host = EditorImageImportHost::default();
    host.advance(&mut project, &mut runtime.0);
    assert!(host.has_pending_work());

    fs::rename(
        fixture.path.join("stashed.png"),
        fixture.path.join("assets/image.png"),
    )
    .unwrap();
    fs::rename(
        fixture.path.join("stashed.meta"),
        fixture.path.join("assets/image.png.meta"),
    )
    .unwrap();
    let capture = newly_selected_image_capture(&project);
    host.enqueue_authoring_capture(&project, &capture);
    let held = runtime.0.hold_next_image_import_through_shutdown();
    host.advance(&mut project, &mut runtime.0);
    assert!(held.wait_until_started());
    // More than enough bounded advances to complete the old catalogue, while the import
    // deliberately cannot physically return or publish its result.
    for _ in 0..40 {
        assert!(host.advance(&mut project, &mut runtime.0).is_none());
    }
    assert!(held.release());
    let deadline = Instant::now() + WAIT;
    while host.has_pending_work() {
        assert!(host.advance(&mut project, &mut runtime.0).is_none());
        assert!(
            Instant::now() < deadline,
            "capture hydration did not finish after the catalogue"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(host.take_hydration_failure().is_none());
    let accepted = host
        .last_good(&AssetPath::new("image.png").unwrap())
        .unwrap();
    assert_eq!(
        accepted.source_identity(),
        fixture.source(&project).source_identity
    );
    assert!(
        runtime
            .0
            .image_import_asset(accepted.handle(), accepted.version())
            .is_some()
    );
}
