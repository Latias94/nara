//! Complete-open-set source/history replacement with Host-observed saved checkpoints.
//!
//! This is separate from classification-only reclassification. The Host proves actual file
//! observations, binds its persistence lanes and project adoption, and publishes Ready before
//! consuming the infallible Workspace commit. Dropping preparations or guards changes no slot.

use std::{mem::size_of, sync::Arc};

use nara_scene::{
    __private::{
        AuthoredFieldUpgrade, AuthoredSchemaUpgradeUsage, AuthoredSchemaUpgradeWork,
        PrefabAuthoredSchemaUpgradeCommit, PrefabAuthoredSchemaUpgradeInput,
        PreparedPrefabAuthoredSchemaUpgrade, PreparedSceneAuthoredSchemaUpgrade,
        RetainedAuthoredSchemaUpgradeSource, SceneAuthoredSchemaUpgradeCommit,
        SceneAuthoredSchemaUpgradeInput,
    },
    AuthoringDocumentSource, AuthoringSchemaBasis, PrefabDocument, PrefabDocumentCandidate,
    SceneAuthoringContentState, SceneAuthoringRevision, SceneDocument, SceneDocumentCandidate,
    SourceDocumentKey, SourceDocumentKind,
};

use super::{
    EditorDocumentDigest, EditorDocumentId, EditorDocumentSlot, EditorDocumentSourceBinding,
    EditorExternalReloadState, EditorPersistenceAuthority, EditorWorkspace,
    EditorWorkspaceCommandReport, EditorWorkspacePersistenceIdentity, WorkspaceDocumentContext,
    workspace_error_report,
};

/// One exact reviewed proposal. The Host admits decoded proposal storage before constructing it.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorAuthoredSchemaUpgradeProposal {
    document: EditorDocumentId,
    source_key: SourceDocumentKey,
    source_binding: EditorDocumentSourceBinding,
    base_revision: SceneAuthoringRevision,
    candidate: ProposalCandidate,
}

#[derive(Debug)]
enum ProposalCandidate {
    Scene(SceneDocumentCandidate),
    Prefab(PrefabDocumentCandidate),
}

impl EditorAuthoredSchemaUpgradeProposal {
    #[must_use]
    pub fn scene(
        document: EditorDocumentId,
        source_key: SourceDocumentKey,
        source_binding: EditorDocumentSourceBinding,
        base_revision: SceneAuthoringRevision,
        candidate: SceneDocumentCandidate,
    ) -> Self {
        Self {
            document,
            source_key,
            source_binding,
            base_revision,
            candidate: ProposalCandidate::Scene(candidate),
        }
    }

    #[must_use]
    pub fn prefab(
        document: EditorDocumentId,
        source_key: SourceDocumentKey,
        source_binding: EditorDocumentSourceBinding,
        base_revision: SceneAuthoringRevision,
        candidate: PrefabDocumentCandidate,
    ) -> Self {
        Self {
            document,
            source_key,
            source_binding,
            base_revision,
            candidate: ProposalCandidate::Prefab(candidate),
        }
    }
}

/// Exact future content observed as saved by the concrete Host, not a publication receipt.
///
/// The Host must prove that the actual current source leaf has this digest and is the encoded
/// prepared successor. It separately retains publication outcomes and the actual new file leaf.
/// Neither constructing this observation nor binding it claims filesystem verification here.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorAuthoredUpgradeSavedSource {
    document: EditorDocumentId,
    source_key: SourceDocumentKey,
    source_binding: EditorDocumentSourceBinding,
    revision: SceneAuthoringRevision,
    content_state: SceneAuthoringContentState,
    digest: EditorDocumentDigest,
}

impl EditorAuthoredUpgradeSavedSource {
    #[must_use]
    pub fn new(
        document: EditorDocumentId,
        source_key: SourceDocumentKey,
        source_binding: EditorDocumentSourceBinding,
        revision: SceneAuthoringRevision,
        content_state: SceneAuthoringContentState,
        digest: EditorDocumentDigest,
    ) -> Self {
        Self {
            document,
            source_key,
            source_binding,
            revision,
            content_state,
            digest,
        }
    }
}

/// Complete ordered open-source retention before the Host admits any unopened classification.
///
/// This move-only proof belongs to one Workspace and its exact previous nominal Schema basis.
/// The source proofs additionally belong to the cumulative Work that retained their incumbents.
/// Capture moves them into the frozen Scene/Prefab inputs for reuse without a second retention.
#[derive(Debug)]
#[must_use]
struct RetainedWorkspaceSources {
    owner: Arc<EditorWorkspacePersistenceIdentity>,
    previous: AuthoringSchemaBasis,
    sources: Vec<RetainedWorkspaceSource>,
}

#[derive(Debug)]
struct RetainedWorkspaceSource {
    document: EditorDocumentId,
    binding: EditorDocumentSourceBinding,
    revision: SceneAuthoringRevision,
    kind: SourceDocumentKind,
    source: RetainedAuthoredSchemaUpgradeSource,
}

