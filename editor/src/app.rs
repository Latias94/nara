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
    project_host::{EditorProjectIntent, EditorProjectOpenError, EditorProjectSession},
    tooling::{
        EditorAction, EditorActionBatch, EditorFrameModel, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction, EditorWorkspaceIntentResult,
    },
    tooling_egui::EguiSceneEditorPanel,
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
        let project_root = open_project_root(options.project_root())?;
        let intent = match options.profile {
            Some(profile) => EditorProjectIntent::new().with_profile(profile),
            None => EditorProjectIntent::new(),
        };
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
        let frame = self.session.current_frame();
        egui::Frame::central_panel(root.style())
            .show(root, |ui| self.panel.show(ui, &frame))
            .inner
    }

    pub fn submit_and_drive(
        &mut self,
        batch: EditorActionBatch,
        real_delta: Duration,
    ) -> EditorApplicationUpdate {
        let actions_observed = match self.session.dispatch_editor_actions(batch) {
            Ok(report) => !report.outcomes().is_empty(),
            Err(_) => true,
        };
        let frame = self.session.drive_editor_frame(real_delta);
        EditorApplicationUpdate {
            actions_observed,
            exit_ready: exit_ready(&frame),
        }
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
