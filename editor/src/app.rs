use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use nara::{
    diagnostic::DiagnosticReport,
    fs::{CapabilityRights, DirectoryCapability, FsError, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorGameInputBatch, EditorGameStartActivation, EditorGameStartRequest,
        EditorGameViewBridgeFailure, EditorGameViewCandidate, EditorGameViewCompletion,
        EditorPreviewCandidate, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession, EditorUiAuthoringFrame, EditorUiPreviewCandidate,
        TrustedCompiledEditorProjectIntent,
    },
    render::Extent2d,
    scene::UnavailableRecordManifest,
    tooling::{
        EditorAction, EditorActionBatch, EditorFrameModel, EditorPersistenceResult,
        EditorPlayAction, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
        EditorWorkspaceIntentPhase, EditorWorkspaceIntentResult,
    },
    tooling_egui::{
        EguiCentralViewportInput, EguiEditorOutput, EguiEditorProjectAction, EguiEditorViewport,
        EguiProjectManagerInput, EguiProjectOpeningInput, EguiProjectRecoveryDiagnostic,
        EguiProjectRecoveryInput, EguiProjectShellAction, EguiProjectShellInput,
        EguiProjectShellPanel, EguiUiAuthoringImages, EguiUiAuthoringInput,
        EguiUiAuthoringMutations,
    },
};
use thiserror::Error;

#[path = "shell.rs"]
mod shell;

pub use shell::EditorApplicationState;
pub(crate) use shell::EditorApplicationSurface;
use shell::{
    EditorProductState, EditorProjectRequest, OpeningProjectState, OpeningProjectStep,
    OpeningReplacementState, ProjectManagerState, PublishedEditorProject, RecoveryState,
};

/// Immutable launch configuration for one private Editor process.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditorLaunchOptions {
    project_root: Option<PathBuf>,
    profile: Option<String>,
}

impl EditorLaunchOptions {
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct EditorApplicationDispatch {
    actions_observed: bool,
    invalidates_game_input: bool,
    retires_cadence: bool,
    starts_cadence: bool,
    requests_step: bool,
    requires_runtime_drive: bool,
}

impl EditorApplicationDispatch {
    pub(crate) const fn actions_observed(self) -> bool {
        self.actions_observed
    }

    pub(crate) const fn invalidates_game_input(self) -> bool {
        self.invalidates_game_input
    }

    pub(crate) const fn retires_cadence(self) -> bool {
        self.retires_cadence
    }

    pub(crate) const fn starts_cadence(self) -> bool {
        self.starts_cadence
    }

    pub(crate) const fn requests_step(self) -> bool {
        self.requests_step
    }

