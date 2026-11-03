//! Project extraction with shared physical, semantic and final-publication owners.
//!
//! Live capture and Workspace installation require tooling. Process recovery uses stored source
//! facts and the same interpretation, file publication and future Project preparation without a
//! Workspace or serialized Editor identity.

use nara_asset::AssetMetaIdentity;
#[cfg(feature = "tooling")]
use nara_scene::__private::{PrefabExtractionDestination, PrefabExtractionFailure};
use nara_scene::{
    __private::{
        CapturedExtractionSource, PrefabExtractionRequest, PrefabExtractionSavedGeneration,
        PrefabExtractionWork,
    },
    SceneFileEncoding, ScenePatchReport, SourceDocumentKey, SourceDocumentKind,
};
#[cfg(feature = "tooling")]
use nara_tooling::{
    EditorDocumentId, EditorPrefabExtractionCorpusBindFailure, EditorPrefabExtractionCorpusBinding,
    EditorWorkspaceExtractionPersistencePrepareFailure,
    EditorWorkspaceExtractionSavedGenerationFailure, EditorWorkspacePrefabExtractionInput,
    EditorWorkspacePrefabExtractionSourcePrepareFailure,
    PreparedEditorWorkspaceExtractionPersistence, PreparedEditorWorkspacePrefabExtraction,
};

use super::super::source_inventory::{SourceInventory, SourceInventoryError};
#[cfg(feature = "tooling")]
use super::super::{
    document::AuthoringDocumentSourceFact,
    source_inventory::{OpenSource, SourceEncoding},
};
use super::*;

mod environments;
#[cfg(feature = "tooling")]
mod outputs;
mod pending;
mod publication;
mod recovered_generation;
mod recovered_inventory;
mod recovery_data;
mod resume;
pub use resume::{
    PrefabExtractionRecovery, PrefabExtractionRecoveryDestination,
    PrefabExtractionRecoveryErrorKind, PrefabExtractionRecoveryFailure,
    PrefabExtractionRecoveryInspectionFailure, PrefabExtractionRecoveryMember,
    PrefabExtractionRecoveryMemberDisposition, PrefabExtractionRecoveryMemberState,
    PrefabExtractionRecoveryOutcome, PrefabExtractionRecoveryPhase, PrefabExtractionRecoveryReview,
    PrefabExtractionRecoveryUsage, PrefabExtractionRecoveryView,
};
#[cfg(feature = "tooling")]
mod review;
#[cfg(feature = "tooling")]
mod session_publication;
#[cfg(feature = "tooling")]
mod session_review;
#[cfg(feature = "tooling")]
pub(crate) use outputs::{
    PreparedProjectPrefabExtractionOutputs, ProjectPrefabExtractionOutput,
    ProjectPrefabExtractionOutputCancelFailure, ProjectPrefabExtractionOutputEncoding,
    ProjectPrefabExtractionOutputError, ProjectPrefabExtractionOutputFailure,
    ProjectPrefabExtractionOutputObservation, ProjectPrefabExtractionOutputRole,
};
#[cfg(feature = "tooling")]
pub(crate) use review::{
    PreparedProjectPrefabExtractionReview, ProjectPrefabExtractionReviewFailure,
};
#[cfg(feature = "tooling")]
pub(crate) use session_publication::{
    PrefabExtractionPersistenceDestination, PrefabExtractionSessionInstallation,
    PrefabExtractionSessionPublication, PrefabExtractionSessionPublicationError,
    PrefabExtractionSessionPublicationPhase,
};
#[cfg(feature = "tooling")]
pub(crate) use session_review::{
    PrefabExtractionReviewRequest, PrefabExtractionReviewRevisionRequest,
    PrefabExtractionSessionPreparationError, PrefabExtractionSessionPreparationFailure,
    PreparedPrefabExtractionSessionReview,
};

#[cfg(all(test, feature = "tooling", any(windows, target_os = "linux")))]
mod tests;

#[cfg(feature = "tooling")]
pub(crate) struct PrefabExtractionOpenInput {
    pub(crate) document: EditorDocumentId,
    pub(crate) source: Option<AuthoringDocumentSourceFact>,
    pub(crate) observation: Option<FileSourceObservation>,
}

