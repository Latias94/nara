use std::{path::PathBuf, sync::Arc};

use nara::{
    project_host::{
        EditorProjectOpenAttempt, EditorProjectOpenFailure, EditorProjectOpenPhase,
        EditorProjectSession,
    },
    tooling::{EditorAuthoringReadiness, EditorFrameModel},
    tooling_egui::{EguiEditorPanel, EguiProjectOpenDraft},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorApplicationState {
    ProjectManager,
    OpeningInitial,
    OpeningReplacement,
    AuthoringReady,
    AuthoringDegraded,
    Recovery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EditorProjectRequest {
    project_root: PathBuf,
    profile: Option<String>,
}

impl EditorProjectRequest {
    pub(super) fn new(project_root: PathBuf, profile: Option<String>) -> Self {
        Self {
            project_root,
            profile: normalize_profile(profile),
        }
    }

    pub(super) fn from_draft(draft: &EguiProjectOpenDraft) -> Self {
        Self::new(
            PathBuf::from(draft.project_path().trim()),
            Some(draft.profile().to_owned()),
        )
    }

    pub(super) fn project_root(&self) -> &std::path::Path {
        &self.project_root
    }

    pub(super) fn owned_profile(&self) -> Option<String> {
        self.profile.clone()
    }

    pub(super) fn draft(&self) -> EguiProjectOpenDraft {
        EguiProjectOpenDraft::new(
            self.project_root.to_string_lossy(),
            self.profile.as_deref().unwrap_or_default(),
        )
    }
}

fn normalize_profile(profile: Option<String>) -> Option<String> {
    profile.and_then(|profile| {
        let profile = profile.trim();
        (!profile.is_empty()).then(|| profile.to_owned())
    })
}

pub(super) struct PublishedEditorProject {
    pub(super) request: EditorProjectRequest,
    pub(super) session: Box<EditorProjectSession>,
    pub(super) panel: EguiEditorPanel,
}

impl PublishedEditorProject {
    pub(super) fn new(request: EditorProjectRequest, session: EditorProjectSession) -> Self {
        Self {
            request,
            session: Box::new(session),
            panel: EguiEditorPanel::new(),
        }
    }
}

impl std::fmt::Debug for PublishedEditorProject {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PublishedEditorProject")
            .field("request", &self.request)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(super) struct ProjectManagerState {
    pub(super) entry_generation: u64,
    pub(super) draft: EguiProjectOpenDraft,
}

impl ProjectManagerState {
    pub(super) fn new(entry_generation: u64, draft: EguiProjectOpenDraft) -> Self {
        Self {
            entry_generation,
            draft,
        }
    }

    pub(super) fn placeholder() -> Self {
        Self::new(0, EguiProjectOpenDraft::default())
    }
}

pub(super) struct OpeningProjectState {
    pub(super) entry_generation: u64,
    pub(super) attempt_generation: u64,
    pub(super) request: EditorProjectRequest,
    pub(super) step: OpeningProjectStep,
}

impl OpeningProjectState {
    pub(super) fn new(
        entry_generation: u64,
        attempt_generation: u64,
        request: EditorProjectRequest,
    ) -> Self {
        Self {
            entry_generation,
            attempt_generation,
            request,
            step: OpeningProjectStep::ProjectRoot,
        }
    }

    pub(super) fn phase_label(&self) -> &'static str {
        match &self.step {
            OpeningProjectStep::ProjectRoot => "Project root",
            OpeningProjectStep::Host(attempt) => open_phase_label(attempt.phase()),
        }
    }

    pub(super) fn summary(&self) -> &'static str {
        match &self.step {
            OpeningProjectStep::ProjectRoot => "Acquiring the exact project-root capability",
            OpeningProjectStep::Host(attempt) => open_phase_summary(attempt.phase()),
        }
    }
}

impl std::fmt::Debug for OpeningProjectState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpeningProjectState")
            .field("entry_generation", &self.entry_generation)
            .field("attempt_generation", &self.attempt_generation)
            .field("request", &self.request)
            .field("phase", &self.phase_label())
            .finish()
    }
}

pub(super) enum OpeningProjectStep {
    ProjectRoot,
    Host(EditorProjectOpenAttempt),
}

