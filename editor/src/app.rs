use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    diagnostic::DiagnosticReport,
    fs::{CapabilityRights, DirectoryCapability, FsError, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorGameInputBatch, EditorGameStartActivation, EditorGameStartRequest,
        EditorGameViewBridgeFailure, EditorGameViewCandidate, EditorGameViewCompletion,
        EditorPreviewCandidate, EditorProjectCloseProgress, EditorProjectOpenHost,
        EditorProjectOpenProgress, EditorUiAuthoringFrame, EditorUiPreviewCandidate,
        TrustedCompiledEditorProjectIntent,
    },
    render::Extent2d,
    scene::UnavailableRecordManifest,
    tooling::{
        EditorAction, EditorActionBatch, EditorActionBatchReport, EditorActionOutcome,
        EditorActionRejection, EditorFrameModel, EditorPersistenceOperation,
        EditorPersistenceResult, EditorPlayAction, EditorProjectBrowserLocalAction,
        EditorWorkspaceIntent, EditorWorkspaceIntentAction, EditorWorkspaceIntentPhase,
        EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
        EditorWorkspaceIntentResult,
    },
};
use thiserror::Error;

const EDITOR_SESSION_CLOSE_RETRY_DELAY: Duration = Duration::from_millis(16);

#[path = "shell.rs"]
mod shell;

use crate::supervised_play::SupervisedPlayWake;
pub use shell::EditorApplicationState;
use shell::{
    EditorProductState, EditorProjectRequest, OpeningProjectState, OpeningProjectStep,
    OpeningReplacementState, ProjectManagerState, PublishedEditorProject, RecoveryState,
};
pub(crate) use shell::{EditorProjectShellAction, EditorProjectShellModel};

/// A project lifecycle request, separate from document mutation and history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorProjectAction {
    OpenProject {
        project_root: PathBuf,
        profile: Option<String>,
    },
    CloseProject,
}

/// Immutable launch configuration for one private Editor process.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditorLaunchOptions {
    project_root: Option<PathBuf>,
    profile: Option<String>,
}

impl EditorLaunchOptions {
    /// Opens an explicitly selected local workspace for cooperative reads and writes.
    ///
    /// The native Host issues `TrustedLocal` filesystem authority on every platform. This does
    /// not authorize project-code execution; ordinary Open remains data-only.
    #[must_use]
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: Some(project_root.into()),
            profile: None,
        }
    }

    #[must_use]
    pub fn project_manager() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    #[must_use]
    pub fn with_optional_profile(mut self, profile: Option<String>) -> Self {
        self.profile = profile;
        self
    }

    #[must_use]
    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Debug, Error)]
pub enum EditorApplicationOpenError {
    #[error("no project root was supplied for the trusted compiled Editor lane")]
    MissingProjectRoot,
    #[error("failed to open project root {path}")]
    ProjectRootOpen {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("project root {path} cannot satisfy the filesystem capability contract")]
    ProjectRootCapability {
        path: PathBuf,
        #[source]
        source: FsError,
    },
    #[error(transparent)]
    Project(#[from] nara::project_host::EditorProjectOpenError),
}

impl EditorApplicationOpenError {
    #[must_use]
    pub fn diagnostics(&self) -> Option<&DiagnosticReport> {
        match self {
            Self::Project(error) => Some(error.diagnostics()),
            Self::MissingProjectRoot
            | Self::ProjectRootOpen { .. }
            | Self::ProjectRootCapability { .. } => None,
        }
    }
}

/// Result of consuming one native redraw through the exact Editor action boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorApplicationUpdate {
    actions_observed: bool,
    exit_ready: bool,
}

impl EditorApplicationUpdate {
    pub(crate) const fn from_action(actions_observed: bool, exit_ready: bool) -> Self {
        Self {
            actions_observed,
            exit_ready,
        }
    }

    #[must_use]
    pub const fn idle() -> Self {
        Self {
            actions_observed: false,
            exit_ready: false,
        }
    }

    #[must_use]
    pub const fn actions_observed(self) -> bool {
        self.actions_observed
    }

