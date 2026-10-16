use std::{path::Path, sync::Arc};

use nara::tooling::{
    EditorAction, EditorDocumentId, EditorDocumentKind, EditorFrameModel, EditorPlayState,
    EditorProjectBrowserAction, EditorProjectBrowserDiagnosticPhase,
    EditorProjectBrowserDiagnosticView, EditorProjectBrowserSnapshot, EditorProjectBrowserStatus,
    EditorProjectBrowserView, EditorProjectContentView, EditorProjectSourceKind,
    EditorProjectSourceOpenCapability, EditorRuntimeAvailability, EditorRuntimeUnavailableReason,
};

use crate::{EditorApplication, EditorApplicationState};

use super::super::{EditorRunError, NativeEditorCanaryAdvance};
use super::{
    BrowserBaseline, EXTERNAL_PREFAB_PATH, EXTERNAL_SCENE_PATH, EditorGenericOpenCanaryStage,
    LINEAGE_CHANGED_CODE, OpenDocumentsState, RefreshedBrowserState, SCRIPTS_ROOT_PATH,
    TEMP_ROOT_PREFIX,
};

pub(super) fn project_browser_action(
    action: EditorProjectBrowserAction,
) -> NativeEditorCanaryAdvance {
    NativeEditorCanaryAdvance::action(EditorAction::ProjectBrowser(action))
}

pub(super) fn require_current_frame(
    application: &EditorApplication,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<Arc<EditorFrameModel>, EditorRunError> {
    application.current_frame().ok_or_else(|| {
        canary_state_error(
            stage,
            application.state(),
            "the canary workspace has no published Editor frame",
        )
    })
}

pub(super) fn require_project_browser_sidebar_presentation(
    application: &EditorApplication,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    if application.project_browser_sidebar_selected() {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application.state(),
        "the native frame did not retain the selected Project Browser sidebar",
    ))
}

pub(super) fn require_browser_baseline(
    application: &EditorApplication,
    baseline: &BrowserBaseline,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    let frame = require_current_frame(application, stage)?;
    require_browser_matches_baseline(
        frame.project_browser(),
        baseline,
        application.state(),
        stage,
    )
}

pub(super) fn require_ready_browser_snapshot<'a>(
    browser: &'a EditorProjectBrowserView,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<&'a EditorProjectBrowserSnapshot, EditorRunError> {
    if !matches!(
        browser.status(),
        EditorProjectBrowserStatus::Ready | EditorProjectBrowserStatus::ReadyWithDiagnostics
    ) {
        return Err(canary_state_error(
            stage,
            application_state,
            "the Project Browser did not publish a ready source-index snapshot",
        ));
    }
    browser.snapshot().ok_or_else(|| {
        canary_state_error(
            stage,
            application_state,
            "the ready Project Browser omitted its immutable source-index snapshot",
        )
    })
}

pub(super) fn require_browser_matches_baseline(
    browser: &EditorProjectBrowserView,
    baseline: &BrowserBaseline,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    let snapshot = require_ready_browser_snapshot(browser, application_state, stage)?;
    if browser.status() == baseline.status
        && snapshot.generation() == baseline.generation
        && snapshot.entries() == baseline.entries.as_ref()
        && snapshot.diagnostics() == baseline.diagnostics.as_ref()
        && snapshot.usage() == baseline.usage
    {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application_state,
        "the Project Browser baseline changed before Refresh",
    ))
}

pub(super) fn require_browser_entry(
    snapshot: &EditorProjectBrowserSnapshot,
    relative_path: &str,
    expected_kind: EditorProjectSourceKind,
    expected_capability: EditorProjectSourceOpenCapability,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    if snapshot.entry(relative_path).is_some_and(|entry| {
        entry.kind() == expected_kind && entry.open_capability() == expected_capability
    }) {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application_state,
        "the Project Browser omitted one required closure-external document",
    ))
}

pub(super) fn require_path_filter(
    browser: &EditorProjectBrowserView,
    expected: &str,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    if browser.path_filter() == expected {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application_state,
        "the Project Browser frame did not publish the requested local path filter",
    ))
}

pub(super) fn require_filtered_row(
    browser: &EditorProjectBrowserView,
    expected_path: &str,
    expected_kind: EditorProjectSourceKind,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    if browser.rows().len() == 1
        && browser.rows()[0].relative_path() == expected_path
        && browser.rows()[0].kind() == expected_kind
        && browser.rows()[0].open_capability().is_document()
    {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application_state,
        "the Project Browser local filter did not settle on the expected document row",
    ))
}

pub(super) fn require_selected_path(
    browser: &EditorProjectBrowserView,
    expected_path: &str,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    if browser.selected_path() == Some(expected_path)
        && browser
            .selected_entry()
            .is_some_and(|entry| entry.open_capability().is_document())
    {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application_state,
        "the Project Browser did not publish the selected document before Open",
    ))
}

