//! Complete open-source capture and selective source installation under one extraction allowance.
//!
//! Only actual changed outputs receive replacement checkpoints. Complete-set binding protects
//! unchanged Sessions without changing their history or saved state. File publication and saved
//! checkpoint evidence remain the persistence Host's separate responsibility. The saved sidecar
//! binds exact published checkpoints before installation; it is not complete Project publication.

use std::{mem::size_of, sync::Arc};

use nara_scene::{
    __private::{
        PrefabExtractionSourceCommit, PrefabExtractionUsage, PrefabExtractionWork,
        PrefabSourceExtractionInput, PreparedPrefabExtraction, PreparedPrefabExtractionSource,
        PreparedSceneExtractionSource, SceneExtractionSourceCommit, SceneSourceExtractionInput,
    },
    AuthoringDocumentSource, AuthoringSchemaBasis, PrefabAuthoringSession, PrefabDocument,
    SceneAuthoringRevision, SceneAuthoringSession, SceneDocument, SceneEntityId, ScenePatchReport,
};

use super::{
    EditorDocumentId, EditorDocumentKind, EditorDocumentSlot, EditorDocumentSourceBinding,
    EditorPersistenceAuthority, EditorWorkspace, EditorWorkspaceCommandReport,
    EditorWorkspacePersistenceIdentity, WorkspaceDocumentContext, workspace_error_report,
};

mod persistence;
use persistence::{BoundSavedFields, WorkspaceSavedInput};
#[doc(hidden)]
pub use persistence::{
    EditorPrefabExtractionDisposition, EditorWorkspaceExtractionCheckpointFailure,
    EditorWorkspaceExtractionEncodingFailure, EditorWorkspaceExtractionPersistenceBindFailure,
    EditorWorkspaceExtractionPersistenceCommit, EditorWorkspaceExtractionPersistencePrepareFailure,
    EditorWorkspaceExtractionSaveFailure, EditorWorkspaceExtractionSavedGenerationFailure,
    PreparedEditorWorkspaceExtractionPersistence,
};

/// The exact complete ordered open set and source-owned cut selected in one Workspace.
///
/// The Scene/Prefab owners retain their actual document, classification and both history branches
/// in the supplied cumulative Work before sharing source inputs. This move-only input can travel
/// with that same Work to a worker without moving or recreating a live Session. Navigation is
/// deliberately excluded from validation; every source remains an incumbent guard participant.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspacePrefabExtractionInput {
    owner: Arc<EditorWorkspacePersistenceIdentity>,
    basis: AuthoringSchemaBasis,
    selected: EditorDocumentId,
    root: SceneEntityId,
    sources: Vec<WorkspaceSourceInput>,
}

/// A snapshot-local association with an original verified corpus member.
///
/// The index conveys no project membership, source replacement or publication authority.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPrefabExtractionCorpusBinding {
    pub document: EditorDocumentId,
    pub source: usize,
}

/// The exact Workspace capture bound to immutable, verified extraction outputs.
///
/// The corpus retains its original cumulative allowance. Every captured source participates in
/// validation, including unchanged guards whose history this binding does not authorize clearing.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct PreparedEditorWorkspacePrefabExtraction {
    input: EditorWorkspacePrefabExtractionInput,
    corpus: PreparedPrefabExtraction,
    bindings: Vec<EditorPrefabExtractionCorpusBinding>,
}

/// Complete source preparation, with owner-issued tokens only for actually changed open sources.
///
/// Every `None` is an unchanged guard, never an unprepared changed source. The original corpus
/// owns the same cumulative Work throughout preparation, binding, refusal and retry.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct PreparedEditorWorkspaceExtractionSources {
    prepared: PreparedEditorWorkspacePrefabExtraction,
    sources: Vec<Option<PreparedSourceKind>>,
}

/// Refused source preparation returns the original bound corpus with all work still spent.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspacePrefabExtractionSourcePrepareFailure {
    prepared: PreparedEditorWorkspacePrefabExtraction,
    report: EditorWorkspaceCommandReport,
}

impl EditorWorkspacePrefabExtractionSourcePrepareFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(
        self,
    ) -> (
        PreparedEditorWorkspacePrefabExtraction,
        EditorWorkspaceCommandReport,
    ) {
        (self.prepared, self.report)
    }
}

/// Refused complete-set binding returns the exact source tokens and their spent corpus.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspacePrefabExtractionSourceBindFailure {
    prepared: PreparedEditorWorkspaceExtractionSources,
    report: EditorWorkspaceCommandReport,
}

impl EditorWorkspacePrefabExtractionSourceBindFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(
        self,
    ) -> (
        PreparedEditorWorkspaceExtractionSources,
        EditorWorkspaceCommandReport,
    ) {
        (self.prepared, self.report)
    }
}

