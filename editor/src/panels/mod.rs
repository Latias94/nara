//! Toolkit-independent panel lifecycle, source access and exact action observations.

pub(crate) mod binding;
mod context;
mod host;
pub(crate) mod submission;

pub use context::EditorPanelContext;
pub use host::{EditorPanel, EditorPanelHost, EditorPanelSubmission};

pub use submission::{
    PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId, PanelSubmitError,
};
