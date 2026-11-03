use std::{
    thread,
    time::{Duration, Instant},
};

use nara::{
    asset::{AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId},
    project_host::EditorPrefabExtractionOutcome,
    scene::{PrefabDocument, SceneEntityId, SceneEntityRecord},
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentOpenResult, EditorProjectBrowserAction,
    },
};

use super::*;
use crate::{EditorApplicationState, EditorLaunchOptions, app::PersistenceFixture};

fn wait(application: &mut EditorApplication, ready: impl Fn(&EditorApplication) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        application.advance(Duration::ZERO);
        if ready(application) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the extraction test operation did not settle: {:?}",
            application.prefab_extraction_diagnostics()
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn queued_confirmation_cannot_confirm_a_superseding_review() {
    let fixture = PersistenceFixture::authored_upgrade();
    let prefab = fixture
        .project_root()
        .join("blueprints/confirmation.prefab.json");
    let original = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("branch").unwrap(),
    )])
    .to_json_string()
    .unwrap();
    std::fs::write(&prefab, &original).unwrap();
    let mut application = EditorApplication::new(
        EditorLaunchOptions::new(fixture.project_root()).with_profile("desktop"),
    );
    application.native_shell_ready();
    wait(&mut application, |application| {
        application.state() == EditorApplicationState::AuthoringReady
            && application.current_frame().is_some_and(|frame| {
                !frame.authoring_operation_work_pending()
                    && !frame.project_browser().has_pending_work()
            })
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let frame = application.current_frame().unwrap();
        let startup = frame.workspace().active_document.unwrap();
        application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Open {
                        relative_path: "blueprints/confirmation.prefab.json".into(),
                    },
                )],
            ),
            Duration::ZERO,
        );
        wait(&mut application, |application| {
            matches!(
                application
                    .current_frame()
                    .unwrap()
                    .document_open()
                    .result(),
                Some(EditorDocumentOpenResult::Opened { .. })
            )
        });
        let frame = application.current_frame().unwrap();
        let EditorDocumentOpenResult::Opened { document, request } =
            frame.document_open().result().unwrap()
        else {
            panic!("actual Prefab Open")
        };
        application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::AcknowledgeOpen { request },
                )],
            ),
            Duration::ZERO,
        );
        wait(&mut application, |application| {
            let frame = application.current_frame().unwrap();
            !frame.image_import_work_pending()
                && !frame.source_asset_import_work_pending()
                && !frame.scene_preview_work_pending()
                && !frame.project_browser().has_pending_work()
        });
        let operation = application
            .begin_prefab_extraction(EditorPrefabExtractionRequest::new(
                document,
                SceneEntityId::new("branch").unwrap(),
                AssetMetaIdentity::new(
                    StableAssetId::try_random().unwrap(),
                    AssetPath::new("confirmation-extracted.prefab.json").unwrap(),
                    AssetSourceKind::Prefab,
                ),
            ))
            .unwrap();
        wait(&mut application, |application| {
            application.prefab_extraction_view().unwrap().phase
                == EditorPrefabExtractionPhase::Review
        });
        let original_frame = application.current_frame().unwrap().revision();
        let queued = ExtractionAction {
            generation: application.workspace_generation(),
            command: ExtractionCommand::Confirm(operation, original_frame),
        };
        application
            .revise_prefab_extraction(operation, vec![startup])
            .unwrap();
        wait(&mut application, |application| {
            application.prefab_extraction_view().unwrap().phase
                == EditorPrefabExtractionPhase::Review
        });
        assert_ne!(
            application.current_frame().unwrap().revision(),
            original_frame
        );
        let mut ui = ExtractionUi::default();
        ui.apply(&mut application, queued);
        assert_eq!(
            application.prefab_extraction_view().unwrap().phase,
            EditorPrefabExtractionPhase::Review
        );
        assert!(ui.notice.as_ref().unwrap().contains("changed"));
        assert_eq!(std::fs::read(&prefab).unwrap(), original.as_bytes());
        assert!(
            !fixture
                .project_root()
                .join("blueprints/confirmation-extracted.prefab.json")
                .exists()
        );
        let current = ExtractionAction {
            generation: application.workspace_generation(),
            command: ExtractionCommand::Confirm(
                operation,
                application.current_frame().unwrap().revision(),
            ),
        };
        ui.apply(&mut application, current);
        wait(&mut application, |application| {
            application.prefab_extraction_view().unwrap().phase
                == EditorPrefabExtractionPhase::Terminal
        });
        assert_eq!(
            application.prefab_extraction_view().unwrap().outcome,
            Some(EditorPrefabExtractionOutcome::Adopted)
        );
    }));
    if let Ok(view) = application.prefab_extraction_view() {
        if view.can_cancel() {
            application
                .cancel_prefab_extraction(view.operation.unwrap())
                .unwrap();
        }
    }
    application.request_exit();
    wait(&mut application, |application| {
        application.current_frame().is_none()
    });
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
