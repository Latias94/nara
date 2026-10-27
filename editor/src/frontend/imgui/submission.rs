use std::sync::Arc;

use nara::tooling::{EditorActionBatch, EditorActionOutcome, EditorActionRejection};

use crate::workspace::EditorPanelInstanceId;

/// Process-local identity for one action queued by a live panel instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PanelSubmissionId(u64);

/// An action was rejected before it entered the frontend batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSubmitError {
    /// The finite per-cycle observation budget has been exhausted.
    Capacity,
}

impl std::fmt::Display for PanelSubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the panel submission budget is exhausted")
    }
}

impl std::error::Error for PanelSubmitError {}

/// Why a queued action never reached its application owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelActionNotDispatched {
    Superseded,
    ProjectChanged,
    Closing,
    NoPublishedFrame,
    PresentationFailed,
    BatchRejected(EditorActionRejection),
}

/// The dispatch result of exactly one queued action, independent of later publication.
#[derive(Clone, Debug, PartialEq)]
pub enum PanelActionDispatch {
    Dispatched(EditorActionOutcome),
    NotDispatched(PanelActionNotDispatched),
}

/// Prepared frontend work. Only the originating frontend can consume its observations.
/// Dropping this value performs no application work or panel callbacks.
#[must_use]
pub struct ImguiEditorSubmission {
    pub(super) frontend: Arc<()>,
    pub(super) ledger: Arc<PendingCandidate>,
    pub(super) batch: EditorActionBatch,
}

#[derive(Clone, Copy)]
pub(super) enum Recipient {
    Contribution,
    Authoring,
    PrefabIntent,
}

#[derive(Clone)]
pub(super) struct PanelOrigin {
    pub instance: EditorPanelInstanceId,
    pub incarnation: Arc<PanelIncarnation>,
}

pub(super) struct PanelIncarnation {
    pub serial: u64,
}

impl Default for PanelIncarnation {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT
            .try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |value| value.checked_add(1),
            )
            .expect("panel incarnation identity exhausted");
        Self { serial }
    }
}

#[derive(Clone)]
pub(super) struct PendingPanelSubmission {
    pub id: PanelSubmissionId,
    pub origin: PanelOrigin,
    pub slot: usize,
    pub recipient: Recipient,
}

pub(super) struct PendingCandidate {
    pub serial: u64,
    pub generation: Option<u64>,
    pub submissions: Vec<PendingPanelSubmission>,
    pub panels: Vec<PanelOrigin>,
    pub action_origins: Vec<(usize, PanelOrigin)>,
    pub failure: Option<PanelActionNotDispatched>,
}

pub(super) const MAX_PANEL_SUBMISSIONS: usize = 1024;

pub(super) fn enqueue(
    next: &mut u64,
    submissions: &mut Vec<PendingPanelSubmission>,
    origin: PanelOrigin,
    slot: usize,
    recipient: Recipient,
) -> Result<PanelSubmissionId, PanelSubmitError> {
    if submissions.len() >= MAX_PANEL_SUBMISSIONS {
        return Err(PanelSubmitError::Capacity);
    }
    let id = PanelSubmissionId(*next);
    *next = next.checked_add(1).ok_or(PanelSubmitError::Capacity)?;
    submissions.push(PendingPanelSubmission {
        id,
        origin,
        slot,
        recipient,
    });
    Ok(id)
}
