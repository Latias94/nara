//! Application lifecycle uses the Session's real unpublished extraction and physical fence.

#[path = "prefab_extraction/selection.rs"]
mod selection;

use nara::{
    asset::{AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId},
    project_host::{
        EditorPrefabExtractionId, EditorPrefabExtractionOutcome, EditorPrefabExtractionPhase,
        EditorPrefabExtractionRequest,
    },
    scene::PrefabDocument,
    tooling::EditorCloseDecision,
};

use super::*;

const PREFAB: &str = "blueprints/external.prefab.json";
const WAIT: Duration = Duration::from_secs(15);

fn fixture() -> PersistenceFixture {
    let fixture = PersistenceFixture::authored_upgrade();
    let prefab = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("selected").unwrap()),
        SceneEntityRecord::new(SceneEntityId::new("outside").unwrap()),
    ]);
    fs::write(fixture.root.join(PREFAB), prefab.to_json_string().unwrap()).unwrap();
    fixture
}

fn review(
    application: &mut EditorApplication,
    save: Option<EditorDocumentId>,
) -> EditorPrefabExtractionId {
    settle_background_imports(application);
    let frame = application.current_frame().unwrap();
    let dispatched = application.dispatch_editor_actions(EditorActionBatch::new(
        frame.revision(),
        [EditorAction::ProjectBrowser(
            EditorProjectBrowserAction::Open {
                relative_path: PREFAB.into(),
            },
        )],
    ));
    assert!(matches!(
        dispatched.feedback().unwrap().unwrap().outcomes(),
        [EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::Preparing { .. }
        )]
    ));
    let deadline = Instant::now() + WAIT;
    let (request, document) = loop {
        application.advance(Duration::ZERO);
        if let Some(EditorDocumentOpenResult::Opened { request, document }) = application
            .current_frame()
            .unwrap()
            .document_open()
            .result()
        {
            break (request, document);
        }
        assert!(Instant::now() < deadline, "the real Prefab did not open");
        std::thread::sleep(Duration::from_millis(1));
    };
    let frame = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::AcknowledgeOpen { request },
            )],
        ),
        Duration::ZERO,
    );
    settle_background_imports(application);
    let mut request = EditorPrefabExtractionRequest::new(
        document,
        SceneEntityId::new("selected").unwrap(),
        AssetMetaIdentity::new(
            StableAssetId::parse_str("1646c498-2830-4d3a-af0d-61a33553d472").unwrap(),
            AssetPath::new("extracted.prefab.json").unwrap(),
            AssetSourceKind::Prefab,
        ),
    );
    request.save_current.extend(save);
    let operation = application.begin_prefab_extraction(request).unwrap();
    let deadline = Instant::now() + WAIT;
    loop {
        let view = application.prefab_extraction_view().unwrap();
        if view.phase == EditorPrefabExtractionPhase::Review {
            assert!(!view.work_pending && view.writes_fenced);
            assert!(
                !application
                    .current_frame()
                    .unwrap()
                    .authoring_operation_work_pending()
            );
            return operation;
        }
        assert_ne!(
            view.phase,
            EditorPrefabExtractionPhase::Terminal,
            "{view:?}; {:?}",
            application.prefab_extraction_diagnostics()
        );
        assert!(
            Instant::now() < deadline,
            "the extraction Review did not return: {view:?}"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn reviewed_extraction_refuses_project_replacement_and_keeps_its_cancel_facade_available() {
    let fixture = fixture();
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    let operation = review(&mut application, None);
    let frame = application.current_frame().unwrap();
    let outputs: Vec<_> = application
        .prefab_extraction_review(operation)
        .unwrap()
        .outputs()
        .map(|output| output.project_path().to_owned())
        .collect();
    assert!(
        outputs
            .iter()
            .any(|path| path == "blueprints/extracted.prefab.json")
    );
    assert!(!application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert!(std::sync::Arc::ptr_eq(
        &frame,
        &application.current_frame().unwrap()
    ));
    application.cancel_prefab_extraction(operation).unwrap();
    assert_eq!(
        application.prefab_extraction_view().unwrap().outcome,
        Some(EditorPrefabExtractionOutcome::Cancelled)
    );
    assert!(!application.prefab_extraction_view().unwrap().writes_fenced);
    assert!(
        !fixture
            .root
            .join("blueprints/extracted.prefab.json")
            .exists()
    );
    close(&mut application);
}

#[test]
fn confirmed_extraction_publishes_its_reviewed_outputs_through_the_application_owner() {
    let fixture = fixture();
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    let operation = review(&mut application, None);
    let original_documents: Vec<_> = application
        .current_frame()
        .unwrap()
        .workspace()
        .documents
        .iter()
        .map(|tab| (tab.document, tab.authoring_schema_basis.clone()))
        .collect();
    let outputs: Vec<_> = application
        .prefab_extraction_review(operation)
        .unwrap()
        .outputs()
        .map(|output| (output.project_path().to_owned(), output.bytes().to_vec()))
        .collect();
    application.confirm_prefab_extraction(operation).unwrap();
    let deadline = Instant::now() + WAIT;
    loop {
        let view = application.prefab_extraction_view().unwrap();
        if view.phase == EditorPrefabExtractionPhase::Terminal {
            assert_eq!(view.outcome, Some(EditorPrefabExtractionOutcome::Adopted));
            assert!(!view.writes_fenced && !view.work_pending);
            break;
        }
        assert_ne!(
            view.phase,
            EditorPrefabExtractionPhase::RequiresReconcile,
            "{view:?}; {:?}",
            application.prefab_extraction_diagnostics(),
        );
        assert!(Instant::now() < deadline, "{view:?}");
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    for (path, bytes) in outputs {
        assert_eq!(fs::read(fixture.root.join(&path)).unwrap(), bytes, "{path}");
    }
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .documents
            .iter()
            .map(|tab| (tab.document, tab.authoring_schema_basis.clone()))
            .collect::<Vec<_>>(),
        original_documents,
        "the actual open set and every published Schema basis remain unchanged; Q stays unopened",
    );
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert!(
        !application
            .current_frame()
            .unwrap()
            .authoring_operation_work_pending()
    );
    close(&mut application);
}

#[test]
fn reviewed_extraction_cancels_before_close_and_exit_resume_the_original_dirty_arbitration() {
    for exit in [false, true] {
        let fixture = fixture();
        let mut application =
            fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
        settle_background_imports(&mut application);
        let dirty = edit_save_source(&mut application);
        let original_disk = fs::read(fixture.root.join("worlds/entry.scene.json")).unwrap();
        let operation = review(&mut application, Some(dirty));
        if exit {
            application.request_exit();
        } else {
            application.submit_project_action(EditorProjectAction::CloseProject);
        }
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(prompt) = application
                .current_frame()
                .unwrap()
                .workspace_intent()
                .decision_prompt()
            {
                let frame = application.current_frame().unwrap();
                application.submit_and_drive(
                    EditorActionBatch::new(
                        frame.revision(),
                        [EditorAction::WorkspaceIntent(
                            prompt.resolve(EditorCloseDecision::Discard),
                        )],
                    ),
                    Duration::ZERO,
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Close did not resume dirty arbitration"
            );
            application.advance(Duration::ZERO);
        }
        if let Ok(view) = application.prefab_extraction_view() {
            assert_eq!(view.operation, Some(operation));
            assert_eq!(view.outcome, Some(EditorPrefabExtractionOutcome::Cancelled));
        }
        let deadline = Instant::now() + WAIT;
        while application.current_frame().is_some() {
            assert!(
                Instant::now() < deadline,
                "Close did not retire the original Session"
            );
            application.advance(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(application.state(), EditorApplicationState::ProjectManager);
        assert_eq!(
            fs::read(fixture.root.join("worlds/entry.scene.json")).unwrap(),
            original_disk
        );
        assert!(
            !fixture
                .root
                .join("blueprints/extracted.prefab.json")
                .exists()
        );
    }
}
