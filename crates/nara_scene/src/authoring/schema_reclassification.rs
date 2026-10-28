//! Additive Schema recovery preserves source identity and proves retained source history.
//!
//! Dormant overlay payloads are preserved. Actual selected historical composition and dependency
//! availability remain separate Host admission through the existing Prefab compiler.

use std::{mem::size_of, sync::Arc};

use nara_core::ByteLimit;

use super::{
    AuthoringHistory, AuthoringHistoryScope, AuthoringSessionAuthority, PrefabAuthoringSession,
    SceneAuthoringRevision, SceneAuthoringSession, admit_history_replay_report,
    history_error_report,
};
use crate::{
    AuthoringSchemaBasis, PrefabDocument, SceneDocument, ScenePatchDocument, ScenePatchReport,
    authoring_decode::{
        self, AuthoringDocumentState, PreparedPrefabAuthoringPatch, PreparedSceneAuthoringPatch,
    },
};

type Admission<'a> = dyn FnMut(&'static str, Option<usize>) -> Result<(), ScenePatchReport> + 'a;

/// Logical work and peak-retention limits for one source/history classification proof.
///
/// Shared immutable Schema catalogs and allocator capacity are excluded. Source/patch copying,
/// classification and reference work use their existing owning cost models, not measured RSS.
/// Bounded terminal refusal diagnostics are outside the allowance, so exhaustion can still be
/// reported. Retained classification diagnostics and dynamic interpretation scratch are charged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaReclassificationLimits {
    work_bytes: ByteLimit,
    retained_bytes: ByteLimit,
}

impl SchemaReclassificationLimits {
    #[must_use]
    pub const fn new(work_bytes: ByteLimit, retained_bytes: ByteLimit) -> Self {
        Self {
            work_bytes,
            retained_bytes,
        }
    }

    #[must_use]
    pub const fn work_bytes(self) -> ByteLimit {
        self.work_bytes
    }

    #[must_use]
    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }
}

impl Default for SchemaReclassificationLimits {
    fn default() -> Self {
        Self::new(
            ByteLimit::new(512 * 1024 * 1024).expect("nonzero classification work limit"),
            ByteLimit::new(256 * 1024 * 1024).expect("nonzero classification retention limit"),
        )
    }
}

/// Observed modeled work, distinct from elapsed time, allocation capacity or process memory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SchemaReclassificationUsage {
    pub work_bytes: usize,
    pub peak_retained_bytes: usize,
}

/// One finite proof allowance for the sources charged by preparation calls so far.
///
/// Work is cumulative. Incumbents and prepared outputs stay charged for this allowance's lifetime,
/// even if a caller drops a candidate early. Temporary edge scratch is released between edges.
/// Uncharged open-source incumbents are outside the reported peak. Whole-open-set admission
/// must charge every incumbent before preparing its first successor; this source helper alone
/// does not establish that Workspace-wide bound.
#[derive(Debug, Default)]
pub struct SchemaReclassificationWork {
    limits: SchemaReclassificationLimits,
    usage: SchemaReclassificationUsage,
    retained: usize,
    scratch: usize,
}

impl SchemaReclassificationWork {
    #[must_use]
    pub const fn new(limits: SchemaReclassificationLimits) -> Self {
        Self {
            limits,
            usage: SchemaReclassificationUsage {
                work_bytes: 0,
                peak_retained_bytes: 0,
            },
            retained: 0,
            scratch: 0,
        }
    }

    #[must_use]
    pub const fn usage(&self) -> SchemaReclassificationUsage {
        self.usage
    }

    fn retain(&mut self, bytes: Option<usize>) -> Result<(), ScenePatchReport> {
        let next = bytes.and_then(|bytes| self.retained.checked_add(bytes));
        let Some(next) = next.filter(|next| *next <= self.limits.retained_bytes.get()) else {
            return Err(limit_error("retained", "incumbent-or-candidate"));
        };
        self.retained = next;
        self.observe_peak()
    }

    fn begin_step(&mut self, held: Option<usize>) -> Result<(), ScenePatchReport> {
        self.scratch = held.ok_or_else(|| limit_error("retained", "history-branch"))?;
        self.observe_peak()
    }

