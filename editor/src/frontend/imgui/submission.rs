pub use crate::panels::{
    PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId, PanelSubmitError,
};

#[cfg(test)]
pub(super) use crate::panels::submission::MAX_PANEL_SUBMISSIONS;
pub(super) use crate::panels::submission::{PanelIncarnation, PanelOrigin, enqueue};
pub(super) type PendingPanelSubmission =
    crate::panels::submission::PendingPanelSubmission<Recipient>;

/// Prepared reference-frontend work, bound to its original panel candidate.
/// Dropping it performs no application work or panel callbacks.
#[must_use]
pub struct ImguiEditorSubmission {
    pub(super) shared: crate::panels::submission::PreparedSubmission<Recipient>,
}

#[derive(Clone, Copy)]
pub(super) enum Recipient {
    Contribution,
    Authoring,
    Hierarchy,
    PrefabIntent,
    SourceTool,
}
