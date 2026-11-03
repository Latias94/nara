//! Actual immutable source inputs for complete-project Prefab extraction.
//!
//! Capture retains the incumbent source, classification and both history branches under one
//! operation allowance. Sharing those inputs grants neither replacement nor publication authority.

use std::{fmt, mem::size_of, sync::Arc};

use nara_asset::{AssetRecord, AssetRef};

mod encoding;
pub use encoding::{EncodedPrefabExtractionSource, PrefabExtractionSourceOutput};

use super::source_checkpoint::{
    BorrowedSource, FrozenSource, PreparedSourceCheckpoint, SourceCheckpointError, admit_incumbent,
};
use super::{
    AuthoringSessionAuthority, PrefabAuthoringSession, SceneAuthoringContentState,
    SceneAuthoringHistoryLimits, SceneAuthoringRevision, SceneAuthoringSession,
    history_error_report,
};
use crate::{
    AuthoringDocumentReadmission, AuthoringDocumentSource, AuthoringSchemaBasis, PrefabDocument,
    ResolvedPrefabSource, SceneCompilationLimits, SceneCompilationOptions, SceneDocument,
    SceneEntityId, SceneEntityPlacement, SceneEntityRecord, SceneFileLimits, ScenePatchReport,
    SourceDocumentKey, SourceDocumentKind,
    prefab::extraction::{
        CapturedExtractionSource, ExtractionDestination, ExtractionError, ExtractionRequest,
        ExtractionWork, PrefabExtractionEnvironment, SavedGenerationRefusal,
        VerifiedExtractionCorpus, VerifiedExtractionSavedGeneration,
    },
};

/// One cumulative work/storage allowance for captured sources and extraction preparation.
///
/// Logical storage is conservatively charged cumulatively, including original history. Dropping
/// input or refused work does not refund this allowance. It is not an allocator/RSS measurement.
#[doc(hidden)]
#[must_use]
pub struct PrefabExtractionWork {
    identity: Arc<ExtractionWorkIdentity>,
    pub(crate) kernel: ExtractionWork,
}

#[derive(Debug)]
struct ExtractionWorkIdentity;

/// Immutable saved-source combination checked through this extraction's actual environments.
/// It has no file receipt, saved checkpoint, Ready or Native execution authority.
#[doc(hidden)]
#[must_use]
pub struct PrefabExtractionSavedGeneration {
    inner: VerifiedExtractionSavedGeneration,
    work: Arc<ExtractionWorkIdentity>,
}

#[doc(hidden)]
#[derive(Debug)]
pub struct PrefabExtractionSavedGenerationError {
    report: ScenePatchReport,
    environment: Option<usize>,
    source: Option<usize>,
    failure: Option<crate::SceneCompilationFailure>,
}

impl PrefabExtractionSavedGenerationError {
    pub fn report(&self) -> &ScenePatchReport {
        &self.report
    }
    pub fn into_report(self) -> ScenePatchReport {
        self.report
    }
    pub fn environment_index(&self) -> Option<usize> {
        self.environment
    }
    pub fn source_index(&self) -> Option<usize> {
        self.source
    }
    pub fn compilation_failure(&self) -> Option<&crate::SceneCompilationFailure> {
        self.failure.as_ref()
    }
}