#[derive(Debug)]
enum PreparedSourceKind {
    Scene(PreparedSceneExtractionSource),
    Prefab(PreparedPrefabExtractionSource),
}

/// Refused binding retains the exact input, corpus, mapping allocation and spent allowance.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorPrefabExtractionCorpusBindFailure {
    input: EditorWorkspacePrefabExtractionInput,
    corpus: PreparedPrefabExtraction,
    bindings: Vec<EditorPrefabExtractionCorpusBinding>,
    report: EditorWorkspaceCommandReport,
}

impl EditorPrefabExtractionCorpusBindFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(
        self,
    ) -> (
        EditorWorkspacePrefabExtractionInput,
        PreparedPrefabExtraction,
        Vec<EditorPrefabExtractionCorpusBinding>,
        EditorWorkspaceCommandReport,
    ) {
        (self.input, self.corpus, self.bindings, self.report)
    }
}

#[derive(Debug)]
struct WorkspaceSourceInput {
    document: EditorDocumentId,
    binding: EditorDocumentSourceBinding,
    saved: WorkspaceSavedInput,
    source: WorkspaceSourceInputKind,
}

#[derive(Debug)]
enum WorkspaceSourceInputKind {
    Scene(SceneSourceExtractionInput),
    Prefab(PrefabSourceExtractionInput),
}

impl WorkspaceSourceInputKind {
    fn kind(&self) -> EditorDocumentKind {
        match self {
            Self::Scene(_) => EditorDocumentKind::Scene,
            Self::Prefab(_) => EditorDocumentKind::Prefab,
        }
    }

    fn revision(&self) -> SceneAuthoringRevision {
        match self {
            Self::Scene(source) => source.revision(),
            Self::Prefab(source) => source.revision(),
        }
    }
}

impl EditorWorkspacePrefabExtractionInput {
    #[must_use]
    pub const fn selected_document(&self) -> EditorDocumentId {
        self.selected
    }

    #[must_use]
    pub const fn root(&self) -> &SceneEntityId {
        &self.root
    }

