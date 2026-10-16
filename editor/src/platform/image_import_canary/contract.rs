use std::{path::Path, sync::Arc};

use nara::{
    diagnostic::DiagnosticReport,
    image::ImageColorSpace,
    project_host::EditorUiAuthoringFrame,
    scene::SceneEntityId,
    tooling::{
        EditorAction, EditorFrameModel, EditorImageColorSpace, EditorImageImportAttemptView,
        EditorImageImportAvailabilityView, EditorImageImportFailureReason,
        EditorImageImportFreshness, EditorImageImportMetadataView, EditorImageImportSettings,
        EditorPersistenceAction, EditorPersistenceResult, EditorPlayState,
        EditorProjectBrowserAction, EditorProjectBrowserStatus, EditorProjectContentView,
        EditorProjectSourceInspectReason, EditorProjectSourceKind,
        EditorProjectSourceOpenCapability, EditorRuntimeAvailability,
        EditorRuntimeUnavailableReason,
    },
    ui::__private::{UiAuthoringFieldKind, UiAuthoringFieldValue, UiBrushAuthoringKind},
};

use crate::{EditorApplication, EditorApplicationState};

use super::super::{EditorRunError, viewport::EditorUiCanvasPresentedObservation};
use super::{
    ASSET_PATH, ASSET_SOURCE_PATH, CanvasImageFact, EditorImageImportCanaryStage, IMAGE_ENTITY,
    ImportedImageFact, PREFAB_PATH, TEMP_ROOT_PREFIX,
};

pub(super) fn require_workspace_contract(
    application: &EditorApplication,
    stage: &EditorImageImportCanaryStage,
) -> Result<(), EditorRunError> {
    let state = application.state();
    if !matches!(
        state,
        EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded
    ) {
        return Err(canary_state_error(
            stage,
            state,
            "the image canary expected one published generic workspace",
        ));
    }
    let frame = application.current_frame().ok_or_else(|| {
        canary_state_error(
            stage,
            state,
            "the image canary workspace omitted its Editor frame",
        )
    })?;
    let runtime = frame.runtime_availability();
    let project_content = frame.project_content();
    let play = frame.play();
    if runtime
        == EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::SupervisedPlayContractMissing,
        )
        && project_content == EditorProjectContentView::NotInstalled
        && play.state() == EditorPlayState::Empty
        && play.generation().is_none()
        && application.pending_game_start_request().is_none()
        && application.active_game_start_request().is_none()
    {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        state,
        "the image canary crossed the ordinary Editor execution boundary",
    ))
}