    #[must_use]
    pub const fn exit_ready(self) -> bool {
        self.exit_ready
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EditorApplicationDispatch {
    actions_observed: bool,
    invalidates_game_input: bool,
    requests_step: bool,
    requires_runtime_drive: bool,
    feedback: Option<Result<EditorActionBatchReport, EditorActionRejection>>,
    original_slots: Vec<Option<usize>>,
    closing_filter: bool,
}

impl EditorApplicationDispatch {
    pub(crate) fn remap_original_slots(&mut self, slots: &[Option<usize>]) {
        self.original_slots = slots
            .iter()
            .map(|slot| slot.and_then(|slot| self.original_slots.get(slot).copied().flatten()))
            .collect();
    }
    pub(crate) fn original_outcome(&self, slot: usize) -> Option<&EditorActionOutcome> {
        let compact = self.original_slots.get(slot).copied().flatten()?;
        self.feedback
            .as_ref()?
            .as_ref()
            .ok()?
            .outcomes()
            .get(compact)
    }

    pub(crate) fn original_slots(&self) -> &[Option<usize>] {
        &self.original_slots
    }

    pub(crate) const fn closing_filter(&self) -> bool {
        self.closing_filter
    }

    pub(crate) const fn actions_observed(&self) -> bool {
        self.actions_observed
    }

    pub(crate) const fn invalidates_game_input(&self) -> bool {
        self.invalidates_game_input
    }

    pub(crate) const fn requests_step(&self) -> bool {
        self.requests_step
    }

    pub(crate) const fn requires_runtime_drive(&self) -> bool {
        self.requires_runtime_drive
    }

    pub(crate) fn feedback(
        &self,
    ) -> Option<Result<&EditorActionBatchReport, EditorActionRejection>> {
        self.feedback
            .as_ref()
            .map(|feedback| feedback.as_ref().map_err(|rejection| *rejection))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorSessionRetirement {
    CloseProject,
    ExitProcess,
}

/// Product-local orchestration around the standalone shell and one Host-owned Editor session.
pub struct EditorApplication {
    panel_workspace: crate::workspace::EditorPanelWorkspace,
    state: EditorProductState,
    open_host: EditorProjectOpenHost,
    initial_request: Option<EditorProjectRequest>,
    native_ready: bool,
    next_entry_generation: u64,
    next_attempt_generation: u64,
    session_retirement: Option<EditorSessionRetirement>,
    session_close_retry_at: Option<Instant>,
    supervised_play_wake: SupervisedPlayWake,
}

impl std::fmt::Debug for EditorApplication {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorApplication")
            .field("state", &self.state)
            .field("native_ready", &self.native_ready)
            .finish_non_exhaustive()
    }
}

impl EditorApplication {
    /// Constructs the native-independent product shell without touching the project filesystem.
    #[must_use]
    pub fn new(options: EditorLaunchOptions) -> Self {
        Self::new_with_source_asset_importers(options, Default::default())
    }

    pub(crate) fn new_with_source_asset_importers(
        options: EditorLaunchOptions,
        importers: nara::project_host::EditorSourceAssetImporters,
    ) -> Self {
        let initial_request = options
            .project_root
            .clone()
            .map(|root| EditorProjectRequest::new(root, options.profile.clone()));
        let suggested_request = initial_request
            .clone()
            .unwrap_or_else(|| EditorProjectRequest::new(PathBuf::new(), options.profile.clone()));
        Self {
            panel_workspace: crate::workspace::EditorPanelWorkspace::default(),
            state: EditorProductState::ProjectManager(ProjectManagerState::new(
                1,
                suggested_request,
            )),
            open_host: EditorProjectOpenHost::new().with_source_asset_importers(importers),
            initial_request,
            native_ready: false,
            next_entry_generation: 2,
            next_attempt_generation: 1,
            session_retirement: None,
            session_close_retry_at: None,
            supervised_play_wake: SupervisedPlayWake::default(),
        }
    }

    /// Opens the explicitly advanced in-process Editor lane compiled with project Runtime code.
    #[doc(hidden)]
    pub fn open_trusted_compiled(
        options: EditorLaunchOptions,
        intent: TrustedCompiledEditorProjectIntent,
    ) -> Result<Self, EditorApplicationOpenError> {
        Self::open_trusted_compiled_with_source_asset_importers(options, intent, Default::default())
    }

    pub(crate) fn open_trusted_compiled_with_source_asset_importers(
        options: EditorLaunchOptions,
        intent: TrustedCompiledEditorProjectIntent,
        importers: nara::project_host::EditorSourceAssetImporters,
    ) -> Result<Self, EditorApplicationOpenError> {
        let project_root = options
            .project_root
            .clone()
            .ok_or(EditorApplicationOpenError::MissingProjectRoot)?;
        let request = EditorProjectRequest::new(project_root.clone(), options.profile.clone());
        let project_root = open_project_root(&project_root)?;
        let open_host = EditorProjectOpenHost::new().with_source_asset_importers(importers);
        let session = open_host.open_trusted_compiled(project_root, intent)?;
        let supervised_play_wake = SupervisedPlayWake::default();
        Ok(Self {
            panel_workspace: crate::workspace::EditorPanelWorkspace::default(),
            state: EditorProductState::from_published(PublishedEditorProject::new(
                1,
                request,
                session,
                supervised_play_wake.clone(),
            )),
            open_host,
            initial_request: None,
            native_ready: false,
            next_entry_generation: 2,
            next_attempt_generation: 1,
            session_retirement: None,
            session_close_retry_at: None,
            supervised_play_wake,
        })
    }

    #[must_use]
    pub fn state(&self) -> EditorApplicationState {
        self.state.kind()
    }

    #[must_use]
    pub const fn panel_workspace(&self) -> &crate::workspace::EditorPanelWorkspace {
        &self.panel_workspace
    }

    pub fn panel_workspace_mut(&mut self) -> &mut crate::workspace::EditorPanelWorkspace {
        &mut self.panel_workspace
    }

    pub(crate) fn install_supervised_play_wake(&mut self, wake: SupervisedPlayWake) {
        self.supervised_play_wake = wake.clone();
        match &mut self.state {
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => {
                project.supervised_play.set_wake(wake);
            }
            EditorProductState::OpeningReplacement(
                OpeningReplacementState::Preparing { retained, .. }
                | OpeningReplacementState::Refreshing { retained, .. },
            ) => {
                retained.supervised_play.set_wake(wake);
            }
            EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                retained,
                candidate,
                ..
            }) => {
                retained.supervised_play.set_wake(wake.clone());
                candidate.supervised_play.set_wake(wake);
            }
            EditorProductState::ProjectManager(_)
            | EditorProductState::OpeningInitial(_)
            | EditorProductState::Recovery(_) => {}
        }
    }

    #[must_use]
    pub(crate) fn ordinary_input_locked(&self) -> bool {
        self.session_retirement.is_some()
            || self.state.workspace().is_some_and(|project| {
                project
                    .session
                    .current_frame()
                    .workspace_intent()
                    .phase()
                    .is_some()
            })
            || !matches!(
                &self.state,
                EditorProductState::AuthoringReady(_) | EditorProductState::AuthoringDegraded(_)
            )
    }

    /// Arms the initial request after the native window and GPU owner exist.
    #[doc(hidden)]
    pub fn native_shell_ready(&mut self) -> EditorApplicationUpdate {
        if self.native_ready {
            return EditorApplicationUpdate::idle();
        }
        self.native_ready = true;
        let Some(request) = self.initial_request.take() else {
            return EditorApplicationUpdate {
                actions_observed: true,
                exit_ready: false,
            };
        };
        if !matches!(&self.state, EditorProductState::ProjectManager(_)) {
            return EditorApplicationUpdate::idle();
        }
        self.start_initial_open(request);
        EditorApplicationUpdate {
            actions_observed: true,
            exit_ready: false,
        }
    }

    /// Advances at most one bounded project-open phase at the current frame boundary.
    #[doc(hidden)]
    pub fn advance_project_open(&mut self) -> EditorApplicationUpdate {
        let state = self.take_state();
        match state {
            EditorProductState::OpeningInitial(opening) => {
                let (state, progressed) = self.advance_initial_open(opening);
                self.state = state;
                EditorApplicationUpdate {
                    actions_observed: progressed,
                    exit_ready: false,
                }
            }
            EditorProductState::OpeningReplacement(replacement) => {
                let (state, progressed) = self.advance_replacement_open(replacement);
                self.state = state;
                EditorApplicationUpdate {
                    actions_observed: progressed,
                    exit_ready: false,
                }
            }
            other => {
                self.state = other;
                EditorApplicationUpdate::idle()
            }
        }
    }

    /// Projects the current lifecycle without lending project or document authority.
    pub(crate) fn project_shell_model(&self) -> Option<EditorProjectShellModel<'_>> {
        match &self.state {
            EditorProductState::ProjectManager(manager) => {
                Some(EditorProjectShellModel::ProjectManager {
                    entry_generation: manager.entry_generation,
                    suggested_request: &manager.suggested_request,
                })
            }
            EditorProductState::OpeningInitial(opening) => Some(EditorProjectShellModel::Opening {
                entry_generation: opening.entry_generation,
                attempt_generation: opening.attempt_generation,
                request: &opening.request,
                phase: opening.phase_label(),
                summary: opening.summary(),
                replacement: false,
            }),
            EditorProductState::OpeningReplacement(replacement) => {
                Some(EditorProjectShellModel::Opening {
                    entry_generation: replacement.entry_generation(),
                    attempt_generation: replacement.attempt_generation(),
                    request: replacement.request(),
                    phase: replacement.phase_label(),
                    summary: replacement.summary(),
                    replacement: true,
                })
            }
            EditorProductState::Recovery(recovery) => Some(EditorProjectShellModel::Recovery {
                entry_generation: recovery.entry_generation,
                attempt_generation: recovery.attempt_generation,
                request: &recovery.request,
                diagnostics: &recovery.diagnostics,
            }),
            EditorProductState::AuthoringReady(_) | EditorProductState::AuthoringDegraded(_) => {
                None
            }
        }
    }

    pub(crate) fn submit_shell_action(
        &mut self,
        action: EditorProjectShellAction,
    ) -> EditorApplicationUpdate {
        let exit_ready = self.apply_shell_action(action);
        EditorApplicationUpdate::from_action(true, exit_ready)
    }

