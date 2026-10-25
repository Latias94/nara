use super::{EditorAuthoringRuntime, EditorAuthoringRuntimeCloseProgress};
use nara_tasks::TaskShutdownReport;

#[derive(Default)]
pub(in crate::project_host::runtime::editor) struct EditorAuthoringRetirements {
    owner: Option<EditorAuthoringRuntime>,
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
        owner: EditorAuthoringRuntime,
    ) {
        assert!(
            self.owner.is_none(),
            "authoring replacement gates a second retired owner"
        );
        self.pending_report = Some(TaskShutdownReport::default());
        self.owner = Some(owner);
    }

    pub(in crate::project_host::runtime::editor) fn poll_for_new_cleanup_failure(
        &mut self,
    ) -> bool {
        let Some(owner) = self.owner.as_mut() else {
            return false;
        };
        let report = match owner.shutdown_tasks() {
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