impl RetainedWorkspaceSources {
    fn validate(
        &self,
        workspace: &EditorWorkspace,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let active = workspace.active_document;
        if !Arc::ptr_eq(&self.owner, &workspace.persistence_identity)
            || self.sources.len() != workspace.documents.len()
        {
            return Err(refused(
                active,
                None,
                "Retained sources require the original Workspace, complete set and nominal basis",
            ));
        }
        let bytes = self.sources.iter().try_fold(0_usize, |bytes, source| {
            bytes
                .checked_add(size_of::<&RetainedWorkspaceSource>())?
                .checked_add(source.binding.locator().len())
        });
        work.with_preparation_work("workspace-retained-source-validation", bytes, || {
            for (source, (document, slot)) in self.sources.iter().zip(&workspace.documents) {
                let kind = match slot {
                    EditorDocumentSlot::Scene(_) => Some(SourceDocumentKind::Scene),
                    EditorDocumentSlot::Prefab(_) => Some(SourceDocumentKind::Prefab),
                    EditorDocumentSlot::Extension(_) => None,
                };
                if source.document != *document
                    || Some(source.kind) != kind
                    || slot.source_binding() != Some(&source.binding)
                    || slot.revision() != source.revision
                    || slot.authoring_schema_basis().as_ref() != Some(&self.previous)
                {
                    return Err(refused(
                        active,
                        Some(*document),
                        "A retained source changed its binding, kind, incarnation, revision or basis",
                    ));
                }
            }
            Ok(())
        })
        .map_err(|report| source_report(active, None, report))?
    }
}

/// Owned immutable input for the complete original open set, without live Session ownership.
///
/// Capture charges every incumbent source, classification and both history branches to the
/// operation's cumulative Work before capturing these source inputs. The source owners keep
/// their original Arcs and nominal revision facts; history stays with the live Workspace and
/// must still validate before installation. Borrowing this input for repeated preparation never
/// retains those originals again. Keep it charged alongside unopened sources and preparations.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceAuthoredSchemaUpgradeInput {
    owner: Arc<EditorWorkspacePersistenceIdentity>,
    active: Option<EditorDocumentId>,
    previous: AuthoringSchemaBasis,
    sources: Vec<WorkspaceSourceInput>,
}

#[derive(Debug)]
struct WorkspaceSourceInput {
    document: EditorDocumentId,
    binding: EditorDocumentSourceBinding,
    source: WorkspaceSourceInputKind,
}

#[derive(Debug)]
enum WorkspaceSourceInputKind {
    Scene(SceneAuthoredSchemaUpgradeInput),
    Prefab(PrefabAuthoredSchemaUpgradeInput),
}

impl WorkspaceSourceInputKind {
    fn revision(&self) -> SceneAuthoringRevision {
        match self {
            Self::Scene(source) => source.revision(),
            Self::Prefab(source) => source.revision(),
        }
    }

    fn source(&self) -> &AuthoringDocumentSource {
        match self {
            Self::Scene(source) => source.source(),
            Self::Prefab(source) => source.source(),
        }
    }
}

impl EditorWorkspaceAuthoredSchemaUpgradeInput {
    #[must_use]
    pub const fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.previous
    }

    pub fn documents(&self) -> impl ExactSizeIterator<Item = EditorDocumentId> + '_ {
        self.sources.iter().map(|source| source.document)
    }

    #[must_use]
    pub fn scene_document(&self, document: EditorDocumentId) -> Option<&SceneDocument> {
        match &self.find(document)?.source {
            WorkspaceSourceInputKind::Scene(source) => Some(source.document()),
            WorkspaceSourceInputKind::Prefab(_) => None,
        }
    }

    #[must_use]
    pub fn prefab_document(&self, document: EditorDocumentId) -> Option<&PrefabDocument> {
        match &self.find(document)?.source {
            WorkspaceSourceInputKind::Prefab(source) => Some(source.document()),
            WorkspaceSourceInputKind::Scene(_) => None,
        }
    }

    #[must_use]
    pub fn document_revision(&self, document: EditorDocumentId) -> Option<SceneAuthoringRevision> {
        self.find(document).map(|source| source.source.revision())
    }

    #[must_use]
    pub fn document_source_binding(
        &self,
        document: EditorDocumentId,
    ) -> Option<&EditorDocumentSourceBinding> {
        self.find(document).map(|source| &source.binding)
    }

    #[must_use]
    pub fn document_source(&self, document: EditorDocumentId) -> Option<&AuthoringDocumentSource> {
        self.find(document).map(|source| source.source.source())
    }

    fn find(&self, document: EditorDocumentId) -> Option<&WorkspaceSourceInput> {
        let index = self
            .sources
            .binary_search_by_key(&document, |source| source.document)
            .ok()?;
        Some(&self.sources[index])
    }
}

/// Prepared successors for every original open document, including inactive dirty working copies.
///
/// The original allowance also reserves the complete bound-guard and saved-input storage. Binding
/// and returning from a refused Ready publication reuse this reservation; each validation still
/// charges cumulative work. The source Vec remains available as rollback storage while bound.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct PreparedEditorWorkspaceAuthoredSchemaUpgrade {
    owner: Arc<EditorWorkspacePersistenceIdentity>,
    previous: AuthoringSchemaBasis,
    successor: AuthoringSchemaBasis,
    sources: Vec<PreparedSource>,
    usage: AuthoredSchemaUpgradeUsage,
}