impl fmt::Debug for PrefabExtractionSavedGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionSavedGeneration")
            .field("sources", &self.inner.sources().len())
            .field("published", &self.inner.published())
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionSavedGeneration {
    pub fn sources(&self) -> &[CapturedExtractionSource] {
        self.inner.sources()
    }
    pub fn published(&self) -> &[bool] {
        self.inner.published()
    }
    pub fn extracted(&self) -> &ResolvedPrefabSource {
        self.inner.extracted()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabExtractionUsage {
    pub work_nodes: usize,
    pub work_bytes: usize,
}

impl fmt::Debug for PrefabExtractionWork {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionWork")
            .field("usage", &self.usage())
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionWork {
    /// Admits the operation's own identity and wrapper before creating them.
    pub fn new(limits: SceneCompilationLimits) -> Result<Self, ScenePatchReport> {
        let kernel = ExtractionWork::new(
            limits,
            size_of::<Self>().checked_add(2 * size_of::<usize>()),
        )
        .map_err(|_| capacity())?;
        Ok(Self {
            identity: Arc::new(ExtractionWorkIdentity),
            kernel,
        })
    }

    #[must_use]
    pub fn usage(&self) -> PrefabExtractionUsage {
        let (work_nodes, work_bytes) = self.kernel.usage();
        PrefabExtractionUsage {
            work_nodes,
            work_bytes,
        }
    }

    /// The enclosing owner admits its finite capture tables and guards before copying them.
    /// This debits the same allowance later consumed by the actual extraction compiler.
    pub fn admit_capture_storage(&mut self, bytes: Option<usize>) -> Result<(), ScenePatchReport> {
        self.admit_capture_work(1, bytes)
    }

    /// Admits a known-cardinality capture scan before its first element is inspected.
    pub fn admit_capture_work(
        &mut self,
        nodes: usize,
        bytes: Option<usize>,
    ) -> Result<(), ScenePatchReport> {
        self.kernel
            .visit(nodes, 0, bytes.ok_or_else(capacity)?)
            .map_err(|_| capacity())
    }
}

fn capacity() -> ScenePatchReport {
    history_error_report(
        "scene.prefab-extraction-capacity",
        "Prefab extraction refused before exceeding its cumulative owning allowance",
    )
}

fn stale() -> ScenePatchReport {
    history_error_report(
        "scene.prefab-extraction-stale",
        "Prefab extraction requires its original source, history, basis and operation allowance",
    )
}

fn capture_error(error: SourceCheckpointError) -> ScenePatchReport {
    match error {
        SourceCheckpointError::Stale => stale(),
        SourceCheckpointError::AuthorityUnavailable => history_error_report(
            "scene.prefab-extraction-authority",
            "Prefab extraction requires the actual published authoring source",
        ),
    }
}

macro_rules! extraction_input {
    ($input:ident, $session:ident, $document:ty, $kind:ident, $inputs:expr, $detached:expr) => {
        /// A share of the actual original source/classification, with exact live observations.
        /// The original Session owns its history throughout capture and worker preparation.
        #[doc(hidden)]
        #[derive(Debug)]
        #[must_use]
        pub struct $input {
            source: FrozenSource<$document>,
            work: Arc<ExtractionWorkIdentity>,
        }

        impl $session {
            #[doc(hidden)]
            pub fn capture_prefab_extraction_input(
                &self,
                work: &mut PrefabExtractionWork,
            ) -> Result<$input, ScenePatchReport> {
                if !($detached)(self) {
                    return Err(history_error_report(
                        "scene.prefab-extraction-live-source",
                        "Prefab extraction source capture requires a detached Scene",
                    ));
                }
                let borrowed =
                    BorrowedSource::from_session(&self.document, &self.authority, &self.history)
                        .map_err(capture_error)?;
                admit_incumbent(
                    &self.document.placements,
                    &self.document.entities,
                    ($inputs)(self.document.as_ref()),
                    borrowed.state,
                    &self.history,
                    0,
                    |_, nodes, bytes| work.admit_capture_work(nodes, bytes),
                )?;
                let source = FrozenSource::capture(borrowed, || {
                    work.admit_capture_storage(Some(size_of::<$input>()))
                })?;
                Ok($input {
                    source,
                    work: Arc::clone(&work.identity),
                })
            }

            /// Rechecks the original owner without installing a source or changing history.
            /// Navigation and worker transfer do not invalidate this source-local observation.
            #[doc(hidden)]
            pub fn validate_prefab_extraction_input(
                &self,
                input: &$input,
                work: &mut PrefabExtractionWork,
            ) -> Result<(), ScenePatchReport> {
                work.admit_capture_storage(Some(size_of::<$input>()))?;
                if !Arc::ptr_eq(&input.work, &work.identity) {
                    return Err(stale());
                }
                if !($detached)(self) {
                    return Err(stale());
                }
                input
                    .source
                    .require_current(
                        &self.document,
                        &self.authority,
                        &self.history,
                        SourceDocumentKind::$kind,
                        &input.source.state.schema_basis(),
                    )
                    .map_err(capture_error)
            }
        }

        impl $input {
            #[must_use]
            pub fn document(&self) -> &$document {
                &self.source.document
            }

            /// A shared immutable input; the enclosing owner admits its own wrapper storage.
            #[must_use]
            pub fn shared_document(&self) -> Arc<$document> {
                Arc::clone(&self.source.document)
            }

            #[must_use]
            pub fn revision(&self) -> SceneAuthoringRevision {
                self.source.snapshot.revision
            }

            #[must_use]
            pub fn content_state(&self) -> SceneAuthoringContentState {
                self.source.snapshot.content_state
            }

            #[must_use]
            pub fn history_limits(&self) -> SceneAuthoringHistoryLimits {
                self.source.snapshot.history_limits
            }

            #[must_use]
            pub fn file_limits(&self) -> SceneFileLimits {
                self.source.state.file_limits()
            }

            #[must_use]
            pub fn source(&self) -> &AuthoringDocumentSource {
                self.source.state.source()
            }

            #[must_use]
            pub fn schema_basis(&self) -> AuthoringSchemaBasis {
                self.source.state.schema_basis()
            }

            /// Retains readmission from this exact frozen classification, never a live Session.
            /// The caller's original allowance admits the wrapper before sharing the state.
            pub fn authoring_readmission(
                &self,
                admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
            ) -> Result<AuthoringDocumentReadmission, ScenePatchReport> {
                extraction_readmission(&self.source.state, self.history_limits(), admit)
            }

            /// Admits actual source-sized codec and one readmission attempt's working storage.
            pub fn admit_persistence_work(
                &self,
                admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
            ) -> Result<(), ScenePatchReport> {
                extraction_persistence_work(
                    &self.source.document.placements,
                    &self.source.document.entities,
                    ($inputs)(self.source.document.as_ref()),
                    &self.source.state,
                    admit,
                )
            }
        }
    };
}

fn scene_inputs(
    _: &SceneDocument,
) -> Option<&std::collections::BTreeSet<nara_identity::ReferenceInputId>> {
    None
}

fn prefab_inputs(
    document: &PrefabDocument,
) -> Option<&std::collections::BTreeSet<nara_identity::ReferenceInputId>> {
    Some(&document.inputs)
}

extraction_input!(
    SceneSourceExtractionInput,
    SceneAuthoringSession,
    SceneDocument,
    Scene,
    scene_inputs,
    |session: &SceneAuthoringSession| session.live_instance().is_none()
);
extraction_input!(
    PrefabSourceExtractionInput,
    PrefabAuthoringSession,
    PrefabDocument,
    Prefab,
    prefab_inputs,
    |_: &PrefabAuthoringSession| true
);

/// Fixed proposed Q identity. The Project owner separately proves file and sidecar absence.
#[doc(hidden)]
#[derive(Debug)]
pub struct PrefabExtractionDestination {
    pub key: SourceDocumentKey,
    pub reference: AssetRef,
    pub metadata: AssetRecord,
}

/// Immutable complete-corpus preparation input, without live-source or publication authority.
///
/// The capture owner supplies its actual open/closed source allocations, associations and basis.
/// Those incumbents and decoded buffers must already be charged to this operation's Work.
#[doc(hidden)]
#[must_use]
pub struct PrefabExtractionRequest {
    inner: ExtractionRequest,
}

impl fmt::Debug for PrefabExtractionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionRequest")
            .field("selected", &self.inner.selected)
            .field("root", &self.inner.root)
            .field("source_count", &self.inner.sources.len())
            .field("environment_count", &self.inner.environments.len())
            .field("selected_environment", &self.inner.selected_environment)
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionRequest {
    #[must_use]
    pub fn new(
        sources: Vec<CapturedExtractionSource>,
        environments: Vec<PrefabExtractionEnvironment>,
        selected_environment: usize,
        basis: AuthoringSchemaBasis,
        selected: SourceDocumentKey,
        root: SceneEntityId,
        destination: PrefabExtractionDestination,
    ) -> Self {
        Self {
            inner: ExtractionRequest {
                sources,
                environments,
                selected_environment,
                basis,
                selected,
                root,
                destination: ExtractionDestination {
                    key: destination.key,
                    reference: destination.reference,
                    metadata: destination.metadata,
                },
                max_depth: SceneCompilationOptions::default().max_depth,
            },
        }
    }

    #[must_use]
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.inner.max_depth = depth;
        self
    }

    pub fn sources(&self) -> &[CapturedExtractionSource] {
        &self.inner.sources
    }

    pub fn environments(&self) -> &[PrefabExtractionEnvironment] {
        &self.inner.environments
    }

    pub fn selected_environment(&self) -> usize {
        self.inner.selected_environment
    }

    /// Checks stored working/output pairs without constructing a second candidate corpus.
    /// Source and Q allocations are retained exactly; refused verification returns every input
    /// together with its original cumulative Work. This grants no live or file authority.
    pub fn admit_outputs(
        self,
        sources: Vec<CapturedExtractionSource>,
        extracted: ResolvedPrefabSource,
        mut work: PrefabExtractionWork,
    ) -> Result<PreparedPrefabExtraction, PrefabExtractionStoredAdmissionFailure> {
        let storage = sources
            .capacity()
            .checked_mul(size_of::<CapturedExtractionSource>())
            .and_then(|bytes| bytes.checked_add(size_of::<PreparedPrefabExtraction>()))
            .and_then(|bytes| {
                bytes.checked_add(size_of::<PrefabExtractionStoredAdmissionFailure>())
            });
        if let Err(report) = work.admit_capture_storage(storage) {
            return Err(PrefabExtractionStoredAdmissionFailure {
                request: self,
                sources,
                extracted,
                work,
                report,
                failure: None,
                omissions: Vec::new(),
                environment: None,
            });
        }
        let PrefabExtractionWork { identity, kernel } = work;
        match self.inner.admit_outputs(sources, extracted, kernel) {
            Ok(prepared) => {
                let (corpus, kernel) = prepared.into_parts();
                Ok(PreparedPrefabExtraction {
                    corpus,
                    work: PrefabExtractionWork { identity, kernel },
                })
            }
            Err(refused) => {
                let (report, failure, omissions) = refusal_report(refused.error);
                Err(PrefabExtractionStoredAdmissionFailure {
                    request: Self {
                        inner: refused.request,
                    },
                    sources: refused.sources,
                    extracted: refused.extracted,
                    work: PrefabExtractionWork {
                        identity,
                        kernel: refused.work,
                    },
                    report,
                    failure,
                    omissions,
                    environment: refused.environment,
                })
            }
        }
    }

    /// Consumes the actual captured allowance. Refusal returns the exact request allocations
    /// and spent Work; successful preparation grants only borrowed source review.
    pub fn prepare(
        self,
        mut work: PrefabExtractionWork,
    ) -> Result<PreparedPrefabExtraction, PrefabExtractionFailure> {
        if let Err(report) = work.admit_capture_storage(Some(size_of::<PreparedPrefabExtraction>()))
        {
            return Err(PrefabExtractionFailure {
                request: self,
                work,
                report,
                failure: None,
                omissions: Vec::new(),
                environment: None,
            });
        }
        let PrefabExtractionWork { identity, kernel } = work;
        match self.inner.prepare(kernel) {
            Ok(prepared) => {
                let (corpus, kernel) = prepared.into_parts();
                Ok(PreparedPrefabExtraction {
                    corpus,
                    work: PrefabExtractionWork { identity, kernel },
                })
            }
            Err(refused) => {
                let (report, failure, omissions) = refusal_report(refused.error);
                Err(PrefabExtractionFailure {
                    request: Self {
                        inner: refused.request,
                    },
                    work: PrefabExtractionWork {
                        identity,
                        kernel: refused.work,
                    },
                    report,
                    failure,
                    omissions,
                    environment: refused.environment,
                })
            }
        }
    }
}

/// Owner-created source construction and complete correspondence, retaining original inputs.
/// This has no Session mutation, saved checkpoint, file receipt or Native execution authority.
#[doc(hidden)]
#[must_use]
pub struct PreparedPrefabExtraction {
    corpus: VerifiedExtractionCorpus,
    work: PrefabExtractionWork,
}

impl fmt::Debug for PreparedPrefabExtraction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedPrefabExtraction")
            .field("selected", &self.corpus.original().selected)
            .field("usage", &self.usage())
            .finish_non_exhaustive()
    }
}