    fn apply_shell_action(&mut self, action: EditorProjectShellAction) -> bool {
        match action {
            EditorProjectShellAction::OpenProject {
                project_root,
                profile,
            } => {
                let request = EditorProjectRequest::new(project_root, profile);
                let state = self.take_state();
                match state {
                    EditorProductState::ProjectManager(_) | EditorProductState::Recovery(_)
                        if request.project_root().as_os_str().is_empty() =>
                    {
                        let entry_generation = self.allocate_entry_generation();
                        let attempt_generation = self.allocate_attempt_generation();
                        self.enter_recovery(RecoveryState::local_failure(
                            entry_generation,
                            attempt_generation,
                            request,
                            "project.editor.project-path-empty",
                            "A project path is required before opening a project",
                            "Enter a project directory and retry.",
                        ));
                    }
                    EditorProductState::ProjectManager(_) | EditorProductState::Recovery(_) => {
                        self.start_initial_open(request);
                    }
                    other => self.state = other,
                }
                false
            }
            EditorProjectShellAction::Retry => {
                let _ = self.retry_project_open();
                false
            }
            EditorProjectShellAction::Cancel => {
                let state = self.take_state();
                match state {
                    EditorProductState::OpeningInitial(opening) => {
                        let entry_generation = self.allocate_entry_generation();
                        self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                            entry_generation,
                            opening.request,
                        ));
                    }
                    EditorProductState::OpeningReplacement(replacement) => {
                        self.cancel_replacement_open(replacement);
                    }
                    other => self.state = other,
                }
                false
            }
            EditorProjectShellAction::CloseProject => {
                let state = self.take_state();
                match state {
                    EditorProductState::Recovery(_) => {
                        let entry_generation = self.allocate_entry_generation();
                        self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                            entry_generation,
                            EditorProjectRequest::new(PathBuf::new(), None),
                        ));
                    }
                    other => self.state = other,
                }
                false
            }
            EditorProjectShellAction::ReturnToProjectManager => {
                let state = self.take_state();
                match state {
                    EditorProductState::Recovery(recovery) => {
                        let entry_generation = self.allocate_entry_generation();
                        self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                            entry_generation,
                            recovery.request,
                        ));
                    }
                    other => self.state = other,
                }
                false
            }
            EditorProjectShellAction::Exit => true,
        }
    }

    fn cancel_replacement_open(&mut self, replacement: OpeningReplacementState) {
        match replacement {
            OpeningReplacementState::Preparing { retained, .. } => {
                self.state = EditorProductState::from_published(retained);
            }
            OpeningReplacementState::Refreshing {
                mut retained,
                opening,
            } => {
                if cancel_pending_exit(&mut retained) {
                    self.state = EditorProductState::from_published(retained);
                } else {
                    self.state = EditorProductState::OpeningReplacement(
                        OpeningReplacementState::Refreshing { retained, opening },
                    );
                }
            }
            OpeningReplacementState::Closing {
                entry_generation,
                attempt_generation,
                mut retained,
                candidate,
                close_started,
                save_attempted,
            } => {
                if !close_started || cancel_pending_exit(&mut retained) {
                    self.state = EditorProductState::from_published(retained);
                } else {
                    self.state =
                        EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                            entry_generation,
                            attempt_generation,
                            retained,
                            candidate,
                            close_started,
                            save_attempted,
                        });
                }
            }
        }
    }

    /// Applies one project lifecycle action without mixing it into the workspace action batch.
    #[doc(hidden)]
    pub fn submit_project_action(
        &mut self,
        action: EditorProjectAction,
    ) -> EditorApplicationUpdate {
        match action {
            EditorProjectAction::OpenProject {
                project_root,
                profile,
            } => {
                let requested = self
                    .start_project_replacement(EditorProjectRequest::new(project_root, profile));
                EditorApplicationUpdate {
                    actions_observed: requested,
                    exit_ready: false,
                }
            }
            EditorProjectAction::CloseProject => self.request_close_project(),
        }
    }

    /// Starts one bounded replacement attempt while retaining the exact last-good session.
    ///
    /// A rejected candidate or failed close handoff restores the retained session without
    /// publishing partial replacement state.
    pub fn request_project_replacement(
        &mut self,
        project_root: impl Into<PathBuf>,
        profile: Option<String>,
    ) -> bool {
        if self.session_retirement.is_some() {
            return false;
        }
        let request = EditorProjectRequest::new(project_root.into(), profile);
        self.start_project_replacement(request)
    }

    fn start_project_replacement(&mut self, request: EditorProjectRequest) -> bool {
        if self.session_retirement.is_some() {
            return false;
        }
        let state = self.take_state();
        match state {
            EditorProductState::AuthoringReady(retained)
            | EditorProductState::AuthoringDegraded(retained) => {
                let opening = self.new_opening(request);
                self.state =
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Preparing {
                        retained,
                        opening,
                    });
                true
            }
            other => {
                self.state = other;
                false
            }
        }
    }

    /// Retries the exact rejected project request with a fresh attempt generation.
    #[must_use]
    pub fn retry_project_open(&mut self) -> bool {
        let state = self.take_state();
        match state {
            EditorProductState::Recovery(recovery) => {
                self.start_initial_open(recovery.request);
                true
            }
            other => {
                self.state = other;
                false
            }
        }
    }

    #[must_use]
    pub fn current_frame(&self) -> Option<Arc<EditorFrameModel>> {
        self.state
            .published()
            .map(|project| project.session.current_frame())
    }

    #[must_use]
    pub(crate) fn workspace_generation(&self) -> Option<u64> {
        self.state.published().map(|project| project.generation)
    }

    #[must_use]
    pub fn apply_project_browser_local_action(
        &mut self,
        action: EditorProjectBrowserLocalAction,
    ) -> bool {
        self.state
            .workspace_mut()
            .is_some_and(|project| project.session.apply_project_browser_local_action(action))
    }

    /// Returns the complete active preservation manifest for generic authoring verification.
    #[doc(hidden)]
    #[must_use]
    pub fn active_authoring_unavailable_record_manifest(
        &self,
    ) -> Option<UnavailableRecordManifest> {
        self.state.workspace().and_then(|project| {
            project
                .session
                .active_authoring_unavailable_record_manifest()
                .cloned()
        })
    }
    pub fn submit_and_drive(
        &mut self,
        batch: EditorActionBatch,
        real_delta: Duration,
    ) -> EditorApplicationUpdate {
        let dispatch = self.dispatch_editor_actions(batch);
        self.drive_editor_frame(real_delta, dispatch.actions_observed(), true)
    }

    pub(crate) fn dispatch_editor_actions(
        &mut self,
        batch: EditorActionBatch,
    ) -> EditorApplicationDispatch {
        let revision = batch.frame_revision();
        let actions = batch.into_actions();
        let action_count = actions.len();
        let batch = EditorActionBatch::new(revision, actions);
        let retirement_pending = self.session_retirement.is_some();
        match &mut self.state {
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => {
                if retirement_pending
                    || project
                        .session
                        .current_frame()
                        .workspace_intent()
                        .intent()
                        .is_some()
                {
                    dispatch_close_resolution_actions(project, batch).0
                } else {
                    dispatch_project_actions(project, batch)
                }
            }
            EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                retained,
                save_attempted,
                ..
            }) => {
                let (dispatch, attempted_save) = dispatch_close_resolution_actions(retained, batch);
                if attempted_save {
                    *save_attempted = true;
                }
                dispatch
            }
            EditorProductState::ProjectManager(_)
            | EditorProductState::OpeningInitial(_)
            | EditorProductState::OpeningReplacement(OpeningReplacementState::Preparing {
                ..
            })
            | EditorProductState::OpeningReplacement(OpeningReplacementState::Refreshing {
                ..
            })
            | EditorProductState::Recovery(_) => EditorApplicationDispatch {
                original_slots: vec![None; action_count],
                ..Default::default()
            },
        }
    }

    pub(crate) fn drive_editor_frame(
        &mut self,
        real_delta: Duration,
        actions_observed: bool,
        drive_runtime: bool,
    ) -> EditorApplicationUpdate {
        self.drive_editor_frame_at(real_delta, actions_observed, drive_runtime, Instant::now())
    }

    fn drive_editor_frame_at(
        &mut self,
        real_delta: Duration,
        actions_observed: bool,
        drive_runtime: bool,
        now: Instant,
    ) -> EditorApplicationUpdate {
        if let EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
            retained,
            ..
        }) = &mut self.state
        {
            let external_observed = drive_supervised_play_owner(retained, now);
            let frame_observed = drive_project_frame(retained, real_delta, drive_runtime);
            return self.finish_replacement_handoff(
                actions_observed || external_observed || frame_observed,
            );
        }
        match &mut self.state {
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => {
                let external_observed = drive_supervised_play_owner(project, now);
                let frame_observed = drive_project_frame(project, real_delta, drive_runtime);
                let intent = project.session.current_frame().workspace_intent();
                if self.session_retirement.is_none()
                    && (intent.intent() == Some(EditorWorkspaceIntent::Exit)
                        || matches!(
                            intent.result(),
                            Some(EditorWorkspaceIntentResult::Applied {
                                intent: EditorWorkspaceIntent::Exit,
                            })
                        ))
                {
                    self.session_retirement = Some(EditorSessionRetirement::ExitProcess);
                    self.session_close_retry_at = None;
                }
                if self.session_retirement.is_some() {
                    self.finish_session_retirement(
                        actions_observed || external_observed || frame_observed,
                    )
                } else {
                    EditorApplicationUpdate {
                        actions_observed: actions_observed || external_observed || frame_observed,
                        exit_ready: false,
                    }
                }
            }
            EditorProductState::ProjectManager(_)
            | EditorProductState::OpeningInitial(_)
            | EditorProductState::OpeningReplacement(OpeningReplacementState::Preparing {
                ..
            })
            | EditorProductState::OpeningReplacement(OpeningReplacementState::Refreshing {
                ..
            })
            | EditorProductState::OpeningReplacement(OpeningReplacementState::Closing { .. })
            | EditorProductState::Recovery(_) => EditorApplicationUpdate {
                actions_observed,
                exit_ready: false,
            },
        }
    }

    /// Advances retained supervised Play owners without consuming presentation-bound input.
    pub(crate) fn drive_supervised_play_effects(
        &mut self,
        now: Instant,
    ) -> EditorApplicationUpdate {
        match &mut self.state {
            EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                retained,
                ..
            }) => {
                let observed = drive_supervised_play_owner(retained, now);
                if observed {
                    retained
                        .session
                        .drive_editor_frame_with_runtime(Duration::ZERO, false);
                }
                self.finish_replacement_handoff(observed)
            }
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => {
                let observed = drive_supervised_play_owner(project, now);
                if observed {
                    project
                        .session
                        .drive_editor_frame_with_runtime(Duration::ZERO, false);
                }
                if self.session_retirement.is_some() {
                    self.finish_session_retirement(observed)
                } else {
                    EditorApplicationUpdate {
                        actions_observed: observed,
                        exit_ready: false,
                    }
                }
            }
            EditorProductState::OpeningReplacement(
                OpeningReplacementState::Preparing { retained, .. }
                | OpeningReplacementState::Refreshing { retained, .. },
            ) => {
                let observed = drive_supervised_play_owner(retained, now);
                if observed {
                    retained
                        .session
                        .drive_editor_frame_with_runtime(Duration::ZERO, false);
                }
                EditorApplicationUpdate {
                    actions_observed: observed,
                    exit_ready: false,
                }
            }
            _ => self.drive_editor_frame_at(Duration::ZERO, false, false, now),
        }
    }

    pub(crate) fn next_supervised_play_deadline(&self) -> Option<Instant> {
        match &self.state {
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => {
                project.supervised_play.next_deadline()
            }
            EditorProductState::OpeningReplacement(
                OpeningReplacementState::Preparing { retained, .. }
                | OpeningReplacementState::Refreshing { retained, .. },
            ) => retained.supervised_play.next_deadline(),
            EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                retained,
                candidate,
                ..
            }) => earliest_instant(
                retained.supervised_play.next_deadline(),
                candidate.supervised_play.next_deadline(),
            ),
            EditorProductState::ProjectManager(_)
            | EditorProductState::OpeningInitial(_)
            | EditorProductState::Recovery(_) => None,
        }
    }

    pub(crate) fn apply_game_view_input_batch(
        &mut self,
        batch: EditorGameInputBatch,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        self.state
            .published_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?
            .session
            .apply_game_view_input_batch(batch)
    }

    pub(crate) fn drive_gpu_recovery_pause(&mut self) -> EditorApplicationUpdate {
        let Some(project) = self.state.published_mut() else {
            return EditorApplicationUpdate::from_action(false, false);
        };
        let _ = project.session.request_runtime_pause_for_gpu_recovery();
        project
            .session
            .drive_editor_frame_with_runtime(Duration::ZERO, true);
        if project.session.cancel_game_step_presentation() {
            project
                .session
                .drive_editor_frame_with_runtime(Duration::ZERO, false);
        }
        self.drive_editor_frame(Duration::ZERO, false, false)
    }

    pub(crate) fn game_step_requires_runtime_drive(&self) -> bool {
        self.state
            .published()
            .is_some_and(|project| project.session.game_step_requires_runtime_drive())
    }

    pub fn set_scene_preview_extent(&mut self, extent: Option<Extent2d>) {
        if let Some(project) = self.state.workspace_mut() {
            project.session.set_editor_preview_extent(extent);
        }
    }

    #[must_use]
    pub fn take_scene_preview_candidate(&mut self) -> Option<EditorPreviewCandidate> {
        self.state
            .workspace_mut()
            .and_then(|project| project.session.take_editor_preview_candidate())
    }

    #[doc(hidden)]
    #[must_use]
    pub fn take_editor_ui_preview_candidate(&mut self) -> Option<EditorUiPreviewCandidate> {
        self.state
            .workspace_mut()
            .and_then(|project| project.session.take_editor_ui_preview_candidate())
    }

    #[doc(hidden)]
    #[must_use]
    pub fn current_ui_authoring_frame(&self) -> Option<Arc<EditorUiAuthoringFrame>> {
        self.state
            .workspace()
            .and_then(|project| project.session.current_ui_authoring_frame())
    }

    #[doc(hidden)]
    #[must_use]
    pub fn pending_game_start_request(&self) -> Option<EditorGameStartRequest> {
        self.state
            .workspace()
            .and_then(|project| project.session.pending_game_start_request())
    }

    #[doc(hidden)]
    #[must_use]
    pub fn active_game_start_request(&self) -> Option<EditorGameStartRequest> {
        self.state
            .published()
            .and_then(|project| project.session.active_game_start_request())
    }

    #[doc(hidden)]
    pub fn prepare_game_start_activation(
        &mut self,
        expected: EditorGameStartRequest,
    ) -> Result<EditorGameStartActivation, EditorGameViewBridgeFailure> {
        self.state
            .workspace_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?
            .session
            .prepare_game_start_activation(expected)
    }

    #[doc(hidden)]
    pub fn commit_game_start_activation(&mut self, activation: EditorGameStartActivation) {
        if let Some(project) = self.state.workspace_mut() {
            project.session.commit_game_start_activation(activation);
        }
    }

    #[doc(hidden)]
    pub fn abort_pending_game_start(
        &mut self,
        expected: EditorGameStartRequest,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        self.state
            .workspace_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?
            .session
            .abort_pending_game_start(expected)
    }

    #[doc(hidden)]
    pub fn take_game_view_candidate(
        &mut self,
    ) -> Result<Option<EditorGameViewCandidate>, EditorGameViewBridgeFailure> {
        self.state
            .workspace_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?
            .session
            .take_game_view_candidate()
    }

    #[doc(hidden)]
    pub fn complete_game_view_candidate(
        &mut self,
        completion: EditorGameViewCompletion,
    ) -> Result<(), EditorGameViewCompletion> {
        let Some(project) = self.state.workspace_mut() else {
            return Err(completion);
        };
        project.session.complete_game_view_candidate(completion)
    }

    #[doc(hidden)]
    pub fn set_game_view_source_active(
        &mut self,
        active: bool,
        retirement_message: &'static str,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        self.state
            .published_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?
            .session
            .set_game_view_source_active(active, retirement_message)
    }

    #[doc(hidden)]
    pub fn resize_game_view(
        &mut self,
        extent: Extent2d,
    ) -> Result<bool, EditorGameViewBridgeFailure> {
        self.state
            .workspace_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?
            .session
            .resize_game_view(extent)
    }

    fn request_close_project(&mut self) -> EditorApplicationUpdate {
        if self.session_retirement.is_some() {
            return EditorApplicationUpdate::idle();
        }
        self.start_session_retirement(EditorSessionRetirement::CloseProject)
    }

    fn start_session_retirement(
        &mut self,
        retirement: EditorSessionRetirement,
    ) -> EditorApplicationUpdate {
        self.session_retirement = Some(retirement);
        self.session_close_retry_at = None;
        let update = match &mut self.state {
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => request_exit_on_project(project),
            EditorProductState::ProjectManager(_)
            | EditorProductState::OpeningInitial(_)
            | EditorProductState::OpeningReplacement(_)
            | EditorProductState::Recovery(_) => {
                self.session_retirement = None;
                self.session_close_retry_at = None;
                return EditorApplicationUpdate::idle();
            }
        };
        let finished = self.finish_session_retirement(true);
        EditorApplicationUpdate {
            actions_observed: update.actions_observed() || finished.actions_observed(),
            exit_ready: finished.exit_ready(),
        }
    }

    fn finish_session_retirement(&mut self, actions_observed: bool) -> EditorApplicationUpdate {
        let Some(retirement) = self.session_retirement else {
            return EditorApplicationUpdate {
                actions_observed,
                exit_ready: false,
            };
        };
        let Some(frame) = self.current_frame() else {
            self.session_retirement = None;
            self.session_close_retry_at = None;
            return EditorApplicationUpdate {
                actions_observed,
                exit_ready: false,
            };
        };
        match frame.workspace_intent().result() {
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::Exit,
            }) => {
                if self.session_close_retry_deferred() {
                    return EditorApplicationUpdate {
                        actions_observed,
                        exit_ready: false,
                    };
                }
                let (supervised_play_ready, supervised_play_retry_required) = match &mut self.state
                {
                    EditorProductState::AuthoringReady(project)
                    | EditorProductState::AuthoringDegraded(project) => {
                        project.supervised_play.request_close(Instant::now());
                        (
                            project.supervised_play.close_complete(),
                            project.supervised_play.close_retry_required(),
                        )
                    }
                    _ => (true, false),
                };
                if !supervised_play_ready {
                    self.defer_supervised_play_close_retry(supervised_play_retry_required);
                    return EditorApplicationUpdate {
                        actions_observed,
                        exit_ready: false,
                    };
                }
                let close_progress = match &mut self.state {
                    EditorProductState::AuthoringReady(project)
                    | EditorProductState::AuthoringDegraded(project) => {
                        Some(project.session.advance_close())
                    }
                    _ => None,
                };
                if matches!(
                    &close_progress,
                    Some(EditorProjectCloseProgress::Incomplete { .. })
                ) {
                    self.defer_session_close_retry();
                    return EditorApplicationUpdate {
                        actions_observed: true,
                        exit_ready: false,
                    };
                }
                let Some(EditorProjectCloseProgress::Complete(_)) = close_progress else {
                    self.session_retirement = None;
                    self.session_close_retry_at = None;
                    return EditorApplicationUpdate {
                        actions_observed,
                        exit_ready: false,
                    };
                };
                self.session_close_retry_at = None;
                let state = self.take_state();
                match state {
                    EditorProductState::AuthoringReady(project)
                    | EditorProductState::AuthoringDegraded(project) => {
                        let request = project.request;
                        let entry_generation = self.allocate_entry_generation();
                        self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                            entry_generation,
                            request,
                        ));
                    }
                    other => self.state = other,
                }
                self.session_retirement = None;
                EditorApplicationUpdate {
                    actions_observed,
                    exit_ready: retirement == EditorSessionRetirement::ExitProcess,
                }
            }
            _ if frame.workspace_intent().phase() == Some(EditorWorkspaceIntentPhase::Prepared)
                && !actions_observed =>
            {
                let dispatch = match &mut self.state {
                    EditorProductState::AuthoringReady(project)
                    | EditorProductState::AuthoringDegraded(project) => {
                        dispatch_workspace_intent_action(
                            project,
                            EditorWorkspaceIntentAction::CommitPrepared,
                        )
                    }
                    _ => EditorApplicationDispatch::default(),
                };
                EditorApplicationUpdate {
                    actions_observed: actions_observed || dispatch.actions_observed(),
                    exit_ready: false,
                }
            }
            Some(
                EditorWorkspaceIntentResult::Cancelled {
                    intent: EditorWorkspaceIntent::Exit,
                }
                | EditorWorkspaceIntentResult::Rejected {
                    intent: EditorWorkspaceIntent::Exit,
                    ..
                },
            ) => {
                self.session_retirement = None;
                self.session_close_retry_at = None;
                EditorApplicationUpdate {
                    actions_observed,
                    exit_ready: false,
                }
            }
            _ if frame.workspace_intent().intent().is_none()
                && frame.persistence().operation() == EditorPersistenceOperation::Idle =>
            {
                // Persistence may have kept the initial Exit request Busy. Retain the user's
                // retirement intent and admit it only after that operation has settled.
                let update = match &mut self.state {
                    EditorProductState::AuthoringReady(project)
                    | EditorProductState::AuthoringDegraded(project) => {
                        request_exit_on_project(project)
                    }
                    _ => EditorApplicationUpdate::idle(),
                };
                EditorApplicationUpdate {
                    actions_observed: actions_observed || update.actions_observed(),
                    exit_ready: false,
                }
            }
            _ => EditorApplicationUpdate {
                actions_observed,
                exit_ready: false,
            },
        }
    }

    pub fn request_exit(&mut self) -> EditorApplicationUpdate {
        if self.session_retirement.is_some() {
            return EditorApplicationUpdate::idle();
        }
        let state = self.take_state();
        match state {
            EditorProductState::ProjectManager(manager) => {
                self.session_retirement = None;
                self.session_close_retry_at = None;
                self.state = EditorProductState::ProjectManager(manager);
                EditorApplicationUpdate {
                    actions_observed: false,
                    exit_ready: true,
                }
            }
            EditorProductState::Recovery(recovery) => {
                self.session_retirement = None;
                self.session_close_retry_at = None;
                self.state = EditorProductState::Recovery(recovery);
                EditorApplicationUpdate {
                    actions_observed: false,
                    exit_ready: true,
                }
            }
            EditorProductState::OpeningInitial(opening) => {
                self.session_retirement = None;
                self.session_close_retry_at = None;
                let entry_generation = self.allocate_entry_generation();
                self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                    entry_generation,
                    opening.request,
                ));
                EditorApplicationUpdate {
                    actions_observed: true,
                    exit_ready: true,
                }
            }
            EditorProductState::OpeningReplacement(replacement) => {
                let retained = match replacement {
                    OpeningReplacementState::Preparing { retained, .. }
                    | OpeningReplacementState::Refreshing { retained, .. }
                    | OpeningReplacementState::Closing { retained, .. } => retained,
                };
                self.state = EditorProductState::from_published(retained);
                self.start_session_retirement(EditorSessionRetirement::ExitProcess)
            }
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => {
                self.state = EditorProductState::from_published(project);
                self.start_session_retirement(EditorSessionRetirement::ExitProcess)
            }
        }
    }

    fn start_initial_open(&mut self, request: EditorProjectRequest) {
        let opening = self.new_opening(request);
        self.state = EditorProductState::OpeningInitial(opening);
    }

    fn new_opening(&mut self, request: EditorProjectRequest) -> OpeningProjectState {
        let entry_generation = self.allocate_entry_generation();
        let attempt_generation = self.allocate_attempt_generation();
        OpeningProjectState::new(entry_generation, attempt_generation, request)
    }

    fn advance_initial_open(
        &mut self,
        mut opening: OpeningProjectState,
    ) -> (EditorProductState, bool) {
        match self.advance_opening_step(&mut opening) {
            OpenStepResult::Pending(changed) => {
                (EditorProductState::OpeningInitial(opening), changed)
            }
            OpenStepResult::Opened(project) => (EditorProductState::from_published(project), true),
            OpenStepResult::Rejected(recovery) => (EditorProductState::Recovery(recovery), true),
        }
    }

    fn advance_replacement_open(
        &mut self,
        replacement: OpeningReplacementState,
    ) -> (EditorProductState, bool) {
        match replacement {
            OpeningReplacementState::Preparing {
                mut retained,
                mut opening,
            } => {
                // Finish admitted persistence before preparing a replacement, so its source
                // and the retained workspace cannot straddle two publication attempts.
                if matches!(&opening.step, OpeningProjectStep::ProjectRoot)
                    && retained.session.persistence_view().operation()
                        != EditorPersistenceOperation::Idle
                {
                    let observed = drive_project_frame(&mut retained, Duration::ZERO, false);
                    if retained.session.current_frame().persistence().operation()
                        != EditorPersistenceOperation::Idle
                    {
                        return (
                            EditorProductState::OpeningReplacement(
                                OpeningReplacementState::Preparing { retained, opening },
                            ),
                            observed,
                        );
                    }
                }
                match self.advance_opening_step(&mut opening) {
                    OpenStepResult::Pending(changed) => (
                        EditorProductState::OpeningReplacement(
                            OpeningReplacementState::Preparing { retained, opening },
                        ),
                        changed,
                    ),
                    OpenStepResult::Rejected(_) => {
                        (EditorProductState::from_published(retained), true)
                    }
                    OpenStepResult::Opened(candidate) => (
                        EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                            entry_generation: opening.entry_generation,
                            attempt_generation: opening.attempt_generation,
                            retained,
                            candidate,
                            close_started: false,
                            save_attempted: false,
                        }),
                        true,
                    ),
                }
            }
            OpeningReplacementState::Refreshing {
                retained,
                mut opening,
            } => match self.advance_opening_step(&mut opening) {
                OpenStepResult::Pending(changed) => (
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Refreshing {
                        retained,
                        opening,
                    }),
                    changed,
                ),
                OpenStepResult::Rejected(_) => {
                    let mut retained = retained;
                    let _ = abort_prepared_exit(&mut retained);
                    (EditorProductState::from_published(retained), true)
                }
                OpenStepResult::Opened(candidate) => (
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                        entry_generation: opening.entry_generation,
                        attempt_generation: opening.attempt_generation,
                        retained,
                        candidate,
                        close_started: true,
                        save_attempted: false,
                    }),
                    true,
                ),
            },
            OpeningReplacementState::Closing {
                entry_generation,
                attempt_generation,
                mut retained,
                candidate,
                close_started,
                save_attempted,
            } => {
                if close_started {
                    return (
                        EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                            entry_generation,
                            attempt_generation,
                            retained,
                            candidate,
                            close_started,
                            save_attempted,
                        }),
                        false,
                    );
                }
                let update = request_exit_on_project(&mut retained);
                let frame = retained.session.current_frame();
                let close_started = frame.workspace_intent().intent()
                    == Some(EditorWorkspaceIntent::Exit)
                    || matches!(
                        frame.workspace_intent().result(),
                        Some(EditorWorkspaceIntentResult::Applied {
                            intent: EditorWorkspaceIntent::Exit,
                        })
                    );
                (
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                        entry_generation,
                        attempt_generation,
                        retained,
                        candidate,
                        close_started,
                        save_attempted,
                    }),
                    update.actions_observed(),
                )
            }
        }
    }

    fn finish_replacement_handoff(
        &mut self,
        mut actions_observed: bool,
    ) -> EditorApplicationUpdate {
        if self.session_close_retry_deferred() {
            return EditorApplicationUpdate {
                actions_observed,
                exit_ready: false,
            };
        }
        let state = self.take_state();
        let EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
            entry_generation,
            attempt_generation,
            mut retained,
            candidate,
            close_started,
            save_attempted,
        }) = state
        else {
            self.state = state;
            return EditorApplicationUpdate {
                actions_observed,
                exit_ready: false,
            };
        };
        let frame = retained.session.current_frame();
        let phase = frame.workspace_intent().phase();
        let persistence_save_failed = save_attempted
            && frame.workspace_intent().phase()
                == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
            && matches!(
                frame.persistence().result(),
                Some(
                    EditorPersistenceResult::Rejected { .. }
                        | EditorPersistenceResult::Failed { .. }
                        | EditorPersistenceResult::PersistenceUncertain { .. }
                )
            );
        match frame.workspace_intent().result() {
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::Exit,
            }) => {
                retained.supervised_play.request_close(Instant::now());
                if !retained.supervised_play.close_complete() {
                    self.defer_supervised_play_close_retry(
                        retained.supervised_play.close_retry_required(),
                    );
                    self.state =
                        EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                            entry_generation,
                            attempt_generation,
                            retained,
                            candidate,
                            close_started,
                            save_attempted,
                        });
                    return EditorApplicationUpdate {
                        actions_observed,
                        exit_ready: false,
                    };
                }
                match retained.session.advance_close() {
                    EditorProjectCloseProgress::Incomplete { .. } => {
                        self.defer_session_close_retry();
                        self.state = EditorProductState::OpeningReplacement(
                            OpeningReplacementState::Closing {
                                entry_generation,
                                attempt_generation,
                                retained,
                                candidate,
                                close_started,
                                save_attempted,
                            },
                        );
                        return EditorApplicationUpdate {
                            actions_observed: true,
                            exit_ready: false,
                        };
                    }
                    EditorProjectCloseProgress::Complete(_) => {
                        self.session_close_retry_at = None;
                        self.state = EditorProductState::from_published(candidate);
                        actions_observed = true;
                    }
                }
            }
            Some(
                EditorWorkspaceIntentResult::Cancelled {
                    intent: EditorWorkspaceIntent::Exit,
                }
                | EditorWorkspaceIntentResult::Rejected {
                    intent: EditorWorkspaceIntent::Exit,
                    ..
                },
            ) => {
                self.session_close_retry_at = None;
                self.state = EditorProductState::from_published(retained);
                actions_observed = true;
            }
            _ if persistence_save_failed && cancel_pending_exit(&mut retained) => {
                self.session_close_retry_at = None;
                self.state = EditorProductState::from_published(retained);
                actions_observed = true;
            }
            _ if persistence_save_failed => {
                self.state =
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                        entry_generation,
                        attempt_generation,
                        retained,
                        candidate,
                        close_started,
                        save_attempted,
                    });
            }
            _ if phase == Some(EditorWorkspaceIntentPhase::Prepared) && actions_observed => {
                self.state =
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                        entry_generation,
                        attempt_generation,
                        retained,
                        candidate,
                        close_started,
                        save_attempted,
                    });
            }
            _ if phase == Some(EditorWorkspaceIntentPhase::Prepared) && save_attempted => {
                let request = candidate.request.clone();
                let attempt_generation = self.allocate_attempt_generation();
                let opening =
                    OpeningProjectState::new(entry_generation, attempt_generation, request);
                self.state =
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Refreshing {
                        retained,
                        opening,
                    });
                return EditorApplicationUpdate {
                    actions_observed: true,
                    exit_ready: false,
                };
            }
            _ if phase == Some(EditorWorkspaceIntentPhase::Prepared) => {
                let dispatch = dispatch_workspace_intent_action(
                    &mut retained,
                    EditorWorkspaceIntentAction::CommitPrepared,
                );
                self.state =
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                        entry_generation,
                        attempt_generation,
                        retained,
                        candidate,
                        close_started,
                        save_attempted,
                    });
                return EditorApplicationUpdate {
                    actions_observed: actions_observed || dispatch.actions_observed(),
                    exit_ready: false,
                };
            }
            _ => {
                self.state =
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                        entry_generation,
                        attempt_generation,
                        retained,
                        candidate,
                        close_started,
                        save_attempted,
                    });
            }
        }
        EditorApplicationUpdate {
            actions_observed,
            exit_ready: false,
        }
    }

    fn advance_opening_step(&mut self, opening: &mut OpeningProjectState) -> OpenStepResult {
        match &mut opening.step {
            OpeningProjectStep::ProjectRoot => {
                let project_root = match open_project_root(opening.request.project_root()) {
                    Ok(project_root) => project_root,
                    Err(error) => {
                        return OpenStepResult::Rejected(RecoveryState::local_failure(
                            opening.entry_generation,
                            opening.attempt_generation,
                            opening.request.clone(),
                            root_error_code(&error),
                            root_error_summary(&error),
                            "Repair the project path or its filesystem capability, then retry.",
                        ));
                    }
                };
                let attempt = self
                    .open_host
                    .begin(project_root, opening.request.owned_profile());
                opening.step = OpeningProjectStep::Host(attempt);
                OpenStepResult::Pending(true)
            }
            OpeningProjectStep::Host(_) => {
                let attempt =
                    match std::mem::replace(&mut opening.step, OpeningProjectStep::ProjectRoot) {
                        OpeningProjectStep::Host(attempt) => attempt,
                        OpeningProjectStep::ProjectRoot => return OpenStepResult::Pending(false),
                    };
                let previous_phase = attempt.phase();
                match self.open_host.advance(attempt) {
                    EditorProjectOpenProgress::Pending(attempt) => {
                        let changed = attempt.phase() != previous_phase;
                        opening.step = OpeningProjectStep::Host(attempt);
                        OpenStepResult::Pending(changed)
                    }
                    EditorProjectOpenProgress::Opened(session) => {
                        OpenStepResult::Opened(PublishedEditorProject::new(
                            opening.entry_generation,
                            opening.request.clone(),
                            session,
                            self.supervised_play_wake.clone(),
                        ))
                    }
                    EditorProjectOpenProgress::Rejected(failure) => {
                        OpenStepResult::Rejected(RecoveryState::from_host_failure(
                            opening.entry_generation,
                            opening.attempt_generation,
                            opening.request.clone(),
                            failure,
                        ))
                    }
                }
            }
        }
    }

    fn enter_recovery(&mut self, recovery: RecoveryState) {
        self.session_close_retry_at = None;
        self.state = EditorProductState::Recovery(recovery);
    }

    fn session_close_retry_deferred(&self) -> bool {
        self.session_close_retry_at
            .is_some_and(|retry_at| Instant::now() < retry_at)
    }

    pub(crate) const fn session_close_retry_deadline(&self) -> Option<Instant> {
        self.session_close_retry_at
    }

    fn defer_session_close_retry(&mut self) {
        self.session_close_retry_at = Some(Instant::now() + EDITOR_SESSION_CLOSE_RETRY_DELAY);
    }

    fn defer_supervised_play_close_retry(&mut self, explicit_retry_required: bool) {
        self.session_close_retry_at =
            (!explicit_retry_required).then(|| Instant::now() + EDITOR_SESSION_CLOSE_RETRY_DELAY);
    }

    fn take_state(&mut self) -> EditorProductState {
        std::mem::replace(&mut self.state, EditorProductState::placeholder())
    }

    fn allocate_entry_generation(&mut self) -> u64 {
        let generation = self.next_entry_generation;
        self.next_entry_generation = self.next_entry_generation.saturating_add(1);
        generation
    }

    fn allocate_attempt_generation(&mut self) -> u64 {
        let generation = self.next_attempt_generation;
        self.next_attempt_generation = self.next_attempt_generation.saturating_add(1);
        generation
    }
}