/// A refused binding with the unchanged prepared Workspace and original ordered saved inputs.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceAuthoredSchemaUpgradeBindFailure {
    pub prepared: PreparedEditorWorkspaceAuthoredSchemaUpgrade,
    pub saved: Vec<EditorAuthoredUpgradeSavedSource>,
    pub report: EditorWorkspaceCommandReport,
}

impl EditorWorkspaceAuthoredSchemaUpgradeBindFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(
        self,
    ) -> (
        PreparedEditorWorkspaceAuthoredSchemaUpgrade,
        Vec<EditorAuthoredUpgradeSavedSource>,
        EditorWorkspaceCommandReport,
    ) {
        (self.prepared, self.saved, self.report)
    }
}

#[derive(Debug)]
struct PreparedSource {
    document: EditorDocumentId,
    binding: EditorDocumentSourceBinding,
    source: PreparedSourceKind,
}

#[derive(Debug)]
enum PreparedSourceKind {
    Scene(PreparedSceneAuthoredSchemaUpgrade),
    Prefab(PreparedPrefabAuthoredSchemaUpgrade),
}

impl PreparedSourceKind {
    fn source_key(&self) -> &SourceDocumentKey {
        match self {
            Self::Scene(source) => source.source_key(),
            Self::Prefab(source) => source.source_key(),
        }
    }

    fn revision(&self) -> SceneAuthoringRevision {
        match self {
            Self::Scene(source) => source.revision(),
            Self::Prefab(source) => source.revision(),
        }
    }

    fn content_state(&self) -> SceneAuthoringContentState {
        match self {
            Self::Scene(source) => source.content_state(),
            Self::Prefab(source) => source.content_state(),
        }
    }

    fn encoded_byte_limit(&self) -> usize {
        match self {
            Self::Scene(source) => source.file_limits().encoded_bytes().get(),
            Self::Prefab(source) => source.file_limits().encoded_bytes().get(),
        }
    }
}

impl PreparedEditorWorkspaceAuthoredSchemaUpgrade {
    /// Preparation usage only; the shared Work also charges subsequent validation and binding.
    #[must_use]
    pub const fn usage(&self) -> AuthoredSchemaUpgradeUsage {
        self.usage
    }

    #[must_use]
    pub const fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.successor
    }

    pub fn documents(&self) -> impl ExactSizeIterator<Item = EditorDocumentId> + '_ {
        self.sources.iter().map(|source| source.document)
    }

    #[must_use]
    pub fn source_binding(
        &self,
        document: EditorDocumentId,
    ) -> Option<&EditorDocumentSourceBinding> {
        self.find(document).map(|source| &source.binding)
    }

    #[must_use]
    pub fn scene(&self, document: EditorDocumentId) -> Option<&PreparedSceneAuthoredSchemaUpgrade> {
        match &self.find(document)?.source {
            PreparedSourceKind::Scene(source) => Some(source),
            PreparedSourceKind::Prefab(_) => None,
        }
    }

    #[must_use]
    pub fn prefab(
        &self,
        document: EditorDocumentId,
    ) -> Option<&PreparedPrefabAuthoredSchemaUpgrade> {
        match &self.find(document)?.source {
            PreparedSourceKind::Prefab(source) => Some(source),
            PreparedSourceKind::Scene(_) => None,
        }
    }

    fn find(&self, document: EditorDocumentId) -> Option<&PreparedSource> {
        let index = self
            .sources
            .binary_search_by_key(&document, |source| source.document)
            .ok()?;
        Some(&self.sources[index])
    }

    /// Revalidates all incumbents without changing a source, saved checkpoint or navigation.
    /// The Host uses the same cumulative Work as preparation and final binding.
    pub fn validate(
        &self,
        workspace: &EditorWorkspace,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let active = workspace.active_document;
        if !Arc::ptr_eq(&self.owner, &workspace.persistence_identity)
            || self.sources.len() != workspace.documents.len()
        {
            return Err(refused(
                active,
                None,
                "The original complete Workspace set changed",
            ));
        }
        // Validation borrows kept preparations and produces no retained output. Its scan work
        // remains cumulative, while the temporary allowance ends on success or refusal.
        let bytes = self.sources.iter().try_fold(0_usize, |bytes, source| {
            bytes
                .checked_add(size_of::<&PreparedSource>())?
                .checked_add(source_metadata_bytes(
                    source.source.source_key(),
                    &source.binding,
                )?)
        });
        work.with_preparation_work("workspace-source-validation", bytes, || {
            for (source, (document, slot)) in self.sources.iter().zip(&workspace.documents) {
                if source.document != *document
                    || slot.source_binding() != Some(&source.binding)
                    || slot.authoring_schema_basis().as_ref() != Some(&self.previous)
                {
                    return Err(refused(
                        active,
                        Some(*document),
                        "The original source binding or basis changed",
                    ));
                }
                let result = match (slot, &source.source) {
                    (EditorDocumentSlot::Scene(slot), PreparedSourceKind::Scene(prepared)) => {
                        if slot.session.live_instance().is_some() {
                            return Err(refused(
                                active,
                                Some(*document),
                                "Source installation requires a detached Scene",
                            ));
                        }
                        slot.session
                            .validate_authored_schema_upgrade(prepared.source_key(), prepared)
                    }
                    (EditorDocumentSlot::Prefab(slot), PreparedSourceKind::Prefab(prepared)) => {
                        slot.session
                            .validate_authored_schema_upgrade(prepared.source_key(), prepared)
                    }
                    _ => {
                        return Err(refused(
                            active,
                            Some(*document),
                            "The original source kind changed",
                        ));
                    }
                };
                result.map_err(|report| source_report(active, Some(*document), report))?;
            }
            Ok(())
        })
        .map_err(|report| source_report(active, None, report))?
    }
}