    #[must_use]
    pub const fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.basis
    }

    /// Borrows the complete membership in Workspace document-ID order, including inactive slots.
    pub fn documents(&self) -> impl ExactSizeIterator<Item = EditorDocumentId> + '_ {
        self.sources.iter().map(|source| source.document)
    }

    /// Borrows the actual owner-issued Scene input, including its original limits and source.
    #[must_use]
    pub fn scene_input(&self, document: EditorDocumentId) -> Option<&SceneSourceExtractionInput> {
        match &self.find(document)?.source {
            WorkspaceSourceInputKind::Scene(source) => Some(source),
            WorkspaceSourceInputKind::Prefab(_) => None,
        }
    }

    /// Borrows the actual Prefab input without constructing a Scene projection.
    #[must_use]
    pub fn prefab_input(&self, document: EditorDocumentId) -> Option<&PrefabSourceExtractionInput> {
        match &self.find(document)?.source {
            WorkspaceSourceInputKind::Scene(_) => None,
            WorkspaceSourceInputKind::Prefab(source) => Some(source),
        }
    }

    #[must_use]
    pub fn scene_document(&self, document: EditorDocumentId) -> Option<&SceneDocument> {
        self.scene_input(document).map(|source| source.document())
    }

    #[must_use]
    pub fn prefab_document(&self, document: EditorDocumentId) -> Option<&PrefabDocument> {
        self.prefab_input(document).map(|source| source.document())
    }

    #[must_use]
    pub fn document_revision(&self, document: EditorDocumentId) -> Option<SceneAuthoringRevision> {
        self.find(document).map(|source| source.source.revision())
    }

    #[must_use]
    pub fn document_kind(&self, document: EditorDocumentId) -> Option<EditorDocumentKind> {
        self.find(document).map(|source| source.source.kind())
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
        Some(match &self.find(document)?.source {
            WorkspaceSourceInputKind::Scene(source) => source.source(),
            WorkspaceSourceInputKind::Prefab(source) => source.source(),
        })
    }

    /// Rechecks every incumbent through its actual owner and the original cumulative Work.
    ///
    /// No source, history, saved checkpoint or navigation is changed on success or refusal.
    /// Validation work stays spent, including when the first changed member causes refusal.
    pub fn validate(
        &self,
        workspace: &EditorWorkspace,
        work: &mut PrefabExtractionWork,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let active = workspace.active_document;
        self.require_open_set(workspace, |bytes, document| {
            charge(work, bytes, active, document)
        })?;
        for (source, (document, slot)) in self.sources.iter().zip(&workspace.documents) {
            let result = match (&source.source, slot) {
                (WorkspaceSourceInputKind::Scene(input), EditorDocumentSlot::Scene(slot))
                    if slot.session.live_instance().is_none() =>
                {
                    slot.session.validate_prefab_extraction_input(input, work)
                }
                (WorkspaceSourceInputKind::Prefab(input), EditorDocumentSlot::Prefab(slot)) => {
                    slot.session.validate_prefab_extraction_input(input, work)
                }
                _ => {
                    return Err(refused(
                        active,
                        Some(*document),
                        "An extraction source changed kind or acquired a live Scene instance",
                    ));
                }
            };
            result.map_err(|report| source_report(active, Some(*document), report))?;
        }
        Ok(())
    }

    /// Binds complete ordered open membership to the corpus's actual original source inputs.
    /// Refusal returns all supplied owners, so corrected mappings reuse the same verified output
    /// and spent Work. The mapping may omit closed corpus members; it must cover every open slot.
    pub fn bind_corpus(
        self,
        mut prepared: PreparedPrefabExtraction,
        bindings: Vec<EditorPrefabExtractionCorpusBinding>,
    ) -> Result<PreparedEditorWorkspacePrefabExtraction, EditorPrefabExtractionCorpusBindFailure>
    {
        // Include the complete owned mapping capacity, both possible result wrappers, and the
        // full seen table before allocating it. Source bodies/locators remain capture-owned.
        let storage = bindings
            .capacity()
            .checked_mul(size_of::<EditorPrefabExtractionCorpusBinding>())
            .and_then(|bytes| {
                bytes.checked_add(
                    size_of::<PreparedEditorWorkspacePrefabExtraction>()
                        .max(size_of::<EditorPrefabExtractionCorpusBindFailure>()),
                )
            })
            .and_then(|bytes| bytes.checked_add(size_of::<Vec<bool>>()))
            .and_then(|bytes| {
                bytes.checked_add(
                    prepared
                        .original_sources()
                        .len()
                        .checked_mul(size_of::<bool>())?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    self.sources
                        .len()
                        .checked_mul(size_of::<WorkspaceSourceInput>())?,
                )
            })
            .and_then(|bytes| bytes.checked_add(self.root.as_str().len()))
            .and_then(|bytes| bytes.checked_add(prepared.root().as_str().len()));
        let result = prepared
            .admit_binding_storage(storage)
            .map_err(|report| source_report(None, Some(self.selected), report))
            .and_then(|()| self.require_corpus(&mut prepared, &bindings));
        match result {
            Ok(()) => Ok(PreparedEditorWorkspacePrefabExtraction {
                input: self,
                corpus: prepared,
                bindings,
            }),
            Err(report) => Err(EditorPrefabExtractionCorpusBindFailure {
                input: self,
                corpus: prepared,
                bindings,
                report,
            }),
        }
    }

    fn require_corpus(
        &self,
        prepared: &mut PreparedPrefabExtraction,
        bindings: &[EditorPrefabExtractionCorpusBinding],
    ) -> Result<(), EditorWorkspaceCommandReport> {
        if bindings.len() != self.sources.len()
            || prepared.schema_basis() != &self.basis
            || prepared.root() != &self.root
        {
            return Err(refused(
                None,
                Some(self.selected),
                "Corpus binding requires complete open membership and the captured root and nominal Schema basis",
            ));
        }
        let mut seen = vec![false; prepared.original_sources().len()];
        let mut selected = false;
        for (source, binding) in self.sources.iter().zip(bindings) {
            let Some(used) = seen.get_mut(binding.source) else {
                return Err(refused(
                    None,
                    Some(source.document),
                    "A corpus binding has no original source member",
                ));
            };
            if source.document != binding.document || *used {
                return Err(refused(
                    None,
                    Some(source.document),
                    "Corpus bindings must follow Workspace document-ID order without duplicate source members",
                ));
            }
            *used = true;
            match &source.source {
                WorkspaceSourceInputKind::Scene(input) => {
                    prepared.require_scene_input(binding.source, input)
                }
                WorkspaceSourceInputKind::Prefab(input) => {
                    prepared.require_prefab_input(binding.source, input)
                }
            }
            .map_err(|report| source_report(None, Some(source.document), report))?;
            if source.document == self.selected {
                let bytes = prepared
                    .original_sources()
                    .get(binding.source)
                    .and_then(|source| source.key().checked_retained_bytes())
                    .and_then(|bytes| {
                        bytes.checked_add(prepared.selected_source().checked_retained_bytes()?)
                    });
                prepared
                    .admit_binding_storage(bytes)
                    .map_err(|report| source_report(None, Some(source.document), report))?;
                if !prepared
                    .original_sources()
                    .get(binding.source)
                    .is_some_and(|source| source.key() == prepared.selected_source())
                {
                    return Err(refused(
                        None,
                        Some(source.document),
                        "The selected Prefab must be the corpus extraction source",
                    ));
                }
                selected = true;
            }
        }
        if !selected {
            return Err(refused(
                None,
                Some(self.selected),
                "The selected Prefab has no corpus binding",
            ));
        }
        Ok(())
    }

    fn require_open_set(
        &self,
        workspace: &EditorWorkspace,
        mut admit: impl FnMut(
            Option<usize>,
            Option<EditorDocumentId>,
        ) -> Result<(), EditorWorkspaceCommandReport>,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let active = workspace.active_document;
        admit(Some(size_of::<Self>()), Some(self.selected))?;
        if !Arc::ptr_eq(&self.owner, &workspace.persistence_identity)
            || self.sources.len() != workspace.documents.len()
        {
            return Err(refused(
                active,
                Some(self.selected),
                "Prefab extraction requires its original Workspace and complete open set",
            ));
        }
        admit(
            self.sources
                .len()
                .checked_mul(size_of::<&WorkspaceSourceInput>()),
            Some(self.selected),
        )?;
        for (source, (document, slot)) in self.sources.iter().zip(&workspace.documents) {
            // Admit both entire locators before equality can inspect their contents. Metadata
            // and basis observations are also admitted before copying their Arc handles.
            let bytes = size_of::<WorkspaceSourceInput>()
                .checked_add(source.binding.locator().len())
                .and_then(|bytes| {
                    bytes.checked_add(
                        slot.source_binding()
                            .map_or(0, |binding| binding.locator().len()),
                    )
                });
            admit(bytes, Some(*document))?;
            if source.document != *document
                || slot.kind() != source.source.kind()
                || slot.source_binding() != Some(&source.binding)
                || slot.revision() != source.source.revision()
                || slot.authoring_schema_basis().as_ref() != Some(&self.basis)
                || WorkspaceSavedInput::capture(slot) != Some(source.saved)
            {
                return Err(refused(
                    active,
                    Some(*document),
                    "An extraction source changed its membership, kind, binding, revision, saved state or nominal Schema basis",
                ));
            }
        }
        Ok(())
    }

    fn find(&self, document: EditorDocumentId) -> Option<&WorkspaceSourceInput> {
        let index = self
            .sources
            .binary_search_by_key(&document, |source| source.document)
            .ok()?;
        self.sources.get(index)
    }
}

