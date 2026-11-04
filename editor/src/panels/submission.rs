use std::sync::Arc;

use nara::tooling::{EditorActionBatch, EditorActionOutcome, EditorActionRejection};

use crate::{EditorApplication, app::EditorApplicationDispatch, workspace::EditorPanelInstanceId};

/// Observation identity scoped to the originating panel Host.
///
/// The original candidate and live panel incarnation determine completion delivery.
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

pub(crate) struct PreparedSubmission<R> {
    pub(crate) frontend: Arc<()>,
    pub(crate) ledger: Arc<PendingCandidate<R>>,
    pub(crate) batch: EditorActionBatch,
}

#[derive(Clone)]
pub(crate) struct PanelOrigin {
    pub(crate) instance: EditorPanelInstanceId,
    pub(crate) incarnation: Arc<PanelIncarnation>,
}

pub(crate) struct PanelIncarnation {
    pub(crate) serial: u64,
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
pub(crate) struct PendingPanelSubmission<R> {
    pub(crate) id: PanelSubmissionId,
    pub(crate) origin: PanelOrigin,
    pub(crate) slot: usize,
    pub(crate) recipient: R,
}

pub(crate) struct PendingCandidate<R> {
    pub(crate) serial: u64,
    pub(crate) generation: Option<u64>,
    pub(crate) submissions: Vec<PendingPanelSubmission<R>>,
    pub(crate) panels: Vec<PanelOrigin>,
    pub(crate) action_origins: Vec<(usize, PanelOrigin)>,
    pub(crate) failure: Option<PanelActionNotDispatched>,
}

pub(crate) const MAX_PANEL_SUBMISSIONS: usize = 1024;

pub(crate) fn enqueue<R>(
    next: &mut u64,
    submissions: &mut Vec<PendingPanelSubmission<R>>,
    origin: PanelOrigin,
    slot: usize,
    recipient: R,
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

pub(crate) struct SubmissionOwner<R> {
    pub(crate) identity: Arc<()>,
    pub(crate) next_candidate: u64,
    pub(crate) next_submission: u64,
    pub(crate) pending: Option<Arc<PendingCandidate<R>>>,
}

impl<R> Default for SubmissionOwner<R> {
    fn default() -> Self {
        Self {
            identity: Arc::new(()),
            next_candidate: 0,
            next_submission: 0,
            pending: None,
        }
    }
}

pub(crate) enum SubmissionAdmission {
    Foreign,
    ProjectChanged,
    Dispatched(EditorApplicationDispatch),
}

impl<R> SubmissionOwner<R> {
    pub(crate) fn stage(
        &mut self,
        generation: Option<u64>,
        submissions: Vec<PendingPanelSubmission<R>>,
        panels: Vec<PanelOrigin>,
        action_origins: Vec<(usize, PanelOrigin)>,
    ) -> u64 {
        let serial = self.next_candidate;
        self.next_candidate = self
            .next_candidate
            .checked_add(1)
            .expect("frontend candidate identity exhausted");
        self.pending = Some(Arc::new(PendingCandidate {
            serial,
            generation,
            submissions,
            panels,
            action_origins,
            failure: None,
        }));
        serial
    }

    pub(crate) fn seal(
        &self,
        serial: u64,
        batch: EditorActionBatch,
    ) -> Option<PreparedSubmission<R>> {
        let pending = self
            .pending
            .as_ref()
            .filter(|pending| pending.serial == serial)?;
        Some(PreparedSubmission {
            frontend: Arc::clone(&self.identity),
            ledger: Arc::clone(pending),
            batch,
        })
    }

    pub(crate) fn owns(&self, submission: &PreparedSubmission<R>) -> bool {
        Arc::ptr_eq(&self.identity, &submission.frontend)
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| Arc::ptr_eq(pending, &submission.ledger))
    }

    pub(crate) fn dispatch(
        &self,
        application: &mut EditorApplication,
        submission: PreparedSubmission<R>,
        live: impl Fn(&PanelOrigin) -> bool,
    ) -> SubmissionAdmission {
        if !self.owns(&submission) {
            return SubmissionAdmission::Foreign;
        }
        if submission.ledger.generation != application.workspace_generation() {
            return SubmissionAdmission::ProjectChanged;
        }
        let retired = submission
            .ledger
            .action_origins
            .iter()
            .filter_map(|(slot, origin)| (!live(origin)).then_some(*slot))
            .collect::<std::collections::BTreeSet<_>>();
        let revision = submission.batch.frame_revision();
        let mut actions = Vec::new();
        let slots = submission
            .batch
            .into_actions()
            .into_iter()
            .enumerate()
            .map(|(slot, action)| {
                if retired.contains(&slot) {
                    None
                } else {
                    let compact = actions.len();
                    actions.push(action);
                    Some(compact)
                }
            })
            .collect::<Vec<_>>();
        let mut dispatch =
            application.dispatch_editor_actions(EditorActionBatch::new(revision, actions));
        dispatch.remap_original_slots(&slots);
        SubmissionAdmission::Dispatched(dispatch)
    }
}

pub(crate) fn resolve_result(
    slot: usize,
    dispatch: Option<&EditorApplicationDispatch>,
    reason: Option<PanelActionNotDispatched>,
) -> PanelActionDispatch {
    if let Some(reason) = reason {
        PanelActionDispatch::NotDispatched(reason)
    } else if let Some(outcome) = dispatch.and_then(|dispatch| dispatch.original_outcome(slot)) {
        PanelActionDispatch::Dispatched(outcome.clone())
    } else {
        let dispatch = dispatch.expect("dispatch settlement has a dispatch or explicit reason");
        let reason =
            if dispatch.original_slots().get(slot) == Some(&None) && dispatch.closing_filter() {
                PanelActionNotDispatched::Closing
            } else if let Some(Err(rejection)) = dispatch.feedback() {
                PanelActionNotDispatched::BatchRejected(rejection)
            } else {
                PanelActionNotDispatched::NoPublishedFrame
            };
        PanelActionDispatch::NotDispatched(reason)
    }
}