#[cfg(feature = "tooling")]
impl PrefabExtractionOpenInput {
    pub(crate) fn retained_bytes(
        source: Option<&AuthoringDocumentSourceFact>,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        checked_sum([
            size_of::<Self>(),
            source.map_or(Ok(0), |source| {
                source
                    .checked_retained_bytes()
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
            })?,
        ])
    }
}

/// Q's fixed identity and selected-profile alias, allocated once before the first capture.
#[cfg(feature = "tooling")]
pub(crate) struct PrefabExtractionTarget {
    pub(crate) metadata: AssetMetaIdentity,
}

#[cfg(feature = "tooling")]
pub(crate) struct PrefabExtractionCaptureRequest {
    schema: Arc<AuthoringSchemaReceipt>,
    adapters: Arc<[AuthoringAdapterReceipt]>,
    basis: AuthoringSchemaBasis,
    scene_limits: SceneFileLimits,
    input: EditorWorkspacePrefabExtractionInput,
    open: Vec<PrefabExtractionOpenInput>,
    inputs: AuthoringSourceCaptureInputs,
    work: PrefabExtractionWork,
    target: PrefabExtractionTarget,
}

#[cfg(feature = "tooling")]
pub(crate) struct PrefabExtractionCaptureFailure {
    pub(crate) request: PrefabExtractionCaptureRequest,
    pub(crate) error: PrefabExtractionProjectError,
}

/// Disk observations stay separate from the actual open working allocations through Review.
#[cfg(feature = "tooling")]
pub(crate) struct ProjectExtractionContext {
    schema: Arc<AuthoringSchemaReceipt>,
    adapters: Arc<[AuthoringAdapterReceipt]>,
    basis: nara_scene::AuthoringSchemaBasis,
    inventory: SourceInventory,
    disk: Vec<CapturedExtractionSource>,
    open: Vec<PrefabExtractionOpenInput>,
    destination: ObservedExtractionDestination,
    budget: AuthoringProjectAdmissionBudget,
}

#[cfg(feature = "tooling")]
pub(crate) struct PrefabExtractionBaseline {
    context: ProjectExtractionContext,
    input: EditorWorkspacePrefabExtractionInput,
    request: PrefabExtractionRequest,
    work: PrefabExtractionWork,
    bindings: Vec<EditorPrefabExtractionCorpusBinding>,
}

#[cfg(feature = "tooling")]
pub(crate) struct PreparedProjectPrefabExtraction {
    context: ProjectExtractionContext,
    pub(crate) workspace: PreparedEditorWorkspacePrefabExtraction,
}

#[cfg(feature = "tooling")]
pub(crate) enum ProjectPrefabExtractionPreparationFailure {
    Core {
        context: ProjectExtractionContext,
        input: EditorWorkspacePrefabExtractionInput,
        failure: PrefabExtractionFailure,
        bindings: Vec<EditorPrefabExtractionCorpusBinding>,
    },
    Workspace {
        context: ProjectExtractionContext,
        failure: EditorPrefabExtractionCorpusBindFailure,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PrefabExtractionProjectError {
    #[error("Prefab extraction requires the complete original bound open set")]
    OpenSet,
    #[error("Prefab extraction exceeded the original source allowance")]
    SourceWork(ScenePatchReport),
    #[error("Prefab extraction destination or metadata is not a valid new Prefab")]
    Destination,
    #[error("Prefab extraction destination or sidecar is already occupied")]
    Occupied,
    #[error("Prefab extraction Host admission failed")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("Prefab extraction source admission failed")]
    Project(#[from] AuthoringProjectSourceAdmissionError),
    #[error("Prefab extraction physical inventory failed")]
    Inventory(#[from] SourceInventoryError),
    #[error("Prefab extraction source decoding failed")]
    Decode(#[from] SourceDecodeError),
    #[error("Prefab extraction destination observation failed")]
    Host(#[from] FsError),
}

#[cfg(feature = "tooling")]
impl std::fmt::Debug for PrefabExtractionCaptureFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionCaptureFailure")
            .field("error", &self.error)
            .field("work", &self.request.work.usage())
            .finish()
    }
}

#[cfg(feature = "tooling")]
impl std::fmt::Debug for PrefabExtractionBaseline {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionBaseline")
            .field("request", &self.request)
            .field("work", &self.work.usage())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "tooling")]
impl std::fmt::Debug for PreparedProjectPrefabExtraction {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedProjectPrefabExtraction")
            .field("workspace", &self.workspace)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "tooling")]
impl std::fmt::Debug for ProjectPrefabExtractionPreparationFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core { failure, .. } => formatter
                .debug_tuple("Core")
                .field(failure.report())
                .finish(),
            Self::Workspace { failure, .. } => formatter
                .debug_tuple("Workspace")
                .field(failure.report())
                .finish(),
        }
    }
}

