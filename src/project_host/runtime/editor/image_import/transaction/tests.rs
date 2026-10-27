use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_image::ImageColorSpace;

use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionHost, PreparedAuthoringProject, project_authoring_adapters,
    },
    runtime::editor::{EditorProjectCloseProgress, EditorProjectSession},
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

    fn editor(&self) -> EditorProjectSession {
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(self.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters = project_authoring_adapters(schema.bundle()).unwrap();
        let receipt = PreparedAuthoringProject::new(schema, adapters)
            .admit_startup_closure()
            .unwrap()
            .publish();
        let mut editor = EditorProjectSession::open(receipt, None).unwrap();
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
                .editor_image_last_good(&AssetPath::new("textures/imported.png").unwrap())
                .is_none()
        );
        assert_eq!(publication.try_with(|_| ()), None);
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