    fn reserve(
        &mut self,
        stage: &'static str,
        bytes: Option<usize>,
    ) -> Result<(), ScenePatchReport> {
        let work = bytes.and_then(|bytes| self.usage.work_bytes.checked_add(bytes));
        let Some(work) = work.filter(|work| *work <= self.limits.work_bytes.get()) else {
            return Err(limit_error("work", stage));
        };
        let scratch = bytes.and_then(|bytes| self.scratch.checked_add(bytes));
        let Some(scratch) = scratch else {
            return Err(limit_error("retained", stage));
        };
        let total = self.retained.checked_add(scratch);
        if total.is_none_or(|total| total > self.limits.retained_bytes.get()) {
            return Err(limit_error("retained", stage));
        }
        self.usage.work_bytes = work;
        self.scratch = scratch;
        self.observe_peak()
    }

    fn observe_peak(&mut self) -> Result<(), ScenePatchReport> {
        let total = self
            .retained
            .checked_add(self.scratch)
            .filter(|total| *total <= self.limits.retained_bytes.get())
            .ok_or_else(|| limit_error("retained", "retained-and-scratch"))?;
        self.usage.peak_retained_bytes = self.usage.peak_retained_bytes.max(total);
        Ok(())
    }

    fn retain_classification(&mut self, bytes: Option<usize>) -> Result<(), ScenePatchReport> {
        let bytes = bytes.ok_or_else(|| limit_error("retained", "classification-output"))?;
        if bytes > self.scratch {
            return Err(limit_error("retained", "classification-admission-model"));
        }
        self.scratch = 0;
        self.retain(Some(bytes))
    }

    fn end_step(&mut self) {
        self.scratch = 0;
    }
}

fn limit_error(kind: &'static str, stage: &'static str) -> ScenePatchReport {
    let diagnostic = crate::diagnostics::with_public_locator(
        crate::diagnostics::with_public_locator(
            crate::diagnostics::error(
                "scene.authoring-schema-reclassification-limit",
                "Schema reclassification refused before its next owning preparation",
            ),
            "budget-kind",
            kind,
        ),
        "preparation-stage",
        stage,
    );
    let mut diagnostics = nara_diagnostic::DiagnosticReport::default();
    diagnostics.push(diagnostic);
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

fn refused(summary: &'static str) -> ScenePatchReport {
    history_error_report("scene.authoring-schema-reclassification-rejected", summary)
}

/// An unpublished classification successor; source and history remain in their original owner.
#[derive(Debug)]
pub struct PreparedSceneSchemaReclassification {
    revision: SceneAuthoringRevision,
    previous: AuthoringSchemaBasis,
    state: Arc<AuthoringDocumentState>,
}

/// Nominal Prefab counterpart; it cannot be bound to a Scene session.
#[derive(Debug)]
pub struct PreparedPrefabSchemaReclassification {
    revision: SceneAuthoringRevision,
    previous: AuthoringSchemaBasis,
    state: Arc<AuthoringDocumentState>,
}

impl PreparedSceneSchemaReclassification {
    #[must_use]
    pub fn schema_basis(&self) -> AuthoringSchemaBasis {
        self.state.schema_basis()
    }
    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.revision
    }
    #[must_use]
    pub fn readiness(&self) -> crate::AuthoringDocumentReadiness {
        self.state.readiness()
    }
    #[must_use]
    pub fn unavailable_record_manifest(&self) -> &crate::UnavailableRecordManifest {
        self.state.unavailable_manifest()
    }
    #[must_use]
    pub fn diagnostics(&self) -> &nara_diagnostic::DiagnosticReport {
        self.state.diagnostics()
    }
}

impl PreparedPrefabSchemaReclassification {
    #[must_use]
    pub fn schema_basis(&self) -> AuthoringSchemaBasis {
        self.state.schema_basis()
    }
    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.revision
    }
    #[must_use]
    pub fn readiness(&self) -> crate::AuthoringDocumentReadiness {
        self.state.readiness()
    }
    #[must_use]
    pub fn unavailable_record_manifest(&self) -> &crate::UnavailableRecordManifest {
        self.state.unavailable_manifest()
    }
    #[must_use]
    pub fn diagnostics(&self) -> &nara_diagnostic::DiagnosticReport {
        self.state.diagnostics()
    }
}

/// Validated current Scene source under an unpublished classification successor.
///
/// Both borrows remain live: the session cannot change source or classification and the
/// candidate cannot be consumed while this view is used. This proves only their current-source
/// association; selected composition, dependencies and later publication remain Host admission.
#[doc(hidden)]
#[derive(Debug)]
pub struct PreparedSceneSchemaSource<'a> {
    document: &'a Arc<SceneDocument>,
    prepared: &'a PreparedSceneSchemaReclassification,
}

