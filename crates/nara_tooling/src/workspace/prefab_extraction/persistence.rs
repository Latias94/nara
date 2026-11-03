//! Selective saved checkpoints over the existing complete-set source guard.
//!
//! This sidecar owns no compiler, transaction, history or Project publication protocol. A concrete
//! Host publishes the fixed owner-encoded checkpoints and binds its other destinations separately.

use nara_scene::__private::{
    CapturedExtractionSource, EncodedPrefabExtractionSource, PrefabExtractionSavedGeneration,
    PrefabExtractionSavedGenerationError, PrefabExtractionSourceOutput,
};
use nara_scene::{
    AuthoringDocumentReadmission, SceneAuthoringContentState, SceneFileEncoding,
    SceneFileEncodingAdmissionError, SceneFileLimits,
};

use super::*;
use crate::workspace::{
    EditorDocumentDigest, EditorExternalReloadState, EditorPersistenceCaptureIdentity,
    EditorPersistenceCheckpoint, EditorPersistenceCommit, EditorPersistenceContent,
    EditorPersistenceEncodingError, EncodedEditorPersistenceCheckpoint,
};

/// Review's explicit authority for one captured open document.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrefabExtractionDisposition {
    /// Save and install the actual prepared successor and its new history checkpoint.
    ReplaceSource,
    /// Save the exact captured working input, retaining its source, classification and history.
    SaveCurrent,
    /// Preserve source, history, saved fields and external-reload state without a save.
    Guard,
}

/// Fixed linear saves issued only from the original inputs or actual prepared successors.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct PreparedEditorWorkspaceExtractionPersistence {
    sources: PreparedEditorWorkspaceExtractionSources,
    entries: Vec<SavedEntry>,
}

#[doc(hidden)]
#[derive(Debug)]
pub enum EditorWorkspaceExtractionSavedGenerationFailure {
    Workspace(EditorWorkspaceCommandReport),
    Scene(PrefabExtractionSavedGenerationError),
}

impl std::fmt::Display for EditorWorkspaceExtractionSavedGenerationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("The reviewed extraction saved generation could not be established")
    }
}

impl std::error::Error for EditorWorkspaceExtractionSavedGenerationFailure {}

#[derive(Debug)]
struct SavedEntry {
    document: EditorDocumentId,
    disposition: EditorPrefabExtractionDisposition,
    save: Option<SavedCheckpoint>,
}

#[derive(Debug)]
struct SavedCheckpoint {
    capture: Arc<EditorPersistenceCaptureIdentity>,
    kind: EditorDocumentKind,
    binding: EditorDocumentSourceBinding,
    revision: SceneAuthoringRevision,
    content_state: SceneAuthoringContentState,
    content: SavedContent,
    basis: AuthoringSchemaBasis,
    readmission: AuthoringDocumentReadmission,
    evidence: SavedEvidence,
}

#[derive(Debug)]
enum SavedContent {
    Scene(Arc<SceneDocument>),
    Prefab(Arc<PrefabDocument>),
}

#[derive(Debug)]
enum SavedEvidence {
    Available(EditorPersistenceCheckpoint),
    Taken,
    Published(EditorPersistenceCommit),
}

/// Preparation refusal returns the exact source owner and unmodified review selection.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceExtractionPersistencePrepareFailure {
    sources: PreparedEditorWorkspaceExtractionSources,
    save_current: Vec<EditorDocumentId>,
    report: EditorWorkspaceCommandReport,
}

impl EditorWorkspaceExtractionPersistencePrepareFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(
        self,
    ) -> (
        PreparedEditorWorkspaceExtractionSources,
        Vec<EditorDocumentId>,
        EditorWorkspaceCommandReport,
    ) {
        (self.sources, self.save_current, self.report)
    }
}

/// A rejected checkpoint is returned intact, including its nominal capture identity.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceExtractionCheckpointFailure {
    checkpoint: EditorPersistenceCheckpoint,
    report: EditorWorkspaceCommandReport,
}

/// Encoding refusal leaves the identical checkpoint available in its original stage.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceExtractionEncodingFailure {
    report: EditorWorkspaceCommandReport,
    error: Option<EditorPersistenceEncodingError<EditorWorkspaceCommandReport>>,
}

impl EditorWorkspaceExtractionEncodingFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn error(&self) -> Option<&EditorPersistenceEncodingError<EditorWorkspaceCommandReport>> {
        self.error.as_ref()
    }

    pub fn into_report(self) -> EditorWorkspaceCommandReport {
        self.report
    }
}

impl EditorWorkspaceExtractionCheckpointFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(self) -> (EditorPersistenceCheckpoint, EditorWorkspaceCommandReport) {
        (self.checkpoint, self.report)
    }
}

/// Rejected publication retains its actual checkpoint and accepted filesystem evidence.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceExtractionSaveFailure {
    commit: EditorPersistenceCommit,
    report: EditorWorkspaceCommandReport,
}

impl EditorWorkspaceExtractionSaveFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(self) -> (EditorPersistenceCommit, EditorWorkspaceCommandReport) {
        (self.commit, self.report)
    }
}