    pub(crate) const fn requires_runtime_drive(self) -> bool {
        self.requires_runtime_drive
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorSessionRetirement {
    CloseProject,
    ExitProcess,
}

/// Product-local orchestration around the standalone shell and one Host-owned Editor session.
pub struct EditorApplication {
    state: EditorProductState,
    open_host: EditorProjectOpenHost,
    shell_panel: EguiProjectShellPanel,
    initial_request: Option<EditorProjectRequest>,
    native_ready: bool,
    next_entry_generation: u64,
    next_attempt_generation: u64,
    session_retirement: Option<EditorSessionRetirement>,
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
        let initial_request = options
            .project_root
            .clone()
            .map(|root| EditorProjectRequest::new(root, options.profile.clone()));
        let draft = initial_request
            .as_ref()
            .map(EditorProjectRequest::draft)
            .unwrap_or_else(|| {
                nara::tooling_egui::EguiProjectOpenDraft::new(
                    "",
                    options.profile.as_deref().unwrap_or_default(),
                )
            });
        Self {
            state: EditorProductState::ProjectManager(ProjectManagerState::new(1, draft)),
            open_host: EditorProjectOpenHost::new(),
            shell_panel: EguiProjectShellPanel::new(),
            initial_request,
            native_ready: false,
            next_entry_generation: 2,
            next_attempt_generation: 1,
            session_retirement: None,
        }
    }

    /// Opens the explicitly advanced in-process Editor lane compiled with project Runtime code.
    #[doc(hidden)]
    pub fn open_trusted_compiled(
        options: EditorLaunchOptions,
        intent: TrustedCompiledEditorProjectIntent,
    ) -> Result<Self, EditorApplicationOpenError> {
        let project_root = options
            .project_root
            .clone()
            .ok_or(EditorApplicationOpenError::MissingProjectRoot)?;
        let request = EditorProjectRequest::new(project_root.clone(), options.profile.clone());
        let project_root = open_project_root(&project_root)?;
        let session = EditorProjectSession::open_trusted_compiled(project_root, intent)?;
        Ok(Self {
            state: EditorProductState::from_published(PublishedEditorProject::new(
                request, session,
            )),
            open_host: EditorProjectOpenHost::new(),
            shell_panel: EguiProjectShellPanel::new(),
            initial_request: None,
            native_ready: false,
            next_entry_generation: 2,
            next_attempt_generation: 1,
            session_retirement: None,
        })
    }

    #[must_use]
    pub fn state(&self) -> EditorApplicationState {
        self.state.kind()
    }

    #[must_use]
    pub(crate) fn surface(&self) -> EditorApplicationSurface {
        match self.state.published() {
            Some(project) => EditorApplicationSurface::Workspace {
                frame: project.session.current_frame(),
                ui_authoring: project.session.current_ui_authoring_frame(),
                read_only: self.ordinary_input_locked(),
            },
            None => EditorApplicationSurface::ProjectShell,
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

    /// Renders Project Manager, Opening, or Recovery and applies one typed shell action.
    pub(crate) fn show_project_shell(&mut self, root: &mut egui::Ui) -> EditorApplicationUpdate {
        if matches!(
            &self.state,
            EditorProductState::AuthoringReady(_) | EditorProductState::AuthoringDegraded(_)
        ) {
            return EditorApplicationUpdate::idle();
        }
        let output = egui::Frame::central_panel(root.style())
            .show(root, |ui| self.show_project_shell_contents(ui))
            .inner;
        let action_observed = output.action().is_some();
        let (draft, action) = output.into_parts();
        let exit_ready = self.apply_shell_action(draft, action);
        EditorApplicationUpdate {
            actions_observed: action_observed,
            exit_ready,
        }
    }

    fn show_project_shell_contents(
        &mut self,
        ui: &mut egui::Ui,
    ) -> nara::tooling_egui::EguiProjectShellOutput {
        match &self.state {
            EditorProductState::ProjectManager(manager) => self.shell_panel.show(
                ui,
                EguiProjectShellInput::ProjectManager(EguiProjectManagerInput::new(
                    manager.entry_generation,
                    &manager.draft,
                )),
            ),
            EditorProductState::OpeningInitial(opening) => {
                let draft = opening.request.draft();
                self.shell_panel.show(
                    ui,
                    EguiProjectShellInput::OpeningInitial(EguiProjectOpeningInput::new(
                        opening.entry_generation,
                        &draft,
                        opening.attempt_generation,
                        opening.phase_label(),
                        opening.summary(),
                    )),
                )
            }
            EditorProductState::OpeningReplacement(replacement) => {
                let draft = replacement.draft();
                let input = EguiProjectOpeningInput::new(
                    replacement.entry_generation(),
                    &draft,
                    replacement.attempt_generation(),
                    replacement.phase_label(),
                    replacement.summary(),
                );
                self.shell_panel
                    .show(ui, EguiProjectShellInput::OpeningReplacement(input))
            }
            EditorProductState::Recovery(recovery) => {
                let draft = recovery.request.draft();
                let diagnostics = recovery
                    .diagnostics
                    .iter()
                    .map(|diagnostic| {
                        EguiProjectRecoveryDiagnostic::new(
                            &diagnostic.code,
                            &diagnostic.phase,
                            diagnostic.relative_source.as_deref(),
                            &diagnostic.summary,
                            &diagnostic.impact,
                            &diagnostic.repair_hint,
                        )
                    })
                    .collect::<Vec<_>>();
                self.shell_panel.show(
                    ui,
                    EguiProjectShellInput::Recovery(EguiProjectRecoveryInput::new(
                        recovery.entry_generation,
                        &draft,
                        recovery.attempt_generation,
                        &diagnostics,
                    )),
                )
            }
            EditorProductState::AuthoringReady(_) | EditorProductState::AuthoringDegraded(_) => {
                unreachable!("a published workspace is rendered through the workspace Adapter")
            }
        }
    }

    fn apply_shell_action(
        &mut self,
        draft: nara::tooling_egui::EguiProjectOpenDraft,
        action: Option<EguiProjectShellAction>,
    ) -> bool {
        match action {
            Some(EguiProjectShellAction::OpenProject) => {
                let request = EditorProjectRequest::from_draft(&draft);
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
            Some(EguiProjectShellAction::Retry) => {
                let _ = self.retry_project_open();
                false
            }
            Some(EguiProjectShellAction::Cancel) => {
                let state = self.take_state();
                match state {
                    EditorProductState::OpeningInitial(opening) => {
                        let entry_generation = self.allocate_entry_generation();
                        self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                            entry_generation,
                            opening.request.draft(),
                        ));
                    }
                    EditorProductState::OpeningReplacement(replacement) => {
                        self.cancel_replacement_open(replacement);
                    }
                    other => self.state = other,
                }
                false
            }
            Some(EguiProjectShellAction::CloseProject) => {
                let state = self.take_state();
                match state {
                    EditorProductState::Recovery(_) => {
                        let entry_generation = self.allocate_entry_generation();
                        self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                            entry_generation,
                            nara::tooling_egui::EguiProjectOpenDraft::default(),
                        ));
                    }
                    other => self.state = other,
                }
                false
            }
            Some(EguiProjectShellAction::ReturnToProjectManager) => {
                let state = self.take_state();
                match state {
                    EditorProductState::Recovery(recovery) => {
                        let entry_generation = self.allocate_entry_generation();
                        self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                            entry_generation,
                            recovery.request.draft(),
                        ));
                    }
                    other => self.state = other,
                }
                false
            }
            Some(EguiProjectShellAction::Exit) => true,
            None => {
                match &mut self.state {
                    EditorProductState::ProjectManager(manager) => manager.draft = draft,
                    EditorProductState::OpeningInitial(_)
                    | EditorProductState::OpeningReplacement(_)
                    | EditorProductState::AuthoringReady(_)
                    | EditorProductState::AuthoringDegraded(_)
                    | EditorProductState::Recovery(_) => {}
                }
                false
            }
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