pub(super) fn require_browser_ready(
    frame: &EditorFrameModel,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<(), EditorRunError> {
    let browser = frame.project_browser();
    if browser.has_pending_work() || frame.image_import_work_pending() {
        return Err(canary_state_error(
            stage,
            state,
            "the image canary observed pending Browser or image work as ready",
        ));
    }
    if !matches!(
        browser.status(),
        EditorProjectBrowserStatus::Ready | EditorProjectBrowserStatus::ReadyWithDiagnostics
    ) {
        return Err(canary_state_error(
            stage,
            state,
            "the image canary Project Browser did not publish a ready snapshot",
        ));
    }
    let snapshot = browser.snapshot().ok_or_else(|| {
        canary_state_error(
            stage,
            state,
            "the image canary ready Browser omitted its immutable snapshot",
        )
    })?;
    let source = snapshot.entry(ASSET_SOURCE_PATH).ok_or_else(|| {
        canary_state_error(
            stage,
            state,
            "the image canary Browser omitted the imported PNG",
        )
    })?;
    if source.kind() != EditorProjectSourceKind::AssetSource
        || source.open_capability()
            != EditorProjectSourceOpenCapability::InspectOnly(
                EditorProjectSourceInspectReason::ReadOnlySource,
            )
    {
        return Err(canary_state_error(
            stage,
            state,
            "the imported PNG did not retain its read-only source contract",
        ));
    }
    let prefab = snapshot.entry(PREFAB_PATH).ok_or_else(|| {
        canary_state_error(
            stage,
            state,
            "the image canary Browser omitted the UI Prefab",
        )
    })?;
    if prefab.kind() != EditorProjectSourceKind::Prefab
        || prefab.open_capability() != EditorProjectSourceOpenCapability::Prefab
    {
        return Err(canary_state_error(
            stage,
            state,
            "the UI Prefab did not retain its document-open contract",
        ));
    }
    Ok(())
}

pub(super) fn require_missing_detail(
    frame: &EditorFrameModel,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<EditorAction, EditorRunError> {
    let detail = require_selected_detail(frame, stage, state)?;
    if detail.metadata() != EditorImageImportMetadataView::Missing
        || detail.availability() != EditorImageImportAvailabilityView::Unavailable
        || detail.attempt() != EditorImageImportAttemptView::Idle
        || !detail.can_submit()
        || frame.image_import_work_pending()
    {
        return Err(canary_state_error(
            stage,
            state,
            "the selected PNG did not publish the Missing import contract",
        ));
    }
    Ok(EditorAction::ImageImport(detail.submit_action(
        EditorImageImportSettings::new(EditorImageColorSpace::Srgb),
    )))
}

pub(super) fn observe_import_success(
    frame: &EditorFrameModel,
    expected: EditorImageColorSpace,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<Option<ImportedImageFact>, EditorRunError> {
    let detail = require_selected_detail(frame, stage, state)?;
    if frame.image_import_work_pending()
        || matches!(
            detail.attempt(),
            EditorImageImportAttemptView::Pending { .. }
        )
    {
        return Ok(None);
    }
    if !matches!(
        detail.attempt(),
        EditorImageImportAttemptView::Succeeded { .. }
    ) {
        return Err(canary_state_error(
            stage,
            state,
            "the explicit image import did not reach Succeeded",
        ));
    }
    let EditorImageImportMetadataView::Ready {
        stable_id: metadata_id,
        committed_settings,
    } = detail.metadata()
    else {
        return Err(canary_state_error(
            stage,
            state,
            "the successful image import did not publish canonical metadata",
        ));
    };
    let EditorImageImportAvailabilityView::LastGood {
        stable_id,
        version,
        source_hash,
        active_settings,
        dimensions,
        freshness,
    } = detail.availability()
    else {
        return Err(canary_state_error(
            stage,
            state,
            "the successful image import did not publish last-good availability",
        ));
    };
    if stable_id != metadata_id
        || committed_settings.color_space() != expected
        || active_settings.color_space() != expected
        || freshness != EditorImageImportFreshness::Fresh
        || dimensions.width() != 1
        || dimensions.height() != 1
    {
        return Err(canary_state_error(
            stage,
            state,
            "the successful image import published inconsistent product facts",
        ));
    }
    Ok(Some(ImportedImageFact {
        stable_id,
        version,
        source_hash,
    }))
}

pub(super) fn require_hydrated_detail(
    frame: &EditorFrameModel,
    expected: &ImportedImageFact,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<EditorAction, EditorRunError> {
    let detail = require_selected_detail(frame, stage, state)?;
    let EditorImageImportMetadataView::Ready {
        stable_id,
        committed_settings,
    } = detail.metadata()
    else {
        return Err(canary_state_error(
            stage,
            state,
            "the reopened image did not retain canonical metadata",
        ));
    };
    let EditorImageImportAvailabilityView::LastGood {
        stable_id: active_id,
        source_hash,
        active_settings,
        freshness,
        ..
    } = detail.availability()
    else {
        return Err(canary_state_error(
            stage,
            state,
            "the reopened image was not restored from the authoring closure",
        ));
    };
    if stable_id != expected.stable_id
        || active_id != expected.stable_id
        || source_hash != expected.source_hash
        || committed_settings.color_space() != EditorImageColorSpace::Srgb
        || active_settings.color_space() != EditorImageColorSpace::Srgb
        || freshness != EditorImageImportFreshness::SourceUnverified
        || !detail.can_submit()
        || frame.image_import_work_pending()
    {
        return Err(canary_state_error(
            stage,
            state,
            "the reopened image hydration facts did not match the saved source",
        ));
    }
    Ok(EditorAction::ImageImport(detail.submit_action(
        EditorImageImportSettings::new(EditorImageColorSpace::Linear),
    )))
}

pub(super) fn observe_corrupt_failure(
    frame: &EditorFrameModel,
    expected: &ImportedImageFact,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<Option<()>, EditorRunError> {
    let detail = require_selected_detail(frame, stage, state)?;
    if frame.image_import_work_pending()
        || matches!(
            detail.attempt(),
            EditorImageImportAttemptView::Pending { .. }
        )
    {
        return Ok(None);
    }
    let EditorImageImportAttemptView::Failed { reason, .. } = detail.attempt() else {
        return Err(canary_state_error(
            stage,
            state,
            "the corrupt-source Reimport did not publish a terminal failure",
        ));
    };
    if reason != EditorImageImportFailureReason::ImageProcessing {
        return Err(canary_state_error(
            stage,
            state,
            "the corrupt-source Reimport reported the wrong safe failure category",
        ));
    }
    let EditorImageImportAvailabilityView::LastGood {
        stable_id,
        version,
        source_hash,
        active_settings,
        ..
    } = detail.availability()
    else {
        return Err(canary_state_error(
            stage,
            state,
            "the corrupt-source Reimport retired the prior last-good image",
        ));
    };
    if stable_id != expected.stable_id
        || version != expected.version
        || source_hash != expected.source_hash
        || active_settings.color_space() != EditorImageColorSpace::Linear
    {
        return Err(canary_state_error(
            stage,
            state,
            "the corrupt-source Reimport changed the prior Linear product",
        ));
    }
    Ok(Some(()))
}

pub(super) fn repeat_linear_import_action(
    frame: &EditorFrameModel,
    expected: &ImportedImageFact,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<EditorAction, EditorRunError> {
    let detail = require_selected_detail(frame, stage, state)?;
    let EditorImageImportMetadataView::Ready {
        stable_id,
        committed_settings,
    } = detail.metadata()
    else {
        return Err(canary_state_error(
            stage,
            state,
            "the Linear image lost its canonical metadata before corrupt Reimport",
        ));
    };
    let EditorImageImportAvailabilityView::LastGood {
        stable_id: active_id,
        version,
        source_hash,
        active_settings,
        ..
    } = detail.availability()
    else {
        return Err(canary_state_error(
            stage,
            state,
            "the Linear image lost its last-good product before corrupt Reimport",
        ));
    };
    if stable_id != expected.stable_id
        || active_id != expected.stable_id
        || version != expected.version
        || source_hash != expected.source_hash
        || committed_settings.color_space() != EditorImageColorSpace::Linear
        || active_settings.color_space() != EditorImageColorSpace::Linear
        || !detail.can_submit()
        || frame.image_import_work_pending()
    {
        return Err(canary_state_error(
            stage,
            state,
            "the corrupt Reimport was not bound to the exact Linear product",
        ));
    }
    Ok(EditorAction::ImageImport(detail.submit_action(
        EditorImageImportSettings::new(EditorImageColorSpace::Linear),
    )))
}

pub(super) fn open_prefab_action() -> EditorAction {
    EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
        relative_path: Arc::from(PREFAB_PATH),
    })
}

pub(super) fn lower_image_brush_action(
    authoring: &EditorUiAuthoringFrame,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<EditorAction, EditorRunError> {
    let entity = SceneEntityId::new(IMAGE_ENTITY)
        .map_err(|_| canary_state_error(stage, state, "the image canary entity ID is invalid"))?;
    let projection = authoring.projection().map_err(|_| {
        canary_state_error(
            stage,
            state,
            "the image canary UI Prefab did not publish a typed projection",
        )
    })?;
    let descriptor = projection
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == &entity
                && field.key().primary_field().as_str() == "background.kind"
                && matches!(field.kind(), UiAuthoringFieldKind::Brush)
        })
        .ok_or_else(|| {
            canary_state_error(
                stage,
                state,
                "the image canary UI node omitted its background brush descriptor",
            )
        })?;
    let mutation = authoring
        .lower_discrete_field_edit(
            descriptor.key(),
            UiAuthoringFieldValue::Brush {
                kind: UiBrushAuthoringKind::Image,
                red: 1.0,
                green: 1.0,
                blue: 1.0,
                alpha: 1.0,
                reference: ASSET_PATH.to_owned(),
            },
        )
        .map_err(|_| {
            canary_state_error(
                stage,
                state,
                "the current authoring image receipt rejected its exact brush edit",
            )
        })?;
    Ok(EditorAction::Mutate(mutation))
}

pub(super) fn save_action(
    frame: &EditorFrameModel,
    document: nara::tooling::EditorDocumentId,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<EditorAction, EditorRunError> {
    let tab = frame.workspace().document(document).ok_or_else(|| {
        canary_state_error(
            stage,
            state,
            "the image canary UI Prefab closed before Save",
        )
    })?;
    if !tab.dirty {
        return Err(canary_state_error(
            stage,
            state,
            "the image brush mutation did not dirty the UI Prefab",
        ));
    }
    Ok(EditorAction::Persistence(EditorPersistenceAction::Save {
        document,
        base_revision: tab.revision,
    }))
}

pub(super) fn observe_saved(
    frame: &EditorFrameModel,
    document: nara::tooling::EditorDocumentId,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<bool, EditorRunError> {
    match frame.persistence().result() {
        Some(EditorPersistenceResult::Saved {
            document: saved, ..
        }) if saved == document => {
            let tab = frame.workspace().document(document).ok_or_else(|| {
                canary_state_error(stage, state, "the saved UI Prefab tab disappeared")
            })?;
            if tab.dirty || tab.saved_revision != tab.revision {
                return Err(canary_state_error(
                    stage,
                    state,
                    "the Save receipt did not advance the exact clean checkpoint",
                ));
            }
            Ok(true)
        }
        Some(
            EditorPersistenceResult::Rejected { .. }
            | EditorPersistenceResult::Failed { .. }
            | EditorPersistenceResult::PersistenceUncertain { .. }
            | EditorPersistenceResult::Saved { .. },
        ) => Err(canary_state_error(
            stage,
            state,
            "the image canary UI Prefab Save did not complete safely",
        )),
        Some(EditorPersistenceResult::Opened { .. }) | None => Ok(false),
    }
}

pub(super) fn observe_canvas_image(
    observation: Option<&EditorUiCanvasPresentedObservation<'_>>,
    document: nara::tooling::EditorDocumentId,
) -> Option<CanvasImageFact> {
    let observation = observation?;
    if observation.stamp().document() != document {
        return None;
    }
    observation
        .images()
        .iter()
        .find(|image| image.id() == ASSET_PATH)
        .cloned()
}

pub(super) fn require_initial_canvas(
    canvas: CanvasImageFact,
    imported: &ImportedImageFact,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<CanvasImageFact, EditorRunError> {
    if canvas.id() == ASSET_PATH
        && canvas.asset_version() == imported.version
        && canvas.source_hash() == imported.source_hash
        && canvas.color_space() == ImageColorSpace::Srgb
    {
        return Ok(canvas);
    }
    Err(canary_state_error(
        stage,
        state,
        "the first native UI Canvas did not use the imported sRGB image",
    ))
}

pub(super) fn require_reopened_canvas(
    canvas: CanvasImageFact,
    predecessor: &CanvasImageFact,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<CanvasImageFact, EditorRunError> {
    if canvas.id() == predecessor.id()
        && canvas.descriptor_hash() == predecessor.descriptor_hash()
        && canvas.source_hash() == predecessor.source_hash()
        && canvas.color_space() == ImageColorSpace::Srgb
    {
        return Ok(canvas);
    }
    Err(canary_state_error(
        stage,
        state,
        "the reopened native UI Canvas did not restore the saved sRGB image",
    ))
}

pub(super) fn require_linear_canvas(
    canvas: CanvasImageFact,
    imported: &ImportedImageFact,
    srgb: &CanvasImageFact,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<CanvasImageFact, EditorRunError> {
    if canvas.id() == ASSET_PATH
        && canvas.asset_version() == imported.version
        && canvas.source_hash() == imported.source_hash
        && canvas.source_hash() == srgb.source_hash()
        && canvas.descriptor_hash() != srgb.descriptor_hash()
        && canvas.color_space() == ImageColorSpace::Linear
    {
        return Ok(canvas);
    }
    Err(canary_state_error(
        stage,
        state,
        "the native UI Canvas did not publish the Linear Reimport successor",
    ))
}

pub(super) fn require_exact_canvas(
    actual: &CanvasImageFact,
    expected: &CanvasImageFact,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<(), EditorRunError> {
    if actual == expected {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        state,
        "the corrupt-source failure changed the exact prior native Canvas image",
    ))
}

pub(super) fn require_diagnostics_safe(
    report: &DiagnosticReport,
    guarded_root: &Path,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<(), EditorRunError> {
    let guarded_root = guarded_root.display().to_string();
    let safe = report.iter().all(|diagnostic| {
        let fields_safe = diagnostic
            .fields()
            .iter()
            .all(|field| safe_text(&field.display_value(), &guarded_root));
        safe_text(diagnostic.code().as_str(), &guarded_root)
            && safe_text(diagnostic.summary().as_str(), &guarded_root)
            && fields_safe
    });
    if safe {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        state,
        "the image import failure diagnostics exposed native or raw source facts",
    ))
}

fn require_selected_detail<'a>(
    frame: &'a EditorFrameModel,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<&'a nara::tooling::EditorImageImportView, EditorRunError> {
    let detail = frame.image_import().ok_or_else(|| {
        canary_state_error(
            stage,
            state,
            "the selected PNG omitted its image import detail",
        )
    })?;
    if detail.relative_path() == ASSET_SOURCE_PATH {
        return Ok(detail);
    }
    Err(canary_state_error(
        stage,
        state,
        "the image import detail targeted a different Browser selection",
    ))
}

fn safe_text(text: &str, guarded_root: &str) -> bool {
    !text.contains(guarded_root)
        && !text.contains(TEMP_ROOT_PREFIX)
        && !text.contains("FileIdentity")
        && !text.contains(":\\")
        && !text.contains(":/")
        && !text.contains("asset_meta")
        && !text.contains("color_space")
}

pub(super) fn canary_state_error(
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
    summary: &str,
) -> EditorRunError {
    EditorRunError::Platform(format!(
        "{summary}; image canary stage={stage:?}, application state={state:?}"
    ))
}