/// Late refusal retains the complete stage, fixed captures, commits and cumulative spent Work.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceExtractionPersistenceBindFailure {
    prepared: PreparedEditorWorkspaceExtractionPersistence,
    report: EditorWorkspaceCommandReport,
}

impl EditorWorkspaceExtractionPersistenceBindFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(
        self,
    ) -> (
        PreparedEditorWorkspaceExtractionPersistence,
        EditorWorkspaceCommandReport,
    ) {
        (self.prepared, self.report)
    }
}

impl PreparedEditorWorkspaceExtractionSources {
    /// Every actual replacement must be saved. Only explicitly named unchanged inputs may be
    /// saved as current; all other unchanged members remain guards. No live source is recaptured.
    pub fn prepare_persistence(
        mut self,
        save_current: Vec<EditorDocumentId>,
    ) -> Result<
        PreparedEditorWorkspaceExtractionPersistence,
        EditorWorkspaceExtractionPersistencePrepareFailure,
    > {
        match prepare_saved_entries(&mut self, &save_current, save_current.capacity()) {
            Ok(entries) => Ok(PreparedEditorWorkspaceExtractionPersistence {
                sources: self,
                entries,
            }),
            Err(report) => Err(EditorWorkspaceExtractionPersistencePrepareFailure {
                sources: self,
                save_current,
                report,
            }),
        }
    }
}

fn prepare_saved_entries(
    sources: &mut PreparedEditorWorkspaceExtractionSources,
    save_current: &[EditorDocumentId],
    selection_capacity: usize,
) -> Result<Vec<SavedEntry>, EditorWorkspaceCommandReport> {
    let count = sources.sources.len();
    let storage = count
        .checked_mul(size_of::<SavedEntry>() + size_of::<bool>())
        .and_then(|bytes| bytes.checked_add(size_of::<Vec<bool>>()))
        .and_then(|bytes| {
            bytes.checked_add(selection_capacity.checked_mul(size_of::<EditorDocumentId>())?)
        })
        .and_then(|bytes| {
            bytes.checked_add(
                size_of::<PreparedEditorWorkspaceExtractionPersistence>()
                    .max(size_of::<EditorWorkspaceExtractionPersistencePrepareFailure>()),
            )
        });
    admit(sources, storage, None)?;
    sources.require_complete_sources(None)?;
    let mut selected = vec![false; count];
    for document in save_current {
        // Admit bounded membership work before inspecting the supplied review selection.
        admit(
            sources,
            count.checked_mul(size_of::<EditorDocumentId>()),
            Some(*document),
        )?;
        let Ok(index) = sources
            .prepared
            .bindings
            .binary_search_by_key(document, |binding| binding.document)
        else {
            return Err(refused(
                None,
                Some(*document),
                "SaveCurrent requires an original open document",
            ));
        };
        if selected[index] || sources.sources[index].is_some() {
            return Err(refused(
                None,
                Some(*document),
                "SaveCurrent must name each unchanged source at most once",
            ));
        }
        selected[index] = true;
    }
    let mut entries = Vec::with_capacity(count);
    for (index, save_current) in selected.into_iter().enumerate() {
        let document = sources.prepared.input.sources[index].document;
        let disposition = if sources.sources[index].is_some() {
            EditorPrefabExtractionDisposition::ReplaceSource
        } else if save_current {
            EditorPrefabExtractionDisposition::SaveCurrent
        } else {
            EditorPrefabExtractionDisposition::Guard
        };
        let save = if disposition == EditorPrefabExtractionDisposition::Guard {
            None
        } else {
            Some(prepare_saved_checkpoint(sources, index)?)
        };
        entries.push(SavedEntry {
            document,
            disposition,
            save,
        });
    }
    Ok(entries)
}

