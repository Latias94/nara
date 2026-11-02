//! Prepared source checkpoints for the reviewed authored-data upgrade.
//!
//! This is internal source preparation, not file publication or a saved checkpoint. The complete
//! Workspace/persistence owner must bind every participant and its actual new persistence leaf
//! before installing any source. Dropping either preparation or a bound guard changes no session.
//! Old history is retained by its session until commit; no old inverse is replayed under a new
//! Schema. All future source, classification and history storage is ready before binding.

use std::{mem::size_of, sync::Arc};

use nara_core::ByteLimit;

use super::schema_preparation::SchemaPreparationBudget;
use super::{
    AuthoringHistory, AuthoringSessionAuthority, PrefabAuthoringSession,
    SceneAuthoringContentState, SceneAuthoringHistoryLimits, SceneAuthoringRevision,
    SceneAuthoringSession, diagnostic_error, history_error_report,
};
use crate::{
    AuthoringDocumentKind, AuthoringDocumentReadiness, AuthoringDocumentSource,
    AuthoringSchemaBasis, PrefabDocument, PrefabDocumentCandidate, SceneDocument,
    SceneDocumentCandidate, SceneEntityRecord, SceneFileLimits, ScenePatchReport,
    SourceDocumentKey, SourceDocumentKind, UnavailableRecordManifest,
    authoring_decode::{AuthoringDocumentState, schema_upgrade::AuthoredFieldUpgrade},
};

mod input;
mod recovery;

pub use input::{PrefabAuthoredSchemaUpgradeInput, SceneAuthoredSchemaUpgradeInput};
pub use recovery::{
    validate_authored_upgrade_prefab_transition, validate_authored_upgrade_scene_transition,
};

pub(crate) type AdmissionCallback<'a> =
    dyn FnMut(&'static str, Option<usize>) -> Result<(), ScenePatchReport> + 'a;

/// Finite modeled work and coexisting source preparation storage for a reviewed upgrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoredSchemaUpgradeLimits {
    work_bytes: ByteLimit,
    retained_bytes: ByteLimit,
}

impl AuthoredSchemaUpgradeLimits {
    pub const fn new(work_bytes: ByteLimit, retained_bytes: ByteLimit) -> Self {
        Self {
            work_bytes,
            retained_bytes,
        }
    }

    pub const fn work_bytes(self) -> ByteLimit {
        self.work_bytes
    }

    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }
}