impl PreparedEditorWorkspacePrefabExtraction {
    #[must_use]
    pub fn input(&self) -> &EditorWorkspacePrefabExtractionInput {
        &self.input
    }

    #[must_use]
    pub fn corpus(&self) -> &PreparedPrefabExtraction {
        &self.corpus
    }

    #[must_use]
    pub fn bindings(&self) -> &[EditorPrefabExtractionCorpusBinding] {
        &self.bindings
    }

    #[must_use]
    pub fn usage(&self) -> PrefabExtractionUsage {
        self.corpus.usage()
    }

    /// Prepares actual changed outputs through their source owners under the original allowance.
    /// Refusal drops newly prepared locals before returning this original corpus; no live Session
    /// is borrowed or changed. Unchanged members receive no new history or replacement token.
    pub fn prepare_sources(
        mut self,
    ) -> Result<
        PreparedEditorWorkspaceExtractionSources,
        EditorWorkspacePrefabExtractionSourcePrepareFailure,
    > {
        match prepare_source_tokens(&mut self) {
            Ok(sources) => Ok(PreparedEditorWorkspaceExtractionSources {
                prepared: self,
                sources,
            }),
            Err(report) => Err(EditorWorkspacePrefabExtractionSourcePrepareFailure {
                prepared: self,
                report,
            }),
        }
    }

    /// Rechecks complete live membership and each actual source owner under the corpus's Work.
    /// Navigation is allowed; source/history/basis/incarnation changes cause refusal without
    /// installing any checkpoint or changing any live source, selection or history branch.
    pub fn validate(
        &mut self,
        workspace: &EditorWorkspace,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let active = workspace.active_document;
        self.corpus
            .admit_binding_storage(Some(size_of::<Self>()))
            .map_err(|report| source_report(active, Some(self.input.selected), report))?;
        self.input.require_open_set(workspace, |bytes, document| {
            self.corpus
                .admit_binding_storage(bytes)
                .map_err(|report| source_report(active, document, report))
        })?;
        // The ordered mapping is immutable after binding. The shared membership scan above
        // checks every slot's locator/kind/revision/basis before its owner checks exact history.
        for ((source, binding), (document, slot)) in self
            .input
            .sources
            .iter()
            .zip(&self.bindings)
            .zip(&workspace.documents)
        {
            let result = match (&source.source, slot) {
                (WorkspaceSourceInputKind::Scene(input), EditorDocumentSlot::Scene(slot)) => self
                    .corpus
                    .validate_scene_input(binding.source, input, &slot.session),
                (WorkspaceSourceInputKind::Prefab(input), EditorDocumentSlot::Prefab(slot)) => self
                    .corpus
                    .validate_prefab_input(binding.source, input, &slot.session),
                _ => {
                    return Err(refused(
                        active,
                        Some(*document),
                        "An extraction source changed kind",
                    ));
                }
            };
            result.map_err(|report| source_report(active, Some(*document), report))?;
        }
        Ok(())
    }
}