impl EditorWorkspace {
    /// Retains every open incumbent before the Host allocates unopened classifications.
    ///
    /// Collection storage is admitted before allocation or copying. All source kinds, bindings
    /// and exact previous bases must agree; the Scene/Prefab owners retain each actual source,
    /// classification and both history branches under the supplied cumulative Work.
    fn retain_authored_upgrade_sources(
        &self,
        previous: &AuthoringSchemaBasis,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<RetainedWorkspaceSources, EditorWorkspaceCommandReport> {
        let active = self.active_document;
        let storage = self
            .documents
            .len()
            .checked_mul(size_of::<RetainedWorkspaceSource>())
            .and_then(|bytes| bytes.checked_add(size_of::<RetainedWorkspaceSources>()))
            .and_then(|bytes| {
                self.documents.values().try_fold(bytes, |bytes, slot| {
                    bytes
                        .checked_add(
                            slot.source_binding()
                                .map_or(0, |binding| binding.locator().len()),
                        )?
                        .checked_add(2 * size_of::<usize>())
                })
            });
        work.with_preparation_work("workspace-source-retention-validation", storage, || {
            for (document, slot) in &self.documents {
                if matches!(slot, EditorDocumentSlot::Extension(_))
                    || slot.source_binding().is_none()
                    || slot.authoring_schema_basis().as_ref() != Some(previous)
                    || matches!(slot, EditorDocumentSlot::Scene(slot) if slot.session.live_instance().is_some())
                {
                    return Err(refused(
                        active,
                        Some(*document),
                        "Source retention requires a bound detached authoring Scene or Prefab under the exact previous basis",
                    ));
                }
            }
            Ok(())
        })
        .map_err(|report| source_report(active, None, report))??;
        charge(work, storage, active)?;
        let mut sources = Vec::with_capacity(self.documents.len());
        for (document, slot) in &self.documents {
            let (kind, source) = match slot {
                EditorDocumentSlot::Scene(slot) => (
                    SourceDocumentKind::Scene,
                    slot.session.retain_authored_schema_upgrade_source(work),
                ),
                EditorDocumentSlot::Prefab(slot) => (
                    SourceDocumentKind::Prefab,
                    slot.session.retain_authored_schema_upgrade_source(work),
                ),
                EditorDocumentSlot::Extension(_) => {
                    unreachable!("nominal source kinds were checked before retention")
                }
            };
            let source = source.map_err(|report| source_report(active, Some(*document), report))?;
            sources.push(RetainedWorkspaceSource {
                document: *document,
                binding: slot
                    .source_binding()
                    .expect("the complete bound set was checked")
                    .clone(),
                revision: slot.revision(),
                kind,
                source,
            });
        }
        Ok(RetainedWorkspaceSources {
            owner: Arc::clone(&self.persistence_identity),
            previous: previous.clone(),
            sources,
        })
    }

    /// Captures one complete immutable open set before the Host prepares any candidate.
    ///
    /// Every incumbent, including both history branches, is charged before any source snapshot
    /// is captured. This owns no live Sessions and can move with the same Work to a finite worker.
    /// The Host admits proposal decoding separately and keeps this input charged across retries.
    /// On refusal, all new capture locals drop before their storage reservations are settled;
    /// performed work, previous reservations and the observed peak are preserved.
    #[doc(hidden)]
    pub fn capture_authored_schema_upgrade_input(
        &self,
        previous: &AuthoringSchemaBasis,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<EditorWorkspaceAuthoredSchemaUpgradeInput, EditorWorkspaceCommandReport> {
        work.with_preparation_attempt(|work| {
            let retained = self.retain_authored_upgrade_sources(previous, work)?;
            self.capture_retained_authored_upgrade_input(retained, work)
        })
    }

    fn capture_retained_authored_upgrade_input(
        &self,
        retained: RetainedWorkspaceSources,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<EditorWorkspaceAuthoredSchemaUpgradeInput, EditorWorkspaceCommandReport> {
        let active = self.active_document;
        retained.validate(self, work)?;
        let storage = retained
            .sources
            .len()
            .checked_mul(size_of::<WorkspaceSourceInput>())
            .and_then(|bytes| {
                bytes.checked_add(size_of::<EditorWorkspaceAuthoredSchemaUpgradeInput>())
            });
        // The retained-source table and this snapshot table coexist during capture. Their
        // collection storage is admitted separately before either allocation; bindings move.
        charge(work, storage, active)?;
        let mut sources = Vec::with_capacity(retained.sources.len());
        for (retained, (document, slot)) in retained.sources.into_iter().zip(&self.documents) {
            let source = match slot {
                EditorDocumentSlot::Scene(slot) => slot
                    .session
                    .capture_authored_schema_upgrade_input(retained.source, work)
                    .map(WorkspaceSourceInputKind::Scene),
                EditorDocumentSlot::Prefab(slot) => slot
                    .session
                    .capture_authored_schema_upgrade_input(retained.source, work)
                    .map(WorkspaceSourceInputKind::Prefab),
                EditorDocumentSlot::Extension(_) => {
                    unreachable!("the complete nominal source set was validated")
                }
            }
            .map_err(|report| source_report(active, Some(*document), report))?;
            sources.push(WorkspaceSourceInput {
                document: retained.document,
                binding: retained.binding,
                source,
            });
        }
        Ok(EditorWorkspaceAuthoredSchemaUpgradeInput {
            owner: retained.owner,
            active,
            previous: retained.previous,
            sources,
        })
    }

    /// Prepares the complete open set from exact nominal proposals, in any input order.
    ///
    /// The Host admits the complete decoded proposal payload before calling this method.
    /// Workspace collection/key/binding storage and every incumbent are admitted here before
    /// the first successor is prepared. All operation stages share the supplied Work.
    #[doc(hidden)]
    pub fn prepare_authored_schema_upgrade(
        &self,
        upgrade: &AuthoredFieldUpgrade,
        proposals: Vec<EditorAuthoredSchemaUpgradeProposal>,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedEditorWorkspaceAuthoredSchemaUpgrade, EditorWorkspaceCommandReport> {
        work.with_preparation_attempt(|work| {
            self.capture_authored_schema_upgrade_input(upgrade.previous_basis(), work)?
                .prepare(upgrade, proposals, work)
        })
    }
}

impl EditorWorkspaceAuthoredSchemaUpgradeInput {
    /// Prepares the exact frozen set without observing or mutating a live Workspace.
    ///
    /// The Host admits the decoded proposal payload before constructing it. Every attempt uses
    /// the original source tokens and the same cumulative Work as capture. Refusal releases only
    /// this attempt's new preparations; this borrowed input, its originals and their reservations
    /// remain intact. Installation still needs validation and binding against the real Workspace.
    pub fn prepare(
        &self,
        upgrade: &AuthoredFieldUpgrade,
        proposals: Vec<EditorAuthoredSchemaUpgradeProposal>,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedEditorWorkspaceAuthoredSchemaUpgrade, EditorWorkspaceCommandReport> {
        work.with_preparation_attempt(|work| self.prepare_sources(upgrade, proposals, work))
    }

    fn prepare_sources(
        &self,
        upgrade: &AuthoredFieldUpgrade,
        mut proposals: Vec<EditorAuthoredSchemaUpgradeProposal>,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedEditorWorkspaceAuthoredSchemaUpgrade, EditorWorkspaceCommandReport> {
        let active = self.active;
        if &self.previous != upgrade.previous_basis() {
            return Err(refused(
                active,
                None,
                "Frozen sources require their original nominal Schema basis",
            ));
        }
        if proposals.len() != self.sources.len() {
            return Err(refused(
                active,
                None,
                "Proposals must cover the complete open document set",
            ));
        }
        let per_source = size_of::<EditorAuthoredSchemaUpgradeProposal>()
            .checked_add(size_of::<PreparedSource>())
            .and_then(|bytes| bytes.checked_add(size_of::<BoundSource<'_>>()))
            .and_then(|bytes| bytes.checked_add(size_of::<EditorAuthoredUpgradeSavedSource>()))
            .and_then(|bytes| bytes.checked_add(size_of::<&SourceDocumentKey>()));
        let storage = per_source
            .and_then(|bytes| bytes.checked_mul(proposals.len()))
            .and_then(|bytes| {
                bytes.checked_add(size_of::<PreparedEditorWorkspaceAuthoredSchemaUpgrade>())
            })
            .and_then(|bytes| {
                bytes.checked_add(size_of::<EditorWorkspaceAuthoredSchemaUpgradeCommit<'_>>())
            })
            .and_then(|bytes| {
                proposals.iter().try_fold(bytes, |bytes, proposal| {
                    let metadata =
                        source_metadata_bytes(&proposal.source_key, &proposal.source_binding)?;
                    // Prepared and saved inputs own their keys/bindings independently. The
                    // bound guard moves the prepared metadata and keeps the original Vec empty.
                    bytes.checked_add(metadata.checked_mul(2)?)
                })
            });
        charge(work, storage, active)?;
        proposals.sort_unstable_by_key(|proposal| proposal.document);
        for (source, proposal) in self.sources.iter().zip(&proposals) {
            let kind_matches = matches!(
                (
                    &source.source,
                    &proposal.candidate,
                    proposal.source_key.kind()
                ),
                (
                    WorkspaceSourceInputKind::Scene(_),
                    ProposalCandidate::Scene(_),
                    SourceDocumentKind::Scene
                ) | (
                    WorkspaceSourceInputKind::Prefab(_),
                    ProposalCandidate::Prefab(_),
                    SourceDocumentKind::Prefab
                )
            );
            if proposal.document != source.document
                || !kind_matches
                || source.binding != proposal.source_binding
                || source.source.revision() != proposal.base_revision
            {
                return Err(refused(
                    active,
                    Some(source.document),
                    "Proposal requires its exact original document, binding, revision and basis",
                ));
            }
        }
        {
            let mut keys = Vec::with_capacity(proposals.len());
            keys.extend(proposals.iter().map(|proposal| &proposal.source_key));
            let comparisons = (usize::BITS - keys.len().leading_zeros()) as usize + 1;
            let ordering_work = keys.iter().try_fold(0usize, |bytes, key| {
                bytes.checked_add(key.retained_bytes().checked_mul(comparisons)?)
            });
            work.with_preparation_work("workspace-source-key-ordering", ordering_work, || {
                keys.sort_unstable();
            })
            .map_err(|report| source_report(active, None, report))?;
            if keys.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(refused(
                    active,
                    None,
                    "Distinct open documents require distinct complete source keys",
                ));
            }
        }
        let mut sources = Vec::with_capacity(self.sources.len());
        for (source, proposal) in self.sources.iter().zip(proposals) {
            let EditorAuthoredSchemaUpgradeProposal {
                source_key,
                source_binding,
                candidate,
                ..
            } = proposal;
            let prepared = match (&source.source, candidate) {
                (WorkspaceSourceInputKind::Scene(source), ProposalCandidate::Scene(candidate)) => {
                    source
                        .prepare(&source_key, candidate, upgrade, work)
                        .map(PreparedSourceKind::Scene)
                }
                (
                    WorkspaceSourceInputKind::Prefab(source),
                    ProposalCandidate::Prefab(candidate),
                ) => source
                    .prepare(&source_key, candidate, upgrade, work)
                    .map(PreparedSourceKind::Prefab),
                _ => unreachable!("nominal source kinds were checked before preparation"),
            }
            .map_err(|report| source_report(active, Some(source.document), report))?;
            sources.push(PreparedSource {
                document: source.document,
                binding: source_binding,
                source: prepared,
            });
        }
        Ok(PreparedEditorWorkspaceAuthoredSchemaUpgrade {
            owner: Arc::clone(&self.owner),
            previous: self.previous.clone(),
            successor: upgrade.successor_basis().clone(),
            sources,
            usage: work.usage(),
        })
    }
}

/// Every source and saved-checkpoint destination is exclusively bound before the first swap.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceAuthoredSchemaUpgradeCommit<'a> {
    prepared: PreparedEditorWorkspaceAuthoredSchemaUpgrade,
    saved: Vec<EditorAuthoredUpgradeSavedSource>,
    sources: Vec<BoundSource<'a>>,
}

#[derive(Debug)]
struct BoundSource<'a> {
    document: EditorDocumentId,
    binding: EditorDocumentSourceBinding,
    source: BoundSourceKind<'a>,
}

#[derive(Debug)]
enum BoundSourceKind<'a> {
    Scene(
        SceneAuthoredSchemaUpgradeCommit<'a>,
        BoundSavedCheckpoint<'a>,
    ),
    Prefab(
        PrefabAuthoredSchemaUpgradeCommit<'a>,
        BoundSavedCheckpoint<'a>,
    ),
}

#[derive(Debug)]
struct BoundSavedCheckpoint<'a> {
    saved_revision: &'a mut SceneAuthoringRevision,
    saved_content_state: &'a mut SceneAuthoringContentState,
    saved_digest: &'a mut Option<EditorDocumentDigest>,
    external_reload: &'a mut EditorExternalReloadState,
    revision: SceneAuthoringRevision,
    content_state: SceneAuthoringContentState,
    digest: EditorDocumentDigest,
}

impl BoundSavedCheckpoint<'_> {
    fn commit(self) {
        *self.saved_revision = self.revision;
        *self.saved_content_state = self.content_state;
        *self.saved_digest = Some(self.digest);
        *self.external_reload = EditorExternalReloadState::Clean;
    }
}

impl EditorWorkspaceAuthoredSchemaUpgradeCommit<'_> {
    /// Releases all exclusive destinations without installation after Ready refuses or is uncertain.
    ///
    /// Returns the original candidates, nominal bases, future identities and saved-input order.
    /// Moving each source back uses its original Vec capacity; no Schema, document, history or
    /// saved input is cloned, prepared again or recharged as retained storage.
    pub fn into_prepared(
        self,
    ) -> (
        PreparedEditorWorkspaceAuthoredSchemaUpgrade,
        Vec<EditorAuthoredUpgradeSavedSource>,
    ) {
        let Self {
            mut prepared,
            saved,
            mut sources,
        } = self;
        restore_bound_sources(&mut prepared, &mut sources);
        (prepared, saved)
    }

    /// Installs only prebound state. There are no lookups, allocations or fallible admissions.
    pub fn commit(self) {
        for source in self.sources.into_iter().rev() {
            match source.source {
                BoundSourceKind::Scene(source, saved) => {
                    let acknowledged = source
                        .commit()
                        .acknowledge_source_saved(saved.content_state);
                    debug_assert!(
                        acknowledged,
                        "the exact future saved state was bound before installation"
                    );
                    saved.commit();
                }
                BoundSourceKind::Prefab(source, saved) => {
                    let acknowledged = source
                        .commit()
                        .acknowledge_source_saved(saved.content_state);
                    debug_assert!(
                        acknowledged,
                        "the exact future saved state was bound before installation"
                    );
                    saved.commit();
                }
            }
        }
    }
}

impl EditorPersistenceAuthority {
    /// Binds the complete source/history/saved transition to this hosted Workspace authority.
    ///
    /// Observations must name the exact prepared future states; an old Save checkpoint cannot
    /// stand in for them. The Host separately binds actual new leaves, lane facts and project
    /// adoption before publishing Ready. Refusal returns both original owners without mutation.
    /// All validation finishes before consuming a source, using the preparation's preadmitted
    /// guard storage and temporary, cumulatively charged ordering work.
    #[doc(hidden)]
    pub fn bind_authored_schema_upgrade<'w>(
        &mut self,
        workspace: &'w mut EditorWorkspace,
        mut prepared: PreparedEditorWorkspaceAuthoredSchemaUpgrade,
        saved: Vec<EditorAuthoredUpgradeSavedSource>,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<
        EditorWorkspaceAuthoredSchemaUpgradeCommit<'w>,
        EditorWorkspaceAuthoredSchemaUpgradeBindFailure,
    > {
        let active = workspace.active_document;
        let result = if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            Err(refused(
                active,
                None,
                "Saved adoption requires the original hosted Workspace authority",
            ))
        } else {
            bind_sources(workspace, &mut prepared, &saved, work)
        };
        match result {
            Ok(sources) => Ok(EditorWorkspaceAuthoredSchemaUpgradeCommit {
                prepared,
                saved,
                sources,
            }),
            Err(report) => Err(EditorWorkspaceAuthoredSchemaUpgradeBindFailure {
                prepared,
                saved,
                report,
            }),
        }
    }
}

fn bind_sources<'w>(
    workspace: &'w mut EditorWorkspace,
    prepared: &mut PreparedEditorWorkspaceAuthoredSchemaUpgrade,
    saved: &[EditorAuthoredUpgradeSavedSource],
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<Vec<BoundSource<'w>>, EditorWorkspaceCommandReport> {
    let active = workspace.active_document;
    prepared.validate(workspace, work)?;
    if saved.len() != prepared.sources.len() {
        return Err(refused(
            active,
            None,
            "Saved observations must cover every prepared source exactly once",
        ));
    }
    let comparisons = (usize::BITS - saved.len().leading_zeros()) as usize + 1;
    let ordering_work = size_of::<usize>()
        .checked_mul(saved.len())
        .and_then(|bytes| bytes.checked_mul(comparisons))
        .and_then(|bytes| {
            prepared.sources.iter().try_fold(bytes, |bytes, source| {
                bytes.checked_add(source_metadata_bytes(
                    source.source.source_key(),
                    &source.binding,
                )?)
            })
        });
    work.with_preparation_work("workspace-saved-source-binding", ordering_work, move || {
        // Sort only preadmitted indexes, leaving both input owners and saved order untouched on
        // every refusal. Saved metadata must match the already reserved exact source metadata.
        let mut order = Vec::with_capacity(saved.len());
        order.extend(0..saved.len());
        order.sort_unstable_by_key(|index| saved[*index].document);
        for (source, index) in prepared.sources.iter().zip(&order) {
            let saved = &saved[*index];
            if saved.document != source.document
                || saved.source_key != *source.source.source_key()
                || saved.source_binding != source.binding
                || saved.revision != source.source.revision()
                || saved.content_state != source.source.content_state()
                || saved.digest.encoded_bytes()
                    > u64::try_from(source.source.encoded_byte_limit()).unwrap_or(u64::MAX)
            {
                return Err(refused(
                    active,
                    Some(source.document),
                    "Saved observation requires the exact prepared source, binding and future checkpoint",
                ));
            }
        }
        let mut sources = Vec::with_capacity(prepared.sources.len());
        // Pop in reverse document order. On any later refusal, the unchanged prefix can accept
        // the refused source followed by previously bound sources, without another allocation.
        for ((document, slot), index) in workspace
            .documents
            .iter_mut()
            .rev()
            .zip(order.into_iter().rev())
        {
            let Some(source) = prepared.sources.pop() else {
                restore_bound_sources(prepared, &mut sources);
                return Err(refused(
                    active,
                    Some(*document),
                    "The prepared source set changed during binding",
                ));
            };
            match bind_source(slot, source, &saved[index], active) {
                Ok(source) => sources.push(source),
                Err((source, report)) => {
                    prepared.sources.push(source);
                    restore_bound_sources(prepared, &mut sources);
                    return Err(report);
                }
            }
        }
        Ok(sources)
    })
    .map_err(|report| source_report(active, None, report))?
}