#[cfg(feature = "tooling")]
impl ProjectExtractionContext {
    /// Rechecks the actual original Ready authority, complete physical corpus and both missing
    /// leaves. The retained inventory keeps every old object and ancestor alive throughout.
    fn validate(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
    ) -> Result<(), PrefabExtractionProjectError> {
        if !Arc::ptr_eq(&self.schema, &project.schema) {
            return Err(PrefabExtractionProjectError::OpenSet);
        }
        self.destination
            .validate(&self.schema, &self.inventory, &mut self.budget)?;
        let bytes = checked_product(self.open.len(), size_of::<OpenSource<'_>>())?;
        self.budget.charge_source_bytes(0, 0, bytes, bytes)?;
        let sources = self
            .open
            .iter()
            .map(|open| {
                open.source
                    .as_ref()
                    .map(OpenSource::Bound)
                    .unwrap_or(OpenSource::Unbound)
            })
            .collect::<Vec<_>>();
        let result = self
            .inventory
            .validate(&self.schema, &sources, &mut self.budget);
        drop(sources);
        self.budget.reconcile_retained_reservation(bytes, 0)?;
        result.map_err(Into::into)
    }
}

#[cfg(feature = "tooling")]
impl PrefabExtractionBaseline {
    pub(crate) fn revalidate_project(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
    ) -> Result<(), PrefabExtractionProjectError> {
        self.context.validate(project)
    }
}

#[cfg(feature = "tooling")]
impl PreparedProjectPrefabExtraction {
    /// This proves project observations only. Workspace source/history binding is a separate
    /// late guard supplied by `workspace`; neither proof alone publishes or installs a source.
    pub(crate) fn revalidate_project(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
    ) -> Result<(), PrefabExtractionProjectError> {
        self.context.validate(project)
    }
}

#[cfg(feature = "tooling")]
impl AuthoringProjectSessionSnapshot {
    /// The caller admits `metadata_retained_bytes` plus every worker selection before copying
    /// them, and obtains each proof through these same inputs. No source body is cloned here.
    pub(crate) fn capture_prefab_extraction_request(
        &self,
        input: EditorWorkspacePrefabExtractionInput,
        open: Vec<PrefabExtractionOpenInput>,
        inputs: AuthoringSourceCaptureInputs,
        work: PrefabExtractionWork,
        target: PrefabExtractionTarget,
    ) -> PrefabExtractionCaptureRequest {
        PrefabExtractionCaptureRequest {
            schema: Arc::clone(&self.schema),
            adapters: Arc::clone(&self.adapters),
            basis: self.startup_scene.schema_basis(),
            scene_limits: self.scene_limits,
            input,
            open,
            inputs,
            work,
            target,
        }
    }
}

#[cfg(feature = "tooling")]
impl PrefabExtractionCaptureRequest {
    pub(crate) fn metadata_retained_bytes(
        project: &AuthoringProjectSessionSnapshot,
        target: &AssetMetaIdentity,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        checked_sum([
            size_of::<Self>(),
            size_of::<AuthoringSchemaReceipt>(),
            checked_product(
                project.schema.bundle_bytes().len(),
                AUTHORING_BASIS_ENCODED_MULTIPLIER,
            )?,
            target.path().as_str().len(),
            4 * size_of::<usize>(),
        ])
    }