fn prepare_source_tokens(
    prepared: &mut PreparedEditorWorkspacePrefabExtraction,
) -> Result<Vec<Option<PreparedSourceKind>>, EditorWorkspaceCommandReport> {
    let storage = prepared
        .input
        .sources
        .len()
        .checked_mul(size_of::<Option<PreparedSourceKind>>())
        .and_then(|bytes| {
            bytes.checked_add(
                size_of::<PreparedEditorWorkspaceExtractionSources>()
                    .max(size_of::<EditorWorkspacePrefabExtractionSourcePrepareFailure>()),
            )
        });
    prepared
        .corpus
        .admit_binding_storage(storage)
        .map_err(|report| source_report(None, Some(prepared.input.selected), report))?;
    if prepared.bindings.len() != prepared.input.sources.len() {
        return Err(refused(
            None,
            None,
            "Source preparation requires the complete binding set",
        ));
    }
    let mut sources = Vec::with_capacity(prepared.input.sources.len());
    for (input, binding) in prepared.input.sources.iter().zip(&prepared.bindings) {
        let source = match prepared.corpus.source_changed(binding.source) {
            Some(true) => Some(
                match &input.source {
                    WorkspaceSourceInputKind::Scene(input) => prepared
                        .corpus
                        .prepare_scene_source(binding.source, input)
                        .map(PreparedSourceKind::Scene),
                    WorkspaceSourceInputKind::Prefab(input) => prepared
                        .corpus
                        .prepare_prefab_source(binding.source, input)
                        .map(PreparedSourceKind::Prefab),
                }
                .map_err(|report| source_report(None, Some(input.document), report))?,
            ),
            Some(false) => None,
            None => {
                return Err(refused(
                    None,
                    Some(input.document),
                    "An extraction source has no verified change classification",
                ));
            }
        };
        sources.push(source);
    }
    Ok(sources)
}

impl PreparedEditorWorkspaceExtractionSources {
    #[must_use]
    pub fn prepared(&self) -> &PreparedEditorWorkspacePrefabExtraction {
        &self.prepared
    }

    #[must_use]
    pub fn usage(&self) -> PrefabExtractionUsage {
        self.prepared.usage()
    }

    /// Borrows an actual changed Scene successor; unchanged and non-Scene members return `None`.
    #[must_use]
    pub fn scene_source(
        &self,
        document: EditorDocumentId,
    ) -> Option<&PreparedSceneExtractionSource> {
        match self.source(document)? {
            PreparedSourceKind::Scene(source) => Some(source),
            PreparedSourceKind::Prefab(_) => None,
        }
    }

    /// Borrows an actual changed Prefab successor without granting construction authority.
    #[must_use]
    pub fn prefab_source(
        &self,
        document: EditorDocumentId,
    ) -> Option<&PreparedPrefabExtractionSource> {
        match self.source(document)? {
            PreparedSourceKind::Scene(_) => None,
            PreparedSourceKind::Prefab(source) => Some(source),
        }
    }

    fn source(&self, document: EditorDocumentId) -> Option<&PreparedSourceKind> {
        let index = self
            .prepared
            .bindings
            .binary_search_by_key(&document, |binding| binding.document)
            .ok()?;
        self.sources.get(index)?.as_ref()
    }

    fn require_complete_sources(
        &mut self,
        active: Option<EditorDocumentId>,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        self.prepared
            .corpus
            .admit_binding_storage(
                self.sources
                    .len()
                    .checked_mul(size_of::<&Option<PreparedSourceKind>>()),
            )
            .map_err(|report| source_report(active, None, report))?;
        if self.sources.len() != self.prepared.input.sources.len()
            || self.sources.len() != self.prepared.bindings.len()
        {
            return Err(refused(
                active,
                None,
                "Source preparations require the complete open set",
            ));
        }
        for ((input, binding), source) in self
            .prepared
            .input
            .sources
            .iter()
            .zip(&self.prepared.bindings)
            .zip(&self.sources)
        {
            if binding.document != input.document {
                return Err(refused(
                    active,
                    Some(input.document),
                    "The source binding order changed",
                ));
            }
            let result = match (
                self.prepared.corpus.source_changed(binding.source),
                source,
                &input.source,
            ) {
                (Some(false), None, _) => Ok(()),
                (
                    Some(true),
                    Some(PreparedSourceKind::Scene(source)),
                    WorkspaceSourceInputKind::Scene(_),
                ) => self
                    .prepared
                    .corpus
                    .require_scene_checkpoint(binding.source, source),
                (
                    Some(true),
                    Some(PreparedSourceKind::Prefab(source)),
                    WorkspaceSourceInputKind::Prefab(_),
                ) => self
                    .prepared
                    .corpus
                    .require_prefab_checkpoint(binding.source, source),
                _ => {
                    return Err(refused(
                        active,
                        Some(input.document),
                        "Only actually changed sources may have replacement preparations",
                    ));
                }
            };
            result.map_err(|report| source_report(active, Some(input.document), report))?;
        }
        Ok(())
    }
}