enum OpenStepResult {
    Pending(bool),
    Opened(PublishedEditorProject),
    Rejected(RecoveryState),
}

fn dispatch_project_actions(
    project: &mut PublishedEditorProject,
    batch: EditorActionBatch,
) -> EditorApplicationDispatch {
    let frame_revision = batch.frame_revision();
    let actions = batch.into_actions();
    let mut dispatch = EditorApplicationDispatch::default();
    dispatch.original_slots = (0..actions.len()).map(Some).collect();
    for action in &actions {
        match action {
            EditorAction::Play(EditorPlayAction::Stop)
            | EditorAction::Play(EditorPlayAction::Restart)
            | EditorAction::Play(EditorPlayAction::Cancel)
            | EditorAction::Play(EditorPlayAction::RetryRetirement)
            | EditorAction::Play(EditorPlayAction::RetryClose)
            | EditorAction::WorkspaceIntent(_) => {
                dispatch.invalidates_game_input = true;
                dispatch.requires_runtime_drive = true;
            }
            EditorAction::Play(EditorPlayAction::Pause) => {
                dispatch.requires_runtime_drive = true;
            }
            EditorAction::Play(EditorPlayAction::Resume)
            | EditorAction::Play(EditorPlayAction::Play { .. })
            | EditorAction::Play(EditorPlayAction::RunLastGood) => {
                dispatch.requires_runtime_drive = true;
            }
            EditorAction::Play(EditorPlayAction::StepFixedTick) => {
                dispatch.requests_step = true;
                dispatch.requires_runtime_drive = true;
            }
            _ => {}
        }
    }
    let batch = EditorActionBatch::new(frame_revision, actions);
    dispatch.actions_observed = match project.session.dispatch_editor_actions(batch) {
        Ok(report) => {
            let observed = !report.outcomes().is_empty();
            if observed {
                dispatch.feedback = Some(Ok(report));
            }
            observed
        }
        Err(rejection) => {
            dispatch.feedback = Some(Err(rejection));
            true
        }
    };
    submit_supervised_play_request(project);
    dispatch
}