/// Nominal Prefab source counterpart, retaining the original Session document Arc.
#[doc(hidden)]
#[derive(Debug)]
pub struct PreparedPrefabSchemaSource<'a> {
    document: &'a Arc<PrefabDocument>,
    prepared: &'a PreparedPrefabSchemaReclassification,
}

/// Borrows a prepared Scene's exact current source after the same check used by commit bind.
#[doc(hidden)]
pub fn borrow_prepared_scene_schema_source<'a>(
    session: &'a SceneAuthoringSession,
    prepared: &'a PreparedSceneSchemaReclassification,
) -> Result<PreparedSceneSchemaSource<'a>, ScenePatchReport> {
    require_prepared_source(
        session.revision(),
        session.authoring_schema_basis(),
        prepared.revision,
        &prepared.previous,
    )?;
    Ok(PreparedSceneSchemaSource {
        document: &session.document,
        prepared,
    })
}

/// Borrows a prepared Prefab's exact current source without a Scene projection or source copy.
#[doc(hidden)]
pub fn borrow_prepared_prefab_schema_source<'a>(
    session: &'a PrefabAuthoringSession,
    prepared: &'a PreparedPrefabSchemaReclassification,
) -> Result<PreparedPrefabSchemaSource<'a>, ScenePatchReport> {
    require_prepared_source(
        session.revision(),
        session.authoring_schema_basis(),
        prepared.revision,
        &prepared.previous,
    )?;
    Ok(PreparedPrefabSchemaSource {
        document: &session.document,
        prepared,
    })
}

impl PreparedSceneSchemaSource<'_> {
    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        self.document.as_ref()
    }

    /// Clones only the original Session's immutable source handle.
    #[must_use]
    pub fn shared_document(&self) -> Arc<SceneDocument> {
        Arc::clone(self.document)
    }

    #[must_use]
    pub fn schema_basis(&self) -> AuthoringSchemaBasis {
        self.prepared.schema_basis()
    }

    #[must_use]
    pub fn source(&self) -> &crate::AuthoringDocumentSource {
        self.prepared.state.source()
    }

    #[must_use]
    pub fn readiness(&self) -> crate::AuthoringDocumentReadiness {
        self.prepared.readiness()
    }

    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.prepared.base_revision()
    }

    #[must_use]
    pub fn file_limits(&self) -> crate::SceneFileLimits {
        self.prepared.state.file_limits()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &nara_diagnostic::DiagnosticReport {
        self.prepared.diagnostics()
    }

    #[must_use]
    pub fn unavailable_record_manifest(&self) -> &crate::UnavailableRecordManifest {
        self.prepared.unavailable_record_manifest()
    }
}

impl PreparedPrefabSchemaSource<'_> {
    #[must_use]
    pub fn document(&self) -> &PrefabDocument {
        self.document.as_ref()
    }

    #[must_use]
    pub fn shared_document(&self) -> Arc<PrefabDocument> {
        Arc::clone(self.document)
    }

    #[must_use]
    pub fn schema_basis(&self) -> AuthoringSchemaBasis {
        self.prepared.schema_basis()
    }

    #[must_use]
    pub fn source(&self) -> &crate::AuthoringDocumentSource {
        self.prepared.state.source()
    }

    #[must_use]
    pub fn readiness(&self) -> crate::AuthoringDocumentReadiness {
        self.prepared.readiness()
    }

    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.prepared.base_revision()
    }

    #[must_use]
    pub fn file_limits(&self) -> crate::SceneFileLimits {
        self.prepared.state.file_limits()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &nara_diagnostic::DiagnosticReport {
        self.prepared.diagnostics()
    }

    #[must_use]
    pub fn unavailable_record_manifest(&self) -> &crate::UnavailableRecordManifest {
        self.prepared.unavailable_record_manifest()
    }
}

fn require_prepared_source(
    revision: SceneAuthoringRevision,
    basis: Option<AuthoringSchemaBasis>,
    prepared_revision: SceneAuthoringRevision,
    previous: &AuthoringSchemaBasis,
) -> Result<(), ScenePatchReport> {
    if revision != prepared_revision || basis.as_ref() != Some(previous) {
        return Err(refused(
            "Schema reclassification requires its original source revision and basis",
        ));
    }
    Ok(())
}