    /// Refusal returns the same request. Drop temporary storage before refunding it, keeping
    /// all performed reads and comparisons and the original cumulative extraction Work spent.
    pub(crate) fn capture(
        mut self,
    ) -> Result<PrefabExtractionBaseline, PrefabExtractionCaptureFailure> {
        let retained = self.inputs.budget.retained_candidate_bytes;
        let handles = self.inputs.budget.retained_open_handles;
        match self.capture_sources() {
            Ok(captured) => {
                let Self {
                    schema,
                    adapters,
                    basis,
                    input,
                    open,
                    inputs,
                    work,
                    ..
                } = self;
                Ok(PrefabExtractionBaseline {
                    context: ProjectExtractionContext {
                        schema,
                        adapters,
                        basis,
                        inventory: captured.inventory,
                        disk: captured.disk,
                        open,
                        destination: captured.destination,
                        budget: inputs.budget,
                    },
                    input,
                    request: captured.request,
                    bindings: captured.bindings,
                    work,
                })
            }
            Err(mut error) => {
                let settlement = (|| {
                    let temporary = self
                        .inputs
                        .budget
                        .retained_candidate_bytes
                        .checked_sub(retained)
                        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
                    self.inputs
                        .budget
                        .reconcile_retained_reservation(temporary, 0)?;
                    let closed = self
                        .inputs
                        .budget
                        .retained_open_handles
                        .checked_sub(handles)
                        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
                    self.inputs.budget.release_open_handles(closed);
                    Ok::<_, AuthoringProjectSourceAdmissionError>(())
                })();
                if let Err(failed) = settlement {
                    error = failed.into();
                }
                Err(PrefabExtractionCaptureFailure {
                    request: self,
                    error,
                })
            }
        }
    }