/// All open Sessions are bound before source installation; saved state is not adopted here.
///
/// Changed sources hold their owner's exclusive guard. Unchanged sources hold their original
/// Sessions; disjoint saved fields are bound too, for reuse by the selective persistence sidecar.
/// Dropping this guard installs nothing. Q remains a closed corpus member.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspacePrefabExtractionSourceCommit<'w> {
    prepared: PreparedEditorWorkspacePrefabExtraction,
    unbound_sources: Vec<Option<PreparedSourceKind>>,
    sources: Vec<BoundSource<'w>>,
}

#[derive(Debug)]
struct BoundSource<'w> {
    index: usize,
    kind: BoundSourceKind<'w>,
    saved: BoundSavedFields<'w>,
}

#[derive(Debug)]
enum BoundSourceKind<'w> {
    Scene(SceneExtractionSourceCommit<'w>),
    Prefab(PrefabExtractionSourceCommit<'w>),
    SceneGuard(&'w mut SceneAuthoringSession),
    PrefabGuard(&'w mut PrefabAuthoringSession),
}

impl EditorWorkspacePrefabExtractionSourceCommit<'_> {
    #[must_use]
    pub fn prepared(&self) -> &PreparedEditorWorkspacePrefabExtraction {
        &self.prepared
    }

    #[must_use]
    pub fn scene_source(
        &self,
        document: EditorDocumentId,
    ) -> Option<&PreparedSceneExtractionSource> {
        match self.source(document)? {
            BoundSourceKind::Scene(source) => Some(source.prepared()),
            _ => None,
        }
    }

    #[must_use]
    pub fn prefab_source(
        &self,
        document: EditorDocumentId,
    ) -> Option<&PreparedPrefabExtractionSource> {
        match self.source(document)? {
            BoundSourceKind::Prefab(source) => Some(source.prepared()),
            _ => None,
        }
    }

    fn source(&self, document: EditorDocumentId) -> Option<&BoundSourceKind<'_>> {
        let index = self
            .prepared
            .bindings
            .binary_search_by_key(&document, |binding| binding.document)
            .ok()?;
        Some(&self.sources.get(index)?.kind)
    }

    /// Releases every borrow and restores exact tokens into their existing indexed storage.
    /// Future incarnations, output allocations and cumulative usage are unchanged.
    pub fn into_prepared(self) -> PreparedEditorWorkspaceExtractionSources {
        let Self {
            prepared,
            mut unbound_sources,
            mut sources,
        } = self;
        restore_bound_sources(&mut unbound_sources, &mut sources);
        PreparedEditorWorkspaceExtractionSources {
            prepared,
            sources: unbound_sources,
        }
    }

    /// Swaps only prebound changed source/classification/history, without allocation or admission.
    /// Saved checkpoints, dirty acknowledgement, external reload and navigation await their owners.
    /// This source-only operation supplies neither file receipts nor complete-project Ready proof.
    pub fn commit(self) {
        for source in self.sources {
            source.commit(None);
        }
    }
}

impl EditorPersistenceAuthority {
    /// Binds complete Workspace membership and exact source tokens before any live mutation.
    /// Refusal preserves all incumbents and returns the same tokens with validation work spent.
    #[doc(hidden)]
    pub fn bind_prefab_extraction_sources<'w>(
        &self,
        workspace: &'w mut EditorWorkspace,
        mut prepared: PreparedEditorWorkspaceExtractionSources,
    ) -> Result<
        EditorWorkspacePrefabExtractionSourceCommit<'w>,
        EditorWorkspacePrefabExtractionSourceBindFailure,
    > {
        let result = if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            Err(refused(
                workspace.active_document,
                None,
                "Source installation requires the original hosted Workspace authority",
            ))
        } else {
            bind_sources(workspace, &mut prepared)
        };
        match result {
            Ok(sources) => Ok(EditorWorkspacePrefabExtractionSourceCommit {
                prepared: prepared.prepared,
                unbound_sources: prepared.sources,
                sources,
            }),
            Err(report) => {
                Err(EditorWorkspacePrefabExtractionSourceBindFailure { prepared, report })
            }
        }
    }
}

