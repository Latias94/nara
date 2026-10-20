#[cfg(windows)]
use super::*;

#[cfg(windows)]
use crate::project_host::file_publication::CleanupTestControl;

// Each recovery case below requires exact temporary discard after the held failure releases.
// Unix instead preserves a recoverable orphan, as verified by nara_fs filesystem_contract.
#[cfg(windows)]
#[test]
fn terminal_failed_image_metadata_keeps_cleanup_custody_after_session_drop() {
    for existing_metadata in [false, true] {
        assert_abandoned_metadata_cleanup(existing_metadata, false, false);
    }
}

#[cfg(windows)]
#[test]
fn image_metadata_cleanup_keeps_runtime_retained_after_tasks_close_and_session_drop() {
    assert_abandoned_metadata_cleanup(true, true, false);
}

#[cfg(windows)]
#[test]
fn panic_after_image_metadata_publication_preserves_cleanup_through_session_drop() {
    assert_abandoned_metadata_cleanup(true, false, true);
}

#[cfg(windows)]
fn assert_abandoned_metadata_cleanup(
    existing_metadata: bool,
    close_before_drop: bool,
    panic_after_publication: bool,
) {
    let project = TestProject::new("image-publication-custody");
    let textures = project.root.join("assets/textures");
    fs::create_dir_all(&textures).unwrap();
    let source = textures.join("imported.png");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        &source,
    )
    .unwrap();
    let metadata = textures.join("imported.png.meta");
    if existing_metadata {
        let original = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("11c0ecad-5dba-4c1a-80f4-1bc93da1a506").unwrap(),
                AssetPath::new("textures/imported.png").unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::default(),
        );
        fs::write(&metadata, original.to_json_string().unwrap()).unwrap();
    }
    let source_bytes = fs::read(&source).unwrap();
    let metadata_bytes = fs::read(&metadata).ok();
    let scene = project.root.join("scenes/startup.scene.json");
    let scene_bytes = fs::read(&scene).unwrap();

    // Declare the atomic-only guard first: unwinding drops the session before releasing cleanup.
    let guard;
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let entries = directory_entries(&textures);
    let workspace = editor.workspace_model();
    let frame = editor.current_frame();
    guard = CleanupGuard::new(editor.test_fail_next_image_metadata_after_write());
    if panic_after_publication {
        editor.test_panic_after_image_metadata_publication();
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        editor.request_editor_image_import(
            frame.revision(),
            "assets/textures/imported.png",
            ImageImportSettings::new(ImageColorSpace::Linear),
        )
    }));
    if panic_after_publication {
        assert!(
            outcome.is_err(),
            "the actual post-publication seam must unwind"
        );
    } else {
        assert!(matches!(
            outcome.unwrap(),
            editor::EditorImageImportRequestResult::Completed(
                editor::EditorImageImportState::Failed {
                    failure: editor::EditorImageImportFailure::CleanupPending,
                    ..
                }
            )
        ));
    }
    assert_eq!(editor.workspace_model(), workspace);
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(fs::read(&metadata).ok(), metadata_bytes);
    assert_eq!(fs::read(&scene).unwrap(), scene_bytes);
    let pending_entries = directory_entries(&textures);
    let temporary = pending_entries
        .iter()
        .filter(|path| !entries.contains(path))
        .collect::<Vec<_>>();
    assert_eq!(pending_entries.len(), entries.len() + 1);
    assert_eq!(temporary.len(), 1);
    assert!(!fs::read(temporary[0]).unwrap().is_empty());

    if close_before_drop {
        let deadline = Instant::now() + Duration::from_secs(5);
        let close = loop {
            let close = editor.advance_close();
            if close.task_shutdown().is_complete() {
                break close;
            }
            assert!(
                Instant::now() < deadline,
                "task shutdown did not settle: {close:#?}"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(close.task_shutdown().is_complete());
        assert!(matches!(
            close,
            editor::EditorProjectCloseProgress::Incomplete {
                file_cleanup_pending: true,
                ..
            }
        ));
        assert!(matches!(
            editor.advance_close(),
            editor::EditorProjectCloseProgress::Incomplete {
                file_cleanup_pending: true,
                ..
            }
        ));
        assert_eq!(directory_entries(&textures), pending_entries);
    }

    drop(editor);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        guard.retained_before + 1
    );
    for _ in 0..3 {
        assert_eq!(
            drive_runtime_quarantine().current_thread_retained(),
            guard.retained_before + 1
        );
        assert_eq!(directory_entries(&textures), pending_entries);
        assert_eq!(fs::read(&metadata).ok(), metadata_bytes);
    }
    guard.release_and_drain();
    assert_eq!(directory_entries(&textures), entries);
    assert!(!temporary[0].exists());
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(fs::read(&metadata).ok(), metadata_bytes);
    assert_eq!(fs::read(&scene).unwrap(), scene_bytes);
}

#[cfg(windows)]
fn directory_entries(directory: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

#[cfg(windows)]
struct CleanupGuard {
    control: CleanupTestControl,
    retained_before: usize,
}

#[cfg(windows)]
impl CleanupGuard {
    fn new(control: CleanupTestControl) -> Self {
        Self {
            control,
            retained_before: runtime_quarantine_status().current_thread_retained(),
        }
    }

    fn release_and_drain(&self) {
        self.control.release();
        assert!(
            self.drain(),
            "Image metadata cleanup did not physically retire"
        );
        assert_eq!(
            runtime_quarantine_status().current_thread_retained(),
            self.retained_before
        );
    }

    fn drain(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime_quarantine_status().current_thread_retained() > self.retained_before {
            let _ = drive_runtime_quarantine();
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        true
    }
}

#[cfg(windows)]
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        self.control.release();
        let _ = self.drain();
    }
}
