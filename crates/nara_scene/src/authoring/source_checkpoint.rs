//! Private immutable source capture and prepared history checkpoints for authoring operations.
//!
//! Callers own operation validation and complete incumbent/storage admission. Capturing a source
//! shares its actual document and classification and copies observations only; it never creates
//! history. Preparing a checkpoint creates future history only after the caller admits its storage.

use std::{collections::BTreeSet, mem::size_of, sync::Arc};

use super::{
    AuthoringHistory, AuthoringSessionAuthority, PrefabAuthoringSession,
    SceneAuthoringContentState, SceneAuthoringHistoryLimits, SceneAuthoringRevision,
    SceneAuthoringSession, history_error_report,
};
use crate::{
    AuthoringDocumentKind, AuthoringSchemaBasis, PrefabDocument, SceneDocument,
    SceneEntityPlacement, SceneEntityRecord, ScenePatchReport, SourceDocumentKey,
    SourceDocumentKind, authoring_decode::AuthoringDocumentState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceCheckpointError {
    AuthorityUnavailable,
    Stale,
}

/// Exact observations of the original history, without retaining or recreating its owner.
#[derive(Debug, Clone, Copy)]
pub(super) struct SourceSnapshot {
    pub(super) revision: SceneAuthoringRevision,
    pub(super) content_state: SceneAuthoringContentState,
    pub(super) history_limits: SceneAuthoringHistoryLimits,
}

impl SourceSnapshot {
    pub(super) fn from_history(history: &AuthoringHistory) -> Self {
        Self {
            revision: history.revision(),
            content_state: history.content_state(),
            history_limits: history.limits(),
        }
    }
}

/// The same actual nominal Scene or Prefab source feeds direct and detached preparation.
pub(super) struct BorrowedSource<'a, D> {
    pub(super) document: &'a Arc<D>,
    pub(super) state: &'a Arc<AuthoringDocumentState>,
    pub(super) snapshot: SourceSnapshot,
}

impl<'a, D> BorrowedSource<'a, D> {
    pub(super) fn from_session(
        document: &'a Arc<D>,
        authority: &'a AuthoringSessionAuthority,
        history: &AuthoringHistory,
    ) -> Result<Self, SourceCheckpointError> {
        let AuthoringSessionAuthority::Published(state) = authority else {
            return Err(SourceCheckpointError::AuthorityUnavailable);
        };
        Ok(Self {
            document,
            state,
            snapshot: SourceSnapshot::from_history(history),
        })
    }

    pub(super) fn require_current(
        &self,
        document: &Arc<D>,
        authority: &AuthoringSessionAuthority,
        history: &AuthoringHistory,
        source_kind: SourceDocumentKind,
        basis: &AuthoringSchemaBasis,
    ) -> Result<(), SourceCheckpointError> {
        let current = BorrowedSource::from_session(document, authority, history)?;
        let document_kind = match source_kind {
            SourceDocumentKind::Scene => AuthoringDocumentKind::Scene,
            SourceDocumentKind::Prefab => AuthoringDocumentKind::Prefab,
        };
        if !Arc::ptr_eq(self.document, current.document)
            || !Arc::ptr_eq(self.state, current.state)
            || current.snapshot.revision != self.snapshot.revision
            || current.snapshot.content_state != self.snapshot.content_state
            || current.snapshot.history_limits != self.snapshot.history_limits
            || current.state.document_kind() != document_kind
            || &current.state.schema_basis() != basis
        {
            return Err(SourceCheckpointError::Stale);
        }
        Ok(())
    }
}

/// An admitted share of the original source and classification, with no history owner or token.
#[derive(Debug)]
pub(super) struct FrozenSource<D> {
    pub(super) document: Arc<D>,
    pub(super) state: Arc<AuthoringDocumentState>,
    pub(super) snapshot: SourceSnapshot,
}

impl<D> FrozenSource<D> {
    /// The caller admits its complete new input, including any operation-specific token/wrapper,
    /// before either Arc is cloned. Original bodies and both history branches must already be
    /// retained by the enclosing operation. This capture neither mints nor copies history.
    pub(super) fn capture<E>(
        source: BorrowedSource<'_, D>,
        admit: impl FnOnce() -> Result<(), E>,
    ) -> Result<Self, E> {
        admit()?;
        Ok(Self {
            document: Arc::clone(source.document),
            state: Arc::clone(source.state),
            snapshot: source.snapshot,
        })
    }

    pub(super) fn borrowed(&self) -> BorrowedSource<'_, D> {
        BorrowedSource {
            document: &self.document,
            state: &self.state,
            snapshot: self.snapshot,
        }
    }

    pub(super) fn require_current(
        &self,
        document: &Arc<D>,
        authority: &AuthoringSessionAuthority,
        history: &AuthoringHistory,
        source_kind: SourceDocumentKind,
        basis: &AuthoringSchemaBasis,
    ) -> Result<(), SourceCheckpointError> {
        self.borrowed()
            .require_current(document, authority, history, source_kind, basis)
    }
}

#[derive(Debug)]
pub(super) struct PreparedSourceCheckpoint {
    pub(super) source_key: SourceDocumentKey,
    pub(super) base_revision: SceneAuthoringRevision,
    pub(super) base_content_state: SceneAuthoringContentState,
    pub(super) original_state: Arc<AuthoringDocumentState>,
    pub(super) state: Arc<AuthoringDocumentState>,
    pub(super) history: AuthoringHistory,
}