fn bind_sources<'w>(
    workspace: &'w mut EditorWorkspace,
    prepared: &mut PreparedEditorWorkspaceExtractionSources,
) -> Result<Vec<BoundSource<'w>>, EditorWorkspaceCommandReport> {
    let active = workspace.active_document;
    prepared.prepared.validate(workspace)?;
    prepared.require_complete_sources(active)?;
    // Admit the complete guard container and either result wrapper before allocating or taking
    // any mutable Session borrow. All owner and paired-corpus validation is already complete.
    let storage = prepared
        .prepared
        .input
        .sources
        .len()
        .checked_mul(size_of::<BoundSource<'w>>())
        .and_then(|bytes| {
            bytes.checked_add(
                size_of::<EditorWorkspacePrefabExtractionSourceCommit<'w>>()
                    .max(size_of::<EditorWorkspacePrefabExtractionSourceBindFailure>()),
            )
        });
    prepared
        .prepared
        .corpus
        .admit_binding_storage(storage)
        .map_err(|report| source_report(active, None, report))?;
    let mut sources = Vec::with_capacity(prepared.sources.len());
    for (index, ((_, slot), input)) in workspace
        .documents
        .iter_mut()
        .zip(&prepared.prepared.input.sources)
        .enumerate()
    {
        let token = prepared.sources[index].take();
        match bind_source(slot, token, active, input.document) {
            Ok((kind, saved)) => sources.push(BoundSource { index, kind, saved }),
            Err((token, report)) => {
                prepared.sources[index] = token;
                restore_bound_sources(&mut prepared.sources, &mut sources);
                return Err(report);
            }
        }
    }
    Ok(sources)
}

fn bind_source<'w>(
    slot: &'w mut EditorDocumentSlot,
    token: Option<PreparedSourceKind>,
    active: Option<EditorDocumentId>,
    document: EditorDocumentId,
) -> Result<
    (BoundSourceKind<'w>, BoundSavedFields<'w>),
    (Option<PreparedSourceKind>, EditorWorkspaceCommandReport),
> {
    match (slot, token) {
        (EditorDocumentSlot::Scene(slot), Some(PreparedSourceKind::Scene(token))) => {
            let saved = BoundSavedFields::new(
                &mut slot.saved_revision,
                &mut slot.saved_content_state,
                &mut slot.saved_digest,
                &mut slot.external_reload,
            );
            slot.session
                .bind_extracted_source(token)
                .map(|source| (BoundSourceKind::Scene(source), saved))
                .map_err(|failure| {
                    let (token, report) = failure.into_parts();
                    (
                        Some(PreparedSourceKind::Scene(token)),
                        source_report(active, Some(document), report),
                    )
                })
        }
        (EditorDocumentSlot::Prefab(slot), Some(PreparedSourceKind::Prefab(token))) => {
            let saved = BoundSavedFields::new(
                &mut slot.saved_revision,
                &mut slot.saved_content_state,
                &mut slot.saved_digest,
                &mut slot.external_reload,
            );
            slot.session
                .bind_extracted_source(token)
                .map(|source| (BoundSourceKind::Prefab(source), saved))
                .map_err(|failure| {
                    let (token, report) = failure.into_parts();
                    (
                        Some(PreparedSourceKind::Prefab(token)),
                        source_report(active, Some(document), report),
                    )
                })
        }
        (EditorDocumentSlot::Scene(slot), None) => Ok((
            BoundSourceKind::SceneGuard(&mut slot.session),
            BoundSavedFields::new(
                &mut slot.saved_revision,
                &mut slot.saved_content_state,
                &mut slot.saved_digest,
                &mut slot.external_reload,
            ),
        )),
        (EditorDocumentSlot::Prefab(slot), None) => Ok((
            BoundSourceKind::PrefabGuard(&mut slot.session),
            BoundSavedFields::new(
                &mut slot.saved_revision,
                &mut slot.saved_content_state,
                &mut slot.saved_digest,
                &mut slot.external_reload,
            ),
        )),
        (_, token) => Err((
            token,
            refused(active, Some(document), "The original source kind changed"),
        )),
    }
}

fn restore_bound_sources(
    prepared: &mut [Option<PreparedSourceKind>],
    sources: &mut Vec<BoundSource<'_>>,
) {
    while let Some(source) = sources.pop() {
        prepared[source.index] = match source.kind {
            BoundSourceKind::Scene(source) => {
                Some(PreparedSourceKind::Scene(source.into_prepared()))
            }
            BoundSourceKind::Prefab(source) => {
                Some(PreparedSourceKind::Prefab(source.into_prepared()))
            }
            BoundSourceKind::SceneGuard(session) => {
                let _ = session;
                None
            }
            BoundSourceKind::PrefabGuard(session) => {
                let _ = session;
                None
            }
        };
    }
}

