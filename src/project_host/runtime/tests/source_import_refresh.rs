use super::*;

use nara_asset::{AssetImportError, AssetImportLimits, AssetImportSource, SourceAssetImporter};
use nara_core::ByteLimit;
use nara_tooling::{
    EditorSourceAssetImportActionOutcome, EditorSourceAssetImportAttempt,
    EditorSourceAssetImportRejection, EditorSourceAssetMetadataView,
};

struct SettingsDomain;

impl nara_asset::AssetDomain for SettingsDomain {
    type Product = u32;
}

impl SourceAssetImporter for SettingsDomain {
    type Settings = u32;
    const SOURCE_KIND: &'static str = "test.source-refresh";

    fn limits() -> AssetImportLimits {
        let limit = ByteLimit::new(1024).unwrap();
        AssetImportLimits::new(limit, limit, limit)
    }

    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<Self::Product, AssetImportError<E>> {
        Ok(*source
            .metadata()
            .admit::<Self::Settings>()
            .unwrap()
            .settings())
    }
}

impl editor::EditorSourceAssetDomain for SettingsDomain {
    type Domain = Self;

    fn default_settings() -> u32 {
        64
    }
}

#[test]
fn source_metadata_import_freshly_admits_equal_bytes_and_refuses_changed_catalog_content() {
    for changed in [false, true] {
        with_runtime_cleanup(|| {
            let project = TestProject::new("source-import-fresh-admission");
            let stable_id = StableAssetId::try_random().unwrap();
            fs::write(project.root.join("assets/value.blob"), b"source").unwrap();
            let target = project.root.join("assets/value.blob.meta");
            let original = AssetMeta::new(
                AssetMetaIdentity::new(
                    stable_id,
                    AssetPath::new("value.blob").unwrap(),
                    AssetSourceKind::Other(SettingsDomain::SOURCE_KIND.to_owned()),
                ),
                64_u32,
            )
            .to_json_string()
            .unwrap();
            fs::write(&target, &original).unwrap();
            let mut editor = open(&project);
            let root = project.capability();
            let relative = RelativePath::new("assets/value.blob.meta").unwrap();
            let original_identity = root.open_file(&relative).unwrap().identity();
            // Keep the original object named elsewhere so this probe does not rely on ID reuse.
            fs::rename(&target, project.root.join("assets/previous.meta")).unwrap();
            let replacement = if changed {
                format!("{original}\n")
            } else {
                original
            };
            fs::write(&target, &replacement).unwrap();
            assert_ne!(
                root.open_file(&relative).unwrap().identity(),
                original_identity
            );

            let outcome = submit(&mut editor, 128);
            if changed {
                assert_eq!(
                    outcome,
                    EditorSourceAssetImportActionOutcome::Rejected(
                        EditorSourceAssetImportRejection::PublicationUnavailable
                    )
                );
                assert_eq!(fs::read_to_string(&target).unwrap(), replacement);
            } else {
                assert_eq!(outcome, EditorSourceAssetImportActionOutcome::Started);
                wait_for_preparation(&mut editor);
                drive_image_import_host_until_idle(&mut editor);
                let candidate = nara_asset::AssetMetaIdentityCandidate::decode_json_bytes(
                    &fs::read(&target).unwrap(),
                )
                .unwrap();
                assert_eq!(candidate.identity().stable_id(), stable_id);
                assert_eq!(*candidate.admit::<u32>().unwrap().settings(), 128);
            }
        });
    }
}

