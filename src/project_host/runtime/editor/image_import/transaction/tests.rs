use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara_asset::AssetPath;
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_image::ImageColorSpace;

use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionHost, AuthoringProjectReceipt, PreparedAuthoringProject,
        project_authoring_adapters,
    },
    runtime::editor::{EditorExecutionLane, EditorProjectCloseProgress, EditorProjectSession},
};

use super::*;

const SOURCE: &str = "assets/textures/imported.png";
const METADATA: &str = "assets/textures/imported.png.meta";
const MOVED: &str = "assets/textures/retired.meta";
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(settings: Option<ImageImportSettings>) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara-image-import-source-custody-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets/textures", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            "schema_version = 1\n[project]\nsource_universe = '6d57f8c5-867c-5463-b74a-057a8cabe2bc'\nnative_document_identity = 'project_location'\nname = \"Image custody\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n[capabilities]\nrequested = [\"runtime-2d\"]\n",
        )
        .unwrap();
        let bundle = nara_reflect::AuthoringSchemaBundle::from_trusted_provider_definitions(
            crate::project_host::advanced::built_in_schema_providers(),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            nara_scene::SceneDocument::default()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
            root.join(SOURCE),
        )
        .unwrap();
        if let Some(settings) = settings {
            fs::write(
                root.join(METADATA),
                metadata(settings).to_json_string().unwrap(),
            )
            .unwrap();
        }
        Self { root }
    }

    fn capability(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE,
            };
            fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(target_os = "linux")]
        let directory = fs::File::open(&self.root).unwrap();
        DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    fn receipt(&self) -> AuthoringProjectReceipt {
        Self::receipt_from(self.capability())
    }

    fn receipt_from(root: DirectoryCapability) -> AuthoringProjectReceipt {
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
    }

    fn editor(&self) -> EditorProjectSession {
        let mut editor = EditorProjectSession::open(self.receipt(), None).unwrap();
        drive_until(&mut editor, |editor| {
            !editor.project_source_index_work_pending()
                && !editor.editor_image_import_work_pending()
        });
        editor.drive_editor_frame(Duration::ZERO);
        editor
    }
}

fn metadata_identity(editor: &EditorProjectSession) -> FileIdentity {
    editor
        .authoring_project()
        .unwrap()
        .project_root()
        .open_file(&RelativePath::new(METADATA).unwrap())
        .unwrap()
        .identity()
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn metadata(settings: ImageImportSettings) -> AssetMeta<ImageImportSettings> {
    AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str("11c0ecad-5dba-4c1a-80f4-1bc93da1a506").unwrap(),
            AssetPath::new("textures/imported.png").unwrap(),
            AssetSourceKind::Image,
        ),
        settings,
    )
}