fn dispatch_close_resolution_actions(
    project: &mut PublishedEditorProject,
    batch: EditorActionBatch,
) -> (EditorApplicationDispatch, bool) {
    let frame_revision = batch.frame_revision();
    let original_actions = batch.into_actions().into_iter();
    let mut original_slots = Vec::new();
    let mut actions = Vec::new();
    for action in original_actions {
        if matches!(
            action,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve { .. })
                | EditorAction::Play(EditorPlayAction::RetryClose)
                | EditorAction::Play(EditorPlayAction::RetryRetirement)
        ) {
            original_slots.push(Some(actions.len()));
            actions.push(action);
        } else {
            original_slots.push(None);
        }
    }
    let save_actions = actions
        .iter()
        .map(|action| {
            matches!(
                action,
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve {
                    decision: nara::tooling::EditorCloseDecision::Save,
                    ..
                })
            )
        })
        .collect::<Vec<_>>();
    let mut dispatch = if actions.is_empty() {
        EditorApplicationDispatch::default()
    } else {
        dispatch_project_actions(project, EditorActionBatch::new(frame_revision, actions))
    };
    dispatch.original_slots = original_slots;
    dispatch.closing_filter = true;
    let attempted_save = dispatch
        .feedback()
        .and_then(Result::ok)
        .is_some_and(|report| {
            save_actions
                .into_iter()
                .zip(report.outcomes())
                .any(|(is_save, outcome)| {
                    is_save
                        && matches!(
                            outcome,
                            EditorActionOutcome::WorkspaceIntent(
                                EditorWorkspaceIntentRequestResult::Accepted
                                    | EditorWorkspaceIntentRequestResult::Rejected(
                                        EditorWorkspaceIntentRejection::PersistenceRejected(_),
                                    ),
                            )
                        )
                })
        });
    (dispatch, attempted_save)
}