#[test]
fn source_metadata_publication_rejects_current_frame_reimport_until_its_catalog_refreshes() {
    with_runtime_cleanup(|| {
        let project = TestProject::new("source-import-refresh");
        let stable_id = StableAssetId::try_random().unwrap();
        fs::write(project.root.join("assets/value.blob"), b"source").unwrap();
        let target = project.root.join("assets/value.blob.meta");
        fs::write(
            &target,
            AssetMeta::new(
                AssetMetaIdentity::new(
                    stable_id,
                    AssetPath::new("value.blob").unwrap(),
                    AssetSourceKind::Other(SettingsDomain::SOURCE_KIND.to_owned()),
                ),
                64_u32,
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let mut editor = open(&project);
        let before_index = editor.project_source_index_snapshot().unwrap();
        let gate = editor.hold_next_source_index_scan(
            crate::project_host::authoring::ScanTestPhase::BeforeBegin,
        );
        assert_eq!(
            submit(&mut editor, 128),
            EditorSourceAssetImportActionOutcome::Started
        );
        wait_for_preparation(&mut editor);
        assert!(Arc::ptr_eq(
            &before_index,
            &editor.project_source_index_snapshot().unwrap()
        ));
        let published = fs::read(&target).unwrap();
        let repeated = submit(&mut editor, 256);

        editor.drive_editor_frame(Duration::ZERO);
        assert!(gate.wait_until_started());
        gate.release();
        wait_for_preparation(&mut editor);
        drive_image_import_host_until_idle(&mut editor);
        assert_eq!(
            repeated,
            EditorSourceAssetImportActionOutcome::Rejected(
                EditorSourceAssetImportRejection::Unavailable
            )
        );
        assert_eq!(fs::read(&target).unwrap(), published);
        let frame = editor.current_frame();
        let view = frame.source_asset_import().unwrap();
        assert_eq!(
            view.attempt,
            EditorSourceAssetImportAttempt::MetadataPublished { stable_id }
        );
        assert!(
            matches!(&view.metadata, EditorSourceAssetMetadataView::Ready { settings: Some(settings), .. } if settings.as_ref() == "128")
        );

        assert_eq!(
            submit(&mut editor, 256),
            EditorSourceAssetImportActionOutcome::Started
        );
        wait_for_preparation(&mut editor);
        drive_image_import_host_until_idle(&mut editor);
        let candidate =
            nara_asset::AssetMetaIdentityCandidate::decode_json_bytes(&fs::read(target).unwrap())
                .unwrap();
        assert_eq!(candidate.identity().stable_id(), stable_id);
        assert_eq!(*candidate.admit::<u32>().unwrap().settings(), 256);
    });
}

#[test]
fn failed_source_metadata_refresh_cannot_admit_old_catalog_during_manual_retry() {
    with_runtime_cleanup(|| {
        let project = TestProject::new("source-import-refresh-retry");
        fs::write(project.root.join("assets/value.blob"), b"source").unwrap();
        let mut editor = open(&project);
        let before_index = editor.project_source_index_snapshot().unwrap();
        let failed_scan = editor.hold_next_source_index_scan(
            crate::project_host::authoring::ScanTestPhase::BeforeRevalidation,
        );
        assert_eq!(
            submit(&mut editor, 128),
            EditorSourceAssetImportActionOutcome::Started
        );
        wait_for_preparation(&mut editor);
        editor.drive_editor_frame(Duration::ZERO);
        assert!(failed_scan.wait_until_started());
        fs::rename(
            project.root.join("assets"),
            project.root.join("assets-before-refresh"),
        )
        .unwrap();
        fs::create_dir(project.root.join("assets")).unwrap();
        failed_scan.release();
        drive_image_import_host_until_idle(&mut editor);
        assert!(editor.project_source_index_failure().is_some());
        assert!(!editor.current_frame().source_asset_import_work_pending());
        assert!(!editor.editor_image_import_work_pending());

        fs::remove_dir(project.root.join("assets")).unwrap();
        fs::rename(
            project.root.join("assets-before-refresh"),
            project.root.join("assets"),
        )
        .unwrap();
        let target = project.root.join("assets/value.blob.meta");
        let published = fs::read(&target).unwrap();
        let retry_scan = editor.hold_next_source_index_scan(
            crate::project_host::authoring::ScanTestPhase::BeforeBegin,
        );
        let frame = editor.current_frame();
        let report = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Refresh,
                )],
            ))
            .unwrap();
        assert!(matches!(
            report.outcomes(),
            [EditorActionOutcome::ProjectBrowser(
                EditorProjectBrowserActionOutcome::RefreshRequested
            )]
        ));
        editor.drive_editor_frame(Duration::ZERO);
        assert!(retry_scan.wait_until_started());
        let deadline = Instant::now() + Duration::from_secs(5);
        while editor.editor_image_import_work_pending() {
            assert!(
                Instant::now() < deadline,
                "retained catalog did not settle during held retry"
            );
            editor.drive_editor_frame(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(Arc::ptr_eq(
            &before_index,
            &editor.project_source_index_snapshot().unwrap()
        ));
        let repeated = submit(&mut editor, 256);
        retry_scan.release();
        wait_for_preparation(&mut editor);
        drive_image_import_host_until_idle(&mut editor);
        assert_eq!(
            repeated,
            EditorSourceAssetImportActionOutcome::Rejected(
                EditorSourceAssetImportRejection::Unavailable
            )
        );
        assert_eq!(fs::read(&target).unwrap(), published);
        let stable_id = nara_asset::AssetMetaIdentityCandidate::decode_json_bytes(&published)
            .unwrap()
            .identity()
            .stable_id();
        assert_eq!(
            editor
                .current_frame()
                .source_asset_import()
                .unwrap()
                .attempt,
            EditorSourceAssetImportAttempt::MetadataPublished { stable_id }
        );
        assert_eq!(
            submit(&mut editor, 256),
            EditorSourceAssetImportActionOutcome::Started
        );
        wait_for_preparation(&mut editor);
        drive_image_import_host_until_idle(&mut editor);
        let candidate =
            nara_asset::AssetMetaIdentityCandidate::decode_json_bytes(&fs::read(target).unwrap())
                .unwrap();
        assert_eq!(candidate.identity().stable_id(), stable_id);
        assert_eq!(*candidate.admit::<u32>().unwrap().settings(), 256);
    });
}

fn with_runtime_cleanup(test: impl FnOnce()) {
    let result = catch_unwind(AssertUnwindSafe(test));
    let deadline = Instant::now() + Duration::from_secs(10);
    while runtime_quarantine_status().current_thread_retained() != 0 && Instant::now() < deadline {
        let _ = drive_runtime_quarantine();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(runtime_quarantine_status().current_thread_retained(), 0);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn open(project: &TestProject) -> editor::EditorProjectSession {
    let mut importers = editor::EditorSourceAssetImporters::new();
    importers.add::<SettingsDomain>().unwrap();
    let mut editor = open_generic_editor_with_host(
        project,
        editor::EditorProjectOpenHost::new().with_source_asset_importers(importers),
    );
    drive_image_import_host_until_idle(&mut editor);
    editor
}

fn submit(
    editor: &mut editor::EditorProjectSession,
    value: u32,
) -> EditorSourceAssetImportActionOutcome {
    let frame = editor.current_frame();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::SourceAssetImport(
                <SettingsDomain as editor::EditorSourceAssetDomain>::import_action(
                    "assets/value.blob",
                    &value,
                )
                .unwrap(),
            )],
        ))
        .unwrap();
    let [EditorActionOutcome::SourceAssetImport(outcome)] = report.outcomes() else {
        panic!("unexpected source import outcome: {:?}", report.outcomes());
    };
    *outcome
}

fn wait_for_preparation(editor: &mut editor::EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if !frame.source_asset_import_work_pending() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "source preparation did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