impl EditorWorkspace {
    /// Captures all open published sources under the exact unchanged authoring Schema basis.
    ///
    /// The cut must name a directly owned record of the selected Prefab. Panel selection and
    /// expanded projection membership cannot confer declaration ownership. Full validation and
    /// storage requests precede copies/allocations; refused captures never refund performed work.
    #[doc(hidden)]
    pub fn capture_prefab_extraction_input(
        &self,
        selected: EditorDocumentId,
        root: &SceneEntityId,
        basis: &AuthoringSchemaBasis,
        work: &mut PrefabExtractionWork,
    ) -> Result<EditorWorkspacePrefabExtractionInput, EditorWorkspaceCommandReport> {
        let active = self.active_document;
        charge(
            work,
            self.documents
                .len()
                .checked_mul(size_of::<(&EditorDocumentId, &EditorDocumentSlot)>())
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<EditorWorkspacePrefabExtractionInput>())
                }),
            active,
            Some(selected),
        )?;
        let Some(EditorDocumentSlot::Prefab(prefab)) = self.documents.get(&selected) else {
            return Err(refused(
                active,
                Some(selected),
                "Prefab extraction requires an open selected Prefab document",
            ));
        };

        for (document, slot) in &self.documents {
            charge(
                work,
                Some(size_of::<WorkspaceSourceInput>()),
                active,
                Some(*document),
            )?;
            if matches!(slot, EditorDocumentSlot::Extension(_))
                || slot.source_binding().is_none()
                || matches!(slot, EditorDocumentSlot::Scene(slot) if slot.session.live_instance().is_some())
                || slot.authoring_schema_basis().as_ref() != Some(basis)
            {
                return Err(refused(
                    active,
                    Some(*document),
                    "Prefab extraction requires bound detached published sources under the exact nominal Schema basis",
                ));
            }
        }

        let mut found = false;
        for entity in &prefab.session.document().entities {
            charge(
                work,
                size_of::<SceneEntityId>()
                    .checked_add(root.as_str().len())
                    .and_then(|bytes| bytes.checked_add(entity.id.as_str().len())),
                active,
                Some(selected),
            )?;
            if &entity.id == root {
                found = true;
                break;
            }
        }
        if !found {
            return Err(refused(
                active,
                Some(selected),
                "The extraction root must be a directly owned record of the selected Prefab",
            ));
        }

        // Reserve the entire requested Vec, root ID and retained binding metadata before the
        // first allocation/clone. Each source owner separately charges its actual incumbent and
        // history to this same Work before sharing its document and classification.
        let storage = self
            .documents
            .len()
            .checked_mul(size_of::<WorkspaceSourceInput>())
            .and_then(|bytes| bytes.checked_add(size_of::<EditorWorkspacePrefabExtractionInput>()))
            .and_then(|bytes| bytes.checked_add(root.as_str().len()));
        charge(work, storage, active, Some(selected))?;
        for (document, slot) in &self.documents {
            charge(
                work,
                slot.source_binding().and_then(|binding| {
                    binding.locator().len().checked_add(2 * size_of::<usize>())
                }),
                active,
                Some(*document),
            )?;
        }
        let mut sources = Vec::with_capacity(self.documents.len());
        for (document, slot) in &self.documents {
            let Some(binding) = slot.source_binding() else {
                return Err(refused(
                    active,
                    Some(*document),
                    "An extraction source has no binding",
                ));
            };
            let source = match slot {
                EditorDocumentSlot::Scene(slot) => slot
                    .session
                    .capture_prefab_extraction_input(work)
                    .map(WorkspaceSourceInputKind::Scene),
                EditorDocumentSlot::Prefab(slot) => slot
                    .session
                    .capture_prefab_extraction_input(work)
                    .map(WorkspaceSourceInputKind::Prefab),
                EditorDocumentSlot::Extension(_) => {
                    return Err(refused(
                        active,
                        Some(*document),
                        "Extension sources cannot join Prefab extraction",
                    ));
                }
            }
            .map_err(|report| source_report(active, Some(*document), report))?;
            let saved = WorkspaceSavedInput::capture(slot).ok_or_else(|| {
                refused(active, Some(*document), "An extraction source changed kind")
            })?;
            sources.push(WorkspaceSourceInput {
                document: *document,
                binding: binding.clone(),
                saved,
                source,
            });
        }
        Ok(EditorWorkspacePrefabExtractionInput {
            owner: Arc::clone(&self.persistence_identity),
            basis: basis.clone(),
            selected,
            root: root.clone(),
            sources,
        })
    }
}

fn charge(
    work: &mut PrefabExtractionWork,
    bytes: Option<usize>,
    active: Option<EditorDocumentId>,
    document: Option<EditorDocumentId>,
) -> Result<(), EditorWorkspaceCommandReport> {
    work.admit_capture_storage(bytes)
        .map_err(|report| source_report(active, document, report))
}

fn refused(
    active: Option<EditorDocumentId>,
    document: Option<EditorDocumentId>,
    message: &'static str,
) -> EditorWorkspaceCommandReport {
    workspace_error_report(
        WorkspaceDocumentContext::new(active, document),
        "tooling.workspace-prefab-extraction-rejected",
        message,
    )
}

fn source_report(
    active: Option<EditorDocumentId>,
    document: Option<EditorDocumentId>,
    report: ScenePatchReport,
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