    /// Applies one egui product action without mixing it into the workspace action batch.
    #[doc(hidden)]
    pub fn submit_project_action(
        &mut self,
        action: EguiEditorProjectAction,
    ) -> EditorApplicationUpdate {
        match action {
            EguiEditorProjectAction::OpenProject(draft) => {
                let requested =
                    self.start_project_replacement(EditorProjectRequest::from_draft(&draft));
                EditorApplicationUpdate {
                    actions_observed: requested,
                    exit_ready: false,
                }
            }
            EguiEditorProjectAction::CloseProject => self.request_close_project(),
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

    #[must_use]
    pub fn show(&mut self, root: &mut egui::Ui) -> Option<EditorActionBatch> {
        self.show_with_viewport(
            root,
            EguiCentralViewportInput::unavailable(),
            EguiEditorViewport::Authoring,
        )
        .map(|output| output.into_parts().0)
    }

    #[must_use]
    pub fn show_with_viewport(
        &mut self,
        root: &mut egui::Ui,
        viewport: EguiCentralViewportInput,
        selected_viewport: EguiEditorViewport,
    ) -> Option<EguiEditorOutput> {
        let replacement_opening = match &self.state {
            EditorProductState::OpeningReplacement(replacement) => Some((
                replacement.entry_generation(),
                replacement.attempt_generation(),
                replacement.draft(),
                replacement.phase_label(),
                replacement.summary(),
            )),
            _ => None,
        };
        let read_only = self.ordinary_input_locked();
        let project = match &mut self.state {
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project)
            | EditorProductState::OpeningReplacement(OpeningReplacementState::Preparing {
                retained: project,
                ..
            })
            | EditorProductState::OpeningReplacement(OpeningReplacementState::Refreshing {
                retained: project,
                ..
            })
            | EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                retained: project,
                ..
            }) => project,
            EditorProductState::ProjectManager(_)
            | EditorProductState::OpeningInitial(_)
            | EditorProductState::Recovery(_) => return None,
        };
        let frame = project.session.current_frame();
        let ui_authoring = project.session.current_ui_authoring_frame();
        let editor_output = egui::Frame::central_panel(root.style())
            .show(root, |ui| {
                let field_editing_enabled = |key: &nara::ui::__private::UiAuthoringFieldKey| {
                    ui_authoring
                        .as_ref()
                        .is_some_and(|authoring| authoring.field_editing_enabled(key))
                };
                let mut lower_continuous_field =
                    |key: &nara::ui::__private::UiAuthoringFieldKey,
                     value: nara::ui::__private::UiAuthoringFieldValue| {
                        ui_authoring
                            .as_ref()
                            .ok_or_else(|| "UI authoring is unavailable".to_owned())?
                            .lower_field_edit(key, value)
                            .map_err(|error| error.to_string())
                    };
                let mut lower_discrete_field =
                    |key: &nara::ui::__private::UiAuthoringFieldKey,
                     value: nara::ui::__private::UiAuthoringFieldValue| {
                        ui_authoring
                            .as_ref()
                            .ok_or_else(|| "UI authoring is unavailable".to_owned())?
                            .lower_discrete_field_edit(key, value)
                            .map_err(|error| error.to_string())
                    };
                let mut lower_direct = |edit: &nara::ui::__private::UiAuthoringDirectEdit| {
                    ui_authoring
                        .as_ref()
                        .ok_or_else(|| "UI authoring is unavailable".to_owned())?
                        .lower_direct_edit(edit)
                        .map_err(|error| error.to_string())
                };
                let mut lower_structure = |intent: &nara::ui::__private::UiStructureIntent| {
                    ui_authoring
                        .as_ref()
                        .ok_or_else(|| "UI authoring is unavailable".to_owned())?
                        .lower_structure_intent(intent)
                        .map_err(|error| error.to_string())
                };
                let authoring = ui_authoring.as_ref().and_then(|authoring| {
                    (authoring.editor_frame() == frame.revision()).then(|| {
                        EguiUiAuthoringInput::new(
                            authoring.document(),
                            authoring.revision(),
                            authoring.selection(),
                            authoring.gesture(),
                            authoring.projection(),
                            EguiUiAuthoringImages::new(
                                authoring.image_choice_generation(),
                                authoring.image_choices(),
                            ),
                            EguiUiAuthoringMutations::new(
                                &field_editing_enabled,
                                authoring.direct_editing_enabled(),
                                authoring.structure_editing_enabled(),
                                &mut lower_continuous_field,
                                &mut lower_discrete_field,
                                &mut lower_direct,
                                &mut lower_structure,
                            ),
                        )
                    })
                });
                project.panel.show_with_authoring_lock(
                    ui,
                    &frame,
                    authoring,
                    viewport,
                    selected_viewport,
                    read_only,
                )
            })
            .inner;