impl PreparedPrefabExtraction {
    /// Uses exact verified outputs for published members and original disk data otherwise.
    /// Refusal retains this prepared owner and all spent work for an explicit revised Review.
    pub fn prepare_saved_generation(
        &mut self,
        disk: &[CapturedExtractionSource],
        published: &[bool],
    ) -> Result<PrefabExtractionSavedGeneration, PrefabExtractionSavedGenerationError> {
        let result = self
            .work
            .admit_capture_storage(Some(
                size_of::<PrefabExtractionSavedGeneration>()
                    + size_of::<PrefabExtractionSavedGenerationError>(),
            ))
            .map_err(|report| PrefabExtractionSavedGenerationError {
                report,
                environment: None,
                source: None,
                failure: None,
            });
        result?;
        self.corpus
            .prepare_saved_generation(disk, published, &mut self.work.kernel)
            .map(|inner| PrefabExtractionSavedGeneration {
                inner,
                work: Arc::clone(&self.work.identity),
            })
            .map_err(
                |SavedGenerationRefusal {
                     environment,
                     source,
                     error,
                 }| {
                    let (report, failure, _) = refusal_report(error);
                    PrefabExtractionSavedGenerationError {
                        report,
                        environment,
                        source,
                        failure,
                    }
                },
            )
    }