fn prepare_saved_checkpoint(
    sources: &mut PreparedEditorWorkspaceExtractionSources,
    index: usize,
) -> Result<SavedCheckpoint, EditorWorkspaceCommandReport> {
    let input = &sources.prepared.input.sources[index];
    let document = input.document;
    // Both binding handles, expected output handles, the fresh nominal capture/control block,
    // linear checkpoint and either returned failure coexist with the original retained corpus.
    let storage = input
        .binding
        .locator()
        .len()
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(size_of::<SavedCheckpoint>()))
        .and_then(|bytes| bytes.checked_add(size_of::<EditorPersistenceCheckpoint>()))
        .and_then(|bytes| {
            bytes
                .checked_add(size_of::<EditorPersistenceCaptureIdentity>() + 2 * size_of::<usize>())
        })
        .and_then(|bytes| {
            bytes.checked_add(
                size_of::<EditorWorkspaceExtractionCheckpointFailure>()
                    .max(size_of::<EditorWorkspaceExtractionSaveFailure>()),
            )
        });
    admit(sources, storage, Some(document))?;
    let input = &sources.prepared.input.sources[index];
    let corpus = &mut sources.prepared.corpus;
    let mut readmit = |bytes| corpus.admit_binding_storage(bytes);
    let (revision, content_state, basis, content, readmission) =
        match (&sources.sources[index], &input.source) {
            (Some(PreparedSourceKind::Scene(source)), WorkspaceSourceInputKind::Scene(_)) => (
                source.revision(),
                source.content_state(),
                source.schema_basis(),
                EditorPersistenceContent::Scene(source.shared_document()),
                source.authoring_readmission(&mut readmit),
            ),
            (Some(PreparedSourceKind::Prefab(source)), WorkspaceSourceInputKind::Prefab(_)) => (
                source.revision(),
                source.content_state(),
                source.schema_basis(),
                EditorPersistenceContent::Prefab(source.shared_document()),
                source.authoring_readmission(&mut readmit),
            ),
            (None, WorkspaceSourceInputKind::Scene(source)) => (
                source.revision(),
                source.content_state(),
                source.schema_basis(),
                EditorPersistenceContent::Scene(source.shared_document()),
                source.authoring_readmission(&mut readmit),
            ),
            (None, WorkspaceSourceInputKind::Prefab(source)) => (
                source.revision(),
                source.content_state(),
                source.schema_basis(),
                EditorPersistenceContent::Prefab(source.shared_document()),
                source.authoring_readmission(&mut readmit),
            ),
            _ => {
                return Err(refused(
                    None,
                    Some(document),
                    "A saved checkpoint requires its exact source kind",
                ));
            }
        };
    let readmission = readmission.map_err(|report| source_report(None, Some(document), report))?;
    let expected_content = match &content {
        EditorPersistenceContent::Scene(source) => SavedContent::Scene(Arc::clone(source)),
        EditorPersistenceContent::Prefab(source) => SavedContent::Prefab(Arc::clone(source)),
        EditorPersistenceContent::Extension(_) => {
            unreachable!("only source-owned Scene/Prefab inputs were admitted")
        }
    };
    let checkpoint = EditorPersistenceAuthority::capture_shared(
        &sources.prepared.input.owner,
        document,
        input.source.kind(),
        Some(input.binding.clone()),
        revision,
        content_state,
        content,
    );
    Ok(SavedCheckpoint {
        capture: Arc::clone(&checkpoint.capture),
        kind: checkpoint.kind,
        binding: input.binding.clone(),
        revision,
        content_state,
        content: expected_content,
        basis,
        readmission,
        evidence: SavedEvidence::Available(checkpoint),
    })
}

impl PreparedEditorWorkspaceExtractionPersistence {
    /// Canonical recovery data from the actual reviewed working input, including dirty guards.
    /// This neither takes a saved checkpoint nor grants publication authority to the bytes.
    pub fn encode_reviewed_working_source(
        &mut self,
        source: usize,
        encoding: SceneFileEncoding,
        limits: SceneFileLimits,
    ) -> Result<Vec<u8>, SceneFileEncodingAdmissionError<ScenePatchReport>> {
        self.sources
            .prepared
            .corpus
            .encode_original_source(source, encoding, limits)
    }