fn bind_source<'w>(
    slot: &'w mut EditorDocumentSlot,
    prepared: PreparedSource,
    saved: &EditorAuthoredUpgradeSavedSource,
    active: Option<EditorDocumentId>,
) -> Result<BoundSource<'w>, (PreparedSource, EditorWorkspaceCommandReport)> {
    let PreparedSource {
        document,
        binding,
        source,
    } = prepared;
    let source = match (slot, source) {
        (EditorDocumentSlot::Scene(slot), PreparedSourceKind::Scene(prepared)) => {
            match slot
                .session
                .bind_authored_schema_upgrade(&saved.source_key, prepared)
            {
                Ok(source) => BoundSourceKind::Scene(
                    source,
                    BoundSavedCheckpoint {
                        saved_revision: &mut slot.saved_revision,
                        saved_content_state: &mut slot.saved_content_state,
                        saved_digest: &mut slot.saved_digest,
                        external_reload: &mut slot.external_reload,
                        revision: saved.revision,
                        content_state: saved.content_state,
                        digest: saved.digest,
                    },
                ),
                Err(failure) => {
                    let (source, report) = failure.into_parts();
                    return Err((
                        PreparedSource {
                            document,
                            binding,
                            source: PreparedSourceKind::Scene(source),
                        },
                        source_report(active, Some(document), report),
                    ));
                }
            }
        }
        (EditorDocumentSlot::Prefab(slot), PreparedSourceKind::Prefab(prepared)) => {
            match slot
                .session
                .bind_authored_schema_upgrade(&saved.source_key, prepared)
            {
                Ok(source) => BoundSourceKind::Prefab(
                    source,
                    BoundSavedCheckpoint {
                        saved_revision: &mut slot.saved_revision,
                        saved_content_state: &mut slot.saved_content_state,
                        saved_digest: &mut slot.saved_digest,
                        external_reload: &mut slot.external_reload,
                        revision: saved.revision,
                        content_state: saved.content_state,
                        digest: saved.digest,
                    },
                ),
                Err(failure) => {
                    let (source, report) = failure.into_parts();
                    return Err((
                        PreparedSource {
                            document,
                            binding,
                            source: PreparedSourceKind::Prefab(source),
                        },
                        source_report(active, Some(document), report),
                    ));
                }
            }
        }
        (_, source) => {
            return Err((
                PreparedSource {
                    document,
                    binding,
                    source,
                },
                refused(active, Some(document), "The original source kind changed"),
            ));
        }
    };
    Ok(BoundSource {
        document,
        binding,
        source,
    })
}