    /// A proof cannot be transferred to another operation with equal source bytes.
    pub fn require_saved_generation(
        &mut self,
        generation: &PrefabExtractionSavedGeneration,
    ) -> Result<(), ScenePatchReport> {
        self.work
            .admit_capture_storage(Some(size_of::<PrefabExtractionSavedGeneration>()))?;
        if !Arc::ptr_eq(&self.work.identity, &generation.work)
            || !std::ptr::eq(
                self.extracted().document(),
                generation.extracted().document(),
            )
        {
            return Err(stale());
        }
        Ok(())
    }

    /// Borrows the actual captured interpretation contexts used by every correspondence proof.
    pub fn environments(&self) -> &[PrefabExtractionEnvironment] {
        &self.corpus.original().environments
    }

    pub fn selected_environment(&self) -> usize {
        self.corpus.original().selected_environment
    }
    /// Actual complete source intent changed, as reported by its construction/rewrite owner.
    /// Allocation replacement and a potential dependency context do not imply this fact.
    pub fn source_changed(&self, source: usize) -> Option<bool> {
        self.corpus.source_changed(source)
    }
    pub fn original_sources(&self) -> &[CapturedExtractionSource] {
        &self.corpus.original().sources
    }
    pub fn sources(&self) -> &[CapturedExtractionSource] {
        self.corpus.sources()
    }
    pub fn extracted(&self) -> &ResolvedPrefabSource {
        self.corpus.extracted()
    }
    pub fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.corpus.original().basis
    }
    pub fn selected_source(&self) -> &SourceDocumentKey {
        &self.corpus.original().selected
    }
    pub fn root(&self) -> &SceneEntityId {
        &self.corpus.original().root
    }
    pub fn usage(&self) -> PrefabExtractionUsage {
        self.work.usage()
    }

    /// Debits later finite preparation/guard storage without exposing a replaceable Work owner.
    pub fn admit_binding_storage(&mut self, bytes: Option<usize>) -> Result<(), ScenePatchReport> {
        self.work.admit_capture_storage(bytes)
    }

    /// Matches one actual open-source capture with its original corpus member. An index is
    /// snapshot-local lookup only; exact allocations, basis and operation identity are checked.
    pub fn require_scene_input(
        &mut self,
        source: usize,
        input: &SceneSourceExtractionInput,
    ) -> Result<(), ScenePatchReport> {
        self.work
            .admit_capture_storage(Some(size_of::<SceneSourceExtractionInput>()))?;
        let Some(CapturedExtractionSource::Scene {
            document, basis, ..
        }) = self.original_sources().get(source)
        else {
            return Err(stale());
        };
        if !Arc::ptr_eq(&self.work.identity, &input.work)
            || !Arc::ptr_eq(document, &input.source.document)
            || *basis != input.schema_basis()
        {
            return Err(stale());
        }
        Ok(())
    }

    pub fn require_prefab_input(
        &mut self,
        source: usize,
        input: &PrefabSourceExtractionInput,
    ) -> Result<(), ScenePatchReport> {
        self.work
            .admit_capture_storage(Some(size_of::<PrefabSourceExtractionInput>()))?;
        let Some(CapturedExtractionSource::Prefab { source, .. }) =
            self.original_sources().get(source)
        else {
            return Err(stale());
        };
        if !Arc::ptr_eq(&self.work.identity, &input.work)
            || !std::ptr::eq(source.document(), input.document())
            || !source.matches_authority(crate::schema_authority::SchemaAuthority::Authoring(
                &input.schema_basis(),
            ))
        {
            return Err(stale());
        }
        Ok(())
    }

    pub fn validate_scene_input(
        &mut self,
        source: usize,
        input: &SceneSourceExtractionInput,
        session: &SceneAuthoringSession,
    ) -> Result<(), ScenePatchReport> {
        self.require_scene_input(source, input)?;
        session.validate_prefab_extraction_input(input, &mut self.work)
    }

    pub fn validate_prefab_input(
        &mut self,
        source: usize,
        input: &PrefabSourceExtractionInput,
        session: &PrefabAuthoringSession,
    ) -> Result<(), ScenePatchReport> {
        self.require_prefab_input(source, input)?;
        session.validate_prefab_extraction_input(input, &mut self.work)
    }
}