    /// Encodes a changed closed source or the new Prefab directly from the verified corpus.
    /// Open documents keep their actual move-only saved checkpoints through `encode_checkpoint`.
    pub fn encode_closed_source_output(
        &mut self,
        source: PrefabExtractionSourceOutput,
        encoding: SceneFileEncoding,
        limits: SceneFileLimits,
    ) -> Result<EncodedPrefabExtractionSource, SceneFileEncodingAdmissionError<ScenePatchReport>>
    {
        use SceneFileEncodingAdmissionError::Admission;
        let admission = |report: EditorWorkspaceCommandReport| {
            Admission(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics: report.diagnostics,
            })
        };
        admit(
            &mut self.sources,
            Some(size_of::<EncodedPrefabExtractionSource>()),
            None,
        )
        .map_err(admission)?;
        if let PrefabExtractionSourceOutput::Existing(index) = source {
            let storage = self
                .sources
                .prepared
                .bindings
                .len()
                .checked_mul(size_of::<EditorPrefabExtractionCorpusBinding>());
            admit(&mut self.sources, storage, None).map_err(admission)?;
            if self.sources.prepared.corpus.source_changed(index) != Some(true)
                || self
                    .sources
                    .prepared
                    .bindings
                    .iter()
                    .any(|binding| binding.source == index)
            {
                let report = refused(
                    None,
                    None,
                    "Closed output encoding requires an actual changed source outside the captured open set",
                );
                return Err(admission(report));
            }
        }
        self.sources
            .prepared
            .corpus
            .encode_source_output(source, encoding, limits)
    }

    pub fn require_encoded_source_output(
        &mut self,
        output: &EncodedPrefabExtractionSource,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        self.sources
            .prepared
            .corpus
            .require_encoded_source_output(output)
            .map_err(|report| source_report(None, None, report))
    }

    /// A pre-publication Review can choose a different explicit SaveCurrent set while keeping
    /// the exact source preparations and cumulative Work. Outstanding or accepted file evidence
    /// must be reconciled through its existing owner before changing that selection.
    pub fn into_sources_before_publication(
        self,
    ) -> Result<PreparedEditorWorkspaceExtractionSources, Self> {
        if self.entries.iter().any(|entry| {
            entry
                .save
                .as_ref()
                .is_some_and(|save| !matches!(save.evidence, SavedEvidence::Available(_)))
        }) {
            return Err(self);
        }
        let Self { sources, entries } = self;
        drop(entries);
        Ok(sources)
    }

    /// Combines the exact immutable saved checkpoints with original disk guards. No caller
    /// supplies an alternative output or publication mask. Every source change must be saved.
    pub fn prepare_saved_generation(
        &mut self,
        disk: &[CapturedExtractionSource],
    ) -> Result<PrefabExtractionSavedGeneration, EditorWorkspaceExtractionSavedGenerationFailure>
    {
        let published = saved_publication_selection(self)
            .map_err(EditorWorkspaceExtractionSavedGenerationFailure::Workspace)?;
        self.sources
            .prepared
            .corpus
            .prepare_saved_generation(disk, &published)
            .map_err(EditorWorkspaceExtractionSavedGenerationFailure::Scene)
    }

    /// Retains the original nominal extraction proof and this stage's fixed Review selection.
    pub fn require_saved_generation(
        &mut self,
        generation: &PrefabExtractionSavedGeneration,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        self.sources
            .prepared
            .corpus
            .require_saved_generation(generation)
            .map_err(|report| source_report(None, None, report))?;
        let expected = saved_publication_selection(self)?;
        if expected != generation.published() {
            return Err(refused(
                None,
                None,
                "Saved-generation proof belongs to a different Review selection",
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn sources(&self) -> &PreparedEditorWorkspaceExtractionSources {
        &self.sources
    }

    /// Reuses complete source, history and saved-state validation under the original corpus's
    /// cumulative Work. This neither recaptures inputs nor binds or installs any checkpoint.
    pub fn validate(
        &mut self,
        workspace: &EditorWorkspace,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        self.sources.prepared.validate(workspace)
    }

    #[must_use]
    pub fn usage(&self) -> PrefabExtractionUsage {
        self.sources.usage()
    }

    /// Charges Host file bindings, leaf validation, additional readmission and other actual work
    /// to this same original corpus. Refusal never resets or replaces the spent allowance.
    pub fn admit_persistence_storage(
        &mut self,
        bytes: Option<usize>,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        admit(&mut self.sources, bytes, None)
    }

    /// Borrows the already admitted classification corresponding to the fixed saved output.
    /// Additional Host decode/readmission or filesystem attempts require their own admission.
    #[must_use]
    pub fn authoring_readmission(
        &self,
        document: EditorDocumentId,
    ) -> Option<&AuthoringDocumentReadmission> {
        Some(
            &self
                .entries
                .get(self.index(document)?)?
                .save
                .as_ref()?
                .readmission,
        )
    }

    /// Admits actual bounded input/canonical/decode/classification work before encoding, then
    /// charges every output buffer growth and digest scan through the same corpus. Failure restores the exact
    /// checkpoint in place; neither a live capture nor a new allowance is used on retry.
    pub fn encode_checkpoint(
        &mut self,
        document: EditorDocumentId,
        encoding: SceneFileEncoding,
    ) -> Result<EncodedEditorPersistenceCheckpoint, EditorWorkspaceExtractionEncodingFailure> {
        let preparation = self.admit_encoding(document);
        let (index, limits) =
            preparation.map_err(|report| EditorWorkspaceExtractionEncodingFailure {
                report,
                error: None,
            })?;
        let checkpoint = self
            .take_checkpoint(document)
            .map(|(checkpoint, _)| checkpoint)
            .map_err(|report| EditorWorkspaceExtractionEncodingFailure {
                report,
                error: None,
            })?;
        let corpus = &mut self.sources.prepared.corpus;
        match checkpoint.encode_with_admission(encoding, limits, |capacity| {
            corpus
                .admit_binding_storage(Some(capacity))
                .map_err(|report| source_report(None, Some(document), report))
        }) {
            Ok(encoded) => Ok(encoded),
            Err(failure) => {
                let (checkpoint, error) = failure.into_parts();
                self.entries[index]
                    .save
                    .as_mut()
                    .expect("encoding requires a fixed save")
                    .evidence = SavedEvidence::Available(checkpoint);
                let (report, error) = match error {
                    EditorPersistenceEncodingError::Admission(report) => (report, None),
                    error => (
                        refused(
                            None,
                            Some(document),
                            "The fixed extraction persistence checkpoint could not be encoded",
                        ),
                        Some(error),
                    ),
                };
                Err(EditorWorkspaceExtractionEncodingFailure { report, error })
            }
        }
    }

    fn admit_encoding(
        &mut self,
        document: EditorDocumentId,
    ) -> Result<(usize, SceneFileLimits), EditorWorkspaceCommandReport> {
        self.admit_lookup(document, None)?;
        let Some(index) = self.index(document) else {
            return Err(refused(
                None,
                Some(document),
                "No captured document has this ID",
            ));
        };
        if !self.entries[index]
            .save
            .as_ref()
            .is_some_and(|save| matches!(&save.evidence, SavedEvidence::Available(_)))
        {
            return Err(refused(
                None,
                Some(document),
                "Encoding requires an available fixed save checkpoint",
            ));
        }
        admit(
            &mut self.sources,
            Some(
                size_of::<EncodedEditorPersistenceCheckpoint>()
                    + size_of::<EditorWorkspaceExtractionEncodingFailure>(),
            ),
            Some(document),
        )?;
        self.admit_source_persistence_work(index, document)?;
        Ok((
            index,
            self.file_limits(document)
                .expect("fixed source was found above"),
        ))
    }

    fn admit_source_persistence_work(
        &mut self,
        index: usize,
        document: EditorDocumentId,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let input = &self.sources.prepared.input.sources[index];
        let corpus = &mut self.sources.prepared.corpus;
        let mut admission = |bytes| corpus.admit_binding_storage(bytes);
        match (&self.sources.sources[index], &input.source) {
            (Some(PreparedSourceKind::Scene(source)), _) => {
                source.admit_persistence_work(&mut admission)
            }
            (Some(PreparedSourceKind::Prefab(source)), _) => {
                source.admit_persistence_work(&mut admission)
            }
            (None, WorkspaceSourceInputKind::Scene(source)) => {
                source.admit_persistence_work(&mut admission)
            }
            (None, WorkspaceSourceInputKind::Prefab(source)) => {
                source.admit_persistence_work(&mut admission)
            }
        }
        .map_err(|report| source_report(None, Some(document), report))
    }

    pub fn documents(
        &self,
    ) -> impl ExactSizeIterator<Item = (EditorDocumentId, EditorPrefabExtractionDisposition)> + '_
    {
        self.entries
            .iter()
            .map(|entry| (entry.document, entry.disposition))
    }

    #[must_use]
    pub fn disposition(
        &self,
        document: EditorDocumentId,
    ) -> Option<EditorPrefabExtractionDisposition> {
        Some(self.entries.get(self.index(document)?)?.disposition)
    }

    /// Borrows the fixed saved Scene output, including while its checkpoint is taken or published.
    /// Guards, Prefab members and unknown documents return `None`.
    #[must_use]
    pub fn saved_scene(&self, document: EditorDocumentId) -> Option<&Arc<SceneDocument>> {
        let save = self.entries.get(self.index(document)?)?.save.as_ref()?;
        match &save.content {
            SavedContent::Scene(source) => Some(source),
            SavedContent::Prefab(_) => None,
        }
    }

    /// Borrows the fixed saved Prefab output, including while its checkpoint is taken or published.
    /// Guards, Scene members and unknown documents return `None`.
    #[must_use]
    pub fn saved_prefab(&self, document: EditorDocumentId) -> Option<&Arc<PrefabDocument>> {
        let save = self.entries.get(self.index(document)?)?.save.as_ref()?;
        match &save.content {
            SavedContent::Scene(_) => None,
            SavedContent::Prefab(source) => Some(source),
        }
    }

    #[must_use]
    pub fn file_limits(&self, document: EditorDocumentId) -> Option<SceneFileLimits> {
        let index = self.index(document)?;
        Some(
            match (
                &self.sources.sources[index],
                &self.sources.prepared.input.sources[index].source,
            ) {
                (Some(PreparedSourceKind::Scene(source)), _) => source.file_limits(),
                (Some(PreparedSourceKind::Prefab(source)), _) => source.file_limits(),
                (None, WorkspaceSourceInputKind::Scene(source)) => source.file_limits(),
                (None, WorkspaceSourceInputKind::Prefab(source)) => source.file_limits(),
            },
        )
    }

    /// Takes the one fixed checkpoint and borrows its already admitted exact readmission.
    /// Encoding failure must return `failure.into_checkpoint()` through `return_checkpoint`.
    /// A Host using this primitive must admit canonical/codec/readmission/fs work separately;
    /// `encode_checkpoint` supplies the bounded codec path for the extraction Host.
    pub fn take_checkpoint(
        &mut self,
        document: EditorDocumentId,
    ) -> Result<
        (EditorPersistenceCheckpoint, &AuthoringDocumentReadmission),
        EditorWorkspaceCommandReport,
    > {
        self.admit_lookup(document, None)?;
        let index = self
            .index(document)
            .ok_or_else(|| refused(None, Some(document), "No captured document has this ID"))?;
        let save = self.entries[index]
            .save
            .as_mut()
            .ok_or_else(|| refused(None, Some(document), "Guard has no persistence checkpoint"))?;
        if !matches!(&save.evidence, SavedEvidence::Available(_)) {
            return Err(refused(
                None,
                Some(document),
                "The fixed checkpoint is already taken or published",
            ));
        }
        let SavedEvidence::Available(checkpoint) =
            std::mem::replace(&mut save.evidence, SavedEvidence::Taken)
        else {
            unreachable!("available checkpoint was checked above")
        };
        Ok((checkpoint, &save.readmission))
    }

    /// Returns the same unconsumed checkpoint after failed encoding or an explicit deferred retry.
    /// An ordinary recapture, including the same actual source/revision, cannot replace it.
    pub fn return_checkpoint(
        &mut self,
        checkpoint: EditorPersistenceCheckpoint,
    ) -> Result<(), EditorWorkspaceExtractionCheckpointFailure> {
        let result = self.require_checkpoint(&checkpoint);
        match result {
            Ok(index) => {
                self.entries[index]
                    .save
                    .as_mut()
                    .expect("matched save")
                    .evidence = SavedEvidence::Available(checkpoint);
                Ok(())
            }
            Err(report) => Err(EditorWorkspaceExtractionCheckpointFailure { checkpoint, report }),
        }
    }

    /// Accepts only this stage's outstanding nominal checkpoint and exact immutable output.
    /// No live Workspace or saved field is observed or changed here.
    pub fn accept_commit(
        &mut self,
        commit: EditorPersistenceCommit,
    ) -> Result<(), EditorWorkspaceExtractionSaveFailure> {
        match self.require_checkpoint(&commit.checkpoint) {
            Ok(index) => {
                self.entries[index]
                    .save
                    .as_mut()
                    .expect("matched save")
                    .evidence = SavedEvidence::Published(commit);
                Ok(())
            }
            Err(report) => Err(EditorWorkspaceExtractionSaveFailure { commit, report }),
        }
    }

    fn index(&self, document: EditorDocumentId) -> Option<usize> {
        self.entries
            .binary_search_by_key(&document, |entry| entry.document)
            .ok()
    }

    fn admit_lookup(
        &mut self,
        document: EditorDocumentId,
        candidate: Option<&EditorPersistenceCheckpoint>,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let storage = self
            .entries
            .len()
            .checked_mul(size_of::<SavedEntry>())
            .and_then(|bytes| {
                bytes.checked_add(
                    candidate
                        .and_then(|checkpoint| checkpoint.source_binding.as_ref())
                        .map_or(0, |binding| binding.locator().len()),
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    self.sources
                        .prepared
                        .input
                        .document_source_binding(document)
                        .map_or(0, |binding| binding.locator().len()),
                )
            });
        admit(&mut self.sources, storage, Some(document))
    }

    fn require_checkpoint(
        &mut self,
        checkpoint: &EditorPersistenceCheckpoint,
    ) -> Result<usize, EditorWorkspaceCommandReport> {
        let document = checkpoint.document;
        self.admit_lookup(document, Some(checkpoint))?;
        let Some(index) = self.index(document) else {
            return Err(refused(
                None,
                Some(document),
                "A persistence checkpoint has no captured document",
            ));
        };
        let Some(save) = &self.entries[index].save else {
            return Err(refused(
                None,
                Some(document),
                "Guard cannot accept a persistence checkpoint or commit",
            ));
        };
        if !matches!(&save.evidence, SavedEvidence::Taken)
            || !save.matches(checkpoint, document, &self.sources.prepared.input.owner)
        {
            return Err(refused(
                None,
                Some(document),
                "Persistence requires the outstanding exact capture, binding, output and source state",
            ));
        }
        Ok(index)
    }

    fn require_complete_saves(&mut self) -> Result<(), EditorWorkspaceCommandReport> {
        let storage = self
            .entries
            .len()
            .checked_mul(size_of::<SavedEntry>())
            .and_then(|bytes| {
                bytes.checked_add(
                    size_of::<EditorWorkspaceExtractionPersistenceCommit<'_>>()
                        .max(size_of::<EditorWorkspaceExtractionPersistenceBindFailure>()),
                )
            });
        admit(&mut self.sources, storage, None)?;
        if self.entries.len() != self.sources.sources.len()
            || self.entries.len() != self.sources.prepared.input.sources.len()
        {
            return Err(refused(
                None,
                None,
                "Persistence requires complete captured membership",
            ));
        }
        for ((entry, input), source) in self
            .entries
            .iter()
            .zip(&self.sources.prepared.input.sources)
            .zip(&self.sources.sources)
        {
            self.sources
                .prepared
                .corpus
                .admit_binding_storage(
                    input.binding.locator().len().checked_add(
                        entry
                            .save
                            .as_ref()
                            .map_or(0, |save| save.binding.locator().len()),
                    ),
                )
                .map_err(|report| source_report(None, Some(input.document), report))?;
            let disposition_matches = matches!(
                (entry.disposition, source.is_some(), entry.save.is_some()),
                (EditorPrefabExtractionDisposition::ReplaceSource, true, true)
                    | (EditorPrefabExtractionDisposition::SaveCurrent, false, true)
                    | (EditorPrefabExtractionDisposition::Guard, false, false)
            );
            if entry.document != input.document || !disposition_matches {
                return Err(refused(
                    None,
                    Some(input.document),
                    "Saved dispositions must cover the exact original source set",
                ));
            }
            if let Some(save) = &entry.save {
                let SavedEvidence::Published(commit) = &save.evidence else {
                    return Err(refused(
                        None,
                        Some(input.document),
                        "Every ReplaceSource and SaveCurrent requires its exact published commit",
                    ));
                };
                if !save.matches(
                    &commit.checkpoint,
                    input.document,
                    &self.sources.prepared.input.owner,
                ) || !save.matches_source(input, source.as_ref())
                    || save.basis != self.sources.prepared.input.basis
                {
                    return Err(refused(
                        None,
                        Some(input.document),
                        "Saved commit differs from its fixed input or prepared future source",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn saved_publication_selection(
    stage: &mut PreparedEditorWorkspaceExtractionPersistence,
) -> Result<Vec<bool>, EditorWorkspaceCommandReport> {
    let sources = &mut stage.sources;
    let corpus = &mut sources.prepared.corpus;
    let count = corpus.sources().len();
    corpus
        .admit_binding_storage(count.checked_mul(size_of::<bool>()))
        .map_err(|report| source_report(None, None, report))?;
    let mut published = Vec::with_capacity(count);
    for source in 0..count {
        published.push(corpus.source_changed(source).ok_or_else(|| {
            refused(
                None,
                None,
                "A physical source is absent from the extraction corpus",
            )
        })?);
    }
    for (entry, binding) in stage.entries.iter().zip(&sources.prepared.bindings) {
        let Some(output) = corpus.sources().get(binding.source) else {
            return Err(refused(
                None,
                Some(entry.document),
                "A saved checkpoint has no physical corpus member",
            ));
        };
        let matches = match (&entry.save, output) {
            (None, _) => entry.disposition == EditorPrefabExtractionDisposition::Guard,
            (Some(saved), CapturedExtractionSource::Scene { document, .. }) => {
                matches!(&saved.content, SavedContent::Scene(saved) if Arc::ptr_eq(saved, document))
            }
            (Some(saved), CapturedExtractionSource::Prefab { source }) => {
                matches!(&saved.content, SavedContent::Prefab(saved) if std::ptr::eq(saved.as_ref(), source.document()))
            }
        };
        if !matches || entry.document != binding.document {
            return Err(refused(
                None,
                Some(entry.document),
                "Saved-generation input requires the exact fixed source output",
            ));
        }
        published[binding.source] = entry.save.is_some();
    }
    Ok(published)
}

impl SavedCheckpoint {
    fn matches(
        &self,
        checkpoint: &EditorPersistenceCheckpoint,
        document: EditorDocumentId,
        owner: &Arc<EditorWorkspacePersistenceIdentity>,
    ) -> bool {
        Arc::ptr_eq(&self.capture, &checkpoint.capture)
            && Arc::ptr_eq(owner, &checkpoint.identity)
            && checkpoint.document == document
            && checkpoint.kind == self.kind
            && checkpoint.source_binding.as_ref() == Some(&self.binding)
            && checkpoint.revision == self.revision
            && checkpoint.content_state == self.content_state
            && match (&self.content, &checkpoint.content) {
                (SavedContent::Scene(expected), EditorPersistenceContent::Scene(actual)) => {
                    Arc::ptr_eq(expected, actual)
                }
                (SavedContent::Prefab(expected), EditorPersistenceContent::Prefab(actual)) => {
                    Arc::ptr_eq(expected, actual)
                }
                _ => false,
            }
    }

    fn matches_source(
        &self,
        input: &WorkspaceSourceInput,
        source: Option<&PreparedSourceKind>,
    ) -> bool {
        if self.binding != input.binding || self.kind != input.source.kind() {
            return false;
        }
        let (revision, content_state, basis, same_output) =
            match (source, &input.source, &self.content) {
                (
                    Some(PreparedSourceKind::Scene(source)),
                    WorkspaceSourceInputKind::Scene(_),
                    SavedContent::Scene(output),
                ) => (
                    source.revision(),
                    source.content_state(),
                    source.schema_basis(),
                    std::ptr::eq(source.document(), output.as_ref()),
                ),
                (
                    Some(PreparedSourceKind::Prefab(source)),
                    WorkspaceSourceInputKind::Prefab(_),
                    SavedContent::Prefab(output),
                ) => (
                    source.revision(),
                    source.content_state(),
                    source.schema_basis(),
                    std::ptr::eq(source.document(), output.as_ref()),
                ),
                (None, WorkspaceSourceInputKind::Scene(source), SavedContent::Scene(output)) => (
                    source.revision(),
                    source.content_state(),
                    source.schema_basis(),
                    std::ptr::eq(source.document(), output.as_ref()),
                ),
                (None, WorkspaceSourceInputKind::Prefab(source), SavedContent::Prefab(output)) => (
                    source.revision(),
                    source.content_state(),
                    source.schema_basis(),
                    std::ptr::eq(source.document(), output.as_ref()),
                ),
                _ => return false,
            };
        same_output
            && revision == self.revision
            && content_state == self.content_state
            && basis == self.basis
    }
}

/// Complete source, saved-field and Session borrows acquired before the first swap.
/// This guard proves only the open Workspace transition, not Project Ready/publication.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspaceExtractionPersistenceCommit<'w> {
    sources: EditorWorkspacePrefabExtractionSourceCommit<'w>,
    entries: Vec<SavedEntry>,
}

impl EditorWorkspaceExtractionPersistenceCommit<'_> {
    pub fn prepared(&self) -> &PreparedEditorWorkspacePrefabExtraction {
        self.sources.prepared()
    }

    /// Releases into the same stage without allocation, admission, capture or new source IDs.
    pub fn into_prepared(self) -> PreparedEditorWorkspaceExtractionPersistence {
        PreparedEditorWorkspaceExtractionPersistence {
            sources: self.sources.into_prepared(),
            entries: self.entries,
        }
    }

    /// Performs only prebound source swaps and exact saved acknowledgement. The concrete Host
    /// must bind file/lane/project destinations before calling this infallible installation.
    pub fn commit(self) {
        for (source, entry) in self.sources.sources.into_iter().zip(self.entries) {
            let commit = entry.save.map(|save| {
                let SavedEvidence::Published(commit) = save.evidence else {
                    unreachable!("complete persistence was validated before binding")
                };
                commit
            });
            source.commit(commit);
        }
    }
}

impl EditorPersistenceAuthority {
    /// Reuses the complete source guard after matching every mandatory saved checkpoint.
    /// Failure returns the original stage and accepted commits, with all performed Work spent.
    #[doc(hidden)]
    pub fn bind_prefab_extraction_persistence<'w>(
        &self,
        workspace: &'w mut EditorWorkspace,
        mut prepared: PreparedEditorWorkspaceExtractionPersistence,
    ) -> Result<
        EditorWorkspaceExtractionPersistenceCommit<'w>,
        EditorWorkspaceExtractionPersistenceBindFailure,
    > {
        if let Err(report) = prepared.require_complete_saves() {
            return Err(EditorWorkspaceExtractionPersistenceBindFailure { prepared, report });
        }
        let PreparedEditorWorkspaceExtractionPersistence { sources, entries } = prepared;
        match self.bind_prefab_extraction_sources(workspace, sources) {
            Ok(sources) => Ok(EditorWorkspaceExtractionPersistenceCommit { sources, entries }),
            Err(failure) => {
                let (sources, report) = failure.into_parts();
                Err(EditorWorkspaceExtractionPersistenceBindFailure {
                    prepared: PreparedEditorWorkspaceExtractionPersistence { sources, entries },
                    report,
                })
            }
        }
    }
}

/// Captured before worker dispatch; ordinary saves or reload changes make late binding stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WorkspaceSavedInput {
    revision: SceneAuthoringRevision,
    content_state: SceneAuthoringContentState,
    digest: Option<EditorDocumentDigest>,
    reload: EditorExternalReloadState,
}

impl WorkspaceSavedInput {
    pub(super) fn capture(slot: &EditorDocumentSlot) -> Option<Self> {
        Some(match slot {
            EditorDocumentSlot::Scene(slot) => Self {
                revision: slot.saved_revision,
                content_state: slot.saved_content_state,
                digest: slot.saved_digest,
                reload: slot.external_reload,
            },
            EditorDocumentSlot::Prefab(slot) => Self {
                revision: slot.saved_revision,
                content_state: slot.saved_content_state,
                digest: slot.saved_digest,
                reload: slot.external_reload,
            },
            EditorDocumentSlot::Extension(_) => return None,
        })
    }
}

#[derive(Debug)]
pub(super) struct BoundSavedFields<'w> {
    revision: &'w mut SceneAuthoringRevision,
    content_state: &'w mut SceneAuthoringContentState,
    digest: &'w mut Option<EditorDocumentDigest>,
    reload: &'w mut EditorExternalReloadState,
}

impl<'w> BoundSavedFields<'w> {
    pub(super) fn new(
        revision: &'w mut SceneAuthoringRevision,
        content_state: &'w mut SceneAuthoringContentState,
        digest: &'w mut Option<EditorDocumentDigest>,
        reload: &'w mut EditorExternalReloadState,
    ) -> Self {
        Self {
            revision,
            content_state,
            digest,
            reload,
        }
    }

    fn commit(self, commit: &EditorPersistenceCommit) {
        *self.revision = commit.checkpoint.revision;
        *self.content_state = commit.checkpoint.content_state;
        *self.digest = Some(commit.digest);
        if *self.reload != EditorExternalReloadState::Conflict {
            *self.reload = EditorExternalReloadState::Clean;
        }
    }
}

impl BoundSource<'_> {
    pub(super) fn commit(self, commit: Option<EditorPersistenceCommit>) {
        // The same mechanical installation serves source-only and source+saved callers. Exact
        // source/content ownership was checked while binding all Sessions, before the first swap.
        let acknowledged = match self.kind {
            BoundSourceKind::Scene(source) => {
                let session = source.commit();
                commit.as_ref().is_none_or(|commit| {
                    session.acknowledge_source_saved(commit.checkpoint.content_state)
                })
            }
            BoundSourceKind::Prefab(source) => {
                let session = source.commit();
                commit.as_ref().is_none_or(|commit| {
                    session.acknowledge_source_saved(commit.checkpoint.content_state)
                })
            }
            BoundSourceKind::SceneGuard(session) => commit.as_ref().is_none_or(|commit| {
                session.acknowledge_source_saved(commit.checkpoint.content_state)
            }),
            BoundSourceKind::PrefabGuard(session) => commit.as_ref().is_none_or(|commit| {
                session.acknowledge_source_saved(commit.checkpoint.content_state)
            }),
        };
        debug_assert!(
            acknowledged,
            "the exact current or future saved content was prebound"
        );
        if let Some(commit) = commit {
            self.saved.commit(&commit);
        }
    }
}

fn admit(
    sources: &mut PreparedEditorWorkspaceExtractionSources,
    bytes: Option<usize>,
    document: Option<EditorDocumentId>,
) -> Result<(), EditorWorkspaceCommandReport> {
    sources
        .prepared
        .corpus
        .admit_binding_storage(bytes)
        .map_err(|report| source_report(None, document, report))
}

#[cfg(test)]
mod tests;