impl BoundSource<'_> {
    fn into_prepared(self) -> PreparedSource {
        let source = match self.source {
            BoundSourceKind::Scene(source, _) => PreparedSourceKind::Scene(source.into_prepared()),
            BoundSourceKind::Prefab(source, _) => {
                PreparedSourceKind::Prefab(source.into_prepared())
            }
        };
        PreparedSource {
            document: self.document,
            binding: self.binding,
            source,
        }
    }
}

fn restore_bound_sources(
    prepared: &mut PreparedEditorWorkspaceAuthoredSchemaUpgrade,
    sources: &mut Vec<BoundSource<'_>>,
) {
    while let Some(source) = sources.pop() {
        prepared.sources.push(source.into_prepared());
    }
}

fn source_metadata_bytes(
    key: &SourceDocumentKey,
    binding: &EditorDocumentSourceBinding,
) -> Option<usize> {
    key.retained_bytes()
        .checked_add(binding.locator().len())?
        .checked_add(2 * size_of::<usize>())
}

fn charge(
    work: &mut AuthoredSchemaUpgradeWork,
    bytes: Option<usize>,
    active: Option<EditorDocumentId>,
) -> Result<(), EditorWorkspaceCommandReport> {
    let bytes = bytes.ok_or_else(|| {
        refused(
            active,
            None,
            "Workspace upgrade storage accounting overflowed",
        )
    })?;
    work.retain_preparation_storage(bytes)
        .map_err(|report| source_report(active, None, report))
}

fn refused(
    active: Option<EditorDocumentId>,
    document: Option<EditorDocumentId>,
    message: &'static str,
) -> EditorWorkspaceCommandReport {
    workspace_error_report(
        WorkspaceDocumentContext::new(active, document),
        "tooling.workspace-authored-schema-upgrade-rejected",
        message,
    )
}

fn source_report(
    active: Option<EditorDocumentId>,
    document: Option<EditorDocumentId>,
    report: nara_scene::ScenePatchReport,
) -> EditorWorkspaceCommandReport {
    EditorWorkspaceCommandReport {
        applied: false,
        active_document: active,
        document,
        diagnostics: report.diagnostics.clone(),
        patch_report: Some(report),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests;