fn dispatch_workspace_intent_action(
    project: &mut PublishedEditorProject,
    action: EditorWorkspaceIntentAction,
) -> EditorApplicationDispatch {
    let frame = project.session.current_frame();
    dispatch_project_actions(
        project,
        EditorActionBatch::new(frame.revision(), [EditorAction::WorkspaceIntent(action)]),
    )
}

fn abort_prepared_exit(project: &mut PublishedEditorProject) -> bool {
    let dispatch =
        dispatch_workspace_intent_action(project, EditorWorkspaceIntentAction::AbortPrepared);
    if dispatch.actions_observed() {
        project
            .session
            .drive_editor_frame_with_runtime(Duration::ZERO, false);
    }
    dispatch.actions_observed()
}

fn cancel_pending_exit(project: &mut PublishedEditorProject) -> bool {
    let frame = project.session.current_frame();
    match frame.workspace_intent().phase() {
        Some(EditorWorkspaceIntentPhase::AwaitingDecision) => {
            let Some(prompt) = frame.workspace_intent().decision_prompt() else {
                return false;
            };
            let dispatch = dispatch_workspace_intent_action(
                project,
                prompt.resolve(nara::tooling::EditorCloseDecision::Cancel),
            );
            if dispatch.actions_observed() {
                project
                    .session
                    .drive_editor_frame_with_runtime(Duration::ZERO, false);
            }
        }
        Some(EditorWorkspaceIntentPhase::Prepared) => {
            let _ = abort_prepared_exit(project);
        }
        Some(EditorWorkspaceIntentPhase::Saving | EditorWorkspaceIntentPhase::RetiringRuntime)
        | None => {}
    }

    matches!(
        project.session.current_frame().workspace_intent().result(),
        Some(
            EditorWorkspaceIntentResult::Cancelled {
                intent: EditorWorkspaceIntent::Exit,
            } | EditorWorkspaceIntentResult::Rejected {
                intent: EditorWorkspaceIntent::Exit,
                ..
            }
        )
    )
}

