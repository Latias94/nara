use super::{EditorAuthoringRuntime, EditorAuthoringRuntimeCloseProgress};
use nara_app::{
    RuntimeCandidateFailure, RuntimeCandidateRetirementState, RuntimeCloseEvidence,
    RuntimePreparationRetirement, RuntimeStartFailure,
};
use nara_ecs::World;
use nara_tasks::TaskShutdownReport;

#[derive(Debug)]
pub(in crate::project_host::runtime::editor) enum EditorAuthoringRetirement {
    Configuration(RuntimePreparationRetirement),
    Admission(RuntimeStartFailure),
    Startup(RuntimeCandidateFailure),
    Runtime(EditorAuthoringRuntime),
}

impl From<EditorAuthoringRuntime> for EditorAuthoringRetirement {
    fn from(runtime: EditorAuthoringRuntime) -> Self {
        Self::Runtime(runtime)
    }
}

impl EditorAuthoringRetirement {
    pub(in crate::project_host::runtime::editor) fn state(
        &self,
    ) -> RuntimeCandidateRetirementState {
        match self {
            Self::Configuration(owner) => owner.retirement_state(),
            Self::Admission(owner) => owner.retirement_state(),
            Self::Startup(owner) => owner.retirement_state(),
            Self::Runtime(owner) if owner.runtime.state() == nara_app::RuntimeState::Stopped => {
                RuntimeCandidateRetirementState::Retired
            }
            Self::Runtime(owner)
                if owner.runtime.state() == nara_app::RuntimeState::CloseIncomplete =>
            {
                RuntimeCandidateRetirementState::RetirementIncomplete
            }
            Self::Runtime(_) => RuntimeCandidateRetirementState::Retiring,
        }
    }

    pub(in crate::project_host::runtime::editor) fn close_evidence(
        &self,
    ) -> Option<&RuntimeCloseEvidence> {
        match self {
            Self::Configuration(owner) => owner.close_evidence(),
            Self::Admission(owner) => Some(owner.close_evidence()),
            Self::Startup(owner) => Some(owner.close_evidence()),
            Self::Runtime(owner) => Some(owner.runtime.close_evidence()),
        }
    }

    pub(in crate::project_host::runtime::editor) fn has_close_failure(&self) -> bool {
        self.close_evidence()
            .is_some_and(RuntimeCloseEvidence::has_terminal_failure)
    }

    fn world(&self) -> Option<&World> {
        match self {
            Self::Configuration(owner) => owner.world(),
            Self::Admission(owner) => Some(owner.world()),
            Self::Startup(owner) => Some(owner.world()),
            Self::Runtime(owner) => Some(owner.runtime.world()),
        }
    }

    pub(in crate::project_host::runtime::editor) fn poll(
        &mut self,
    ) -> EditorAuthoringRuntimeCloseProgress {
        let state = match self {
            Self::Configuration(owner) => owner.drive_retirement(),
            Self::Admission(owner) => owner.drive_retirement(),
            Self::Startup(owner) => owner.drive_retirement(),
            Self::Runtime(owner) => return owner.shutdown_tasks(),
        };
        let report = self
            .world()
            .and_then(|world| world.get_resource::<TaskShutdownReport>())
            .cloned();
        if state == RuntimeCandidateRetirementState::Retired {
            let report = report.unwrap_or_else(|| TaskShutdownReport::aggregate([]));
            if report.is_complete() {
                EditorAuthoringRuntimeCloseProgress::Complete(report)
            } else {
                EditorAuthoringRuntimeCloseProgress::Incomplete(report)
            }
        } else {
            EditorAuthoringRuntimeCloseProgress::Incomplete(report.unwrap_or_default())
        }
    }
}

#[derive(Default)]
pub(in crate::project_host::runtime::editor) struct EditorAuthoringRetirements {
    owner: Option<EditorAuthoringRetirement>,
    pending_report: Option<TaskShutdownReport>,
    completed: Option<TaskShutdownReport>,
    failed: bool,
}

impl EditorAuthoringRetirements {
    pub(in crate::project_host::runtime::editor) fn is_pending(&self) -> bool {
        self.owner.is_some()
    }

    pub(in crate::project_host::runtime::editor) fn retain(
        &mut self,
        owner: impl Into<EditorAuthoringRetirement>,
    ) {
        assert!(
            self.owner.is_none(),
            "authoring replacement gates a second retired owner"
        );
        self.pending_report = Some(TaskShutdownReport::default());
        self.owner = Some(owner.into());
    }

    pub(in crate::project_host::runtime::editor) fn poll_for_new_cleanup_failure(
        &mut self,
    ) -> bool {
        let Some(owner) = self.owner.as_mut() else {
            return false;
        };
        let report = match owner.poll() {
            EditorAuthoringRuntimeCloseProgress::Incomplete(report) => {
                self.pending_report = Some(report);
                return false;
            }
            EditorAuthoringRuntimeCloseProgress::Complete(report) => report,
        };
        let was_failed = self.failed;
        self.failed |= report.has_cleanup_failure() || owner.has_close_failure();
        self.completed = Some(TaskShutdownReport::aggregate(
            self.completed.iter().chain(std::iter::once(&report)),
        ));
        self.pending_report = None;
        self.owner = None;
        self.failed && !was_failed
    }

    pub(in crate::project_host::runtime::editor) fn report_with(
        &self,
        live: &TaskShutdownReport,
    ) -> TaskShutdownReport {
        TaskShutdownReport::aggregate(
            std::iter::once(live)
                .chain(self.completed.iter())
                .chain(self.pending_report.iter()),
        )
    }

    pub(in crate::project_host::runtime::editor) fn failed(&self) -> bool {
        self.failed
    }
}