fn drive_until(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "the image import fixture did not settle"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn drive_to_terminal(editor: &mut EditorProjectSession) -> EditorImageImportState {
    drive_until(editor, |editor| {
        !matches!(
            editor.editor_image_import_state(),
            EditorImageImportState::Idle | EditorImageImportState::Pending { .. }
        )
    });
    editor.editor_image_import_state()
}

fn pending_metadata(editor: &EditorProjectSession) -> RetainedHost<FilePublicationHost> {
    assert!(editor.image_import.publication_cleanup.is_none());
    editor
        .image_import
        .pending
        .as_ref()
        .unwrap()
        .metadata
        .clone()
}

fn held_bytes(publication: &RetainedHost<FilePublicationHost>) -> Option<Vec<u8>> {
    publication
        .try_with(|host| host.test_accepted_source_bytes())
        .flatten()
}

fn close_editor(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(
            Instant::now() < deadline,
            "metadata custody did not physically close"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn pending_metadata_pins_the_accepted_leaf_after_rename_and_unlink_and_rejects_name_drift() {
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    // Cover initial publication, settings replacement, and the no-write settings path.
    for previous in [None, Some(ImageImportSettings::default()), Some(linear)] {
        let fixture = Fixture::new(previous);
        let mut editor = fixture.editor();
        let gate = editor.test_hold_next_image_import();
        let frame = editor.current_frame();
        assert!(matches!(
            editor.request_editor_image_import(frame.revision(), SOURCE, linear),
            EditorImageImportRequestResult::Started(_)
        ));
        assert!(gate.wait_until_started());
        let publication = pending_metadata(&editor);
        let bytes = fs::read(fixture.root.join(METADATA)).unwrap();
        let accepted_identity = metadata_identity(&editor);
        assert_eq!(held_bytes(&publication), Some(bytes.clone()));

        fs::rename(fixture.root.join(METADATA), fixture.root.join(MOVED)).unwrap();
        assert_eq!(held_bytes(&publication), Some(bytes.clone()));
        // Identical bytes isolate current-name identity drift from digest drift.
        fs::write(fixture.root.join(METADATA), &bytes).unwrap();
        assert_ne!(metadata_identity(&editor), accepted_identity);
        assert_eq!(held_bytes(&publication), Some(bytes.clone()));
        fs::remove_file(fixture.root.join(MOVED)).unwrap();
        assert_eq!(held_bytes(&publication), Some(bytes));

        assert!(gate.release());
        assert!(matches!(
            drive_to_terminal(&mut editor),
            EditorImageImportState::Failed {
                failure: EditorImageImportFailure::MetadataPublicationRejected,
                ..
            }
        ));
        assert!(
            editor
                .editor_image_last_good(&AssetSourcePath::new(SOURCE).unwrap())
                .is_none()
        );
        assert_eq!(publication.try_with(|_| ()), None);
        close_editor(&mut editor);
    }
}

#[test]
fn custom_overlapping_roots_use_canonical_image_records_and_local_sidecar_aliases() {
    let fixture = Fixture::new(Some(ImageImportSettings::default()));
    let manifest = fixture.root.join("nara.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("assets = \"assets\"", "assets = \"content/art\"")
            .replace("prefabs = \"prefabs\"", "prefabs = \"content/art/prefabs\""),
    )
    .unwrap();
    fs::create_dir_all(fixture.root.join("content")).unwrap();
    fs::rename(
        fixture.root.join("assets"),
        fixture.root.join("content/art"),
    )
    .unwrap();
    fs::rename(
        fixture.root.join("prefabs"),
        fixture.root.join("content/art/prefabs"),
    )
    .unwrap();
    for (root, path, kind, id) in [
        (
            "content/art",
            "shared.prefab.json",
            AssetSourceKind::Other("text".into()),
            "f8b7e28a-7bb8-4402-939d-7ce8e4e57978",
        ),
        (
            "content/art/prefabs",
            "shared.prefab.json",
            AssetSourceKind::Prefab,
            "b336525a-bc0d-4ae3-99d8-981a7c6dd6db",
        ),
        (
            "content/art",
            "unclassified.source",
            AssetSourceKind::Unknown,
            "fa5782c2-c75b-4c08-9bcd-e6288bb23815",
        ),
        (
            "content/art/prefabs",
            "unreferenced.scene.json",
            AssetSourceKind::Scene,
            "5fd40641-ab55-4b30-9e10-5ab5982b13ed",
        ),
        (
            "content/art/prefabs",
            "unregistered.source",
            AssetSourceKind::Other("unregistered-domain".into()),
            "ec139863-8eb0-4e34-b06a-0021618cb05b",
        ),
    ] {
        fs::write(
            fixture.root.join(root).join(format!("{path}.meta")),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str(id).unwrap(),
                    AssetPath::new(path).unwrap(),
                    kind,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
    }
    let source = "content/art/textures/imported.png";
    let source_path = AssetSourcePath::new(source).unwrap();
    let mut editor = fixture.editor();
    {
        let catalog = editor.image_import.catalog.as_ref().unwrap();
        let entry = catalog.entry(source).unwrap();
        assert_eq!(entry.asset_path(), &source_path);
        assert_eq!(entry.authored_path().as_str(), "textures/imported.png");
        assert!(matches!(
            entry.metadata(),
            EditorImageMetadataStatus::Ready(_),
        ));
        for path in [
            "content/art/shared.prefab.json",
            "content/art/prefabs/shared.prefab.json",
            "content/art/unclassified.source",
            "content/art/prefabs/unreferenced.scene.json",
            "content/art/prefabs/unregistered.source",
        ] {
            let path = AssetSourcePath::new(path).unwrap();
            assert_eq!(catalog.asset_identity(&path).unwrap().0.path(), &path);
        }
    }
    let frame = editor.current_frame();
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            source,
            ImageImportSettings::new(ImageColorSpace::Linear),
        ),
        EditorImageImportRequestResult::Started(_)
    ));
    assert!(matches!(
        drive_to_terminal(&mut editor),
        EditorImageImportState::Succeeded { .. },
    ));
    let image = editor.editor_image_last_good(&source_path).unwrap();
    assert_eq!(
        editor
            .authoring_runtime
            .image_import_asset(image.handle(), image.version())
            .unwrap()
            .source()
            .unwrap()
            .path(),
        &source_path,
    );
    assert!(
        editor
            .editor_image_last_good(&AssetSourcePath::new("textures/imported.png").unwrap())
            .is_none()
    );
    let bytes = fs::read(fixture.root.join(format!("{source}.meta"))).unwrap();
    let saved = AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(&bytes).unwrap();
    assert_eq!(saved.identity().path().as_str(), "textures/imported.png");
    #[cfg(feature = "runtime-ui")]
    {
        editor.drive_editor_frame(Duration::ZERO);
        let choices = editor.authoring_images.receipt().share_choices();
        assert!(
            choices
                .choices()
                .iter()
                .any(|choice| choice.project_path() == "textures/imported.png")
        );
        assert!(
            !choices
                .choices()
                .iter()
                .any(|choice| choice.project_path() == source)
        );
    }
    close_editor(&mut editor);
}