pub(super) fn require_document(
    frame: &EditorFrameModel,
    source_path: &str,
    kind: EditorDocumentKind,
    active: bool,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<EditorDocumentId, EditorRunError> {
    frame
        .workspace()
        .documents
        .iter()
        .find(|document| {
            document.kind == kind
                && document.active == active
                && document
                    .source_binding
                    .as_ref()
                    .is_some_and(|binding| binding.locator() == source_path)
        })
        .map(|document| document.document)
        .ok_or_else(|| {
            canary_state_error(
                stage,
                application_state,
                "the frame did not retain the expected source-bound document lane",
            )
        })
}

pub(super) fn require_document_id(
    frame: &EditorFrameModel,
    document_id: EditorDocumentId,
    source_path: &str,
    kind: EditorDocumentKind,
    active: bool,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    let observed = require_document(frame, source_path, kind, active, application_state, stage)?;
    if observed == document_id {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application_state,
        "the source-bound document lane changed identity",
    ))
}

pub(super) fn require_open_documents(
    frame: &EditorFrameModel,
    documents: &OpenDocumentsState,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    require_document_id(
        frame,
        documents.scene_document,
        EXTERNAL_SCENE_PATH,
        EditorDocumentKind::Scene,
        false,
        application_state,
        stage,
    )?;
    require_document_id(
        frame,
        documents.prefab_document,
        EXTERNAL_PREFAB_PATH,
        EditorDocumentKind::Prefab,
        true,
        application_state,
        stage,
    )
}

pub(super) fn require_last_good_snapshot(
    browser: &EditorProjectBrowserView,
    refreshed: &RefreshedBrowserState,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    if browser.snapshot().is_some_and(|snapshot| {
        snapshot.generation() == refreshed.generation
            && snapshot.entries() == refreshed.documents.browser.entries.as_ref()
            && snapshot.diagnostics() == refreshed.diagnostics.as_ref()
            && snapshot.usage() == refreshed.usage
    }) {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application_state,
        "the failed Refresh did not retain the last-good source-index snapshot",
    ))
}

pub(super) fn require_safe_lineage_failure(
    failure: Option<&EditorProjectBrowserDiagnosticView>,
    guarded_root: &Path,
    application_state: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    let Some(failure) = failure else {
        return Err(canary_state_error(
            stage,
            application_state,
            "the unavailable Project Browser omitted its safe failure diagnostic",
        ));
    };
    if failure.code() != LINEAGE_CHANGED_CODE
        || failure.phase() != EditorProjectBrowserDiagnosticPhase::LineageRevalidation
        || failure.relative_source() != Some(SCRIPTS_ROOT_PATH)
    {
        return Err(canary_state_error(
            stage,
            application_state,
            "the failed Refresh did not report the expected scripts-root lineage diagnostic",
        ));
    }
    let guarded_root = guarded_root.display().to_string();
    let safe = [
        failure.code(),
        failure.summary(),
        failure.impact(),
        failure.repair_hint(),
        failure.relative_source().unwrap_or_default(),
    ]
    .into_iter()
    .all(|text| {
        !text.contains(&guarded_root)
            && !text.contains(TEMP_ROOT_PREFIX)
            && !text.contains("FileIdentity")
            && !text.contains(":\\")
            && !text.contains(":/")
    });
    if safe {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        application_state,
        "the Project Browser lineage diagnostic exposed native authority or an absolute path",
    ))
}

pub(super) fn ready_status_label(status: EditorProjectBrowserStatus) -> &'static str {
    match status {
        EditorProjectBrowserStatus::Ready => "ready",
        EditorProjectBrowserStatus::ReadyWithDiagnostics => "ready_with_diagnostics",
        EditorProjectBrowserStatus::NotRequested
        | EditorProjectBrowserStatus::Scanning
        | EditorProjectBrowserStatus::Unavailable => "invalid",
    }
}

pub(super) fn require_workspace_contract(
    application: &EditorApplication,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    let state = application.state();
    let frame = application.current_frame().ok_or_else(|| {
        canary_state_error(
            stage,
            state,
            "the repaired workspace has no published Editor frame",
        )
    })?;
    let runtime = frame.runtime_availability();
    let project_content = frame.project_content();
    let play = frame.play();
    let pending_game_start = application.pending_game_start_request().is_some();
    let active_game_start = application.active_game_start_request().is_some();
    if runtime
        == EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::SupervisedPlayContractMissing,
        )
        && project_content == EditorProjectContentView::NotInstalled
        && play.state() == EditorPlayState::Empty
        && play.generation().is_none()
        && !pending_game_start
        && !active_game_start
    {
        return Ok(());
    }
    Err(EditorRunError::Platform(format!(
        "the generic Open/Recovery canary workspace violated the ordinary execution boundary; canary stage={stage:?}, application state={state:?}, runtime={runtime:?}, project content={project_content:?}, play state={:?}, play generation={:?}, pending game start={pending_game_start}, active game start={active_game_start}",
        play.state(),
        play.generation(),
    )))
}

pub(super) fn require_rendered_state(
    actual: EditorApplicationState,
    expected: EditorApplicationState,
    stage: &EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    if actual == expected {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        actual,
        "the native window presented an unexpected product surface",
    ))
}

pub(super) fn canary_state_error(
    stage: &EditorGenericOpenCanaryStage,
    application_state: EditorApplicationState,
    summary: &str,
) -> EditorRunError {
    EditorRunError::Platform(format!(
        "{summary}; canary stage={stage:?}, application state={application_state:?}"
    ))
}