fn drive_project_frame(
    project: &mut PublishedEditorProject,
    real_delta: Duration,
    drive_runtime: bool,
) -> bool {
    let before = project.session.current_frame();
    let after = project
        .session
        .drive_editor_frame_with_runtime(real_delta, drive_runtime);
    before.persistence() != after.persistence()
        || before.source_asset_import() != after.source_asset_import()
        || before.source_asset_import_work_pending() != after.source_asset_import_work_pending()
        || before.image_import_work_pending() != after.image_import_work_pending()
}

fn request_exit_on_project(project: &mut PublishedEditorProject) -> EditorApplicationUpdate {
    let frame = project.session.current_frame();
    let mut actions = Vec::with_capacity(2);
    if frame.workspace_intent().intent().is_none() {
        if frame.workspace_intent().result().is_some() {
            actions.push(EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::AcknowledgeResult,
            ));
        }
        actions.push(EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit),
        ));
    }
    let dispatch = if actions.is_empty() {
        EditorApplicationDispatch::default()
    } else {
        dispatch_project_actions(project, EditorActionBatch::new(frame.revision(), actions))
    };
    let frame_observed = drive_project_frame(project, Duration::ZERO, true);
    EditorApplicationUpdate {
        actions_observed: dispatch.actions_observed() || frame_observed,
        exit_ready: false,
    }
}

fn submit_supervised_play_request(project: &mut PublishedEditorProject) {
    let Some(request) = project.session.take_external_play_request() else {
        return;
    };
    project.supervised_play.submit(
        request,
        project.request.project_root().to_path_buf(),
        Instant::now(),
    );
}

fn drive_supervised_play_owner(project: &mut PublishedEditorProject, now: Instant) -> bool {
    let Some(observation) = project.supervised_play.advance(now) else {
        return false;
    };
    project.session.apply_external_play_observation(observation)
}

fn earliest_instant(left: Option<Instant>, right: Option<Instant>) -> Option<Instant> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
        (None, None) => None,
    }
}

fn root_error_code(error: &EditorApplicationOpenError) -> &'static str {
    match error {
        EditorApplicationOpenError::MissingProjectRoot => "project.editor.project-root-missing",
        EditorApplicationOpenError::ProjectRootOpen { .. } => {
            "project.editor.project-root-open-failed"
        }
        EditorApplicationOpenError::ProjectRootCapability { .. } => {
            "project.editor.project-root-capability-rejected"
        }
        EditorApplicationOpenError::Project(_) => "project.editor.project-root-rejected",
    }
}

fn root_error_summary(error: &EditorApplicationOpenError) -> &'static str {
    match error {
        EditorApplicationOpenError::MissingProjectRoot => {
            "No project directory was supplied for this open attempt"
        }
        EditorApplicationOpenError::ProjectRootOpen { .. } => {
            "The project directory could not be opened"
        }
        EditorApplicationOpenError::ProjectRootCapability { .. } => {
            "The project directory did not satisfy the filesystem capability policy"
        }
        EditorApplicationOpenError::Project(_) => {
            "The project was rejected before an authoring session could be published"
        }
    }
}