    fn capture_sources(&mut self) -> Result<CapturedProjectSources, PrefabExtractionProjectError> {
        let unused_slots = self
            .open
            .capacity()
            .checked_sub(self.open.len())
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let selected_bytes = self.open.iter().try_fold(
            checked_product(unused_slots, size_of::<PrefabExtractionOpenInput>())?,
            |bytes, selected| {
                checked_sum([
                    bytes,
                    PrefabExtractionOpenInput::retained_bytes(selected.source.as_ref())?,
                ])
            },
        )?;
        let metadata = checked_sum([
            size_of::<Self>(),
            size_of::<AuthoringSchemaReceipt>(),
            checked_product(
                self.schema.bundle_bytes().len(),
                AUTHORING_BASIS_ENCODED_MULTIPLIER,
            )?,
            self.target.metadata.path().as_str().len(),
            4 * size_of::<usize>(),
        ])?;
        if self.inputs.worker_retained_bytes < checked_sum([selected_bytes, metadata])?
            || self.inputs.observation_count < self.open.len()
            || self.inputs.basis != self.basis
            || self.input.schema_basis() != &self.basis
            || self.input.documents().len() != self.open.len()
        {
            return Err(PrefabExtractionProjectError::OpenSet);
        }
        let comparisons = (usize::BITS - self.open.len().leading_zeros()) as usize + 1;
        self.inputs.budget.charge_source_bytes(
            0,
            0,
            checked_product(selected_bytes, comparisons * 2)?,
            0,
        )?;
        self.open.sort_unstable_by_key(|selected| selected.document);
        for (selected, document) in self.open.iter().zip(self.input.documents()) {
            let source = selected
                .source
                .as_ref()
                .ok_or(PrefabExtractionProjectError::OpenSet)?;
            let observation = selected
                .observation
                .as_ref()
                .ok_or(PrefabExtractionProjectError::OpenSet)?;
            if selected.document != document
                || self.input.document_source_binding(document).is_none()
                || !capture::source_locator_matches(
                    self.input.document_source(document),
                    source.relative_path(),
                )
                || match source.source_key().kind() {
                    SourceDocumentKind::Scene => self.input.scene_input(document).is_none(),
                    SourceDocumentKind::Prefab => self.input.prefab_input(document).is_none(),
                }
            {
                return Err(PrefabExtractionProjectError::OpenSet);
            }
            capture::source_observations::check_document_observation(source, observation)?;
        }
        self.open.sort_unstable_by(|left, right| {
            left.source
                .as_ref()
                .map(AuthoringDocumentSourceFact::relative_path)
                .cmp(
                    &right
                        .source
                        .as_ref()
                        .map(AuthoringDocumentSourceFact::relative_path),
                )
        });
        if self.open.windows(2).any(|pair| {
            pair[0]
                .source
                .as_ref()
                .map(AuthoringDocumentSourceFact::relative_path)
                == pair[1]
                    .source
                    .as_ref()
                    .map(AuthoringDocumentSourceFact::relative_path)
        }) {
            return Err(PrefabExtractionProjectError::OpenSet);
        }
        let budget = &mut self.inputs.budget;
        let scratch = checked_product(self.open.len(), size_of::<OpenSource<'_>>())?;
        budget.charge_source_bytes(0, 0, scratch, scratch)?;
        let open_sources: Vec<_> = self
            .open
            .iter()
            .map(|open| {
                open.source
                    .as_ref()
                    .map(OpenSource::Bound)
                    .unwrap_or(OpenSource::Unbound)
            })
            .collect();
        let inventory =
            SourceInventory::capture(&self.schema, &open_sources, self.scene_limits, budget);
        drop(open_sources);
        budget.reconcile_retained_reservation(scratch, 0)?;
        let inventory = inventory?;
        let custody = inventory.shared_custody();
        for selected in &self.open {
            let source = selected
                .source
                .as_ref()
                .ok_or(PrefabExtractionProjectError::OpenSet)?;
            let member = inventory
                .participant(source.relative_path())
                .ok_or(PrefabExtractionProjectError::OpenSet)?;
            let observation = selected
                .observation
                .as_ref()
                .ok_or(PrefabExtractionProjectError::OpenSet)?;
            let file = custody
                .file(member.file().path())
                .ok_or(PrefabExtractionProjectError::OpenSet)?;
            budget.charge_source_bytes(0, 0, source.relative_path().len(), 0)?;
            if observation.observation()
                != (
                    member.file().identity(),
                    member.file().parent_identity(),
                    member.file().digest(),
                )
                || !observation.matches(file)?
            {
                return Err(PrefabExtractionProjectError::OpenSet);
            }
        }
        let destination = ObservedExtractionDestination::capture(
            &self.schema,
            &inventory,
            &self.target.metadata,
            budget,
        )?;
        let count = inventory.participants().len();
        self.work
            .admit_capture_storage(count.checked_mul(2 * size_of::<CapturedExtractionSource>()))
            .map_err(PrefabExtractionProjectError::SourceWork)?;
        let mut disk = Vec::with_capacity(count);
        let mut sources = Vec::with_capacity(count);
        self.work
            .admit_capture_storage(
                self.open
                    .len()
                    .checked_mul(size_of::<EditorPrefabExtractionCorpusBinding>()),
            )
            .map_err(PrefabExtractionProjectError::SourceWork)?;
        let mut bindings = Vec::with_capacity(self.open.len());
        for (index, member) in inventory.participants().enumerate() {
            let encoding = match member.encoding() {
                SourceEncoding::Json => SceneFileEncoding::Json,
                SourceEncoding::Ron => SceneFileEncoding::Ron,
            };
            let tree = decode_source(
                member.kind(),
                encoding,
                member.bytes(),
                self.scene_limits,
                budget,
            )?;
            let bytes = tree.retained_bytes()?;
            let storage = checked_sum([
                bytes,
                member.source_key().retained_bytes(),
                4 * size_of::<usize>(),
            ])?;
            self.work
                .admit_capture_storage(Some(storage))
                .map_err(PrefabExtractionProjectError::SourceWork)?;
            let original = match tree {
                SourceTree::Scene(document) => CapturedExtractionSource::Scene {
                    key: member.source_key().clone(),
                    document: Arc::new(document),
                    basis: self.basis.clone(),
                },
                SourceTree::Prefab(document) => CapturedExtractionSource::Prefab {
                    source: associated_prefab(
                        Arc::new(document),
                        member.source_key(),
                        member.file().path(),
                        &self.basis,
                        &inventory,
                    )?,
                },
            };
            // The source Work now retains this tree; the Host keeps its wire bytes and custody.
            budget.reconcile_retained_reservation(bytes, 0)?;
            self.work
                .admit_capture_work(
                    comparisons,
                    Some(checked_product(member.file().path().len(), comparisons)?),
                )
                .map_err(PrefabExtractionProjectError::SourceWork)?;
            let selected = self
                .open
                .binary_search_by(|open| {
                    open.source
                        .as_ref()
                        .map(AuthoringDocumentSourceFact::relative_path)
                        .cmp(&Some(member.file().path()))
                })
                .ok();
            self.work
                .admit_capture_storage(Some(member.source_key().retained_bytes()))
                .map_err(PrefabExtractionProjectError::SourceWork)?;
            let working = if let Some(open) = selected {
                let document = self.open[open].document;
                bindings.push(EditorPrefabExtractionCorpusBinding {
                    document,
                    source: index,
                });
                if let Some(input) = self.input.scene_input(document) {
                    CapturedExtractionSource::Scene {
                        key: member.source_key().clone(),
                        document: input.shared_document(),
                        basis: self.basis.clone(),
                    }
                } else {
                    let input = self
                        .input
                        .prefab_input(document)
                        .ok_or(PrefabExtractionProjectError::OpenSet)?;
                    CapturedExtractionSource::Prefab {
                        source: associated_prefab(
                            input.shared_document(),
                            member.source_key(),
                            member.file().path(),
                            &self.basis,
                            &inventory,
                        )?,
                    }
                }
            } else {
                share_source(&original)
            };
            disk.push(original);
            sources.push(working);
        }
        // Workspace bindings follow document-ID order, independent of physical path order.
        self.work
            .admit_capture_work(bindings.len().saturating_mul(comparisons), Some(0))
            .map_err(PrefabExtractionProjectError::SourceWork)?;
        bindings.sort_unstable_by_key(|binding| binding.document);
        let (environments, selected_environment) = environments::capture(
            self.schema.project_root(),
            self.schema.settings().profile_name.as_deref(),
            &inventory,
            &sources,
            environments::ExtractionDestination {
                key: &destination.key,
                record: &destination.record,
                authored_path: self.target.metadata.path(),
            },
            budget,
            &mut self.work,
        )?;
        let selected = self
            .open
            .iter()
            .find(|open| open.document == self.input.selected_document())
            .and_then(|open| open.source.as_ref())
            .ok_or(PrefabExtractionProjectError::OpenSet)?;
        self.work
            .admit_capture_storage(Some(checked_sum([
                selected.source_key().retained_bytes(),
                self.input.root().as_str().len(),
                destination.key.retained_bytes(),
                asset_record_retained_bytes(&destination.record)?,
            ])?))
            .map_err(PrefabExtractionProjectError::SourceWork)?;
        let request = PrefabExtractionRequest::new(
            sources,
            environments,
            selected_environment,
            self.basis.clone(),
            selected.source_key().clone(),
            self.input.root().clone(),
            PrefabExtractionDestination {
                key: destination.key.clone(),
                reference: AssetRef::StableId(destination.record.stable_id()),
                metadata: destination.record.clone(),
            },
        );
        Ok(CapturedProjectSources {
            inventory,
            disk,
            destination,
            request,
            bindings,
        })
    }
}

#[cfg(feature = "tooling")]
struct CapturedProjectSources {
    inventory: SourceInventory,
    disk: Vec<CapturedExtractionSource>,
    destination: ObservedExtractionDestination,
    request: PrefabExtractionRequest,
    bindings: Vec<EditorPrefabExtractionCorpusBinding>,
}

fn share_source(source: &CapturedExtractionSource) -> CapturedExtractionSource {
    match source {
        CapturedExtractionSource::Scene {
            key,
            document,
            basis,
        } => CapturedExtractionSource::Scene {
            key: key.clone(),
            document: Arc::clone(document),
            basis: basis.clone(),
        },
        CapturedExtractionSource::Prefab { source } => CapturedExtractionSource::Prefab {
            source: source.clone(),
        },
    }
}

fn associated_prefab(
    document: Arc<PrefabDocument>,
    key: &SourceDocumentKey,
    path: &str,
    basis: &AuthoringSchemaBasis,
    inventory: &SourceInventory,
) -> Result<ResolvedPrefabSource, PrefabExtractionProjectError> {
    let source = ResolvedPrefabSource::bound_authoring_source(document, basis.clone(), key.clone());
    // Canonical metadata association belongs to the physical source. Visibility still belongs
    // to each configuration and is never inferred from this association alone.
    match inventory.metadata_for_source(path) {
        None => Ok(source),
        Some(record) => source
            .with_source_asset(record.record())
            .map_err(|_| PrefabExtractionProjectError::Destination),
    }
}

#[cfg(feature = "tooling")]
impl PrefabExtractionBaseline {
    pub(crate) fn prepare(
        self,
    ) -> Result<PreparedProjectPrefabExtraction, ProjectPrefabExtractionPreparationFailure> {
        let Self {
            context,
            input,
            request,
            work,
            bindings,
        } = self;
        let corpus = match request.prepare(work) {
            Ok(corpus) => corpus,
            Err(failure) => {
                return Err(ProjectPrefabExtractionPreparationFailure::Core {
                    context,
                    input,
                    failure,
                    bindings,
                });
            }
        };
        match input.bind_corpus(corpus, bindings) {
            Ok(workspace) => Ok(PreparedProjectPrefabExtraction { context, workspace }),
            Err(failure) => {
                Err(ProjectPrefabExtractionPreparationFailure::Workspace { context, failure })
            }
        }
    }
}

#[cfg(feature = "tooling")]
struct ObservedExtractionDestination {
    key: SourceDocumentKey,
    record: AssetRecord,
    metadata: AssetMetaIdentity,
    encoding: SceneFileEncoding,
    path: ProjectPath,
    sidecar: ProjectPath,
}

#[cfg(feature = "tooling")]
impl ObservedExtractionDestination {
    fn capture(
        schema: &AuthoringSchemaReceipt,
        inventory: &SourceInventory,
        metadata: &AssetMetaIdentity,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, PrefabExtractionProjectError> {
        let settings = schema.settings();
        if metadata.source_kind() != &AssetSourceKind::Prefab
            || metadata.stable_id().as_uuid().is_nil()
        {
            return Err(PrefabExtractionProjectError::Destination);
        }
        let bytes = checked_sum([
            settings.paths.prefabs.as_str().len(),
            metadata.path().as_str().len(),
            1,
        ])?;
        budget.charge_source_bytes(
            0,
            0,
            bytes,
            checked_sum([checked_product(bytes, 6)?, size_of::<Self>()])?,
        )?;
        let path = ProjectPath::new(format!(
            "{}/{}",
            settings.paths.prefabs.as_str(),
            metadata.path().as_str()
        ))
        .map_err(|_| PrefabExtractionProjectError::Destination)?;
        let encoding = if path.as_str().ends_with(".json") {
            SceneFileEncoding::Json
        } else if path.as_str().ends_with(".ron") {
            SceneFileEncoding::Ron
        } else {
            return Err(PrefabExtractionProjectError::Destination);
        };
        let sidecar = ProjectPath::new(format!("{}.meta", path.as_str()))
            .map_err(|_| PrefabExtractionProjectError::Destination)?;
        let canonical = AssetSourcePath::new(path.as_str())
            .map_err(|_| PrefabExtractionProjectError::Destination)?;
        let record = AssetRecord::new(
            metadata.stable_id(),
            canonical.clone(),
            AssetSourceKind::Prefab,
        );
        if !inventory.admits_new_asset_record(&record, budget)? {
            return Err(PrefabExtractionProjectError::Destination);
        }
        let key = match settings.project.native_document_identity {
            nara_project::NativeDocumentIdentity::ProjectLocation => {
                SourceDocumentKey::project_location(
                    settings.project.source_universe,
                    canonical,
                    SourceDocumentKind::Prefab,
                )
            }
            nara_project::NativeDocumentIdentity::StableMetadata => {
                SourceDocumentKey::stable_asset(metadata.stable_id(), SourceDocumentKind::Prefab)
            }
        };
        let destination = Self {
            key,
            record,
            metadata: metadata.clone(),
            encoding,
            path,
            sidecar,
        };
        destination.validate(schema, inventory, budget)?;
        Ok(destination)
    }

    fn validate(
        &self,
        schema: &AuthoringSchemaReceipt,
        inventory: &SourceInventory,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), PrefabExtractionProjectError> {
        let root = schema.project_root();
        let custody = inventory.shared_custody();
        for path in [&self.path, &self.sidecar] {
            let scratch = checked_sum([65_536, checked_product(path.as_str().len(), 4)?])?;
            budget.charge_source_bytes(0, 0, scratch, scratch)?;
            let result = (|| {
                budget.admit_handle_peak(4)?;
                let relative = RelativePath::new(path.as_str()).map_err(FsError::from)?;
                let (parent, leaf) = root.resolve_file_parent(&relative)?;
                let original = custody
                    .parent(path.as_str())
                    .ok_or(PrefabExtractionProjectError::Destination)?;
                if !parent.same_object(original)? {
                    return Err(PrefabExtractionProjectError::Destination);
                }
                match parent.open_child_file(&leaf) {
                    Ok(_) => Err(PrefabExtractionProjectError::Occupied),
                    Err(FsError::Io { source, .. })
                        if source.kind() == std::io::ErrorKind::NotFound =>
                    {
                        Ok(())
                    }
                    Err(error) => Err(error.into()),
                }
            })();
            budget.reconcile_retained_reservation(scratch, 0)?;
            result?;
        }
        Ok(())
    }
}