/// Refused exclusive binding returns the exact move-only candidate without installing it.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct PrefabExtractionSourceBindFailure<P> {
    prepared: P,
    report: ScenePatchReport,
}

impl<P> PrefabExtractionSourceBindFailure<P> {
    pub fn report(&self) -> &ScenePatchReport {
        &self.report
    }
    pub fn into_parts(self) -> (P, ScenePatchReport) {
        (self.prepared, self.report)
    }
}

macro_rules! extracted_source {
    ($prepared:ident, $commit:ident, $session:ident, $document:ty, $input:ty, $variant:ident,
     $prepare:ident, $classify:ident, $require:ident, $checkpoint:ident, $inputs:expr,
     $detached:expr, $install:expr) => {
        /// An actual verified working-source successor under its original nominal Schema basis.
        /// It contains no saved token or file receipt; persistence binds those separately.
        #[doc(hidden)]
        #[derive(Debug)]
        #[must_use]
        pub struct $prepared {
            original: Arc<$document>,
            document: Arc<$document>,
            checkpoint: PreparedSourceCheckpoint,
            work: Arc<ExtractionWorkIdentity>,
        }

        impl $prepared {
            pub fn document(&self) -> &$document {
                &self.document
            }
            pub fn shared_document(&self) -> Arc<$document> {
                Arc::clone(&self.document)
            }
            pub fn source_key(&self) -> &SourceDocumentKey {
                &self.checkpoint.source_key
            }
            pub fn source(&self) -> &AuthoringDocumentSource {
                self.checkpoint.state.source()
            }
            pub fn base_revision(&self) -> SceneAuthoringRevision {
                self.checkpoint.base_revision
            }
            pub fn base_content_state(&self) -> SceneAuthoringContentState {
                self.checkpoint.base_content_state
            }
            pub fn revision(&self) -> SceneAuthoringRevision {
                self.checkpoint.history.revision()
            }
            pub fn content_state(&self) -> SceneAuthoringContentState {
                self.checkpoint.history.content_state()
            }
            pub fn schema_basis(&self) -> AuthoringSchemaBasis {
                self.checkpoint.state.schema_basis()
            }
            pub fn history_limits(&self) -> SceneAuthoringHistoryLimits {
                self.checkpoint.history.limits()
            }
            pub fn file_limits(&self) -> SceneFileLimits {
                self.checkpoint.state.file_limits()
            }

            /// Retains the prepared future classification under the enclosing corpus allowance.
            pub fn authoring_readmission(
                &self,
                admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
            ) -> Result<AuthoringDocumentReadmission, ScenePatchReport> {
                extraction_readmission(&self.checkpoint.state, self.history_limits(), admit)
            }

            /// Admits codec and readmission work against the actual future source and policy.
            pub fn admit_persistence_work(
                &self,
                admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
            ) -> Result<(), ScenePatchReport> {
                extraction_persistence_work(
                    &self.document.placements,
                    &self.document.entities,
                    ($inputs)(self.document.as_ref()),
                    &self.checkpoint.state,
                    admit,
                )
            }
        }

        impl PreparedPrefabExtraction {
            /// Rechecks that a prepared checkpoint is this corpus's exact output and Work.
            /// The complete Workspace owner uses this before binding all source destinations.
            pub fn $checkpoint(
                &mut self,
                source: usize,
                checkpoint: &$prepared,
            ) -> Result<(), ScenePatchReport> {
                self.work
                    .admit_capture_storage(Some(size_of::<$prepared>()))?;
                let Some(output) = self.corpus.sources().get(source) else {
                    return Err(stale());
                };
                let output = source_document!($variant, output).ok_or_else(stale)?;
                let key = self.corpus.original().sources[source].key();
                self.work
                    .admit_capture_storage(key.checked_retained_bytes().and_then(|bytes| {
                        bytes.checked_add(checkpoint.source_key().checked_retained_bytes()?)
                    }))?;
                if !Arc::ptr_eq(&self.work.identity, &checkpoint.work)
                    || !Arc::ptr_eq(output, &checkpoint.document)
                    || key != checkpoint.source_key()
                    || self.source_changed(source) != Some(true)
                {
                    return Err(stale());
                }
                Ok(())
            }

            /// Classifies the actual verified changed output; accepts no caller-supplied value
            /// or document. Refusal leaves the corpus/captured input intact and work spent.
            pub fn $prepare(
                &mut self,
                source: usize,
                input: &$input,
            ) -> Result<$prepared, ScenePatchReport> {
                self.$require(source, input)?;
                if self.source_changed(source) != Some(true) {
                    return Err(history_error_report(
                        "scene.prefab-extraction-unchanged-source",
                        "An unchanged captured source is a guard, not a replacement checkpoint",
                    ));
                }
                let document =
                    source_document!($variant, &self.corpus.sources()[source]).ok_or_else(stale)?;
                let key = self.corpus.original().sources[source].key();
                // Future source/history/checkpoint wrapper and Arc controls coexist with the
                // verified corpus. Original source/classification/history remain capture-owned.
                self.work.admit_capture_storage(
                    size_of::<$prepared>()
                        .checked_add(2 * size_of::<usize>())
                        .and_then(|bytes| bytes.checked_add(key.checked_retained_bytes()?)),
                )?;
                let next = input.source.state.$classify(document, &mut |_, bytes| {
                    self.work.admit_capture_storage(bytes)
                })?;
                if next.schema_basis() != input.schema_basis()
                    || next.source() != input.source()
                    || next.document_kind() != input.source.state.document_kind()
                    || next.file_limits() != input.file_limits()
                {
                    return Err(stale());
                }
                Ok($prepared {
                    original: Arc::clone(&input.source.document),
                    document: Arc::clone(document),
                    checkpoint: PreparedSourceCheckpoint::new(
                        key,
                        input.source.snapshot,
                        Arc::clone(&input.source.state),
                        next,
                    ),
                    work: Arc::clone(&self.work.identity),
                })
            }
        }

        impl $session {
            #[doc(hidden)]
            pub fn validate_extracted_source(
                &self,
                prepared: &$prepared,
            ) -> Result<(), ScenePatchReport> {
                if !($detached)(self) || !Arc::ptr_eq(&self.document, &prepared.original) {
                    return Err(stale());
                }
                prepared
                    .checkpoint
                    .require_current(
                        prepared.source_key(),
                        &self.authority,
                        &self.history,
                        SourceDocumentKind::$variant,
                        crate::AuthoringDocumentKind::$variant,
                    )
                    .map_err(capture_error)
            }

            /// Holds the original source exclusively for the complete-set owner's final swap.
            /// Dropping the guard changes nothing; no file publication is implied.
            #[doc(hidden)]
            pub fn bind_extracted_source(
                &mut self,
                prepared: $prepared,
            ) -> Result<$commit<'_>, PrefabExtractionSourceBindFailure<$prepared>> {
                if let Err(report) = self.validate_extracted_source(&prepared) {
                    return Err(PrefabExtractionSourceBindFailure { prepared, report });
                }
                Ok($commit {
                    session: self,
                    prepared,
                })
            }
        }

        #[doc(hidden)]
        #[derive(Debug)]
        #[must_use]
        pub struct $commit<'a> {
            session: &'a mut $session,
            prepared: $prepared,
        }

        impl<'a> $commit<'a> {
            pub fn prepared(&self) -> &$prepared {
                &self.prepared
            }
            pub fn into_prepared(self) -> $prepared {
                self.prepared
            }
            /// Installs prebound source/classification/history only, without lookup/allocation.
            /// The complete owner must bind every participant and persistence destination first.
            pub fn commit(self) -> &'a mut $session {
                self.session.document = self.prepared.document;
                self.session.history = self.prepared.checkpoint.history;
                self.session.authority =
                    AuthoringSessionAuthority::Published(self.prepared.checkpoint.state);
                ($install)(self.session);
                self.session
            }
        }
    };
}

