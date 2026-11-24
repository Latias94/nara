use std::{mem::size_of, sync::Arc};

use nara_identity::ReferenceInputId;

use crate::{
    AuthoringDocumentKind, AuthoringDocumentReadmission, AuthoringEntityView,
    AuthoringPrefabComponentOverrideKind, AuthoringReadError, PrefabAuthoringSession,
    PrefabDocument, SceneAuthoringContentState, SceneAuthoringRevision, SceneAuthoringSession,
    SceneDocument, SceneEntityId, SceneFileEncoding, SceneFileLimits, ScenePatchDocument,
    ScenePatchReport, SourceDocumentKey, SourceDocumentKind,
    authoring::{
        AuthoringHistory, AuthoringSessionAuthority,
        prefab_conversion::retention,
        source_checkpoint::{FrozenSource, PreparedSourceCheckpoint},
    },
    authoring_decode::{
        AuthoringDocumentState, AuthoringSourceParts, PreparedPrefabAuthoringPatch,
        PreparedSceneAuthoringPatch, retained_prefab_inputs_bytes,
    },
    prefab::SourceDocumentInput,
};

use super::{
    capture::{checkpoint_error, stale_source},
    work_limit_report,
};

#[derive(Debug)]
pub(super) struct PreparedSource<D> {
    original: Arc<D>,
    document: Arc<D>,
    checkpoint: PreparedSourceCheckpoint,
    patch: ScenePatchDocument,
}

impl<D> PreparedSource<D> {
    pub(super) fn new(
        source: &FrozenSource<D>,
        source_key: &SourceDocumentKey,
        document: Arc<D>,
        state: AuthoringDocumentState,
        patch: ScenePatchDocument,
    ) -> Self {
        Self {
            original: Arc::clone(&source.document),
            document,
            checkpoint: PreparedSourceCheckpoint::new(
                source_key,
                source.snapshot,
                Arc::clone(&source.state),
                state,
            ),
            patch,
        }
    }

    fn require_current(
        &self,
        document: &Arc<D>,
        authority: &AuthoringSessionAuthority,
        history: &AuthoringHistory,
        kind: SourceDocumentKind,
        document_kind: AuthoringDocumentKind,
    ) -> Result<(), ScenePatchReport> {
        if !Arc::ptr_eq(document, &self.original) {
            return Err(stale_source());
        }
        self.checkpoint
            .require_current(
                &self.checkpoint.source_key,
                authority,
                history,
                kind,
                document_kind,
            )
            .map_err(checkpoint_error)
    }

    fn install(
        self,
        document: &mut Arc<D>,
        authority: &mut AuthoringSessionAuthority,
        history: &mut AuthoringHistory,
    ) {
        *document = self.document;
        *authority = AuthoringSessionAuthority::Published(self.checkpoint.state);
        *history = self.checkpoint.history;
    }
}

#[derive(Debug)]
pub(super) enum ContainingCandidate<'a> {
    Scene(&'a FrozenSource<SceneDocument>, PreparedSceneAuthoringPatch),
    Prefab(
        &'a FrozenSource<PrefabDocument>,
        PreparedPrefabAuthoringPatch,
    ),
}

impl ContainingCandidate<'_> {
    pub(super) fn document(&self) -> SourceDocumentInput<'_> {
        match self {
            Self::Scene(_, candidate) => SourceDocumentInput::Scene(&candidate.document),
            Self::Prefab(_, candidate) => SourceDocumentInput::Prefab(&candidate.document),
        }
    }

    pub(super) fn into_prepared(self, key: &SourceDocumentKey) -> PreparedContaining {
        match self {
            Self::Scene(source, candidate) => PreparedContaining::Scene(PreparedSource::new(
                source,
                key,
                Arc::new(candidate.document),
                candidate.state,
                candidate.patch,
            )),
            Self::Prefab(source, candidate) => PreparedContaining::Prefab(PreparedSource::new(
                source,
                key,
                Arc::new(candidate.document),
                candidate.state,
                candidate.patch,
            )),
        }
    }
}

#[derive(Debug)]
pub(super) enum PreparedContaining {
    Scene(PreparedSource<SceneDocument>),
    Prefab(PreparedSource<PrefabDocument>),
}

impl PreparedContaining {
    fn checkpoint(&self) -> &PreparedSourceCheckpoint {
        match self {
            Self::Scene(source) => &source.checkpoint,
            Self::Prefab(source) => &source.checkpoint,
        }
    }
}