/// Exclusive validated source access for whole-set publication by the Workspace owner.
#[doc(hidden)]
#[derive(Debug)]
pub struct SceneSchemaReclassificationCommit<'a> {
    session: &'a mut SceneAuthoringSession,
    state: Arc<AuthoringDocumentState>,
}

impl SceneSchemaReclassificationCommit<'_> {
    /// Infallibly swaps only classification after all owners in the set have been bound.
    pub fn commit(self) {
        self.session.authority = AuthoringSessionAuthority::Published(self.state);
        self.session.live_dirty = true;
    }
}

/// Exclusive validated Prefab access; the nominal source and all history remain in place.
#[doc(hidden)]
#[derive(Debug)]
pub struct PrefabSchemaReclassificationCommit<'a> {
    session: &'a mut PrefabAuthoringSession,
    state: Arc<AuthoringDocumentState>,
}

impl PrefabSchemaReclassificationCommit<'_> {
    pub fn commit(self) {
        self.session.authority = AuthoringSessionAuthority::Published(self.state);
    }
}

impl SceneAuthoringSession {
    /// Prepares additive classification and every retained history edge without changing source.
    pub fn prepare_schema_reclassification(
        &self,
        successor: &AuthoringSchemaBasis,
        work: &mut SchemaReclassificationWork,
    ) -> Result<PreparedSceneSchemaReclassification, ScenePatchReport> {
        let state = self
            .authority
            .published()
            .ok_or_else(|| refused("Native sources have no admitted authoring basis"))?;
        successor.require_declaration_extension_of(&state.schema_basis())?;
        work.begin_step(Some(0))?;
        let incumbent = incumbent_bytes(
            self.document.as_ref(),
            state,
            &self.history,
            scene_bytes,
            &mut |stage, bytes| work.reserve(stage, bytes),
        )?;
        work.retain(incumbent)?;
        work.begin_step(Some(0))?;
        let next = state.reclassify(&self.document, successor, &mut |stage, bytes| {
            work.reserve(stage, bytes)
        })?;
        work.reserve("classification-authority", Some(2 * size_of::<usize>()))?;
        let retained = next.checked_retained_bytes_with_admission(|bytes| {
            work.reserve("classification-retention-scan", bytes)
        })?;
        work.retain_classification(
            retained.and_then(|bytes| bytes.checked_add(2 * size_of::<usize>())),
        )?;
        prove_history(
            &self.history,
            self.document.as_ref(),
            &next,
            work,
            scene_bytes,
            |document, state, patch, scope, admit| {
                let prepared = prepare_scene_step(document, state, patch, scope, Some(admit))?;
                Ok(ProofStep {
                    document: prepared.document,
                    state: prepared.state,
                    report: prepared.report,
                })
            },
        )?;
        Ok(PreparedSceneSchemaReclassification {
            revision: self.revision(),
            previous: state.schema_basis(),
            state: Arc::new(next),
        })
    }

    /// Commits one candidate while preserving source revision, content state and retained history.
    pub fn commit_schema_reclassification(
        &mut self,
        prepared: PreparedSceneSchemaReclassification,
    ) -> Result<(), ScenePatchReport> {
        self.bind_schema_reclassification(prepared)?.commit();
        Ok(())
    }

    /// Binds exclusive source access so a Workspace can validate its entire set before swapping.
    #[doc(hidden)]
    pub fn bind_schema_reclassification(
        &mut self,
        prepared: PreparedSceneSchemaReclassification,
    ) -> Result<SceneSchemaReclassificationCommit<'_>, ScenePatchReport> {
        require_prepared_source(
            self.revision(),
            self.authoring_schema_basis(),
            prepared.revision,
            &prepared.previous,
        )?;
        Ok(SceneSchemaReclassificationCommit {
            session: self,
            state: prepared.state,
        })
    }
}