        if let Some((entry_generation, attempt_generation, draft, phase, summary)) =
            replacement_opening
        {
            let mut shell_output = None;
            egui::Window::new("Project transition")
                .id(egui::Id::new("nara_editor_replacement_progress"))
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .show(root.ctx(), |ui| {
                    shell_output = Some(self.shell_panel.show(
                        ui,
                        EguiProjectShellInput::OpeningReplacement(EguiProjectOpeningInput::new(
                            entry_generation,
                            &draft,
                            attempt_generation,
                            phase,
                            summary,
                        )),
                    ));
                });
            if let Some(shell_output) = shell_output {
                let (draft, action) = shell_output.into_parts();
                let _ = self.apply_shell_action(draft, action);
            }
        }

        Some(editor_output)
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
            | EditorProductState::Recovery(_) => EditorApplicationDispatch::default(),
        }
    }

    pub(crate) fn drive_editor_frame(
        &mut self,
        real_delta: Duration,
        actions_observed: bool,
        drive_runtime: bool,
    ) -> EditorApplicationUpdate {
        if let EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
            retained,
            ..
        }) = &mut self.state
        {
            retained
                .session
                .drive_editor_frame_with_runtime(real_delta, false);
            return self.finish_replacement_handoff(actions_observed);
        }
        match &mut self.state {
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => {
                project
                    .session
                    .drive_editor_frame_with_runtime(real_delta, drive_runtime);
                if self.session_retirement.is_some() {
                    self.finish_session_retirement(actions_observed)
                } else {
                    EditorApplicationUpdate {
                        actions_observed,
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

    pub(crate) fn apply_game_view_input_batch(
        &mut self,
        batch: EditorGameInputBatch,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        self.state
            .workspace_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?
            .session
            .apply_game_view_input_batch(batch)
    }

    pub(crate) fn cancel_game_step_presentation(&mut self) -> bool {
        self.state
            .workspace_mut()
            .is_some_and(|project| project.session.cancel_game_step_presentation())
    }

    pub(crate) fn game_step_requires_runtime_drive(&self) -> bool {
        self.state
            .workspace()
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
            .workspace()
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
            .workspace_mut()
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
        let update = match &mut self.state {
            EditorProductState::AuthoringReady(project)
            | EditorProductState::AuthoringDegraded(project) => request_exit_on_project(project),
            EditorProductState::ProjectManager(_)
            | EditorProductState::OpeningInitial(_)
            | EditorProductState::OpeningReplacement(_)
            | EditorProductState::Recovery(_) => {
                self.session_retirement = None;
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
            return EditorApplicationUpdate {
                actions_observed,
                exit_ready: false,
            };
        };
        match frame.workspace_intent().result() {
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::Exit,
            }) => {
                let state = self.take_state();
                match state {
                    EditorProductState::AuthoringReady(project)
                    | EditorProductState::AuthoringDegraded(project) => {
                        let draft = project.request.draft();
                        let entry_generation = self.allocate_entry_generation();
                        self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                            entry_generation,
                            draft,
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
                EditorApplicationUpdate {
                    actions_observed,
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
        let state = self.take_state();
        match state {
            EditorProductState::ProjectManager(manager) => {
                self.session_retirement = None;
                self.state = EditorProductState::ProjectManager(manager);
                EditorApplicationUpdate {
                    actions_observed: false,
                    exit_ready: true,
                }
            }
            EditorProductState::Recovery(recovery) => {
                self.session_retirement = None;
                self.state = EditorProductState::Recovery(recovery);
                EditorApplicationUpdate {
                    actions_observed: false,
                    exit_ready: true,
                }
            }
            EditorProductState::OpeningInitial(opening) => {
                self.session_retirement = None;
                let entry_generation = self.allocate_entry_generation();
                self.state = EditorProductState::ProjectManager(ProjectManagerState::new(
                    entry_generation,
                    opening.request.draft(),
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
            OpenStepResult::Pending => (EditorProductState::OpeningInitial(opening), true),
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
                retained,
                mut opening,
            } => match self.advance_opening_step(&mut opening) {
                OpenStepResult::Pending => (
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Preparing {
                        retained,
                        opening,
                    }),
                    true,
                ),
                OpenStepResult::Rejected(_) => (EditorProductState::from_published(retained), true),
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
            },
            OpeningReplacementState::Refreshing {
                retained,
                mut opening,
            } => match self.advance_opening_step(&mut opening) {
                OpenStepResult::Pending => (
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Refreshing {
                        retained,
                        opening,
                    }),
                    true,
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
                (
                    EditorProductState::OpeningReplacement(OpeningReplacementState::Closing {
                        entry_generation,
                        attempt_generation,
                        retained,
                        candidate,
                        close_started: true,
                        save_attempted,
                    }),
                    update.actions_observed(),
                )
            }
        }
    }

    fn finish_replacement_handoff(&mut self, actions_observed: bool) -> EditorApplicationUpdate {
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
                self.state = EditorProductState::from_published(candidate);
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
                self.state = EditorProductState::from_published(retained);
            }
            _ if persistence_save_failed && cancel_pending_exit(&mut retained) => {
                self.state = EditorProductState::from_published(retained);
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
                OpenStepResult::Pending
            }
            OpeningProjectStep::Host(_) => {
                let attempt =
                    match std::mem::replace(&mut opening.step, OpeningProjectStep::ProjectRoot) {
                        OpeningProjectStep::Host(attempt) => attempt,
                        OpeningProjectStep::ProjectRoot => return OpenStepResult::Pending,
                    };
                match self.open_host.advance(attempt) {
                    EditorProjectOpenProgress::Pending(attempt) => {
                        opening.step = OpeningProjectStep::Host(attempt);
                        OpenStepResult::Pending
                    }
                    EditorProjectOpenProgress::Opened(session) => OpenStepResult::Opened(
                        PublishedEditorProject::new(opening.request.clone(), session),
                    ),
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
        self.state = EditorProductState::Recovery(recovery);
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
    Pending,
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
    for action in &actions {
        match action {
            EditorAction::Play(EditorPlayAction::Stop)
            | EditorAction::Play(EditorPlayAction::Restart)
            | EditorAction::Play(EditorPlayAction::Cancel)
            | EditorAction::Play(EditorPlayAction::RetryRetirement)
            | EditorAction::Play(EditorPlayAction::RetryClose)
            | EditorAction::WorkspaceIntent(_) => {
                dispatch.invalidates_game_input = true;
                dispatch.retires_cadence = true;
                dispatch.requires_runtime_drive = true;
            }
            EditorAction::Play(EditorPlayAction::Pause) => {
                dispatch.retires_cadence = true;
                dispatch.requires_runtime_drive = true;
            }
            EditorAction::Play(EditorPlayAction::Resume)
            | EditorAction::Play(EditorPlayAction::Play { .. }) => {
                dispatch.starts_cadence = true;
                dispatch.requires_runtime_drive = true;
            }
            EditorAction::Play(EditorPlayAction::StepFixedTick) => {
                dispatch.retires_cadence = true;
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
            project.panel.observe_action_report(&report);
            observed
        }
        Err(rejection) => {
            project.panel.observe_action_rejection(rejection);
            true
        }
    };
    dispatch
}

fn dispatch_close_resolution_actions(
    project: &mut PublishedEditorProject,
    batch: EditorActionBatch,
) -> (EditorApplicationDispatch, bool) {
    let frame_revision = batch.frame_revision();
    let mut attempted_save = false;
    let actions = batch
        .into_actions()
        .into_iter()
        .filter(|action| {
            matches!(
                action,
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve(_))
            )
        })
        .inspect(|action| {
            if matches!(
                action,
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve(
                    nara::tooling::EditorCloseDecision::Save
                ))
            ) {
                attempted_save = true;
            }
        })
        .collect::<Vec<_>>();
    let dispatch = if actions.is_empty() {
        EditorApplicationDispatch::default()
    } else {
        dispatch_project_actions(project, EditorActionBatch::new(frame_revision, actions))
    };
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
    match project.session.current_frame().workspace_intent().phase() {
        Some(EditorWorkspaceIntentPhase::AwaitingDecision) => {
            let dispatch = dispatch_workspace_intent_action(
                project,
                EditorWorkspaceIntentAction::Resolve(nara::tooling::EditorCloseDecision::Cancel),
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
    project
        .session
        .drive_editor_frame_with_runtime(Duration::ZERO, true);
    EditorApplicationUpdate {
        actions_observed: dispatch.actions_observed(),
        exit_ready: false,
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

fn open_project_root(path: &Path) -> Result<DirectoryCapability, EditorApplicationOpenError> {
    let file =
        open_directory(path).map_err(|source| EditorApplicationOpenError::ProjectRootOpen {
            path: path.to_path_buf(),
            source,
        })?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, portable_project_root_trust()),
    )
    .map_err(|source| EditorApplicationOpenError::ProjectRootCapability {
        path: path.to_path_buf(),
        source,
    })
}

fn portable_project_root_trust() -> TrustMode {
    #[cfg(any(windows, target_os = "linux"))]
    {
        TrustMode::Untrusted
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        TrustMode::TrustedLocal
    }
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

#[cfg(test)]
mod tests {
    use super::{
        EditorApplicationOpenError, portable_project_root_trust, root_error_code,
        root_error_summary,
    };
    use nara::fs::TrustMode;
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

    #[test]
    fn ordinary_project_roots_use_the_portable_strict_trust_policy() {
        #[cfg(any(windows, target_os = "linux"))]
        assert_eq!(portable_project_root_trust(), TrustMode::Untrusted);

        #[cfg(not(any(windows, target_os = "linux")))]
        assert_eq!(portable_project_root_trust(), TrustMode::TrustedLocal);
    }
}
