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
        EditorPreviewCandidate, EditorProjectIntent, EditorProjectOpenError, EditorProjectSession,
        EditorUiAuthoringFrame, EditorUiPreviewCandidate,
    },
    render::Extent2d,
    tooling::{
        EditorAction, EditorActionBatch, EditorFrameModel, EditorPlayAction, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction, EditorWorkspaceIntentResult,
    },
    tooling_egui::{
        EguiCentralViewportInput, EguiEditorViewport, EguiSceneEditorOutput, EguiSceneEditorPanel,
    },
};
use thiserror::Error;

/// Immutable launch configuration for one private Editor process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorLaunchOptions {
    project_root: PathBuf,
    profile: Option<String>,
}

impl EditorLaunchOptions {
    #[must_use]
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            profile: None,
        }
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
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}

#[derive(Debug, Error)]
pub enum EditorApplicationOpenError {
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
    Project(#[from] EditorProjectOpenError),
}

impl EditorApplicationOpenError {
    #[must_use]
    pub fn diagnostics(&self) -> Option<&DiagnosticReport> {
        match self {
            Self::Project(error) => Some(error.diagnostics()),
            Self::ProjectRootOpen { .. } | Self::ProjectRootCapability { .. } => None,
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

/// Product-local orchestration around one Host-owned Editor session and one egui Adapter.
pub struct EditorApplication {
    session: EditorProjectSession,
    panel: EguiSceneEditorPanel,
}

impl std::fmt::Debug for EditorApplication {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorApplication")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl EditorApplication {
    pub fn open(options: EditorLaunchOptions) -> Result<Self, EditorApplicationOpenError> {
        let intent = match options.profile.as_deref() {
            Some(profile) => EditorProjectIntent::new().with_profile(profile),
            None => EditorProjectIntent::new(),
        };
        Self::open_with_intent(options, intent)
    }

    /// Opens one private first-party Editor with an already compiled product intent.
    #[doc(hidden)]
    pub fn open_with_intent(
        options: EditorLaunchOptions,
        intent: EditorProjectIntent,
    ) -> Result<Self, EditorApplicationOpenError> {
        let project_root = open_project_root(options.project_root())?;
        let session = EditorProjectSession::open(project_root, intent)?;
        Ok(Self {
            session,
            panel: EguiSceneEditorPanel::new(),
        })
    }

    #[must_use]
    pub fn current_frame(&self) -> Arc<EditorFrameModel> {
        self.session.current_frame()
    }

    #[must_use]
    pub fn show(&mut self, root: &mut egui::Ui) -> EditorActionBatch {
        self.show_with_viewport(
            root,
            EguiCentralViewportInput::unavailable(),
            EguiEditorViewport::Scene,
        )
        .into_parts()
        .0
    }

    #[must_use]
    pub fn show_with_viewport(
        &mut self,
        root: &mut egui::Ui,
        viewport: EguiCentralViewportInput,
        selected_viewport: EguiEditorViewport,
    ) -> EguiSceneEditorOutput {
        let frame = self.session.current_frame();
        egui::Frame::central_panel(root.style())
            .show(root, |ui| {
                self.panel
                    .show_with_viewport(ui, &frame, viewport, selected_viewport)
            })
            .inner
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
        dispatch.actions_observed = match self.session.dispatch_editor_actions(batch) {
            Ok(report) => !report.outcomes().is_empty(),
            Err(_) => true,
        };
        dispatch
    }

    pub(crate) fn drive_editor_frame(
        &mut self,
        real_delta: Duration,
        actions_observed: bool,
        drive_runtime: bool,
    ) -> EditorApplicationUpdate {
        let frame = self
            .session
            .drive_editor_frame_with_runtime(real_delta, drive_runtime);
        EditorApplicationUpdate {
            actions_observed,
            exit_ready: exit_ready(&frame),
        }
    }

    pub(crate) fn apply_game_view_input_batch(
        &mut self,
        batch: EditorGameInputBatch,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        self.session.apply_game_view_input_batch(batch)
    }

    pub(crate) fn cancel_game_step_presentation(&mut self) -> bool {
        self.session.cancel_game_step_presentation()
    }

    pub(crate) fn game_step_requires_runtime_drive(&self) -> bool {
        self.session.game_step_requires_runtime_drive()
    }

    pub fn set_scene_preview_extent(&mut self, extent: Option<Extent2d>) {
        self.session.set_editor_preview_extent(extent);
    }

    #[must_use]
    pub fn take_scene_preview_candidate(&mut self) -> Option<EditorPreviewCandidate> {
        self.session.take_editor_preview_candidate()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn take_editor_ui_preview_candidate(&mut self) -> Option<EditorUiPreviewCandidate> {
        self.session.take_editor_ui_preview_candidate()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn current_ui_authoring_frame(&self) -> Option<Arc<EditorUiAuthoringFrame>> {
        self.session.current_ui_authoring_frame()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn pending_game_start_request(&self) -> Option<EditorGameStartRequest> {
        self.session.pending_game_start_request()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn active_game_start_request(&self) -> Option<EditorGameStartRequest> {
        self.session.active_game_start_request()
    }

    #[doc(hidden)]
    pub fn prepare_game_start_activation(
        &mut self,
        expected: EditorGameStartRequest,
    ) -> Result<EditorGameStartActivation, EditorGameViewBridgeFailure> {
        self.session.prepare_game_start_activation(expected)
    }

    #[doc(hidden)]
    pub fn commit_game_start_activation(&mut self, activation: EditorGameStartActivation) {
        self.session.commit_game_start_activation(activation);
    }

    #[doc(hidden)]
    pub fn abort_pending_game_start(
        &mut self,
        expected: EditorGameStartRequest,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        self.session.abort_pending_game_start(expected)
    }

    #[doc(hidden)]
    pub fn take_game_view_candidate(
        &mut self,
    ) -> Result<Option<EditorGameViewCandidate>, EditorGameViewBridgeFailure> {
        self.session.take_game_view_candidate()
    }

    #[doc(hidden)]
    pub fn complete_game_view_candidate(
        &mut self,
        completion: EditorGameViewCompletion,
    ) -> Result<(), EditorGameViewCompletion> {
        self.session.complete_game_view_candidate(completion)
    }

    #[doc(hidden)]
    pub fn set_game_view_source_active(
        &mut self,
        active: bool,
        retirement_message: &'static str,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        self.session
            .set_game_view_source_active(active, retirement_message)
    }

    #[doc(hidden)]
    pub fn resize_game_view(
        &mut self,
        extent: Extent2d,
    ) -> Result<bool, EditorGameViewBridgeFailure> {
        self.session.resize_game_view(extent)
    }

    pub fn request_exit(&mut self) -> EditorApplicationUpdate {
        let frame = self.session.current_frame();
        if exit_ready(&frame) {
            return EditorApplicationUpdate {
                actions_observed: false,
                exit_ready: true,
            };
        }
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
        let batch = EditorActionBatch::new(frame.revision(), actions);
        self.submit_and_drive(batch, Duration::ZERO)
    }
}

fn exit_ready(frame: &EditorFrameModel) -> bool {
    frame.workspace().scenes.is_empty()
        && matches!(
            frame.workspace_intent().result(),
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::Exit,
            })
        )
}

fn open_project_root(path: &Path) -> Result<DirectoryCapability, EditorApplicationOpenError> {
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