#[derive(Debug)]
struct PreparedPair {
    destination: PreparedSource<PrefabDocument>,
    destination_inverse: ScenePatchDocument,
    kind: AuthoringPrefabComponentOverrideKind,
    identity: Arc<()>,
    remaining_work_bytes: usize,
    reference_work: (usize, usize),
}

/// Jointly verified candidates and their future history checkpoints.
#[derive(Debug)]
#[must_use]
pub struct PreparedPrefabComponentApply {
    containing: PreparedContaining,
    pair: PreparedPair,
}

impl PreparedPrefabComponentApply {
    pub(super) fn new(
        containing: PreparedContaining,
        destination: PreparedSource<PrefabDocument>,
        destination_inverse: ScenePatchDocument,
        kind: AuthoringPrefabComponentOverrideKind,
        identity: Arc<()>,
        remaining_work_bytes: usize,
        reference_work: (usize, usize),
    ) -> Self {
        Self {
            containing,
            pair: PreparedPair {
                destination,
                destination_inverse,
                kind,
                identity,
                remaining_work_bytes,
                reference_work,
            },
        }
    }

    pub(super) fn matches_capture(&self, identity: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.pair.identity, identity)
    }

    pub fn containing_document(&self) -> SourceDocumentInput<'_> {
        match &self.containing {
            PreparedContaining::Scene(source) => SourceDocumentInput::Scene(&source.document),
            PreparedContaining::Prefab(source) => SourceDocumentInput::Prefab(&source.document),
        }
    }

    pub fn shared_containing_scene(&self) -> Option<Arc<SceneDocument>> {
        if let PreparedContaining::Scene(source) = &self.containing {
            Some(Arc::clone(&source.document))
        } else {
            None
        }
    }

    pub fn shared_containing_prefab(&self) -> Option<Arc<PrefabDocument>> {
        if let PreparedContaining::Prefab(source) = &self.containing {
            Some(Arc::clone(&source.document))
        } else {
            None
        }
    }

    pub fn containing_patch(&self) -> &ScenePatchDocument {
        match &self.containing {
            PreparedContaining::Scene(source) => &source.patch,
            PreparedContaining::Prefab(source) => &source.patch,
        }
    }

    pub fn destination_document(&self) -> &PrefabDocument {
        &self.pair.destination.document
    }

    pub fn shared_destination(&self) -> Arc<PrefabDocument> {
        Arc::clone(&self.pair.destination.document)
    }

    pub fn destination_entity(
        &self,
        entity: &SceneEntityId,
    ) -> Result<AuthoringEntityView<'_>, AuthoringReadError> {
        self.pair
            .destination
            .checkpoint
            .state
            .entity_view(&self.destination_document().entities, entity)
    }

    pub const fn destination_patch(&self) -> &ScenePatchDocument {
        &self.pair.destination.patch
    }

    pub const fn destination_inverse(&self) -> &ScenePatchDocument {
        &self.pair.destination_inverse
    }

    pub fn containing_key(&self) -> &SourceDocumentKey {
        &self.containing.checkpoint().source_key
    }

    pub fn destination_key(&self) -> &SourceDocumentKey {
        &self.pair.destination.checkpoint.source_key
    }

    pub fn containing_revision(&self) -> SceneAuthoringRevision {
        self.containing.checkpoint().base_revision
    }

    pub const fn destination_revision(&self) -> SceneAuthoringRevision {
        self.pair.destination.checkpoint.base_revision
    }

    pub fn containing_future_revision(&self) -> SceneAuthoringRevision {
        self.containing.checkpoint().history.revision()
    }

    pub fn destination_future_revision(&self) -> SceneAuthoringRevision {
        self.pair.destination.checkpoint.history.revision()
    }

    pub fn containing_content_state(&self) -> SceneAuthoringContentState {
        self.containing.checkpoint().history.content_state()
    }

    pub fn destination_content_state(&self) -> SceneAuthoringContentState {
        self.pair.destination.checkpoint.history.content_state()
    }

    pub fn containing_file_encoding(&self) -> Option<SceneFileEncoding> {
        self.containing.checkpoint().original_state.file_encoding()
    }

    pub fn destination_file_encoding(&self) -> Option<SceneFileEncoding> {
        self.pair
            .destination
            .checkpoint
            .original_state
            .file_encoding()
    }

    pub fn containing_file_limits(&self) -> SceneFileLimits {
        self.containing.checkpoint().original_state.file_limits()
    }

    pub fn destination_file_limits(&self) -> SceneFileLimits {
        self.pair
            .destination
            .checkpoint
            .original_state
            .file_limits()
    }

    pub fn containing_readmission(
        &self,
        admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
    ) -> Result<AuthoringDocumentReadmission, ScenePatchReport> {
        readmission(self.containing.checkpoint(), admit)
    }

    pub fn destination_readmission(
        &self,
        admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
    ) -> Result<AuthoringDocumentReadmission, ScenePatchReport> {
        readmission(&self.pair.destination.checkpoint, admit)
    }

    pub fn admit_containing_persistence_work(
        &self,
        admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
    ) -> Result<(), ScenePatchReport> {
        let parts = match self.containing_document() {
            SourceDocumentInput::Scene(document) => document.into(),
            SourceDocumentInput::Prefab(document) => document.into(),
        };
        admit_persistence_work(parts, self.containing_file_limits(), admit)
    }

    pub fn admit_destination_persistence_work(
        &self,
        admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
    ) -> Result<(), ScenePatchReport> {
        admit_persistence_work(
            self.destination_document().into(),
            self.destination_file_limits(),
            admit,
        )
    }

    pub const fn remaining_work_bytes(&self) -> usize {
        self.pair.remaining_work_bytes
    }

    pub const fn reference_work(&self) -> (usize, usize) {
        self.pair.reference_work
    }

    pub const fn kind(&self) -> AuthoringPrefabComponentOverrideKind {
        self.pair.kind
    }
}