pub(super) enum OpeningReplacementState {
    Preparing {
        retained: PublishedEditorProject,
        opening: OpeningProjectState,
    },
    Refreshing {
        retained: PublishedEditorProject,
        opening: OpeningProjectState,
    },
    Closing {
        entry_generation: u64,
        attempt_generation: u64,
        retained: PublishedEditorProject,
        candidate: PublishedEditorProject,
        close_started: bool,
        save_attempted: bool,
    },
}

impl OpeningReplacementState {
    pub(super) fn entry_generation(&self) -> u64 {
        match self {
            Self::Preparing { opening, .. } => opening.entry_generation,
            Self::Refreshing { opening, .. } => opening.entry_generation,
            Self::Closing {
                entry_generation, ..
            } => *entry_generation,
        }
    }

    pub(super) fn attempt_generation(&self) -> u64 {
        match self {
            Self::Preparing { opening, .. } | Self::Refreshing { opening, .. } => {
                opening.attempt_generation
            }
            Self::Closing {
                attempt_generation, ..
            } => *attempt_generation,
        }
    }

    pub(super) fn draft(&self) -> EguiProjectOpenDraft {
        match self {
            Self::Preparing { opening, .. } | Self::Refreshing { opening, .. } => {
                opening.request.draft()
            }
            Self::Closing { candidate, .. } => candidate.request.draft(),
        }
    }

    pub(super) fn phase_label(&self) -> &'static str {
        match self {
            Self::Preparing { opening, .. } | Self::Refreshing { opening, .. } => {
                opening.phase_label()
            }
            Self::Closing { .. } => "Session handoff",
        }
    }

    pub(super) fn summary(&self) -> &'static str {
        match self {
            Self::Preparing { opening, .. } => opening.summary(),
            Self::Refreshing { .. } => "Re-admitting the replacement after saved source changed",
            Self::Closing { close_started, .. } if *close_started => {
                "Resolving the last-good session close before publication"
            }
            Self::Closing { .. } => {
                "The replacement is ready and awaiting last-good session handoff"
            }
        }
    }
}

impl std::fmt::Debug for OpeningReplacementState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpeningReplacementState")
            .field("entry_generation", &self.entry_generation())
            .field("attempt_generation", &self.attempt_generation())
            .field("phase", &self.phase_label())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecoveryDiagnostic {
    pub(super) code: String,
    pub(super) phase: String,
    pub(super) relative_source: Option<String>,
    pub(super) summary: String,
    pub(super) impact: String,
    pub(super) repair_hint: String,
}

#[derive(Debug)]
pub(super) struct RecoveryState {
    pub(super) entry_generation: u64,
    pub(super) attempt_generation: u64,
    pub(super) request: EditorProjectRequest,
    pub(super) diagnostics: Vec<RecoveryDiagnostic>,
}

impl RecoveryState {
    pub(super) fn from_host_failure(
        entry_generation: u64,
        attempt_generation: u64,
        request: EditorProjectRequest,
        failure: EditorProjectOpenFailure,
    ) -> Self {
        let phase = open_phase_label(failure.phase()).to_owned();
        let relative_source = failure.relative_source().map(str::to_owned);
        let impact = "Candidate session was not published".to_owned();
        let repair_hint = failure.repair_hint().to_owned();
        let mut diagnostics = failure
            .diagnostics()
            .iter()
            .map(|diagnostic| RecoveryDiagnostic {
                code: diagnostic.code().as_str().to_owned(),
                phase: phase.clone(),
                relative_source: relative_source.clone(),
                summary: diagnostic.summary().as_str().to_owned(),
                impact: impact.clone(),
                repair_hint: repair_hint.clone(),
            })
            .collect::<Vec<_>>();
        if diagnostics.is_empty() {
            diagnostics.push(RecoveryDiagnostic {
                code: "project.editor.open-rejected".to_owned(),
                phase,
                relative_source,
                summary: "The project-open candidate was rejected".to_owned(),
                impact,
                repair_hint,
            });
        }
        Self {
            entry_generation,
            attempt_generation,
            request,
            diagnostics,
        }
    }

    pub(super) fn local_failure(
        entry_generation: u64,
        attempt_generation: u64,
        request: EditorProjectRequest,
        code: &'static str,
        summary: impl Into<String>,
        repair_hint: &'static str,
    ) -> Self {
        Self {
            entry_generation,
            attempt_generation,
            request,
            diagnostics: vec![RecoveryDiagnostic {
                code: code.to_owned(),
                phase: "Project root".to_owned(),
                relative_source: None,
                summary: summary.into(),
                impact: "Candidate session was not published".to_owned(),
                repair_hint: repair_hint.to_owned(),
            }],
        }
    }
}

