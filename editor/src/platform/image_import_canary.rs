use std::{
    env,
    ffi::OsStr,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    asset::{AssetVersion, SourceHash, StableAssetId},
    project_host::EditorUiAuthoringFrame,
    tooling::{
        EditorDocumentId, EditorDocumentKind, EditorFrameModel, EditorImageColorSpace,
        EditorProjectBrowserLocalAction,
    },
};
use winit::window::WindowId;

use crate::{EditorApplication, EditorApplicationState, EditorLaunchOptions};

use super::{
    EditorRunError, NativeEditorCanaryAdvance,
    viewport::{EditorUiCanvasImageFact, EditorUiCanvasPresentedObservation},
};

mod contract;
mod fixture;

use contract::{
    canary_state_error, lower_image_brush_action, observe_canvas_image, observe_corrupt_failure,
    observe_import_success, observe_saved, open_prefab_action, repeat_linear_import_action,
    require_browser_ready, require_diagnostics_safe, require_exact_canvas, require_hydrated_detail,
    require_initial_canvas, require_linear_canvas, require_missing_detail, require_reopened_canvas,
    require_workspace_contract, save_action,
};
use fixture::{GuardedImageProject, canary_corrupt_error, canary_prepare_error};

const IMAGE_IMPORT_CANARY_ENV: &str = "NARA_EDITOR_IMAGE_IMPORT_CANARY";
const IMAGE_IMPORT_CANARY_DEADLINE: Duration = Duration::from_secs(55);
const PROJECT_MANIFEST_PATH: &str = "nara.toml";
const AUTHORING_BUNDLE_PATH: &str = "schema/authoring.bundle.json";
const STARTUP_SCENE_PATH: &str = "scenes/startup.scene.json";
const PREFAB_PATH: &str = "prefabs/ui.prefab.json";
const ASSET_SOURCE_PATH: &str = "assets/textures/missing.png";
const ASSET_PATH: &str = "textures/missing.png";
const IMAGE_ENTITY: &str = "image-node";
const TEMP_ROOT_PREFIX: &str = "nara-editor-image-import-canary-";
const SOURCE_MEMBER_BYTE_LIMIT: u64 = 32 * 1024 * 1024;

pub(super) fn requested() -> bool {
    env::var_os(IMAGE_IMPORT_CANARY_ENV).as_deref() == Some(OsStr::new("1"))
}