pub(super) fn checkpoint_storage_bytes(
    containing: SourceDocumentInput<'_>,
    containing_key: &SourceDocumentKey,
    destination_key: &SourceDocumentKey,
) -> Option<usize> {
    let document = match containing {
        SourceDocumentInput::Scene(_) => size_of::<SceneDocument>(),
        SourceDocumentInput::Prefab(_) => size_of::<PrefabDocument>(),
    };
    size_of::<PreparedPrefabComponentApply>()
        .max(size_of::<BoundPrefabComponentApply<'_>>())
        .max(size_of::<PrefabComponentApplyBindFailure>())
        .checked_add(document)?
        .checked_add(2 * size_of::<usize>())?
        .checked_add(containing_key.checked_retained_bytes()?)?
        .checked_add(destination_key.checked_retained_bytes()?)
}

fn readmission(
    checkpoint: &PreparedSourceCheckpoint,
    mut admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<AuthoringDocumentReadmission, ScenePatchReport> {
    admit(Some(size_of::<AuthoringDocumentReadmission>()))?;
    Ok(checkpoint.state.readmission(checkpoint.history.limits()))
}

fn admit_persistence_work(
    parts: AuthoringSourceParts<'_>,
    limits: SceneFileLimits,
    mut admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<(), ScenePatchReport> {
    let raw = retention::document_dynamic_bytes_with_admission(
        parts.placements,
        parts.entities,
        limits,
        &mut admit,
    )?;
    let inputs = match parts.inputs {
        Some(inputs) => {
            admit(inputs.len().checked_mul(size_of::<ReferenceInputId>()))?;
            retained_prefab_inputs_bytes(inputs)
        }
        None => Some(0),
    };
    let bytes = raw
        .and_then(|bytes| bytes.checked_add(inputs?))
        .and_then(|bytes| bytes.checked_mul(4))
        .and_then(|bytes| {
            bytes.checked_add(size_of::<SceneDocument>().max(size_of::<PrefabDocument>()))
        });
    admit(bytes)?;
    bytes.map(|_| ()).ok_or_else(work_limit_report)
}

#[derive(Debug)]
pub enum PrefabComponentApplySourceMut<'a> {
    Scene(&'a mut SceneAuthoringSession),
    Prefab(&'a mut PrefabAuthoringSession),
}

#[derive(Debug)]
enum BoundContaining<'a> {
    Scene(&'a mut SceneAuthoringSession, PreparedSource<SceneDocument>),
    Prefab(
        &'a mut PrefabAuthoringSession,
        PreparedSource<PrefabDocument>,
    ),
}

#[derive(Debug)]
#[must_use]
pub struct PrefabComponentApplyBindFailure {
    prepared: PreparedPrefabComponentApply,
    report: ScenePatchReport,
}

impl PrefabComponentApplyBindFailure {
    pub fn report(&self) -> &ScenePatchReport {
        &self.report
    }

    pub fn into_parts(self) -> (PreparedPrefabComponentApply, ScenePatchReport) {
        (self.prepared, self.report)
    }
}

/// Holds both actual sources exclusively for the final source and history swap.
#[derive(Debug)]
#[must_use]
pub struct BoundPrefabComponentApply<'a> {
    containing: BoundContaining<'a>,
    destination: &'a mut PrefabAuthoringSession,
    pair: PreparedPair,
}

pub fn bind_prepared_prefab_component_apply<'a>(
    containing: PrefabComponentApplySourceMut<'a>,
    destination: &'a mut PrefabAuthoringSession,
    prepared: PreparedPrefabComponentApply,
) -> Result<BoundPrefabComponentApply<'a>, PrefabComponentApplyBindFailure> {
    let destination_current = prepared.pair.destination.require_current(
        &destination.document,
        &destination.authority,
        &destination.history,
        SourceDocumentKind::Prefab,
        AuthoringDocumentKind::Prefab,
    );
    if let Err(report) = destination_current {
        return Err(PrefabComponentApplyBindFailure { prepared, report });
    }
    let PreparedPrefabComponentApply {
        containing: candidate,
        pair,
    } = prepared;
    let containing = match (containing, candidate) {
        (PrefabComponentApplySourceMut::Scene(session), PreparedContaining::Scene(source)) => {
            let current = if session.live_instance().is_none() {
                source.require_current(
                    &session.document,
                    &session.authority,
                    &session.history,
                    SourceDocumentKind::Scene,
                    AuthoringDocumentKind::Scene,
                )
            } else {
                Err(stale_source())
            };
            if let Err(report) = current {
                return Err(PrefabComponentApplyBindFailure {
                    prepared: PreparedPrefabComponentApply {
                        containing: PreparedContaining::Scene(source),
                        pair,
                    },
                    report,
                });
            }
            BoundContaining::Scene(session, source)
        }
        (PrefabComponentApplySourceMut::Prefab(session), PreparedContaining::Prefab(source)) => {
            if let Err(report) = source.require_current(
                &session.document,
                &session.authority,
                &session.history,
                SourceDocumentKind::Prefab,
                AuthoringDocumentKind::Prefab,
            ) {
                return Err(PrefabComponentApplyBindFailure {
                    prepared: PreparedPrefabComponentApply {
                        containing: PreparedContaining::Prefab(source),
                        pair,
                    },
                    report,
                });
            }
            BoundContaining::Prefab(session, source)
        }
        (_, containing) => {
            return Err(PrefabComponentApplyBindFailure {
                prepared: PreparedPrefabComponentApply { containing, pair },
                report: stale_source(),
            });
        }
    };
    Ok(BoundPrefabComponentApply {
        containing,
        destination,
        pair,
    })
}

impl<'a> BoundPrefabComponentApply<'a> {
    pub fn into_prepared(self) -> PreparedPrefabComponentApply {
        let containing = match self.containing {
            BoundContaining::Scene(_, source) => PreparedContaining::Scene(source),
            BoundContaining::Prefab(_, source) => PreparedContaining::Prefab(source),
        };
        PreparedPrefabComponentApply {
            containing,
            pair: self.pair,
        }
    }

    /// Installs both prebound checkpoints and returns the actual sessions for Save acknowledgment.
    pub fn commit(
        self,
    ) -> (
        PrefabComponentApplySourceMut<'a>,
        &'a mut PrefabAuthoringSession,
    ) {
        let containing = match self.containing {
            BoundContaining::Scene(session, source) => {
                source.install(
                    &mut session.document,
                    &mut session.authority,
                    &mut session.history,
                );
                session.live_dirty = true;
                PrefabComponentApplySourceMut::Scene(session)
            }
            BoundContaining::Prefab(session, source) => {
                source.install(
                    &mut session.document,
                    &mut session.authority,
                    &mut session.history,
                );
                PrefabComponentApplySourceMut::Prefab(session)
            }
        };
        let destination = self.destination;
        self.pair.destination.install(
            &mut destination.document,
            &mut destination.authority,
            &mut destination.history,
        );
        (containing, destination)
    }
}