pub(crate) fn open_project_root(
    path: &Path,
) -> Result<DirectoryCapability, EditorApplicationOpenError> {
    let file =
        open_directory(path).map_err(|source| EditorApplicationOpenError::ProjectRootOpen {
            path: path.to_path_buf(),
            source,
        })?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .map_err(|source| EditorApplicationOpenError::ProjectRootCapability {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(windows)]
fn open_directory(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_directory(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(all(test, any(windows, all(target_os = "linux", target_arch = "x86_64"))))]
#[path = "app/persistence_tests.rs"]
mod persistence_tests;

#[cfg(test)]
mod tests {
    use super::{EditorApplicationOpenError, root_error_code, root_error_summary};
    use std::{io, path::PathBuf};

    #[test]
    fn project_root_recovery_summary_is_bounded_and_path_free() {
        let private_path = PathBuf::from(r"C:\private\customer\prototype");
        let error = EditorApplicationOpenError::ProjectRootOpen {
            path: private_path.clone(),
            source: io::Error::new(io::ErrorKind::NotFound, "private source detail"),
        };

        assert_eq!(
            root_error_code(&error),
            "project.editor.project-root-open-failed"
        );
        assert_eq!(
            root_error_summary(&error),
            "The project directory could not be opened"
        );
        let private_path = private_path.to_string_lossy();
        assert!(!root_error_summary(&error).contains(private_path.as_ref()));
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn selected_local_workspace_root_admits_cooperative_publication() {
        use nara::fs::{CapabilityRights, ExpectedTarget, TrustMode};

        let root = super::open_project_root(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("the selected local workspace receives a real root capability");
        assert_eq!(root.options().rights(), CapabilityRights::ReadWrite);
        assert_eq!(root.options().trust(), TrustMode::TrustedLocal);
        root.preflight_replace(ExpectedTarget::Any)
            .expect("the local workspace must admit the platform's cooperative replacement tier");
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn startup_preparation_polls_are_quiet_and_cancel_cannot_publish_a_late_initial_project() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reference-game");
        let mut application = super::EditorApplication::new(
            super::EditorLaunchOptions::new(project).with_profile("desktop"),
        );
        let _ = application.native_shell_ready();
        advance_to_startup_preparation(&mut application);
        let launched = application.advance_project_open();
        assert!(
            !launched.actions_observed(),
            "the unchanged Startup phase needs no repaint"
        );
        assert!(application.current_frame().is_none());
        let _ = application.submit_shell_action(super::EditorProjectShellAction::Cancel);
        assert_eq!(
            application.state(),
            super::EditorApplicationState::ProjectManager
        );
        for _ in 0..3 {
            assert!(!application.advance_project_open().actions_observed());
            assert!(application.current_frame().is_none());
        }
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn cancelling_preparing_replacement_preserves_the_exact_published_project() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reference-game");
        let mut application = super::EditorApplication::new(
            super::EditorLaunchOptions::new(project.clone()).with_profile("desktop"),
        );
        let _ = application.native_shell_ready();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while application.current_frame().is_none() {
            assert!(std::time::Instant::now() < deadline);
            application.advance_project_open();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let before = application.current_frame().unwrap();
        let generation = application.workspace_generation();
        assert!(application.request_project_replacement(project, Some("desktop".to_owned())));
        advance_to_startup_preparation(&mut application);
        assert!(!application.advance_project_open().actions_observed());
        let _ = application.submit_shell_action(super::EditorProjectShellAction::Cancel);
        assert_eq!(application.workspace_generation(), generation);
        assert!(std::sync::Arc::ptr_eq(
            &before,
            &application.current_frame().unwrap()
        ));
        assert!(!application.advance_project_open().actions_observed());
        assert!(std::sync::Arc::ptr_eq(
            &before,
            &application.current_frame().unwrap()
        ));
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    pub(super) fn advance_to_startup_preparation(application: &mut super::EditorApplication) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let opening = match &application.state {
                super::EditorProductState::OpeningInitial(opening) => opening,
                super::EditorProductState::OpeningReplacement(
                    super::OpeningReplacementState::Preparing { opening, .. },
                ) => opening,
                state => panic!("the source must reach startup preparation: {state:?}"),
            };
            if matches!(&opening.step, super::OpeningProjectStep::Host(attempt)
                if attempt.phase() == nara::project_host::EditorProjectOpenPhase::StartupClosure)
            {
                return;
            }
            assert!(std::time::Instant::now() < deadline);
            application.advance_project_open();
        }
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn consecutive_batches_preserve_each_originating_feedback_before_redraw() {
        use nara::{
            scene::{
                SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
                ScenePlacement, ScenePlacementOwner,
            },
            tooling::{
                EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentMutation,
                EditorWorkspaceIntent, EditorWorkspaceIntentAction,
            },
        };

        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reference-game");
        let mut application = super::EditorApplication::new(
            super::EditorLaunchOptions::new(project).with_profile("desktop"),
        );
        let _ = application.native_shell_ready();
        let open_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while application.current_frame().is_none() {
            assert!(
                std::time::Instant::now() < open_deadline,
                "project Open did not settle"
            );
            let _ = application.advance_project_open();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mut frontend = crate::ImguiEditorFrontend::new();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context
            .io_mut()
            .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
        context.io_mut().set_display_size([1200.0, 800.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let rendered = frontend
            .show(context.frame(), &mut application)
            .expect("the ordinary Dear ImGui frontend must render")
            .is_some();
        let _ = context.render_legacy();
        assert!(rendered, "the authoring workspace was rendered");
        let first_revision = application.current_frame().unwrap().revision();
        let frame = application.current_frame().unwrap();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let first = application.dispatch_editor_actions(EditorActionBatch::new(
            first_revision,
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document: scene.document,
                    base_revision: scene.revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(
                            SceneEntityId::new("feedback-before-close").unwrap(),
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ));
        let _ = application.drive_editor_frame(
            std::time::Duration::ZERO,
            first.actions_observed(),
            true,
        );
        let second_revision = application.current_frame().unwrap().revision();
        let second = application.dispatch_editor_actions(EditorActionBatch::new(
            second_revision,
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit),
            )],
        ));

        // A native close request can arrive after a widget batch and before the next redraw. The
        // first widget must still receive its own result, including a precise structural rejection.
        let first_feedback_revision = first
            .feedback()
            .map(|feedback| feedback.map(|report| report.frame_revision()));
        let second_feedback_revision = second
            .feedback()
            .map(|feedback| feedback.map(|report| report.frame_revision()));

        // Complete physical session retirement before asserting so a regression failure cannot
        // make the native session's unwind guard obscure the originating-feedback assertion.
        let _ = application.request_exit();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while application.current_frame().is_some() && std::time::Instant::now() < deadline {
            let frame = application.current_frame().unwrap();
            let actions = frame.workspace_intent().decision_prompt().map(|prompt| {
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
            });
            let update = application.submit_and_drive(
                EditorActionBatch::new(frame.revision(), actions),
                std::time::Duration::ZERO,
            );
            if update.exit_ready() {
                break;
            }
            std::thread::yield_now();
        }

        assert!(first.actions_observed());
        assert!(second.actions_observed());
        assert_ne!(first_revision, second_revision);
        assert_eq!(first_feedback_revision, Some(Ok(first_revision)));
        assert_eq!(second_feedback_revision, Some(Ok(second_revision)));
        assert!(application.current_frame().is_none());
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn quiet_supervised_wake_preserves_the_published_authoring_frame() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reference-game");
        let mut application = super::EditorApplication::new(
            super::EditorLaunchOptions::new(project.clone()).with_profile("desktop"),
        );
        let _ = application.native_shell_ready();
        let open_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while application.current_frame().is_none() {
            assert!(
                std::time::Instant::now() < open_deadline,
                "project Open did not settle"
            );
            let _ = application.advance_project_open();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let before = application
            .current_frame()
            .expect("the reference authoring project opens");
        let update = application.drive_supervised_play_effects(std::time::Instant::now());
        assert!(!update.actions_observed());
        assert!(std::sync::Arc::ptr_eq(
            &before,
            &application.current_frame().unwrap()
        ));

        assert!(application.request_project_replacement(project, None));
        let open_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !matches!(
            application.state,
            super::EditorProductState::OpeningReplacement(
                super::OpeningReplacementState::Closing { .. }
            )
        ) {
            assert!(
                std::time::Instant::now() < open_deadline,
                "replacement Open did not settle"
            );
            let _ = application.advance_project_open();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(matches!(
            application.state,
            super::EditorProductState::OpeningReplacement(
                super::OpeningReplacementState::Closing { .. }
            )
        ));
        application.session_close_retry_at =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
        let closing_frame = application.current_frame().unwrap();
        let update = application.drive_supervised_play_effects(std::time::Instant::now());
        let closing_actions_observed = update.actions_observed();
        let closing_frame_preserved =
            std::sync::Arc::ptr_eq(&closing_frame, &application.current_frame().unwrap());
        application.session_close_retry_at = None;
        let replacement_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while matches!(
            application.state,
            super::EditorProductState::OpeningReplacement(_)
        ) {
            let _ = application.advance_project_open();
            let _ = application.drive_editor_frame(std::time::Duration::ZERO, false, false);
            assert!(
                std::time::Instant::now() < replacement_deadline,
                "the replacement did not retire"
            );
            std::thread::yield_now();
        }

        let _ = application.request_exit();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while application.current_frame().is_some() {
            let update = application.drive_editor_frame(std::time::Duration::ZERO, false, false);
            if update.exit_ready() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the quiet Editor did not close"
            );
            std::thread::yield_now();
        }
        assert!(!closing_actions_observed);
        assert!(
            closing_frame_preserved,
            "a late quiet wake must not stale the displayed closing action frame"
        );
    }
}