impl Default for AuthoredSchemaUpgradeLimits {
    fn default() -> Self {
        Self::new(
            ByteLimit::new(512 * 1024 * 1024).expect("nonzero upgrade work limit"),
            ByteLimit::new(256 * 1024 * 1024).expect("nonzero upgrade retention limit"),
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuthoredSchemaUpgradeUsage {
    pub work_bytes: usize,
    pub peak_retained_bytes: usize,
}

/// One allowance shared by the complete source set. Dropped preparations do not refund work.
///
/// Charge every incumbent before preparing any successor. Shared admitted Schema catalogs and
/// allocator capacity are excluded. The enclosing Host owns proposal decoding and disk buffers.
#[derive(Debug)]
pub struct AuthoredSchemaUpgradeWork {
    identity: Arc<UpgradeWorkIdentity>,
    budget: SchemaPreparationBudget,
}

#[derive(Debug)]
struct UpgradeWorkIdentity;

impl Default for AuthoredSchemaUpgradeWork {
    fn default() -> Self {
        Self::new(AuthoredSchemaUpgradeLimits::default())
    }
}

impl AuthoredSchemaUpgradeWork {
    pub fn new(limits: AuthoredSchemaUpgradeLimits) -> Self {
        Self {
            identity: Arc::new(UpgradeWorkIdentity),
            budget: SchemaPreparationBudget::new(limits.work_bytes, limits.retained_bytes),
        }
    }

    pub fn usage(&self) -> AuthoredSchemaUpgradeUsage {
        let (work_bytes, peak_retained_bytes) = self.budget.usage();
        AuthoredSchemaUpgradeUsage {
            work_bytes,
            peak_retained_bytes,
        }
    }

    /// Settles storage from one source-owning preparation attempt on refusal.
    ///
    /// On `Err`, the callback must have dropped every output and retention token charged during
    /// this attempt. Its error and side effects must not retain those owners. Entry storage,
    /// cumulative work, the historical peak and this allowance's nominal identity are preserved.
    /// On `Ok`, outputs remain charged normally. A successful nested attempt remains charged
    /// until its enclosing attempt succeeds or drops those outputs on refusal.
    pub fn with_preparation_attempt<T, E>(
        &mut self,
        prepare: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let checkpoint = self.budget.retention_checkpoint();
        let result = prepare(self);
        if result.is_err() {
            // Refusal carries no charged preparation; the callback's candidate locals have dropped.
            self.budget.restore_retention(checkpoint);
        }
        result
    }

    /// Admits the complete-set owner's storage before its allocation or copy.
    pub fn retain_preparation_storage(&mut self, bytes: usize) -> Result<(), ScenePatchReport> {
        self.begin_step()?;
        self.reserve("preparation-storage", Some(bytes))?;
        self.retain_output(Some(bytes))
    }

    /// Admits a finite consumer's temporary preparation work while its incumbents stay retained.
    /// Work is cumulative; only the step's modeled scratch ends after the supplied operation.
    pub fn with_preparation_work<T>(
        &mut self,
        stage: &'static str,
        bytes: Option<usize>,
        prepare: impl FnOnce() -> T,
    ) -> Result<T, ScenePatchReport> {
        self.begin_step()?;
        let result = self.reserve(stage, bytes).map(|()| prepare());
        self.budget.end_step();
        result
    }

    fn begin_step(&mut self) -> Result<(), ScenePatchReport> {
        self.budget
            .begin_step(Some(0))
            .map_err(upgrade_budget_error)
    }

    fn reserve(
        &mut self,
        stage: &'static str,
        bytes: Option<usize>,
    ) -> Result<(), ScenePatchReport> {
        self.budget
            .reserve(stage, bytes)
            .map_err(upgrade_budget_error)
    }

    fn retain_output(&mut self, bytes: Option<usize>) -> Result<(), ScenePatchReport> {
        self.budget
            .retain_output(bytes)
            .map_err(upgrade_budget_error)
    }

    fn require_retained(
        &self,
        retained: &RetainedAuthoredSchemaUpgradeSource,
        revision: SceneAuthoringRevision,
        basis: &AuthoringSchemaBasis,
        kind: SourceDocumentKind,
    ) -> Result<(), ScenePatchReport> {
        if !Arc::ptr_eq(&self.identity, &retained.work)
            || retained.revision != revision
            || &retained.previous != basis
            || retained.kind != kind
        {
            return Err(stale());
        }
        Ok(())
    }
}

/// A source incumbent charged to one exact allowance, revision and nominal Schema basis.
#[derive(Debug)]
pub struct RetainedAuthoredSchemaUpgradeSource {
    work: Arc<UpgradeWorkIdentity>,
    revision: SceneAuthoringRevision,
    previous: AuthoringSchemaBasis,
    kind: SourceDocumentKind,
}

/// Admits an unopened incumbent Scene under the operation's actual previous Schema basis.
///
/// The caller admits wire decoding and its input buffers before calling this factory. This
/// moves the decoded source without copying it, admits classification before allocation and
/// retains exactly one original Session, including its empty history. The returned token is
/// already charged to `work`: pass it directly to
/// [`SceneAuthoringSession::prepare_authored_upgrade_with_retained_source`] after every open
/// and unopened incumbent has been retained. Do not retain this Session a second time.
///
/// Refusal returns no Session or token and releases this step's temporary storage. Work already
/// performed remains cumulative; previously retained sources and preparations stay charged.
pub fn admit_authored_upgrade_scene_source(
    candidate: SceneDocumentCandidate,
    source: AuthoringDocumentSource,
    previous: &AuthoringSchemaBasis,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<(SceneAuthoringSession, RetainedAuthoredSchemaUpgradeSource), ScenePatchReport> {
    let (document, limits) = candidate.into_authoring_parts();
    work.begin_step()?;
    let result = (|| {
        let state = admit_unopened_source(
            &document,
            source,
            previous,
            AuthoringDocumentKind::Scene,
            Some(0),
            limits,
            work,
        )?;
        let history = AuthoringHistory::new(SceneAuthoringHistoryLimits::default());
        let retained = retained_source(&history, &state, SourceDocumentKind::Scene, work);
        Ok((
            SceneAuthoringSession {
                document: Arc::new(document),
                history,
                live_instance: None,
                live_dirty: true,
                source_upgrade_required: false,
                authority: AuthoringSessionAuthority::Published(Arc::new(state)),
            },
            retained,
        ))
    })();
    work.budget.end_step();
    result
}

/// Admits an unopened nominal Prefab and its already-retained original-source token.
///
/// This has the same complete-set and cumulative-work contract as
/// [`admit_authored_upgrade_scene_source`]. The classifier borrows the original owned placements
/// and records through its Scene-shaped structural input; no document is cloned and no Scene
/// Session is created. The returned Session, classification and token all retain Prefab kind.
pub fn admit_authored_upgrade_prefab_source(
    candidate: PrefabDocumentCandidate,
    source: AuthoringDocumentSource,
    previous: &AuthoringSchemaBasis,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<(PrefabAuthoringSession, RetainedAuthoredSchemaUpgradeSource), ScenePatchReport> {
    let (document, limits) = candidate.into_authoring_parts();
    let inputs = document.inputs;
    let inputs_bytes = crate::authoring_decode::retained_prefab_inputs_bytes(&inputs);
    let document = SceneDocument {
        placements: document.placements,
        entities: document.entities,
    };
    work.begin_step()?;
    let result = (|| {
        let state = admit_unopened_source(
            &document,
            source,
            previous,
            AuthoringDocumentKind::Prefab,
            inputs_bytes,
            limits,
            work,
        )?;
        let history = AuthoringHistory::new(SceneAuthoringHistoryLimits::default());
        let retained = retained_source(&history, &state, SourceDocumentKind::Prefab, work);
        Ok((
            PrefabAuthoringSession {
                document: Arc::new(PrefabDocument {
                    inputs,
                    placements: document.placements,
                    entities: document.entities,
                }),
                history,
                source_upgrade_required: false,
                authority: AuthoringSessionAuthority::Published(Arc::new(state)),
            },
            retained,
        ))
    })();
    work.budget.end_step();
    result
}

fn admit_unopened_source(
    document: &SceneDocument,
    source: AuthoringDocumentSource,
    previous: &AuthoringSchemaBasis,
    kind: AuthoringDocumentKind,
    inputs_bytes: Option<usize>,
    limits: SceneFileLimits,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<AuthoringDocumentState, ScenePatchReport> {
    let original = super::prefab_conversion::retention::document_dynamic_bytes_with_admission(
        &document.placements,
        &document.entities,
        limits,
        |bytes| work.reserve("authored-upgrade-source-scan", bytes),
    )?;
    let original = original.and_then(|bytes| bytes.checked_add(inputs_bytes?));
    work.reserve("authored-upgrade-unopened-source", original)?;
    let storage = incumbent_storage_bytes(kind);
    work.reserve("authored-upgrade-unopened-storage", storage)?;
    work.reserve(
        "classification-source",
        crate::provenance::provenance_asset_ref_bytes(source.as_asset_ref())
            .and_then(|bytes| bytes.checked_add(size_of::<AuthoringDocumentState>())),
    )?;
    let state = AuthoringDocumentState::classify_authored_upgrade_source(
        document,
        source,
        kind,
        previous,
        limits,
        &mut |stage, bytes| work.reserve(stage, bytes),
    )?;
    let classification = state.checked_retained_bytes_with_admission(|bytes| {
        work.reserve("authored-upgrade-classification-scan", bytes)
    })?;
    work.retain_output(
        original.and_then(|bytes| bytes.checked_add(storage?)?.checked_add(classification?)),
    )?;
    Ok(state)
}

fn retained_source(
    history: &AuthoringHistory,
    state: &AuthoringDocumentState,
    kind: SourceDocumentKind,
    work: &AuthoredSchemaUpgradeWork,
) -> RetainedAuthoredSchemaUpgradeSource {
    RetainedAuthoredSchemaUpgradeSource {
        work: Arc::clone(&work.identity),
        revision: history.revision(),
        previous: state.schema_basis(),
        kind,
    }
}

fn upgrade_budget_error(
    error: super::schema_preparation::SchemaPreparationError,
) -> ScenePatchReport {
    let diagnostic = crate::diagnostics::with_public_locator(
        crate::diagnostics::with_public_locator(
            diagnostic_error(
                "scene.authored-schema-upgrade-limit-exceeded",
                "Authored upgrade refused before its next owning preparation",
            ),
            "budget-kind",
            error.kind,
        ),
        "preparation-stage",
        error.stage,
    );
    let mut diagnostics = nara_diagnostic::DiagnosticReport::default();
    diagnostics.push(diagnostic);
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

/// Move-only Scene successor with the exact future identities needed by persistence binding.
#[derive(Debug)]
#[must_use]
pub struct PreparedSceneAuthoredSchemaUpgrade {
    original: Arc<SceneDocument>,
    document: Arc<SceneDocument>,
    checkpoint: PreparedCheckpoint,
}

/// Nominal Prefab successor; it never adopts a Scene projection as its source.
#[derive(Debug)]
#[must_use]
pub struct PreparedPrefabAuthoredSchemaUpgrade {
    original: Arc<PrefabDocument>,
    document: Arc<PrefabDocument>,
    checkpoint: PreparedCheckpoint,
}

/// A refused exclusive binding, retaining the exact move-only successor for reconciliation.
///
/// Binding changes no source or history. Recovering this owner never prepares another checkpoint
/// or refunds work already performed by the complete upgrade operation.
#[derive(Debug)]
#[must_use]
pub struct AuthoredSchemaUpgradeBindFailure<P> {
    prepared: P,
    report: ScenePatchReport,
}

impl<P> AuthoredSchemaUpgradeBindFailure<P> {
    pub fn report(&self) -> &ScenePatchReport {
        &self.report
    }

    pub fn into_parts(self) -> (P, ScenePatchReport) {
        (self.prepared, self.report)
    }
}

#[derive(Debug)]
struct PreparedCheckpoint {
    source_key: SourceDocumentKey,
    base_revision: SceneAuthoringRevision,
    base_content_state: SceneAuthoringContentState,
    original_state: Arc<AuthoringDocumentState>,
    state: Arc<AuthoringDocumentState>,
    history: AuthoringHistory,
}

/// Exact observations of the original history, without retaining or recreating its owner.
#[derive(Debug, Clone, Copy)]
struct UpgradeSourceSnapshot {
    revision: SceneAuthoringRevision,
    content_state: SceneAuthoringContentState,
    history_limits: SceneAuthoringHistoryLimits,
}

impl UpgradeSourceSnapshot {
    fn from_history(history: &AuthoringHistory) -> Self {
        Self {
            revision: history.revision(),
            content_state: history.content_state(),
            history_limits: history.limits(),
        }
    }
}

/// The same borrowed source feeds direct and detached preparation.
struct UpgradeSource<'a, D> {
    document: &'a Arc<D>,
    state: &'a Arc<AuthoringDocumentState>,
    snapshot: UpgradeSourceSnapshot,
}

impl<'a, D> UpgradeSource<'a, D> {
    fn from_session(
        document: &'a Arc<D>,
        authority: &'a AuthoringSessionAuthority,
        history: &AuthoringHistory,
    ) -> Result<Self, ScenePatchReport> {
        let AuthoringSessionAuthority::Published(state) = authority else {
            return Err(unavailable_authority());
        };
        Ok(Self {
            document,
            state,
            snapshot: UpgradeSourceSnapshot::from_history(history),
        })
    }
}

// Both nominal prepared values expose the same immutable checkpoint observations. There is no
// saved token: persistence must prove the exact future document/content state independently.
macro_rules! checkpoint_observations {
    ($prepared:ident, $document:ty) => {
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

            /// The already minted revision that commit will install, not a publication receipt.
            pub fn revision(&self) -> SceneAuthoringRevision {
                self.checkpoint.history.revision()
            }

            pub fn content_state(&self) -> SceneAuthoringContentState {
                self.checkpoint.history.content_state()
            }

            pub fn previous_basis(&self) -> AuthoringSchemaBasis {
                self.checkpoint.original_state.schema_basis()
            }

            pub fn schema_basis(&self) -> AuthoringSchemaBasis {
                self.checkpoint.state.schema_basis()
            }

            pub fn readiness(&self) -> AuthoringDocumentReadiness {
                self.checkpoint.state.readiness()
            }

            pub fn file_limits(&self) -> SceneFileLimits {
                self.checkpoint.state.file_limits()
            }

            pub fn history_limits(&self) -> SceneAuthoringHistoryLimits {
                self.checkpoint.history.limits()
            }

            pub fn unavailable_record_manifest(&self) -> &UnavailableRecordManifest {
                self.checkpoint.state.unavailable_manifest()
            }

            pub fn diagnostics(&self) -> &nara_diagnostic::DiagnosticReport {
                self.checkpoint.state.diagnostics()
            }
        }
    };
}

checkpoint_observations!(PreparedSceneAuthoredSchemaUpgrade, SceneDocument);
checkpoint_observations!(PreparedPrefabAuthoredSchemaUpgrade, PrefabDocument);

impl SceneAuthoringSession {
    /// Charges the incumbent before any document in the complete set prepares its successor.
    #[doc(hidden)]
    pub fn retain_authored_schema_upgrade_source(
        &self,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<RetainedAuthoredSchemaUpgradeSource, ScenePatchReport> {
        retain_incumbent(
            &self.authority,
            &self.history,
            &self.document.placements,
            &self.document.entities,
            SourceDocumentKind::Scene,
            work,
        )
    }

    /// Prepares one source under a bounded allowance, without installing or saving it.
    #[doc(hidden)]
    pub fn prepare_authored_upgrade(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: SceneDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedSceneAuthoredSchemaUpgrade, ScenePatchReport> {
        let retained = self.retain_authored_schema_upgrade_source(work)?;
        self.prepare_authored_upgrade_with_retained_source(
            source_key,
            base_revision,
            candidate,
            upgrade,
            retained,
            work,
        )
    }

    #[doc(hidden)]
    pub fn prepare_authored_upgrade_with_retained_source(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: SceneDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        retained: RetainedAuthoredSchemaUpgradeSource,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedSceneAuthoredSchemaUpgrade, ScenePatchReport> {
        work.require_retained(
            &retained,
            self.revision(),
            upgrade.previous_basis(),
            SourceDocumentKind::Scene,
        )?;
        UpgradeSource::from_session(&self.document, &self.authority, &self.history)?
            .prepare_with_work(source_key, base_revision, candidate, upgrade, work)
    }

    /// Prepares a new source incarnation under the actual published authoring basis.
    ///
    /// The caller supplies its actual logical source binding and one cumulative allowance for
    /// the complete operation. Producing/decoding the proposal needs the caller's prior admission;
    /// this method moves it without copying. Charges cover this incumbent, retained proposal,
    /// classifier scratch/copies and future checkpoint storage. Complete-set incumbent retention
    /// remains the enclosing owner's responsibility before it starts preparing any successors.
    #[cfg(test)]
    pub(crate) fn prepare_authored_schema_upgrade(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: SceneDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        admit: &mut AdmissionCallback<'_>,
    ) -> Result<PreparedSceneAuthoredSchemaUpgrade, ScenePatchReport> {
        let source = UpgradeSource::from_session(&self.document, &self.authority, &self.history)?;
        require_input(
            source.state,
            source.snapshot.revision,
            source_key,
            base_revision,
            SourceDocumentKind::Scene,
            AuthoringDocumentKind::Scene,
            upgrade,
        )?;
        admit_incumbent(
            &self.document.placements,
            &self.document.entities,
            source.state,
            &self.history,
            admit,
        )?;
        source.prepare_with_admission(source_key, base_revision, candidate, upgrade, admit)
    }

    /// Revalidates the original source under exclusive access, without installing anything.
    /// Refusal returns the same prepared successor, including its original future identities.
    #[doc(hidden)]
    pub fn bind_authored_schema_upgrade(
        &mut self,
        source_key: &SourceDocumentKey,
        prepared: PreparedSceneAuthoredSchemaUpgrade,
    ) -> Result<
        SceneAuthoredSchemaUpgradeCommit<'_>,
        AuthoredSchemaUpgradeBindFailure<PreparedSceneAuthoredSchemaUpgrade>,
    > {
        if let Err(report) = self.validate_authored_schema_upgrade(source_key, &prepared) {
            return Err(AuthoredSchemaUpgradeBindFailure { prepared, report });
        }
        Ok(SceneAuthoredSchemaUpgradeCommit {
            session: self,
            prepared,
        })
    }

    #[doc(hidden)]
    pub fn validate_authored_schema_upgrade(
        &self,
        source_key: &SourceDocumentKey,
        prepared: &PreparedSceneAuthoredSchemaUpgrade,
    ) -> Result<(), ScenePatchReport> {
        prepared.checkpoint.require_current(
            source_key,
            &self.authority,
            &self.history,
            SourceDocumentKind::Scene,
            AuthoringDocumentKind::Scene,
        )?;
        if !Arc::ptr_eq(&self.document, &prepared.original) {
            return Err(stale());
        }
        Ok(())
    }
}

impl PrefabAuthoringSession {
    #[doc(hidden)]
    pub fn retain_authored_schema_upgrade_source(
        &self,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<RetainedAuthoredSchemaUpgradeSource, ScenePatchReport> {
        retain_incumbent(
            &self.authority,
            &self.history,
            &self.document.placements,
            &self.document.entities,
            SourceDocumentKind::Prefab,
            work,
        )
    }

    #[doc(hidden)]
    pub fn prepare_authored_upgrade(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: PrefabDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedPrefabAuthoredSchemaUpgrade, ScenePatchReport> {
        let retained = self.retain_authored_schema_upgrade_source(work)?;
        self.prepare_authored_upgrade_with_retained_source(
            source_key,
            base_revision,
            candidate,
            upgrade,
            retained,
            work,
        )
    }

    #[doc(hidden)]
    pub fn prepare_authored_upgrade_with_retained_source(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: PrefabDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        retained: RetainedAuthoredSchemaUpgradeSource,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedPrefabAuthoredSchemaUpgrade, ScenePatchReport> {
        work.require_retained(
            &retained,
            self.revision(),
            upgrade.previous_basis(),
            SourceDocumentKind::Prefab,
        )?;
        UpgradeSource::from_session(&self.document, &self.authority, &self.history)?
            .prepare_with_work(source_key, base_revision, candidate, upgrade, work)
    }

    /// Uses the original Prefab file policy and the owning classifier's admitted projection.
    ///
    /// This moves the already bounded proposal and never calls `instantiate()` before admission.
    /// The caller owns the same complete-operation cumulative allowance as the Scene path.
    #[cfg(test)]
    pub(crate) fn prepare_authored_schema_upgrade(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: PrefabDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        admit: &mut AdmissionCallback<'_>,
    ) -> Result<PreparedPrefabAuthoredSchemaUpgrade, ScenePatchReport> {
        let source = UpgradeSource::from_session(&self.document, &self.authority, &self.history)?;
        require_input(
            source.state,
            source.snapshot.revision,
            source_key,
            base_revision,
            SourceDocumentKind::Prefab,
            AuthoringDocumentKind::Prefab,
            upgrade,
        )?;
        admit_incumbent(
            &self.document.placements,
            &self.document.entities,
            source.state,
            &self.history,
            admit,
        )?;
        source.prepare_with_admission(source_key, base_revision, candidate, upgrade, admit)
    }

    #[doc(hidden)]
    pub fn bind_authored_schema_upgrade(
        &mut self,
        source_key: &SourceDocumentKey,
        prepared: PreparedPrefabAuthoredSchemaUpgrade,
    ) -> Result<
        PrefabAuthoredSchemaUpgradeCommit<'_>,
        AuthoredSchemaUpgradeBindFailure<PreparedPrefabAuthoredSchemaUpgrade>,
    > {
        if let Err(report) = self.validate_authored_schema_upgrade(source_key, &prepared) {
            return Err(AuthoredSchemaUpgradeBindFailure { prepared, report });
        }
        Ok(PrefabAuthoredSchemaUpgradeCommit {
            session: self,
            prepared,
        })
    }

    #[doc(hidden)]
    pub fn validate_authored_schema_upgrade(
        &self,
        source_key: &SourceDocumentKey,
        prepared: &PreparedPrefabAuthoredSchemaUpgrade,
    ) -> Result<(), ScenePatchReport> {
        prepared.checkpoint.require_current(
            source_key,
            &self.authority,
            &self.history,
            SourceDocumentKind::Prefab,
            AuthoringDocumentKind::Prefab,
        )?;
        if !Arc::ptr_eq(&self.document, &prepared.original) {
            return Err(stale());
        }
        Ok(())
    }
}

impl UpgradeSource<'_, SceneDocument> {
    fn prepare_with_work(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: SceneDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedSceneAuthoredSchemaUpgrade, ScenePatchReport> {
        work.begin_step()?;
        let result = (|| {
            let prepared = self.prepare_with_admission(
                source_key,
                base_revision,
                candidate,
                upgrade,
                &mut |stage, bytes| work.reserve(stage, bytes),
            )?;
            let retained = prepared_retained_bytes(
                &prepared.document.placements,
                &prepared.document.entities,
                &prepared.checkpoint,
                size_of::<PreparedSceneAuthoredSchemaUpgrade>() + size_of::<SceneDocument>(),
                work,
            )?;
            work.retain_output(Some(retained))?;
            Ok(prepared)
        })();
        work.budget.end_step();
        result
    }

    fn prepare_with_admission(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: SceneDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        admit: &mut AdmissionCallback<'_>,
    ) -> Result<PreparedSceneAuthoredSchemaUpgrade, ScenePatchReport> {
        require_input(
            self.state,
            self.snapshot.revision,
            source_key,
            base_revision,
            SourceDocumentKind::Scene,
            AuthoringDocumentKind::Scene,
            upgrade,
        )?;
        let (candidate, _) = candidate.into_authoring_parts();
        admit_candidate(
            &candidate.placements,
            &candidate.entities,
            self.state,
            admit,
        )?;
        admit_checkpoint_storage::<PreparedSceneAuthoredSchemaUpgrade, SceneDocument>(
            source_key, admit,
        )?;
        let next =
            self.state
                .prepare_authored_upgrade(self.document, &candidate, upgrade, admit)?;
        require_successor(self.state, &next, upgrade)?;
        Ok(PreparedSceneAuthoredSchemaUpgrade {
            original: Arc::clone(self.document),
            document: Arc::new(candidate),
            checkpoint: PreparedCheckpoint::new(
                source_key,
                self.snapshot,
                Arc::clone(self.state),
                next,
            ),
        })
    }
}

impl UpgradeSource<'_, PrefabDocument> {
    fn prepare_with_work(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: PrefabDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedPrefabAuthoredSchemaUpgrade, ScenePatchReport> {
        work.begin_step()?;
        let result = (|| {
            let prepared = self.prepare_with_admission(
                source_key,
                base_revision,
                candidate,
                upgrade,
                &mut |stage, bytes| work.reserve(stage, bytes),
            )?;
            let retained = prepared_retained_bytes(
                &prepared.document.placements,
                &prepared.document.entities,
                &prepared.checkpoint,
                size_of::<PreparedPrefabAuthoredSchemaUpgrade>() + size_of::<PrefabDocument>(),
                work,
            )?;
            work.retain_output(Some(retained))?;
            Ok(prepared)
        })();
        work.budget.end_step();
        result
    }

    fn prepare_with_admission(
        &self,
        source_key: &SourceDocumentKey,
        base_revision: SceneAuthoringRevision,
        candidate: PrefabDocumentCandidate,
        upgrade: &AuthoredFieldUpgrade,
        admit: &mut AdmissionCallback<'_>,
    ) -> Result<PreparedPrefabAuthoredSchemaUpgrade, ScenePatchReport> {
        require_input(
            self.state,
            self.snapshot.revision,
            source_key,
            base_revision,
            SourceDocumentKind::Prefab,
            AuthoringDocumentKind::Prefab,
            upgrade,
        )?;
        let (candidate, _) = candidate.into_authoring_parts();
        admit_candidate(
            &candidate.placements,
            &candidate.entities,
            self.state,
            admit,
        )?;
        admit_checkpoint_storage::<PreparedPrefabAuthoredSchemaUpgrade, PrefabDocument>(
            source_key, admit,
        )?;
        let next = self.state.prepare_authored_prefab_upgrade(
            self.document,
            &candidate,
            upgrade,
            admit,
        )?;
        require_successor(self.state, &next, upgrade)?;
        Ok(PreparedPrefabAuthoredSchemaUpgrade {
            original: Arc::clone(self.document),
            document: Arc::new(candidate),
            checkpoint: PreparedCheckpoint::new(
                source_key,
                self.snapshot,
                Arc::clone(self.state),
                next,
            ),
        })
    }
}

/// Exclusive source guard for the complete-set owner's final infallible installation.
#[derive(Debug)]
#[must_use]
pub struct SceneAuthoredSchemaUpgradeCommit<'a> {
    session: &'a mut SceneAuthoringSession,
    prepared: PreparedSceneAuthoredSchemaUpgrade,
}

impl<'a> SceneAuthoredSchemaUpgradeCommit<'a> {
    pub fn prepared(&self) -> &PreparedSceneAuthoredSchemaUpgrade {
        &self.prepared
    }

    /// Releases exclusive access without installation and returns the exact prepared owner.
    pub fn into_prepared(self) -> PreparedSceneAuthoredSchemaUpgrade {
        self.prepared
    }

    /// Installs only the source checkpoint; persistence must adopt its own actual evidence.
    pub fn commit(self) -> &'a mut SceneAuthoringSession {
        self.session.document = self.prepared.document;
        self.session.history = self.prepared.checkpoint.history;
        self.session.authority =
            AuthoringSessionAuthority::Published(self.prepared.checkpoint.state);
        self.session.live_dirty = true;
        self.session
    }
}

#[derive(Debug)]
#[must_use]
pub struct PrefabAuthoredSchemaUpgradeCommit<'a> {
    session: &'a mut PrefabAuthoringSession,
    prepared: PreparedPrefabAuthoredSchemaUpgrade,
}

impl<'a> PrefabAuthoredSchemaUpgradeCommit<'a> {
    pub fn prepared(&self) -> &PreparedPrefabAuthoredSchemaUpgrade {
        &self.prepared
    }

    /// Releases exclusive access without installation and returns the exact prepared owner.
    pub fn into_prepared(self) -> PreparedPrefabAuthoredSchemaUpgrade {
        self.prepared
    }

    pub fn commit(self) -> &'a mut PrefabAuthoringSession {
        self.session.document = self.prepared.document;
        self.session.history = self.prepared.checkpoint.history;
        self.session.authority =
            AuthoringSessionAuthority::Published(self.prepared.checkpoint.state);
        self.session
    }
}

impl PreparedCheckpoint {
    fn new(
        source_key: &SourceDocumentKey,
        original: UpgradeSourceSnapshot,
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

    fn require_current(
        &self,
        source_key: &SourceDocumentKey,
        authority: &AuthoringSessionAuthority,
        history: &AuthoringHistory,
        source_kind: SourceDocumentKind,
        document_kind: AuthoringDocumentKind,
    ) -> Result<(), ScenePatchReport> {
        let state = authority.published().ok_or_else(unavailable_authority)?;
        if source_key != &self.source_key
            || source_key.kind() != source_kind
            || state.document_kind() != document_kind
            || history.revision() != self.base_revision
            || history.content_state() != self.base_content_state
            || history.limits() != self.history.limits()
            || state.schema_basis() != self.original_state.schema_basis()
            || !std::ptr::eq(state, self.original_state.as_ref())
        {
            return Err(stale());
        }
        Ok(())
    }
}

fn require_input(
    state: &AuthoringDocumentState,
    revision: SceneAuthoringRevision,
    source_key: &SourceDocumentKey,
    base_revision: SceneAuthoringRevision,
    source_kind: SourceDocumentKind,
    document_kind: AuthoringDocumentKind,
    upgrade: &AuthoredFieldUpgrade,
) -> Result<(), ScenePatchReport> {
    if source_key.kind() != source_kind || state.document_kind() != document_kind {
        return Err(history_error_report(
            "scene.authored-schema-upgrade-source-kind",
            "Authored upgrade requires the actual matching Scene or Prefab source kind",
        ));
    }
    if revision != base_revision || &state.schema_basis() != upgrade.previous_basis() {
        return Err(stale());
    }
    Ok(())
}

fn require_successor(
    original: &AuthoringDocumentState,
    next: &AuthoringDocumentState,
    upgrade: &AuthoredFieldUpgrade,
) -> Result<(), ScenePatchReport> {
    if &next.schema_basis() != upgrade.successor_basis()
        || next.source() != original.source()
        || next.document_kind() != original.document_kind()
        || next.file_limits() != original.file_limits()
    {
        return Err(history_error_report(
            "scene.authored-schema-upgrade-classification-mismatch",
            "Prepared classification must retain the exact source, kind and original file policy",
        ));
    }
    Ok(())
}

fn admit_incumbent(
    original_placements: &[crate::SceneEntityPlacement],
    original_entities: &[SceneEntityRecord],
    state: &AuthoringDocumentState,
    history: &AuthoringHistory,
    admit: &mut AdmissionCallback<'_>,
) -> Result<usize, ScenePatchReport> {
    let original = super::prefab_conversion::retention::document_dynamic_bytes_with_admission(
        original_placements,
        original_entities,
        state.file_limits(),
        |bytes| charge(admit, "authored-upgrade-source-scan", bytes),
    )?;
    let classification = state.checked_retained_bytes_with_admission(|bytes| {
        charge(admit, "authored-upgrade-classification-scan", bytes)
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
            .checked_add(incumbent_storage_bytes(state.document_kind())?)
    });
    charge(admit, "authored-upgrade-incumbent", retained)?;
    retained.ok_or_else(|| {
        history_error_report(
            "scene.authored-schema-upgrade-limit-exceeded",
            "Authored upgrade source exceeds its retained cost model",
        )
    })
}

fn admit_candidate(
    candidate_placements: &[crate::SceneEntityPlacement],
    candidate_entities: &[SceneEntityRecord],
    state: &AuthoringDocumentState,
    admit: &mut AdmissionCallback<'_>,
) -> Result<(), ScenePatchReport> {
    let candidate = super::prefab_conversion::retention::document_dynamic_bytes_with_admission(
        candidate_placements,
        candidate_entities,
        state.file_limits(),
        |bytes| charge(admit, "authored-upgrade-candidate-scan", bytes),
    )?;
    charge(admit, "authored-upgrade-candidate", candidate)
}

fn retain_incumbent(
    authority: &AuthoringSessionAuthority,
    history: &AuthoringHistory,
    placements: &[crate::SceneEntityPlacement],
    entities: &[SceneEntityRecord],
    kind: SourceDocumentKind,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<RetainedAuthoredSchemaUpgradeSource, ScenePatchReport> {
    let AuthoringSessionAuthority::Published(state) = authority else {
        return Err(unavailable_authority());
    };
    work.begin_step()?;
    let result = (|| {
        let bytes = admit_incumbent(placements, entities, state, history, &mut |stage, bytes| {
            work.reserve(stage, bytes)
        })?;
        work.retain_output(Some(bytes))?;
        Ok(retained_source(history, state, kind, work))
    })();
    work.budget.end_step();
    result
}

fn prepared_retained_bytes(
    placements: &[crate::SceneEntityPlacement],
    entities: &[SceneEntityRecord],
    checkpoint: &PreparedCheckpoint,
    fixed: usize,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<usize, ScenePatchReport> {
    let source = super::prefab_conversion::retention::document_dynamic_bytes_with_admission(
        placements,
        entities,
        checkpoint.state.file_limits(),
        |bytes| work.reserve("authored-upgrade-output-source-cost", bytes),
    )?;
    let classification = checkpoint
        .state
        .checked_retained_bytes_with_admission(|bytes| {
            work.reserve("authored-upgrade-output-classification-cost", bytes)
        })?;
    source
        .and_then(|bytes| {
            bytes
                .checked_add(classification?)?
                .checked_add(fixed)?
                .checked_add(4 * size_of::<usize>())?
                .checked_add(checkpoint.source_key.checked_retained_bytes()?)
        })
        .ok_or_else(|| {
            history_error_report(
                "scene.authored-schema-upgrade-limit-exceeded",
                "Authored upgrade output exceeds its retained cost model",
            )
        })
}

fn admit_checkpoint_storage<P, D>(
    source_key: &SourceDocumentKey,
    admit: &mut AdmissionCallback<'_>,
) -> Result<(), ScenePatchReport> {
    // Empty VecDeques do not allocate. Inline checkpoint/history storage, the document Arc and
    // both new Arc control blocks are admitted before their creation. The classifier also
    // charges its own state and dynamic copies; shared Schema/Adapter catalogs are excluded.
    charge(
        admit,
        "authored-upgrade-checkpoint-storage",
        size_of::<P>()
            .checked_add(size_of::<D>())
            .and_then(|bytes| bytes.checked_add(4 * size_of::<usize>()))
            .and_then(|bytes| bytes.checked_add(source_key.checked_retained_bytes()?)),
    )
}

fn incumbent_storage_bytes(kind: AuthoringDocumentKind) -> Option<usize> {
    // Session storage already contains its inline history. Charge the source document, both
    // Arc control blocks and the linear retention token as well; shared catalogs are excluded.
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
        .checked_add(size_of::<RetainedAuthoredSchemaUpgradeSource>())
}

fn charge(
    admit: &mut AdmissionCallback<'_>,
    stage: &'static str,
    bytes: Option<usize>,
) -> Result<(), ScenePatchReport> {
    admit(stage, bytes)?;
    bytes.map(|_| ()).ok_or_else(|| {
        history_error_report(
            "scene.authored-schema-upgrade-limit-exceeded",
            "Authored upgrade source or checkpoint exceeds its owning bounded cost model",
        )
    })
}

fn stale() -> ScenePatchReport {
    history_error_report(
        "scene.authored-schema-upgrade-stale",
        "Authored upgrade requires its exact original source revision, content state and Schema basis",
    )
}

fn unavailable_authority() -> ScenePatchReport {
    history_error_report(
        "scene.authored-schema-upgrade-authority-unavailable",
        "Authored upgrade requires a published non-executable authoring classification",
    )
}

#[cfg(test)]
mod tests;