#[derive(Clone, Debug)]
enum EditorImageImportCanaryStage {
    AwaitingWorkspace,
    AwaitingMissingSelection,
    AwaitingSrgbImport,
    AwaitingPrefabOpen {
        srgb: ImportedImageFact,
    },
    AwaitingSrgbCanvas {
        srgb: ImportedImageFact,
        document: EditorDocumentId,
    },
    AwaitingSave {
        srgb: ImportedImageFact,
        canvas: CanvasImageFact,
        document: EditorDocumentId,
    },
    AwaitingReplacement {
        srgb: ImportedImageFact,
        canvas: CanvasImageFact,
    },
    AwaitingReopenedPrefab {
        srgb: ImportedImageFact,
        predecessor_canvas: CanvasImageFact,
    },
    AwaitingReopenedCanvas {
        srgb: ImportedImageFact,
        predecessor_canvas: CanvasImageFact,
        document: EditorDocumentId,
    },
    AwaitingHydratedSelection {
        srgb: ImportedImageFact,
        reopened_canvas: CanvasImageFact,
        document: EditorDocumentId,
    },
    AwaitingLinearImport {
        srgb: ImportedImageFact,
        reopened_canvas: CanvasImageFact,
        document: EditorDocumentId,
    },
    AwaitingLinearCanvas {
        reopened_canvas: CanvasImageFact,
        linear: ImportedImageFact,
        document: EditorDocumentId,
    },
    AwaitingCorruptFailure {
        linear: ImportedImageFact,
        linear_canvas: CanvasImageFact,
        document: EditorDocumentId,
    },
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImportedImageFact {
    stable_id: StableAssetId,
    version: AssetVersion,
    source_hash: SourceHash,
}

type CanvasImageFact = EditorUiCanvasImageFact;

#[derive(Clone, Copy, Debug)]
struct EditorImageImportCanaryEvidence {
    predecessor_state: EditorApplicationState,
    successor_state: EditorApplicationState,
}

#[derive(Clone, Copy, Debug, Default)]
struct EditorImageImportCanaryEvidenceAccumulator {
    predecessor_state: Option<EditorApplicationState>,
    successor_state: Option<EditorApplicationState>,
}

pub(super) struct EditorImageImportCanaryHost {
    guarded_project: GuardedImageProject,
    profile: Option<String>,
    stage: EditorImageImportCanaryStage,
    evidence: EditorImageImportCanaryEvidenceAccumulator,
    deadline: Instant,
    window_id: Option<WindowId>,
    presented_frames: u64,
}

impl EditorImageImportCanaryHost {
    pub(super) fn new(
        options: EditorLaunchOptions,
    ) -> Result<(EditorApplication, Self), EditorRunError> {
        let source_root = options.project_root().ok_or_else(|| {
            EditorRunError::usage(format!(
                "{IMAGE_IMPORT_CANARY_ENV}=1 requires one source project root"
            ))
        })?;
        let profile = options.profile().map(str::to_owned);
        let guarded_project =
            GuardedImageProject::prepare(source_root).map_err(canary_prepare_error)?;
        let launch =
            EditorLaunchOptions::new(guarded_project.root()).with_optional_profile(profile.clone());
        Ok((
            EditorApplication::new(launch),
            Self {
                guarded_project,
                profile,
                stage: EditorImageImportCanaryStage::AwaitingWorkspace,
                evidence: EditorImageImportCanaryEvidenceAccumulator::default(),
                deadline: Instant::now() + IMAGE_IMPORT_CANARY_DEADLINE,
                window_id: None,
                presented_frames: 0,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_workspace(
        &mut self,
        rendered_state: EditorApplicationState,
        application: &mut EditorApplication,
        frame: &EditorFrameModel,
        ui_authoring: Option<&EditorUiAuthoringFrame>,
        observation: Option<&EditorUiCanvasPresentedObservation<'_>>,
        window_id: WindowId,
        read_only: bool,
    ) -> Result<NativeEditorCanaryAdvance, EditorRunError> {
        self.require_before_deadline(application.state())?;
        self.observe_window(window_id, application.state())?;
        self.presented_frames = self.presented_frames.saturating_add(1);
        let stage = self.stage.clone();
        if !matches!(
            stage,
            EditorImageImportCanaryStage::AwaitingReplacement { .. }
        ) {
            if rendered_state != application.state() || read_only {
                return Err(canary_state_error(
                    &stage,
                    application.state(),
                    "the image canary attempted to edit a stale or read-only workspace",
                ));
            }
            require_workspace_contract(application, &stage)?;
        }
        match stage {
            EditorImageImportCanaryStage::AwaitingWorkspace => {
                if frame.project_browser().has_pending_work() || frame.image_import_work_pending() {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                require_browser_ready(frame, &self.stage, application.state())?;
                if !application.apply_project_browser_local_action(
                    EditorProjectBrowserLocalAction::Select {
                        relative_path: Some(Arc::from(ASSET_SOURCE_PATH)),
                    },
                ) {
                    return Err(canary_state_error(
                        &self.stage,
                        application.state(),
                        "the image canary could not select its PNG source",
                    ));
                }
                self.evidence.predecessor_state = Some(application.state());
                self.stage = EditorImageImportCanaryStage::AwaitingMissingSelection;
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorImageImportCanaryStage::AwaitingMissingSelection => {
                let action = require_missing_detail(frame, &self.stage, application.state())?;
                self.stage = EditorImageImportCanaryStage::AwaitingSrgbImport;
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            EditorImageImportCanaryStage::AwaitingSrgbImport => {
                let Some(srgb) = observe_import_success(
                    frame,
                    EditorImageColorSpace::Srgb,
                    &self.stage,
                    application.state(),
                )?
                else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                self.stage = EditorImageImportCanaryStage::AwaitingPrefabOpen { srgb };
                Ok(NativeEditorCanaryAdvance::action(open_prefab_action()))
            }
            EditorImageImportCanaryStage::AwaitingPrefabOpen { srgb } => {
                let Some(authoring) = ui_authoring else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                require_prefab_document(
                    frame,
                    authoring.document(),
                    &self.stage,
                    application.state(),
                )?;
                let action = lower_image_brush_action(authoring, &self.stage, application.state())?;
                self.stage = EditorImageImportCanaryStage::AwaitingSrgbCanvas {
                    srgb,
                    document: authoring.document(),
                };
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            EditorImageImportCanaryStage::AwaitingSrgbCanvas { srgb, document } => {
                let Some(canvas) = observe_canvas_image(observation, document) else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                let canvas =
                    require_initial_canvas(canvas, &srgb, &self.stage, application.state())?;
                let action = save_action(frame, document, &self.stage, application.state())?;
                self.stage = EditorImageImportCanaryStage::AwaitingSave {
                    srgb,
                    canvas,
                    document,
                };
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            EditorImageImportCanaryStage::AwaitingSave {
                srgb,
                canvas,
                document,
            } => {
                if !observe_saved(frame, document, &self.stage, application.state())? {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                if !application
                    .request_project_replacement(self.guarded_project.root(), self.profile.clone())
                {
                    return Err(canary_state_error(
                        &self.stage,
                        application.state(),
                        "the image canary could not start its same-root replacement",
                    ));
                }
                self.stage = EditorImageImportCanaryStage::AwaitingReplacement { srgb, canvas };
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorImageImportCanaryStage::AwaitingReplacement { srgb, canvas } => match application
                .state()
            {
                EditorApplicationState::OpeningReplacement => Ok(NativeEditorCanaryAdvance::idle()),
                EditorApplicationState::AuthoringReady
                | EditorApplicationState::AuthoringDegraded => {
                    if application.session_close_pending()
                        || frame.project_browser().has_pending_work()
                        || frame.image_import_work_pending()
                    {
                        return Ok(NativeEditorCanaryAdvance::idle());
                    }
                    require_workspace_contract(application, &self.stage)?;
                    require_browser_ready(frame, &self.stage, application.state())?;
                    if frame
                        .workspace()
                        .documents
                        .iter()
                        .any(|document| document.kind == EditorDocumentKind::Prefab)
                        || frame.project_browser().selected_path().is_some()
                    {
                        return Err(canary_state_error(
                            &self.stage,
                            application.state(),
                            "the same-root replacement retained predecessor session-local state",
                        ));
                    }
                    self.evidence.successor_state = Some(application.state());
                    self.stage = EditorImageImportCanaryStage::AwaitingReopenedPrefab {
                        srgb,
                        predecessor_canvas: canvas,
                    };
                    Ok(NativeEditorCanaryAdvance::action(open_prefab_action()))
                }
                EditorApplicationState::Recovery => Err(canary_state_error(
                    &self.stage,
                    application.state(),
                    "the same-root replacement entered Recovery",
                )),
                EditorApplicationState::ProjectManager | EditorApplicationState::OpeningInitial => {
                    Err(canary_state_error(
                        &self.stage,
                        application.state(),
                        "the same-root replacement lost its retained product state",
                    ))
                }
            },
            EditorImageImportCanaryStage::AwaitingReopenedPrefab {
                srgb,
                predecessor_canvas,
            } => {
                let Some(authoring) = ui_authoring else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                require_prefab_document(
                    frame,
                    authoring.document(),
                    &self.stage,
                    application.state(),
                )?;
                self.stage = EditorImageImportCanaryStage::AwaitingReopenedCanvas {
                    srgb,
                    predecessor_canvas,
                    document: authoring.document(),
                };
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorImageImportCanaryStage::AwaitingReopenedCanvas {
                srgb,
                predecessor_canvas,
                document,
            } => {
                let Some(canvas) = observe_canvas_image(observation, document) else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                let reopened_canvas = require_reopened_canvas(
                    canvas,
                    &predecessor_canvas,
                    &self.stage,
                    application.state(),
                )?;
                if !application.apply_project_browser_local_action(
                    EditorProjectBrowserLocalAction::Select {
                        relative_path: Some(Arc::from(ASSET_SOURCE_PATH)),
                    },
                ) {
                    return Err(canary_state_error(
                        &self.stage,
                        application.state(),
                        "the reopened Project Browser could not select the saved image",
                    ));
                }
                self.stage = EditorImageImportCanaryStage::AwaitingHydratedSelection {
                    srgb,
                    reopened_canvas,
                    document,
                };
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorImageImportCanaryStage::AwaitingHydratedSelection {
                srgb,
                reopened_canvas,
                document,
            } => {
                let action =
                    require_hydrated_detail(frame, &srgb, &self.stage, application.state())?;
                self.stage = EditorImageImportCanaryStage::AwaitingLinearImport {
                    srgb,
                    reopened_canvas,
                    document,
                };
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            EditorImageImportCanaryStage::AwaitingLinearImport {
                srgb,
                reopened_canvas,
                document,
            } => {
                let Some(linear) = observe_import_success(
                    frame,
                    EditorImageColorSpace::Linear,
                    &self.stage,
                    application.state(),
                )?
                else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                if linear.stable_id != srgb.stable_id || linear.source_hash != srgb.source_hash {
                    return Err(canary_state_error(
                        &self.stage,
                        application.state(),
                        "the Linear Reimport changed the image source identity",
                    ));
                }
                self.stage = EditorImageImportCanaryStage::AwaitingLinearCanvas {
                    reopened_canvas,
                    linear,
                    document,
                };
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorImageImportCanaryStage::AwaitingLinearCanvas {
                reopened_canvas,
                linear,
                document,
            } => {
                let Some(canvas) = observe_canvas_image(observation, document) else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                let linear_canvas = require_linear_canvas(
                    canvas,
                    &linear,
                    &reopened_canvas,
                    &self.stage,
                    application.state(),
                )?;
                self.guarded_project
                    .corrupt_import_source()
                    .map_err(canary_corrupt_error)?;
                let action =
                    repeat_linear_import_action(frame, &linear, &self.stage, application.state())?;
                self.stage = EditorImageImportCanaryStage::AwaitingCorruptFailure {
                    linear,
                    linear_canvas,
                    document,
                };
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            EditorImageImportCanaryStage::AwaitingCorruptFailure {
                linear,
                linear_canvas,
                document,
            } => {
                if observe_corrupt_failure(frame, &linear, &self.stage, application.state())?
                    .is_none()
                {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                let Some(canvas) = observe_canvas_image(observation, document) else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                require_exact_canvas(&canvas, &linear_canvas, &self.stage, application.state())?;
                require_diagnostics_safe(
                    frame.diagnostics(),
                    self.guarded_project.root(),
                    &self.stage,
                    application.state(),
                )?;
                if !self.guarded_project.project_rust_was_not_executed() {
                    return Err(canary_state_error(
                        &self.stage,
                        application.state(),
                        "the generic Editor executed or rewrote project Rust",
                    ));
                }
                self.stage = EditorImageImportCanaryStage::Complete;
                Ok(NativeEditorCanaryAdvance::exit())
            }
            EditorImageImportCanaryStage::Complete => Ok(NativeEditorCanaryAdvance::idle()),
        }
    }

    pub(super) const fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn timeout_error(&self, state: EditorApplicationState) -> EditorRunError {
        EditorRunError::Platform(format!(
            "the image import canary timed out in {:?}; application state={state:?}",
            self.stage
        ))
    }

    pub(super) fn finish(self) -> Result<(), EditorRunError> {
        match self.stage {
            EditorImageImportCanaryStage::Complete => {}
            stage => {
                return Err(EditorRunError::Platform(format!(
                    "the native Editor exited before the image import canary completed ({stage:?})"
                )));
            }
        }
        let evidence = EditorImageImportCanaryEvidence {
            predecessor_state: self.evidence.predecessor_state.ok_or_else(|| {
                EditorRunError::Platform(
                    "the image import canary did not retain its predecessor workspace state"
                        .to_owned(),
                )
            })?,
            successor_state: self.evidence.successor_state.ok_or_else(|| {
                EditorRunError::Platform(
                    "the image import canary did not retain its successor workspace state"
                        .to_owned(),
                )
            })?,
        };
        println!(
            "{{\"kind\":\"nara_editor_image_import_canary\",\"version\":1,\"window\":\"single\",\"presentation\":{{\"frames_observed\":{},\"deadline_ms\":55000,\"completed_within_deadline\":true}},\"workspace\":{{\"predecessor\":\"{}\",\"successor\":\"{}\",\"same_root_replacement\":true}},\"image\":{{\"path\":\"{}\",\"metadata\":\"published_canonical\",\"stable_id_consistent\":true,\"initial_import\":\"srgb\",\"reimport\":\"linear\"}},\"canvas\":{{\"initial\":\"presented\",\"reopened\":\"presented\",\"linear_successor\":\"presented\",\"corrupt_failure_retained_exact_last_good\":true}},\"persistence\":{{\"prefab_saved\":true,\"closure_hydrated\":true}},\"failure\":{{\"category\":\"image_processing\",\"public_diagnostic_leakage\":false}},\"runtime\":\"external_host_unavailable\",\"project_content\":\"not_installed\",\"play\":\"empty\",\"project_rust\":\"not_executed\",\"result\":\"ok\"}}",
            self.presented_frames,
            workspace_label(evidence.predecessor_state),
            workspace_label(evidence.successor_state),
            ASSET_PATH,
        );
        Ok(())
    }

    fn require_before_deadline(&self, state: EditorApplicationState) -> Result<(), EditorRunError> {
        if Instant::now() < self.deadline {
            return Ok(());
        }
        Err(self.timeout_error(state))
    }

    fn observe_window(
        &mut self,
        window_id: WindowId,
        state: EditorApplicationState,
    ) -> Result<(), EditorRunError> {
        match self.window_id {
            None => {
                self.window_id = Some(window_id);
                Ok(())
            }
            Some(expected) if expected == window_id => Ok(()),
            Some(_) => Err(canary_state_error(
                &self.stage,
                state,
                "the image import canary changed native window identity",
            )),
        }
    }
}

fn require_prefab_document(
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    stage: &EditorImageImportCanaryStage,
    state: EditorApplicationState,
) -> Result<(), EditorRunError> {
    let tab = frame.workspace().document(document).ok_or_else(|| {
        canary_state_error(stage, state, "the active UI Prefab tab is unavailable")
    })?;
    if tab.kind == EditorDocumentKind::Prefab
        && tab.active
        && tab
            .source_binding
            .as_ref()
            .is_some_and(|binding| binding.locator() == PREFAB_PATH)
    {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        state,
        "the UI authoring frame was not bound to the fixture Prefab",
    ))
}

fn workspace_label(state: EditorApplicationState) -> &'static str {
    match state {
        EditorApplicationState::AuthoringReady => "authoring_ready",
        EditorApplicationState::AuthoringDegraded => "authoring_degraded",
        EditorApplicationState::ProjectManager
        | EditorApplicationState::OpeningInitial
        | EditorApplicationState::OpeningReplacement
        | EditorApplicationState::Recovery => "invalid",
    }
}