impl PrefabAuthoringSession {
    pub fn prepare_schema_reclassification(
        &self,
        successor: &AuthoringSchemaBasis,
        work: &mut SchemaReclassificationWork,
    ) -> Result<PreparedPrefabSchemaReclassification, ScenePatchReport> {
        let state = self
            .authority
            .published()
            .ok_or_else(|| refused("Native sources have no admitted authoring basis"))?;
        successor.require_declaration_extension_of(&state.schema_basis())?;
        work.begin_step(Some(0))?;
        let incumbent = incumbent_bytes(
            self.document.as_ref(),
            state,
            &self.history,
            prefab_bytes,
            &mut |stage, bytes| work.reserve(stage, bytes),
        )?;
        work.retain(incumbent)?;
        work.begin_step(Some(0))?;
        let next = state.reclassify_prefab(&self.document, successor, &mut |stage, bytes| {
            work.reserve(stage, bytes)
        })?;
        work.reserve("classification-authority", Some(2 * size_of::<usize>()))?;
        let retained = next.checked_retained_bytes_with_admission(|bytes| {
            work.reserve("classification-retention-scan", bytes)
        })?;
        work.retain_classification(
            retained.and_then(|bytes| bytes.checked_add(2 * size_of::<usize>())),
        )?;
        prove_history(
            &self.history,
            self.document.as_ref(),
            &next,
            work,
            prefab_bytes,
            |document, state, patch, scope, admit| {
                let prepared = prepare_prefab_step(document, state, patch, scope, Some(admit))?;
                Ok(ProofStep {
                    document: prepared.document,
                    state: prepared.state,
                    report: prepared.report,
                })
            },
        )?;
        Ok(PreparedPrefabSchemaReclassification {
            revision: self.revision(),
            previous: state.schema_basis(),
            state: Arc::new(next),
        })
    }

    pub fn commit_schema_reclassification(
        &mut self,
        prepared: PreparedPrefabSchemaReclassification,
    ) -> Result<(), ScenePatchReport> {
        self.bind_schema_reclassification(prepared)?.commit();
        Ok(())
    }

    #[doc(hidden)]
    pub fn bind_schema_reclassification(
        &mut self,
        prepared: PreparedPrefabSchemaReclassification,
    ) -> Result<PrefabSchemaReclassificationCommit<'_>, ScenePatchReport> {
        require_prepared_source(
            self.revision(),
            self.authoring_schema_basis(),
            prepared.revision,
            &prepared.previous,
        )?;
        Ok(PrefabSchemaReclassificationCommit {
            session: self,
            state: prepared.state,
        })
    }
}

fn scene_bytes(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    admit: &mut Admission<'_>,
) -> Result<Option<usize>, ScenePatchReport> {
    super::prefab_conversion::retention::document_dynamic_bytes_with_admission(
        &document.roots,
        &document.entities,
        state.file_limits(),
        |bytes| admit("source-retention-scan", bytes),
    )
}

fn prefab_bytes(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    admit: &mut Admission<'_>,
) -> Result<Option<usize>, ScenePatchReport> {
    super::prefab_conversion::retention::document_dynamic_bytes_with_admission(
        &document.roots,
        &document.entities,
        state.file_limits(),
        |bytes| admit("source-retention-scan", bytes),
    )
}

fn incumbent_bytes<D>(
    document: &D,
    state: &AuthoringDocumentState,
    history: &AuthoringHistory,
    source_bytes: impl Fn(
        &D,
        &AuthoringDocumentState,
        &mut Admission<'_>,
    ) -> Result<Option<usize>, ScenePatchReport>,
    admit: &mut Admission<'_>,
) -> Result<Option<usize>, ScenePatchReport> {
    let retained = source_and_classification_bytes(document, state, &source_bytes, admit)?;
    Ok(retained.and_then(|bytes| {
        bytes
            .checked_add(history.cost.operations.checked_mul(512)?)?
            .checked_add(history.cost.value_nodes.checked_mul(96)?)?
            .checked_add(history.cost.logical_bytes)
    }))
}

fn source_and_classification_bytes<D>(
    document: &D,
    state: &AuthoringDocumentState,
    source_bytes: &impl Fn(
        &D,
        &AuthoringDocumentState,
        &mut Admission<'_>,
    ) -> Result<Option<usize>, ScenePatchReport>,
    admit: &mut Admission<'_>,
) -> Result<Option<usize>, ScenePatchReport> {
    let document = source_bytes(document, state, admit)?;
    let classification = state.checked_retained_bytes_with_admission(|bytes| {
        admit("classification-retention-scan", bytes)
    })?;
    Ok(document.and_then(|bytes| bytes.checked_add(classification?)))
}

struct ProofStep<D> {
    document: D,
    state: AuthoringDocumentState,
    report: ScenePatchReport,
}

