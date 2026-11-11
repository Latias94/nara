//! Bounded admission of immutable graph evaluations into one preview owner.

use std::{error::Error, fmt, sync::Arc};

use nara_core::DocumentRevision;
use nara_engine::tooling::{EditorDocumentId, EditorExtensionDocumentError};

use super::{GraphView, evaluate};

/// Exact document slot and source incarnation/revision captured for an evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphPreviewBasis {
    pub document: EditorDocumentId,
    pub revision: DocumentRevision,
}

/// Checked, monotonically increasing identity within one preview owner.
///
/// IDs are allocated only by successful requests and are never reset by source changes or close.
/// Different owners may allocate equal IDs; private completion provenance keeps them separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphPreviewRequestId(u64);

/// The last successfully admitted evaluation, including a valid incomplete output.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphPreviewValue {
    pub request: GraphPreviewRequestId,
    pub basis: GraphPreviewBasis,
    pub value: Option<f64>,
}

/// An owned observation; changing it cannot affect admission.
///
/// `last_good` retains its original basis across edits, failures, source changes and close.
/// `diagnostic` describes only the latest admitted failure for the current basis.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphPreviewState {
    pub current: Option<GraphPreviewBasis>,
    pub pending: Option<GraphPreviewRequestId>,
    pub last_good: Option<GraphPreviewValue>,
    pub diagnostic: Option<String>,
}

/// A refusal to capture work; neither failure changes owner state or consumes an ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphPreviewRequestError {
    NoCurrentSource,
    RequestIdExhausted,
}

impl fmt::Display for GraphPreviewRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoCurrentSource => "No graph source is bound for preview",
            Self::RequestIdExhausted => "The graph preview request identifier space is exhausted",
        })
    }
}

impl Error for GraphPreviewRequestError {}

/// Whether a completion published a value, published a diagnostic, or was retired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphPreviewDisposition {
    /// Replaced `last_good`, including when the evaluated output is `None`.
    Accepted,
    /// Set the current diagnostic while preserving `last_good`.
    Failed,
    /// Changed no owner state, including its current pending request and diagnostic.
    Stale,
}

/// Move-only immutable capture for execution on the caller's existing background executor.
///
/// It retains no document owner, native resource, task handle or scheduling facility.
#[derive(Debug)]
pub struct GraphPreviewWork {
    admission: Arc<()>,
    id: GraphPreviewRequestId,
    basis: GraphPreviewBasis,
    view: GraphView,
}

impl GraphPreviewWork {
    #[must_use]
    pub const fn id(&self) -> GraphPreviewRequestId {
        self.id
    }

    #[must_use]
    pub const fn basis(&self) -> GraphPreviewBasis {
        self.basis
    }

    /// Evaluates the captured view without accessing or mutating the preview owner.
    #[must_use]
    pub fn execute(self) -> GraphPreviewCompletion {
        GraphPreviewCompletion {
            admission: self.admission,
            id: self.id,
            basis: self.basis,
            result: evaluate(&self.view),
        }
    }
}

/// Move-only evaluation result whose identity and outcome can only come from captured work.
#[derive(Debug)]
pub struct GraphPreviewCompletion {
    admission: Arc<()>,
    id: GraphPreviewRequestId,
    basis: GraphPreviewBasis,
    result: Result<Option<f64>, EditorExtensionDocumentError>,
}

/// Retains one pending admission and one labeled last-good value, without retaining work.
///
/// The caller owns task execution and physical completion, including retired requests. Retiring
/// admission does not cancel execution. This owner does not allocate a worker or retain a queue.
#[derive(Debug, Default)]
pub struct GraphPreviewOwner {
    admission: Arc<()>,
    last_request: u64,
    state: GraphPreviewState,
}

impl GraphPreviewOwner {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Observes the authoritative current source; `None` closes admission.
    ///
    /// A changed basis retires pending admission and clears its diagnostic. Reobserving the same
    /// basis preserves both. Last-good output and the request counter survive every transition.
    pub fn observe(&mut self, current: Option<GraphPreviewBasis>) {
        if self.state.current != current {
            self.state.current = current;
            self.state.pending = None;
            self.state.diagnostic = None;
        }
    }

    /// Captures an owned view for the currently observed basis without evaluating it.
    ///
    /// The caller must obtain the view and basis from the same document observation. A successful
    /// request supersedes every earlier request, even at the same revision, and clears the current
    /// diagnostic. All fallible checks precede mutation, including checked request-ID allocation.
    pub fn request(
        &mut self,
        view: GraphView,
    ) -> Result<GraphPreviewWork, GraphPreviewRequestError> {
        let basis = self
            .state
            .current
            .ok_or(GraphPreviewRequestError::NoCurrentSource)?;
        let next = self
            .last_request
            .checked_add(1)
            .ok_or(GraphPreviewRequestError::RequestIdExhausted)?;
        let id = GraphPreviewRequestId(next);
        let work = GraphPreviewWork {
            admission: Arc::clone(&self.admission),
            id,
            basis,
            view,
        };
        self.last_request = next;
        self.state.pending = Some(id);
        self.state.diagnostic = None;
        Ok(work)
    }

    /// Admits only this owner's latest pending request at the exact current basis.
    #[must_use]
    pub fn complete(&mut self, completion: GraphPreviewCompletion) -> GraphPreviewDisposition {
        if !Arc::ptr_eq(&self.admission, &completion.admission)
            || self.state.current != Some(completion.basis)
            || self.state.pending != Some(completion.id)
        {
            return GraphPreviewDisposition::Stale;
        }
        self.state.pending = None;
        match completion.result {
            Ok(value) => {
                self.state.last_good = Some(GraphPreviewValue {
                    request: completion.id,
                    basis: completion.basis,
                    value,
                });
                self.state.diagnostic = None;
                GraphPreviewDisposition::Accepted
            }
            Err(error) => {
                self.state.diagnostic = Some(error.to_string());
                GraphPreviewDisposition::Failed
            }
        }
    }

    #[must_use]
    pub fn state(&self) -> GraphPreviewState {
        self.state.clone()
    }
}

#[cfg(test)]
mod tests;