#[test]
fn unreferenced_metadata_kinds_preserve_global_identity_conflicts_without_activation() {
    for kind in [AssetSourceKind::Unknown, AssetSourceKind::Scene] {
        let fixture = Fixture::new(Some(ImageImportSettings::default()));
        let stable_id = metadata(ImageImportSettings::default())
            .identity()
            .stable_id();
        fs::write(
            fixture.root.join("assets/unreferenced.source.meta"),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    stable_id,
                    AssetPath::new("unreferenced.source").unwrap(),
                    kind,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let mut editor = fixture.editor();
        {
            let catalog = editor.image_import.catalog.as_ref().unwrap();
            assert!(matches!(
                catalog.entry(SOURCE).unwrap().metadata(),
                EditorImageMetadataStatus::IdentityConflict { stable_id: id } if *id == stable_id
            ));
            assert!(
                catalog
                    .has_current_identity_conflict(
                        &editor.source_project_root(),
                        &AssetSourcePath::new(SOURCE).unwrap(),
                        stable_id,
                    )
                    .unwrap()
            );
        }
        assert!(
            editor
                .editor_image_last_good(&AssetSourcePath::new(SOURCE).unwrap())
                .is_none()
        );
        close_editor(&mut editor);
    }
}

#[test]
fn ready_metadata_is_admitted_from_the_current_leaf_without_catalog_identity_continuity() {
    let srgb = ImageImportSettings::default();
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    for requested in [srgb, linear] {
        let fixture = Fixture::new(Some(srgb));
        let mut editor = fixture.editor();
        let catalog = editor.image_import.catalog.as_ref().unwrap();
        let EditorImageMetadataStatus::Ready(receipt) = catalog.entry(SOURCE).unwrap().metadata()
        else {
            panic!("the fixture metadata must be admitted");
        };
        let catalog_identity = receipt.identity();
        let bytes = fs::read(fixture.root.join(METADATA)).unwrap();
        fs::rename(fixture.root.join(METADATA), fixture.root.join(MOVED)).unwrap();
        fs::write(fixture.root.join(METADATA), &bytes).unwrap();
        let fresh_identity = metadata_identity(&editor);
        assert_ne!(fresh_identity, catalog_identity);

        let gate = editor.test_hold_next_image_import();
        let frame = editor.current_frame();
        assert!(matches!(
            editor.request_editor_image_import(frame.revision(), SOURCE, requested),
            EditorImageImportRequestResult::Started(_)
        ));
        assert!(gate.wait_until_started());
        let publication = pending_metadata(&editor);
        let accepted_identity = publication
            .try_with(|host| host.accepted_source_observation().unwrap().0)
            .unwrap();
        assert_eq!(accepted_identity, metadata_identity(&editor));
        if requested == srgb {
            assert_eq!(accepted_identity, fresh_identity);
            assert_eq!(held_bytes(&publication), Some(bytes));
        }
        fs::remove_file(fixture.root.join(MOVED)).unwrap();
        assert!(gate.release());
        assert!(matches!(
            drive_to_terminal(&mut editor),
            EditorImageImportState::Succeeded { .. }
        ));
        assert_eq!(publication.try_with(|_| ()), None);
        close_editor(&mut editor);
    }
}

#[test]
fn ready_metadata_changes_after_catalog_observation_are_refused_before_import() {
    let srgb = ImageImportSettings::default();
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    for change in [
        "stable_id",
        "path",
        "source_kind",
        "settings",
        "format",
        "encoding",
    ] {
        for requested in [srgb, linear] {
            let fixture = Fixture::new(Some(srgb));
            let mut editor = fixture.editor();
            let identity = metadata_identity(&editor);
            let original = metadata(srgb);
            let changed = match change {
                "stable_id" => AssetMeta::new(
                    AssetMetaIdentity::new(
                        StableAssetId::parse_str("c47a931f-0f54-48bc-98c0-83d54a06763c").unwrap(),
                        original.identity().path().clone(),
                        AssetSourceKind::Image,
                    ),
                    srgb,
                )
                .to_json_string()
                .unwrap(),
                "path" => AssetMeta::new(
                    AssetMetaIdentity::new(
                        original.identity().stable_id(),
                        AssetPath::new("textures/other.png").unwrap(),
                        AssetSourceKind::Image,
                    ),
                    srgb,
                )
                .to_json_string()
                .unwrap(),
                "source_kind" => AssetMeta::new(
                    AssetMetaIdentity::new(
                        original.identity().stable_id(),
                        original.identity().path().clone(),
                        AssetSourceKind::Other(String::from("binary")),
                    ),
                    srgb,
                )
                .to_json_string()
                .unwrap(),
                "settings" => metadata(linear).to_json_string().unwrap(),
                "format" => String::from("invalid metadata"),
                "encoding" => format!("{}\n", original.to_json_string().unwrap()),
                _ => unreachable!(),
            };
            fs::write(fixture.root.join(METADATA), changed.as_bytes()).unwrap();
            assert_eq!(metadata_identity(&editor), identity);
            let admitted_before = editor
                .test_authoring_task_stats()
                .for_kind(nara_tasks::TaskPoolKind::Io)
                .admitted;
            let frame = editor.current_frame();
            assert!(
                matches!(
                    editor.request_editor_image_import(frame.revision(), SOURCE, requested),
                    EditorImageImportRequestResult::Completed(EditorImageImportState::Failed {
                        failure: EditorImageImportFailure::MetadataInvalid
                            | EditorImageImportFailure::MetadataPublicationRejected,
                        ..
                    })
                ),
                "changed {change} must be refused for {requested:?}"
            );
            assert_eq!(
                fs::read(fixture.root.join(METADATA)).unwrap(),
                changed.as_bytes()
            );
            assert!(editor.image_import.pending.is_none());
            assert_eq!(
                editor
                    .test_authoring_task_stats()
                    .for_kind(nara_tasks::TaskPoolKind::Io)
                    .admitted,
                admitted_before
            );
            close_editor(&mut editor);
        }
    }
}

#[test]
fn cancellation_retires_pending_metadata_and_runtime_close_owns_abandoned_metadata() {
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    for previous in [ImageImportSettings::default(), linear] {
        for abandon in [false, true] {
            let fixture = Fixture::new(Some(previous));
            let mut editor = fixture.editor();
            let gate = editor.test_hold_next_image_import();
            let frame = editor.current_frame();
            assert!(matches!(
                editor.request_editor_image_import(frame.revision(), SOURCE, linear),
                EditorImageImportRequestResult::Started(_)
            ));
            assert!(gate.wait_until_started());
            let publication = pending_metadata(&editor);
            let bytes = fs::read(fixture.root.join(METADATA)).unwrap();
            fs::remove_file(fixture.root.join(METADATA)).unwrap();
            assert_eq!(held_bytes(&publication), Some(bytes.clone()));
            if abandon {
                drop(std::mem::take(&mut editor.image_import));
                // The runtime's incumbent HostCustody survives loss of the import owner.
                assert_eq!(held_bytes(&publication), Some(bytes));
            } else {
                editor.image_import.cancel_pending();
                assert_eq!(publication.try_with(|_| ()), None);
            }
            drop(gate);
            close_editor(&mut editor);
            assert_eq!(publication.try_with(|_| ()), None);
        }
    }
}

// Windows can retry exact temporary-file removal after the injected obstruction releases.
// Unix temporary-file failure retains an orphan under its separate filesystem contract.
#[cfg(windows)]
#[test]
fn ordinary_advance_releases_failed_publication_cleanup_without_another_import_or_close() {
    let fixture = Fixture::new(Some(ImageImportSettings::default()));
    let mut editor = fixture.editor();
    let original = fs::read(fixture.root.join(METADATA)).unwrap();
    let cleanup = editor.test_fail_next_image_metadata_after_write();
    let frame = editor.current_frame();
    let outcome = editor.request_editor_image_import(
        frame.revision(),
        SOURCE,
        ImageImportSettings::new(ImageColorSpace::Linear),
    );
    assert!(matches!(
        outcome,
        EditorImageImportRequestResult::Completed(EditorImageImportState::Failed {
            failure: EditorImageImportFailure::CleanupPending,
            ..
        })
    ));
    let failed = editor.image_import.state();
    let publication = editor
        .image_import
        .publication_cleanup
        .as_ref()
        .unwrap()
        .clone();
    for _ in 0..2 {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(editor.image_import.schema_adoption_import_pending());
        assert_eq!(
            editor.image_import.request_blocker(),
            Some(EditorImageImportFailure::CleanupPending)
        );
    }
    cleanup.release();
    editor.drive_editor_frame(Duration::ZERO);
    assert!(!editor.image_import.schema_adoption_import_pending());
    assert!(editor.image_import.publication_cleanup.is_none());
    assert_eq!(editor.image_import.request_blocker(), None);
    assert_eq!(editor.image_import.state(), failed);
    assert_eq!(publication.try_with(|_| ()), None);
    assert_eq!(fs::read(fixture.root.join(METADATA)).unwrap(), original);
    close_editor(&mut editor);
}

#[test]
fn adopting_a_fresh_project_index_replaces_old_refresh_and_reconciliation_targets() {
    for reconcile in [false, true] {
        let fixture = Fixture::new(Some(ImageImportSettings::default()));
        let mut editor = fixture.editor();
        let frame = editor.current_frame();
        assert!(matches!(
            editor.request_editor_image_import(
                frame.revision(),
                SOURCE,
                ImageImportSettings::default(),
            ),
            EditorImageImportRequestResult::Started(_)
        ));
        let EditorImageImportState::Succeeded { generation, .. } = drive_to_terminal(&mut editor)
        else {
            panic!("the fixture must publish actual last-good pixels");
        };
        for _ in 0..3 {
            assert!(editor.request_project_source_index_refresh());
            drive_until(&mut editor, |editor| {
                !editor.project_source_index_work_pending()
                    && !editor.editor_image_import_work_pending()
            });
        }
        let old_index = editor.project_source_index_snapshot().unwrap();
        let old_catalog = editor.image_import.catalog.as_ref().unwrap().clone();
        let old_target = {
            let EditorProjectSession {
                image_import,
                execution,
                ..
            } = &mut editor;
            let EditorExecutionLane::Authoring(project) = execution else {
                unreachable!();
            };
            image_import.schedule_refresh(project).unwrap()
        };
        assert!(old_target > old_index.generation());
        if reconcile {
            editor.image_import.refresh = None;
            editor
                .image_import
                .require_reconciliation(generation, old_target.get());
        }
        let previous_state = editor.image_import.state();
        let path = AssetSourcePath::new(SOURCE).unwrap();
        let previous_image = editor.image_import.last_good(&path).unwrap().clone();
        let previous_detail = editor
            .image_import
            .detail_for(SOURCE)
            .unwrap()
            .detail_revision();
        let metadata_bytes = fs::read(fixture.root.join(METADATA)).unwrap();
        let successor = Fixture::receipt_from(editor.source_project_root())
            .into_session_snapshot()
            .unwrap();
        assert_eq!(
            successor.project_root().identity(),
            old_index.root_identity()
        );
        assert!(
            successor
                .project_root()
                .same_object(&editor.source_project_root())
                .unwrap()
        );
        assert!(successor.source_index_has_pending_work());
        assert!(successor.source_index_snapshot().is_none());
        editor.execution = EditorExecutionLane::Authoring(successor);
        let target = {
            let EditorProjectSession {
                image_import,
                execution,
                ..
            } = &mut editor;
            let EditorExecutionLane::Authoring(project) = execution else {
                unreachable!();
            };
            image_import.adopt_schema_project_index(project);
            let target = image_import.refresh.unwrap().target_generation;
            // Repeated requests coalesce into the successor index's queued refresh.
            assert_eq!(project.request_source_index_refresh(), Some(target));
            target
        };
        assert!(target < old_target);
        assert!(editor.image_import.reconciliation.is_none());
        assert!(editor.image_import.catalog.is_none());
        assert!(editor.image_import.catalog_basis.is_none());
        assert!(editor.image_import.catalog_attempt.is_none());
        assert_eq!(editor.image_import.state(), previous_state);
        let retained = editor.image_import.last_good(&path).unwrap();
        assert_eq!(retained.source_proof(), EditorImageSourceProof::Unverified);
        assert_eq!(retained.handle(), previous_image.handle());
        assert_eq!(retained.stable_id(), previous_image.stable_id());
        assert_eq!(retained.source_identity(), previous_image.source_identity());
        assert_eq!(retained.source_digest(), previous_image.source_digest());
        assert_eq!(retained.settings_hash(), previous_image.settings_hash());
        assert_eq!(retained.version(), previous_image.version());
        assert_eq!(
            editor
                .image_import
                .detail_for(SOURCE)
                .unwrap()
                .detail_revision(),
            previous_detail
        );
        drive_until(&mut editor, |editor| {
            !editor.project_source_index_work_pending()
                && !editor.editor_image_import_work_pending()
        });
        let current = editor.project_source_index_snapshot().unwrap();
        assert_eq!(current.generation(), target);
        assert!(!Arc::ptr_eq(&current, &old_index));
        assert!(!Arc::ptr_eq(
            editor.image_import.catalog.as_ref().unwrap(),
            &old_catalog
        ));
        assert!(editor.image_import.metadata_snapshot(&current).is_some());
        assert!(editor.image_import.refresh.is_none());
        assert!(editor.image_import.reconciliation.is_none());
        assert_eq!(editor.image_import.request_blocker(), None);
        assert_eq!(editor.image_import.state(), previous_state);
        assert_eq!(
            editor.image_import.last_good(&path).unwrap().source_proof(),
            EditorImageSourceProof::Unverified
        );
        assert_eq!(
            editor.image_import.last_good(&path).unwrap().version(),
            previous_image.version()
        );
        assert_eq!(
            fs::read(fixture.root.join(METADATA)).unwrap(),
            metadata_bytes
        );
        close_editor(&mut editor);
    }
}

#[test]
fn refused_successor_refresh_retains_unavailable_observation_without_inventing_success() {
    let fixture = Fixture::new(Some(ImageImportSettings::default()));
    let mut editor = fixture.editor();
    let previous_state = editor.image_import.state();
    let mut successor = Fixture::receipt_from(editor.source_project_root())
        .into_session_snapshot()
        .unwrap();
    successor.cancel_source_index();
    editor
        .image_import
        .adopt_schema_project_index(&mut successor);
    assert!(editor.image_import.refresh.is_none());
    assert!(editor.image_import.reconciliation.is_none());
    assert_eq!(
        editor.image_import.catalog_failure,
        Some(EditorImageCatalogBuildError::SourceIndexUnavailable)
    );
    editor
        .image_import
        .advance(&mut successor, &mut editor.authoring_runtime);
    assert_eq!(
        editor.image_import.catalog_failure,
        Some(EditorImageCatalogBuildError::SourceIndexUnavailable)
    );
    assert!(editor.image_import.catalog.is_none());
    assert_eq!(editor.image_import.state(), previous_state);
    assert!(!successor.source_index_has_pending_work());
    close_editor(&mut editor);
}

#[test]
fn successor_index_generation_collision_cannot_reverify_old_pixels_without_import() {
    use nara_tooling::{EditorImageImportAvailabilityView, EditorImageImportFreshness};

    let fixture = Fixture::new(Some(ImageImportSettings::default()));
    let mut editor = fixture.editor();
    let frame = editor.current_frame();
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            SOURCE,
            ImageImportSettings::default(),
        ),
        EditorImageImportRequestResult::Started(_)
    ));
    assert!(matches!(
        drive_to_terminal(&mut editor),
        EditorImageImportState::Succeeded { .. }
    ));
    {
        let EditorProjectSession {
            image_import,
            execution,
            ..
        } = &mut editor;
        let EditorExecutionLane::Authoring(project) = execution else {
            unreachable!();
        };
        image_import.schedule_refresh(project).unwrap();
    }
    drive_until(&mut editor, |editor| {
        !editor.project_source_index_work_pending() && !editor.editor_image_import_work_pending()
    });
    let path = AssetSourcePath::new(SOURCE).unwrap();
    let previous = editor.image_import.last_good(&path).unwrap().clone();
    let EditorImageSourceProof::SourceIndex {
        generation: verified,
    } = previous.source_proof()
    else {
        panic!("the actual import and refresh must verify the predecessor index");
    };
    let source_bytes = fs::read(fixture.root.join(SOURCE)).unwrap();
    // An in-place rewrite retains the native identity but invalidates the decoded pixels' digest.
    fs::write(
        fixture.root.join(SOURCE),
        b"changed image bytes under the same identity",
    )
    .unwrap();
    let current_source = editor
        .source_project_root()
        .open_file_exact(&RelativePath::new(SOURCE).unwrap())
        .unwrap();
    assert_eq!(current_source.identity(), previous.source_identity());
    assert!(
        current_source
            .verify_digest(previous.source_digest(), source_bytes.len() as u64)
            .is_err()
    );
    drop(current_source);
    let successor = Fixture::receipt_from(editor.source_project_root())
        .into_session_snapshot()
        .unwrap();
    assert_eq!(
        successor.project_root().identity(),
        editor.source_project_root().identity()
    );
    assert!(
        successor
            .project_root()
            .same_object(&editor.source_project_root())
            .unwrap()
    );
    editor.execution = EditorExecutionLane::Authoring(successor);
    {
        let EditorProjectSession {
            image_import,
            execution,
            ..
        } = &mut editor;
        let EditorExecutionLane::Authoring(project) = execution else {
            unreachable!();
        };
        image_import.adopt_schema_project_index(project);
        assert_eq!(image_import.refresh.unwrap().target_generation, verified);
    }
    drive_until(&mut editor, |editor| {
        !editor.project_source_index_work_pending() && !editor.editor_image_import_work_pending()
    });
    let index = editor.project_source_index_snapshot().unwrap();
    assert_eq!(index.generation(), verified);
    let retained = editor.image_import.last_good(&path).unwrap();
    assert_eq!(retained.source_proof(), EditorImageSourceProof::Unverified);
    assert_eq!(retained.handle(), previous.handle());
    assert_eq!(retained.version(), previous.version());
    assert_eq!(retained.source_digest(), previous.source_digest());
    let (_, view) = editor
        .image_import
        .projection_for(&index, SOURCE, &editor.authoring_runtime)
        .unwrap()
        .into_parts();
    assert!(matches!(
        view.availability(),
        EditorImageImportAvailabilityView::LastGood {
            freshness: EditorImageImportFreshness::SourceUnverified,
            ..
        }
    ));

    // Only another actual decode/validation may establish verification in the new index.
    fs::write(fixture.root.join(SOURCE), &source_bytes).unwrap();
    let frame = editor.current_frame();
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            SOURCE,
            ImageImportSettings::default(),
        ),
        EditorImageImportRequestResult::Started(_)
    ));
    assert!(matches!(
        drive_to_terminal(&mut editor),
        EditorImageImportState::Succeeded { .. }
    ));
    assert_eq!(
        editor.image_import.last_good(&path).unwrap().source_proof(),
        EditorImageSourceProof::SourceIndex {
            generation: verified
        }
    );
    let (_, view) = editor
        .image_import
        .projection_for(&index, SOURCE, &editor.authoring_runtime)
        .unwrap()
        .into_parts();
    assert!(matches!(
        view.availability(),
        EditorImageImportAvailabilityView::LastGood {
            freshness: EditorImageImportFreshness::Fresh,
            ..
        }
    ));
    close_editor(&mut editor);
}