impl PreparedSourceCheckpoint {
    /// The caller must admit all checkpoint storage before construction.
    pub(super) fn new(
        source_key: &SourceDocumentKey,
        original: SourceSnapshot,
        original_state: Arc<AuthoringDocumentState>,
        state: AuthoringDocumentState,
    ) -> Self {
        Self {
            source_key: source_key.clone(),
            base_revision: original.revision,
            base_content_state: original.content_state,
            original_state,
            state: Arc::new(state),
            history: AuthoringHistory::new(original.history_limits),
        }
    }

    pub(super) fn require_current(
        &self,
        source_key: &SourceDocumentKey,
        authority: &AuthoringSessionAuthority,
        history: &AuthoringHistory,
        source_kind: SourceDocumentKind,
        document_kind: AuthoringDocumentKind,
    ) -> Result<(), SourceCheckpointError> {
        let state = authority
            .published()
            .ok_or(SourceCheckpointError::AuthorityUnavailable)?;
        if source_key != &self.source_key
            || source_key.kind() != source_kind
            || state.document_kind() != document_kind
            || history.revision() != self.base_revision
            || history.content_state() != self.base_content_state
            || history.limits() != self.history.limits()
            || state.schema_basis() != self.original_state.schema_basis()
            || !std::ptr::eq(state, self.original_state.as_ref())
        {
            return Err(SourceCheckpointError::Stale);
        }
        Ok(())
    }
}

/// Admits the original raw source, classification and both existing history branches.
///
/// Prefab inputs are borrowed from the actual original document, not classification bookkeeping.
/// The operation supplies its own retention-token overhead and admission/diagnostic mapping.
pub(super) fn admit_incumbent(
    original_placements: &[SceneEntityPlacement],
    original_entities: &[SceneEntityRecord],
    original_inputs: Option<&BTreeSet<nara_identity::ReferenceInputId>>,
    state: &AuthoringDocumentState,
    history: &AuthoringHistory,
    retention_token_bytes: usize,
    mut admit: impl FnMut(&'static str, usize, Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<usize, ScenePatchReport> {
    let original = super::prefab_conversion::retention::document_dynamic_bytes_with_admission(
        original_placements,
        original_entities,
        state.file_limits(),
        |bytes| charge(&mut admit, "source-scan", bytes),
    )?;
    let inputs = match original_inputs {
        Some(inputs) => {
            // Cardinality is O(1). Admit traversal before summing the actual ID storage;
            // a node-limited operation must refuse before walking an arbitrarily large set.
            let scan = inputs
                .len()
                .checked_mul(size_of::<nara_identity::ReferenceInputId>());
            admit("input-scan", inputs.len(), scan)?;
            scan.ok_or_else(limit_error)?;
            crate::authoring_decode::retained_prefab_inputs_bytes(inputs)
        }
        None => Some(0),
    };
    let original = original.and_then(|bytes| bytes.checked_add(inputs?));
    let classification = state.checked_retained_bytes_with_admission(|bytes| {
        charge(&mut admit, "classification-scan", bytes)
    })?;
    // Reuse history's owning cost; do not copy or reinterpret either old history branch.
    let retained_history = history.cost.operations.checked_mul(512).and_then(|bytes| {
        bytes
            .checked_add(history.cost.value_nodes.checked_mul(96)?)?
            .checked_add(history.cost.logical_bytes)?
            .checked_add(
                history
                    .undo_stack
                    .len()
                    .checked_add(history.redo_stack.len())?
                    .checked_mul(size_of::<super::SceneAuthoringHistoryEntry>())?,
            )
    });
    let retained = original.and_then(|bytes| {
        bytes
            .checked_add(classification?)?
            .checked_add(retained_history?)?
            .checked_add(incumbent_storage_bytes(
                state.document_kind(),
                retention_token_bytes,
            )?)
    });
    charge(&mut admit, "incumbent", retained)?;
    retained.ok_or_else(limit_error)
}

pub(super) fn incumbent_storage_bytes(
    kind: AuthoringDocumentKind,
    retention_token_bytes: usize,
) -> Option<usize> {
    // Session storage already contains its inline history. Charge the source document, both
    // Arc control blocks and the caller's retention token; shared catalogs are excluded.
    let (session, document) = match kind {
        AuthoringDocumentKind::Scene => (
            size_of::<SceneAuthoringSession>(),
            size_of::<SceneDocument>(),
        ),
        AuthoringDocumentKind::Prefab => (
            size_of::<PrefabAuthoringSession>(),
            size_of::<PrefabDocument>(),
        ),
    };
    session
        .checked_add(document)?
        .checked_add(4 * size_of::<usize>())?
        .checked_add(retention_token_bytes)
}

fn charge(
    admit: &mut impl FnMut(&'static str, usize, Option<usize>) -> Result<(), ScenePatchReport>,
    stage: &'static str,
    bytes: Option<usize>,
) -> Result<(), ScenePatchReport> {
    admit(stage, 1, bytes)?;
    bytes.map(|_| ()).ok_or_else(limit_error)
}

fn limit_error() -> ScenePatchReport {
    history_error_report(
        "scene.source-checkpoint-limit-exceeded",
        "Authoring source exceeds its owning bounded retained cost model",
    )
}