fn prove_history<D: PartialEq>(
    history: &AuthoringHistory,
    document: &D,
    state: &AuthoringDocumentState,
    work: &mut SchemaReclassificationWork,
    source_bytes: impl Fn(
        &D,
        &AuthoringDocumentState,
        &mut Admission<'_>,
    ) -> Result<Option<usize>, ScenePatchReport>,
    prepare: impl Fn(
        &D,
        &AuthoringDocumentState,
        &ScenePatchDocument,
        &AuthoringHistoryScope,
        &mut Admission<'_>,
    ) -> Result<ProofStep<D>, ScenePatchReport>,
) -> Result<(), ScenePatchReport> {
    for stack in [&history.undo_stack, &history.redo_stack] {
        let mut branch: Option<(D, AuthoringDocumentState, usize)> = None;
        let mut content = history.content_state;
        for entry in stack.iter().rev() {
            if entry.from_state != content {
                return Err(refused(
                    "Retained history does not match its source content state",
                ));
            }
            let (source, classification) = branch
                .as_ref()
                .map_or((document, state), |(document, state, _)| (document, state));
            let held = match &branch {
                None => Some(0),
                Some((_, _, bytes)) => Some(*bytes),
            };
            work.begin_step(held)?;
            let forward = prepare(
                source,
                classification,
                &entry.patch,
                &entry.scope,
                &mut |stage, bytes| work.reserve(stage, bytes),
            )?;
            let report = admit_history_replay_report(
                &entry.patch,
                &entry.scope,
                entry.cost,
                forward.report,
                Some(&mut |bytes| work.reserve("history-cost-scan", bytes)),
            )?;
            let inverse = report
                .inverse
                .as_ref()
                .expect("admitted replay owns its inverse");
            let reverse = prepare(
                &forward.document,
                &forward.state,
                inverse,
                &entry.scope,
                &mut |stage, bytes| work.reserve(stage, bytes),
            )?;
            let reverse_report = admit_history_replay_report(
                inverse,
                &entry.scope,
                entry.cost,
                reverse.report,
                Some(&mut |bytes| work.reserve("history-cost-scan", bytes)),
            )?;
            if reverse.document != *source || reverse_report.inverse.as_ref() != Some(&entry.patch)
            {
                return Err(refused(
                    "Retained history must restore the exact source and original patch in both directions",
                ));
            }
            let held = source_and_classification_bytes(
                &forward.document,
                &forward.state,
                &source_bytes,
                &mut |stage, bytes| work.reserve(stage, bytes),
            )?
            .ok_or_else(|| limit_error("retained", "history-branch"))?;
            branch = Some((forward.document, forward.state, held));
            content = entry.to_state;
            work.end_step();
        }
    }
    work.end_step();
    Ok(())
}

pub(super) fn prepare_scene_step(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    scope: &AuthoringHistoryScope,
    admit: Option<&mut Admission<'_>>,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    match (scope, admit) {
        (AuthoringHistoryScope::Ordinary, None) => {
            authoring_decode::prepare_scene_authoring_patch(document, state, patch, true)
        }
        (AuthoringHistoryScope::Ordinary, Some(admit)) => {
            authoring_decode::prepare_scene_authoring_patch_with_admission(
                document, state, patch, true, admit,
            )
        }
        (AuthoringHistoryScope::PrefabConversion(footprint), None) => {
            authoring_decode::prepare_scene_prefab_conversion_patch(
                document, state, patch, footprint,
            )
        }
        (AuthoringHistoryScope::PrefabConversion(footprint), Some(admit)) => {
            authoring_decode::prepare_scene_prefab_conversion_patch_with_admission(
                document, state, patch, footprint, admit,
            )
        }
    }
}

pub(super) fn prepare_prefab_step(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    scope: &AuthoringHistoryScope,
    admit: Option<&mut Admission<'_>>,
) -> Result<PreparedPrefabAuthoringPatch, ScenePatchReport> {
    match (scope, admit) {
        (AuthoringHistoryScope::Ordinary, None) => {
            authoring_decode::prepare_prefab_authoring_patch(document, state, patch, true)
        }
        (AuthoringHistoryScope::Ordinary, Some(admit)) => {
            authoring_decode::prepare_prefab_authoring_patch_with_admission(
                document, state, patch, true, admit,
            )
        }
        (AuthoringHistoryScope::PrefabConversion(footprint), None) => {
            authoring_decode::prepare_prefab_conversion_patch(document, state, patch, footprint)
        }
        (AuthoringHistoryScope::PrefabConversion(footprint), Some(admit)) => {
            authoring_decode::prepare_prefab_conversion_patch_with_admission(
                document, state, patch, footprint, admit,
            )
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod prepared_source_tests;