fn extraction_readmission(
    state: &Arc<crate::authoring_decode::AuthoringDocumentState>,
    history_limits: SceneAuthoringHistoryLimits,
    mut admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<AuthoringDocumentReadmission, ScenePatchReport> {
    // The actual immutable classification is already capture-owned. Readmission shares it;
    // codec/decode/classification work is admitted separately for each actual attempt.
    admit(Some(size_of::<AuthoringDocumentReadmission>()))?;
    Ok(state.readmission(history_limits))
}

fn extraction_persistence_work(
    placements: &[SceneEntityPlacement],
    entities: &[SceneEntityRecord],
    inputs: Option<&std::collections::BTreeSet<nara_identity::ReferenceInputId>>,
    state: &crate::authoring_decode::AuthoringDocumentState,
    mut admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<(), ScenePatchReport> {
    let raw = super::prefab_conversion::retention::document_dynamic_bytes_with_admission(
        placements,
        entities,
        state.file_limits(),
        &mut admit,
    )?;
    let inputs = match inputs {
        Some(inputs) => {
            admit(
                inputs
                    .len()
                    .checked_mul(size_of::<nara_identity::ReferenceInputId>()),
            )?;
            crate::authoring_decode::retained_prefab_inputs_bytes(inputs)
        }
        None => Some(0),
    };
    // Admit actual source copies, ordering/serializer input and decoded storage. Schema-derived
    // defaults and effective-value validation are not bounded by raw source or retained facts;
    // actual readmission debits those through the classifier's admission callback.
    // Output capacity is separately charged by the codec before each buffer growth.
    let storage = raw
        .and_then(|bytes| bytes.checked_add(inputs?))
        .and_then(|bytes| bytes.checked_mul(4))
        .and_then(|bytes| {
            bytes.checked_add(size_of::<SceneDocument>().max(size_of::<PrefabDocument>()))
        });
    admit(storage)
}

macro_rules! source_document {
    (Scene, $source:expr) => {
        match $source {
            CapturedExtractionSource::Scene { document, .. } => Some(document),
            _ => None,
        }
    };
    (Prefab, $source:expr) => {
        match $source {
            CapturedExtractionSource::Prefab { source, .. } => source.authoring_source_allocation(),
            _ => None,
        }
    };
}

extracted_source!(
    PreparedSceneExtractionSource,
    SceneExtractionSourceCommit,
    SceneAuthoringSession,
    SceneDocument,
    SceneSourceExtractionInput,
    Scene,
    prepare_scene_source,
    classify_extracted_scene,
    require_scene_input,
    require_scene_checkpoint,
    scene_inputs,
    |session: &SceneAuthoringSession| session.live_instance().is_none(),
    |session: &mut SceneAuthoringSession| session.live_dirty = true
);
extracted_source!(
    PreparedPrefabExtractionSource,
    PrefabExtractionSourceCommit,
    PrefabAuthoringSession,
    PrefabDocument,
    PrefabSourceExtractionInput,
    Prefab,
    prepare_prefab_source,
    classify_extracted_prefab,
    require_prefab_input,
    require_prefab_checkpoint,
    prefab_inputs,
    |_: &PrefabAuthoringSession| true,
    |_: &mut PrefabAuthoringSession| {}
);

#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct PrefabExtractionFailure {
    request: PrefabExtractionRequest,
    work: PrefabExtractionWork,
    report: ScenePatchReport,
    failure: Option<crate::SceneCompilationFailure>,
    omissions: Vec<crate::prefab::AuthoringSceneCompilationOmission>,
    environment: Option<usize>,
}

/// Pure stored-generation refusal preserves the exact request, fixed outputs and spent Work.
#[doc(hidden)]
#[must_use]
pub struct PrefabExtractionStoredAdmissionFailure {
    request: PrefabExtractionRequest,
    sources: Vec<CapturedExtractionSource>,
    extracted: ResolvedPrefabSource,
    work: PrefabExtractionWork,
    report: ScenePatchReport,
    failure: Option<crate::SceneCompilationFailure>,
    omissions: Vec<crate::prefab::AuthoringSceneCompilationOmission>,
    environment: Option<usize>,
}

impl fmt::Debug for PrefabExtractionStoredAdmissionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionStoredAdmissionFailure")
            .field("request", &self.request)
            .field("source_count", &self.sources.len())
            .field("extracted", self.extracted.key())
            .field("usage", &self.work.usage())
            .field("report", &self.report)
            .field("environment", &self.environment)
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionStoredAdmissionFailure {
    pub fn report(&self) -> &ScenePatchReport {
        &self.report
    }
    pub fn environment_index(&self) -> Option<usize> {
        self.environment
    }
    pub fn compilation_failure(&self) -> Option<&crate::SceneCompilationFailure> {
        self.failure.as_ref()
    }
    pub fn omissions(&self) -> &[crate::prefab::AuthoringSceneCompilationOmission] {
        &self.omissions
    }

    pub fn into_parts(
        self,
    ) -> (
        PrefabExtractionRequest,
        Vec<CapturedExtractionSource>,
        ResolvedPrefabSource,
        PrefabExtractionWork,
        ScenePatchReport,
    ) {
        (
            self.request,
            self.sources,
            self.extracted,
            self.work,
            self.report,
        )
    }
}

impl PrefabExtractionFailure {
    /// Request-local interpretation context of the refusal. No canonical source identity changes.
    pub fn environment_index(&self) -> Option<usize> {
        self.environment
    }
    pub fn environment_name(&self) -> Option<&str> {
        self.request
            .inner
            .environments
            .get(self.environment?)
            .map(|environment| environment.name.as_str())
    }
    pub fn report(&self) -> &ScenePatchReport {
        &self.report
    }
    pub fn compilation_failure(&self) -> Option<&crate::SceneCompilationFailure> {
        self.failure.as_ref()
    }
    pub fn omissions(&self) -> &[crate::prefab::AuthoringSceneCompilationOmission] {
        &self.omissions
    }
    pub fn into_parts(
        self,
    ) -> (
        PrefabExtractionRequest,
        PrefabExtractionWork,
        ScenePatchReport,
    ) {
        (self.request, self.work, self.report)
    }
}

fn refusal_report(
    error: ExtractionError,
) -> (
    ScenePatchReport,
    Option<crate::SceneCompilationFailure>,
    Vec<crate::prefab::AuthoringSceneCompilationOmission>,
) {
    let (code, summary) = match error {
        ExtractionError::Capacity => return (capacity(), None, Vec::new()),
        ExtractionError::Compilation {
            diagnostics,
            failure,
            omissions,
        } => {
            return (
                ScenePatchReport {
                    applied: false,
                    inverse: None,
                    diagnostics,
                },
                failure,
                omissions,
            );
        }
        ExtractionError::InvalidSource => (
            "scene.prefab-extraction-invalid-source",
            "Prefab extraction cannot establish complete source correspondence",
        ),
        ExtractionError::SavedGuardRequiresRelocation => (
            "scene.prefab-extraction-saved-guard-relocation",
            "An unpublished disk source still requires extraction route changes; explicitly save its current source or repair its stored declarations",
        ),
        ExtractionError::UnknownSchema => (
            "scene.prefab-extraction-unknown-schema",
            "Prefab extraction requires the affected component Schema",
        ),
        ExtractionError::AdapterUnavailable => (
            "scene.prefab-extraction-adapter-unavailable",
            "Prefab extraction requires the affected component authoring Adapter",
        ),
        ExtractionError::VersionMismatch => (
            "scene.prefab-extraction-version-mismatch",
            "Prefab extraction requires the affected component version",
        ),
        ExtractionError::OwnerMismatch => (
            "scene.prefab-extraction-owner-mismatch",
            "Prefab extraction requires the affected component Schema owner",
        ),
        ExtractionError::Value(_) => (
            "scene.prefab-extraction-value",
            "Prefab extraction refused an invalid component value",
        ),
        ExtractionError::Property(_) => (
            "scene.prefab-extraction-property",
            "Prefab extraction refused an invalid property declaration",
        ),
        ExtractionError::Reference(_) => (
            "scene.prefab-extraction-reference",
            "Prefab extraction cannot preserve the declared reference",
        ),
    };
    (history_error_report(code, summary), None, Vec::new())
}