pub(super) enum EditorProductState {
    ProjectManager(ProjectManagerState),
    OpeningInitial(OpeningProjectState),
    OpeningReplacement(OpeningReplacementState),
    AuthoringReady(PublishedEditorProject),
    AuthoringDegraded(PublishedEditorProject),
    Recovery(RecoveryState),
}

impl EditorProductState {
    pub(super) fn placeholder() -> Self {
        Self::ProjectManager(ProjectManagerState::placeholder())
    }

    pub(super) fn kind(&self) -> EditorApplicationState {
        match self {
            Self::ProjectManager(_) => EditorApplicationState::ProjectManager,
            Self::OpeningInitial(_) => EditorApplicationState::OpeningInitial,
            Self::OpeningReplacement(_) => EditorApplicationState::OpeningReplacement,
            Self::AuthoringReady(_) => EditorApplicationState::AuthoringReady,
            Self::AuthoringDegraded(_) => EditorApplicationState::AuthoringDegraded,
            Self::Recovery(_) => EditorApplicationState::Recovery,
        }
    }

    pub(super) fn published(&self) -> Option<&PublishedEditorProject> {
        self.workspace().or_else(|| self.replacement_workspace())
    }

    pub(super) fn workspace(&self) -> Option<&PublishedEditorProject> {
        match self {
            Self::AuthoringReady(project) | Self::AuthoringDegraded(project) => Some(project),
            Self::ProjectManager(_)
            | Self::OpeningInitial(_)
            | Self::OpeningReplacement(_)
            | Self::Recovery(_) => None,
        }
    }

    pub(super) fn workspace_mut(&mut self) -> Option<&mut PublishedEditorProject> {
        match self {
            Self::AuthoringReady(project) | Self::AuthoringDegraded(project) => Some(project),
            Self::ProjectManager(_)
            | Self::OpeningInitial(_)
            | Self::OpeningReplacement(_)
            | Self::Recovery(_) => None,
        }
    }

    pub(super) fn replacement_workspace(&self) -> Option<&PublishedEditorProject> {
        match self {
            Self::OpeningReplacement(OpeningReplacementState::Preparing { retained, .. })
            | Self::OpeningReplacement(OpeningReplacementState::Refreshing { retained, .. })
            | Self::OpeningReplacement(OpeningReplacementState::Closing { retained, .. }) => {
                Some(retained)
            }
            _ => None,
        }
    }

    pub(super) fn from_published(project: PublishedEditorProject) -> Self {
        match project.session.project_readiness().authoring() {
            EditorAuthoringReadiness::Complete => Self::AuthoringReady(project),
            EditorAuthoringReadiness::Degraded => Self::AuthoringDegraded(project),
        }
    }
}

impl std::fmt::Debug for EditorProductState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorProductState")
            .field("kind", &self.kind())
            .field(
                "frame",
                &self
                    .published()
                    .map(|project| project.session.current_frame().revision()),
            )
            .finish_non_exhaustive()
    }
}

pub(crate) enum EditorApplicationSurface {
    ProjectShell,
    Workspace {
        frame: Arc<EditorFrameModel>,
        ui_authoring: Option<Arc<nara::project_host::EditorUiAuthoringFrame>>,
        read_only: bool,
    },
}

pub(super) fn open_phase_label(phase: EditorProjectOpenPhase) -> &'static str {
    match phase {
        EditorProjectOpenPhase::ManifestAndProfile => "Manifest and profile",
        EditorProjectOpenPhase::AuthoringSchemaBundle => "Authoring schema bundle",
        EditorProjectOpenPhase::StartupClosure => "Startup source closure",
        EditorProjectOpenPhase::SessionPublication => "Session publication",
    }
}

fn open_phase_summary(phase: EditorProjectOpenPhase) -> &'static str {
    match phase {
        EditorProjectOpenPhase::ManifestAndProfile => {
            "Admitting the project manifest and selected profile"
        }
        EditorProjectOpenPhase::AuthoringSchemaBundle => {
            "Admitting the data-only Authoring schema bundle"
        }
        EditorProjectOpenPhase::StartupClosure => {
            "Admitting the explicit startup Authoring source closure"
        }
        EditorProjectOpenPhase::SessionPublication => {
            "Publishing the admitted Authoring project session"
        }
    }
}
