//! Holds the reviewed live sources through Pending and binds their accepted replacements.

use nara_tooling::{
    EditorDocumentId, EditorPersistenceAuthority, EditorPersistenceCommit,
    EditorPersistenceRejection, EditorWorkspace,
};

use crate::project_host::{
    authoring::{
        document::AuthoringDocumentSourceFact,
        recovery_storage::{
            BoundRecoveryPending, PreparedRecoveryStorage, RecoveryPendingPublication,
            RecoveryStorageError, RecoveryStorageHost, RecoveryStoragePublication, StoredRecovery,
        },
        source_inventory::{SourceRootObservation, release, with_temporary_storage},
    },
    file_publication::{FilePublicationEvidence, FilePublicationHost},
    persistence::{
        PersistenceLeafCommit, PersistenceLeafValidationCost, PrefabPersistenceHost,
        ScenePersistenceHost,
    },
};

use super::{
    physical::{
        ComponentApplyPhysicalError, ComponentApplyPhysicalInventory, capture_pending_inventory,
    },
    publication::{
        AcceptedComponentApplyOutput, ComponentApplyPublication, ComponentApplyPublicationError,
    },
    ready::{ComponentApplyReadyError, PreparedComponentApplyReady},
    recovery_data::{
        PrefabComponentApplyRecoveryData, PrefabComponentApplyRecoveryError,
        PrefabComponentApplyRecoveryInput, PrefabComponentApplyRecoveryLimits,
        RecoveryMetadataGuardInput, RecoveryOutputInput, RecoveryRootInput, RecoverySourceInput,
    },
    review::{ComponentApplyProjectReview, ComponentApplyProjectReviewError, open_sources},
    *,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ComponentApplyLiveError {
    #[error("the original project or saved sources changed")]
    Stale,
    #[error("the operation cannot advance in its current phase")]
    Phase,
    #[error("the accepted source could not bind to its original Workspace and persistence lane")]
    Workspace,
    #[error("recovery package publication was refused")]
    RecoveryFile,
    #[error("Pending manifest publication was refused")]
    PendingFile,
    #[error("a new recovery package identity could not be allocated")]
    RecoveryIdentity,
    #[error(transparent)]
    Review(#[from] ComponentApplyProjectReviewError),
    #[error(transparent)]
    Inventory(#[from] SourceInventoryError),
    #[error(transparent)]
    Storage(#[from] RecoveryStorageError),
    #[error(transparent)]
    Data(#[from] PrefabComponentApplyRecoveryError),
    #[error(transparent)]
    Physical(#[from] ComponentApplyPhysicalError),
    #[error(transparent)]
    Publication(#[from] ComponentApplyPublicationError),
    #[error(transparent)]
    Ready(#[from] ComponentApplyReadyError),
    #[error(transparent)]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error(transparent)]
    File(#[from] FsError),
    #[error("the original persistence leaf refused the accepted output: {0:?}")]
    Leaf(EditorPersistenceRejection),
}

#[derive(Default)]
enum Stage {
    #[default]
    Stopped,
    Prepare,
    Recovery(Box<PreparedRecoveryStorage<PrefabComponentApplyRecoveryData>>),
    Stored(Box<StoredRecovery<PrefabComponentApplyRecoveryData>>),
    Pending(Box<BoundRecoveryPending<PrefabComponentApplyRecoveryData>>),
    Outputs(Box<ComponentApplyPublication>),
}

pub(crate) struct ComponentApplyLivePublication {
    schema: Arc<AuthoringSchemaReceipt>,
    basis: AuthoringSchemaBasis,
    adapters: Arc<[AuthoringAdapterReceipt]>,
    limits: SceneFileLimits,
    budget: AuthoringProjectAdmissionBudget,
    inventory: Option<SourceInventory>,
    open: Vec<Option<AuthoringDocumentSourceFact>>,
    originals: [AuthoringDocumentSourceFact; 2],
    stage: Stage,
    pending: bool,
    _pending_publications: Option<[FilePublicationEvidence; 2]>,
    _uncertain_publication: Option<FilePublicationEvidence>,
    published: usize,
    ready: Option<PreparedComponentApplyReady>,
}

impl ComponentApplyProjectReview {
    pub(crate) fn begin_publication(
        self,
        project: &AuthoringProjectSessionSnapshot,
        open: Vec<Option<AuthoringDocumentSourceFact>>,
    ) -> Result<
        (
            ComponentApplyLivePublication,
            nara_tooling::PreparedEditorWorkspacePrefabComponentApply,
            [nara_tooling::EncodedEditorPersistenceCheckpoint; 2],
        ),
        ComponentApplyLiveError,
    > {
        if !Arc::ptr_eq(&self.schema, &project.schema) {
            return Err(ComponentApplyLiveError::Stale);
        }
        let pair = self.workspace.input();
        let original = |path: &str| {
            self.inventory
                .captured_open_sources()
                .find(|source| source.relative_path() == path)
                .cloned()
                .ok_or(ComponentApplyLiveError::Stale)
        };
        let originals = [
            original(pair.containing_binding().locator())?,
            original(pair.destination_binding().locator())?,
        ];
        let basis = pair.source_pair().schema_basis();
        Ok((
            ComponentApplyLivePublication {
                schema: self.schema,
                basis,
                adapters: Arc::clone(&project.adapters),
                limits: project.scene_limits,
                budget: self.budget,
                inventory: Some(self.inventory),
                open,
                originals,
                stage: Stage::Prepare,
                pending: false,
                _pending_publications: None,
                _uncertain_publication: None,
                published: 0,
                ready: None,
            },
            self.workspace,
            self.outputs,
        ))
    }
}

impl ComponentApplyLivePublication {
    pub(crate) const fn published(&self) -> usize {
        self.published
    }

    pub(crate) const fn requires_recovery(&self) -> bool {
        self.pending
    }

    pub(crate) const fn ready_to_install(&self) -> bool {
        self.ready.is_some()
    }

    pub(crate) fn into_cleanup_publisher(self) -> Option<FilePublicationHost> {
        match self.stage {
            Stage::Recovery(owner) => owner.into_cleanup_publisher(),
            Stage::Pending(owner) => owner.into_cleanup_publisher(),
            Stage::Outputs(owner) => owner.into_cleanup_publisher(),
            _ => None,
        }
    }

    pub(crate) fn advance(&mut self, bytes: [&[u8]; 2]) -> Result<(), ComponentApplyLiveError> {
        if matches!(self.stage, Stage::Pending(_)) {
            self.verify_original_inventory()?;
        }
        match std::mem::take(&mut self.stage) {
            Stage::Prepare => {
                let prepared = self.prepare_storage(bytes)?;
                self.stage = Stage::Recovery(Box::new(prepared));
            }
            Stage::Recovery(owner) => match owner.publish(&mut self.budget) {
                RecoveryStoragePublication::Published(stored) => {
                    self.stage = Stage::Stored(Box::new(stored));
                }
                RecoveryStoragePublication::Refused { prepared, error } => {
                    self.stage = Stage::Recovery(Box::new(prepared));
                    return Err(error.into());
                }
                RecoveryStoragePublication::Rejected { prepared, .. }
                | RecoveryStoragePublication::Failed { prepared, .. } => {
                    self.stage = Stage::Recovery(Box::new(prepared));
                    return Err(ComponentApplyLiveError::RecoveryFile);
                }
                RecoveryStoragePublication::Uncertain {
                    prepared,
                    publication,
                } => {
                    self._uncertain_publication = Some(publication);
                    self.stage = Stage::Recovery(Box::new(prepared));
                    return Err(ComponentApplyLiveError::RecoveryFile);
                }
            },
            Stage::Stored(owner) => match owner.bind_pending(&mut self.budget) {
                Ok(pending) => self.stage = Stage::Pending(Box::new(pending)),
                Err(failure) => {
                    self.stage = Stage::Stored(Box::new(failure.stored));
                    return Err(failure.error.into());
                }
            },
            Stage::Pending(owner) => match owner.publish(&mut self.budget) {
                RecoveryPendingPublication::Published(pending) => {
                    self.pending = true;
                    let (pending, publications) = pending.into_parts();
                    self._pending_publications = Some(publications);
                    let captured =
                        capture_pending_inventory(&pending, self.limits, &mut self.budget)?;
                    let original = self
                        .inventory
                        .take()
                        .ok_or(ComponentApplyLiveError::Phase)?;
                    verify_live_inventory(&original, &captured, &mut self.budget)?;
                    let bytes = original.retained_bytes();
                    let handles = original.retained_handle_count();
                    drop(original);
                    release(&mut self.budget, bytes)?;
                    self.budget.release_open_handles(handles);
                    self.stage = Stage::Outputs(Box::new(ComponentApplyPublication::new(
                        pending,
                        captured,
                        self.budget,
                    )));
                }
                RecoveryPendingPublication::Refused { pending, error } => {
                    self.stage = Stage::Pending(Box::new(pending));
                    return Err(error.into());
                }
                RecoveryPendingPublication::Rejected { pending, .. }
                | RecoveryPendingPublication::Failed { pending, .. } => {
                    self.stage = Stage::Pending(Box::new(pending));
                    return Err(ComponentApplyLiveError::PendingFile);
                }
                RecoveryPendingPublication::Uncertain {
                    pending,
                    publication,
                } => {
                    self.pending = true;
                    self._uncertain_publication = Some(publication);
                    self.stage = Stage::Pending(Box::new(pending));
                    return Err(ComponentApplyLiveError::PendingFile);
                }
            },
            Stage::Outputs(owner) => {
                self.stage = Stage::Outputs(owner);
                let Stage::Outputs(owner) = &mut self.stage else {
                    unreachable!()
                };
                match owner.advance()? {
                    Some(index) => self.published = index + 1,
                    None => {
                        self.ready = Some(owner.prepare_ready(|input, budget| {
                            Ok::<_, ComponentApplyLiveError>(PreparedComponentApplyReady::prepare(
                                &self.schema,
                                &self.basis,
                                Arc::clone(&self.adapters),
                                self.limits,
                                input,
                                budget,
                            )?)
                        })?);
                    }
                }
            }
            Stage::Stopped => return Err(ComponentApplyLiveError::Phase),
        }
        Ok(())
    }

    fn verify_original_inventory(&mut self) -> Result<(), ComponentApplyLiveError> {
        let inventory = self
            .inventory
            .as_ref()
            .ok_or(ComponentApplyLiveError::Phase)?;
        with_temporary_storage(&mut self.budget, |budget| {
            let scratch = checked_product(
                self.open.len(),
                size_of::<super::super::super::source_inventory::OpenSource<'_>>(),
            )?;
            budget.charge_source_bytes(0, 0, scratch, scratch)?;
            inventory.validate(&self.schema, &open_sources(&self.open), budget)?;
            Ok(())
        })
    }

    fn prepare_storage(
        &mut self,
        bytes: [&[u8]; 2],
    ) -> Result<PreparedRecoveryStorage<PrefabComponentApplyRecoveryData>, ComponentApplyLiveError>
    {
        let inventory = self
            .inventory
            .as_ref()
            .ok_or(ComponentApplyLiveError::Phase)?;
        let root = self.schema.project_root();
        let storage = RecoveryStorageHost::capture(root, &mut self.budget)?;
        if !self
            .schema
            .manifest
            .capability()
            .same_object(storage.ready_file())?
            || self.schema.manifest_parent_identity() != storage.ready_file().parent_identity()
        {
            return Err(ComponentApplyLiveError::Stale);
        }
        let count = checked_sum([
            checked_product(
                inventory.participants().len(),
                size_of::<RecoverySourceInput<'_>>(),
            )?,
            checked_product(
                inventory.metadata().len(),
                size_of::<RecoveryMetadataGuardInput<'_>>(),
            )?,
            checked_product(inventory.roots().len(), size_of::<RecoveryRootInput<'_>>())?,
            checked_product(inventory.missing_sidecars().len(), size_of::<&str>())?,
        ])?;
        self.budget.charge_source_bytes(0, 0, count, count)?;
        let recovery = {
            let sources: Vec<_> = inventory
                .participants()
                .map(|source| RecoverySourceInput {
                    path: source.file().path(),
                    key: source.source_key(),
                    encoding: source.encoding(),
                    explicit_native: source.explicit_native(),
                    original_disk: source.bytes(),
                })
                .collect();
            let metadata: Vec<_> = inventory
                .metadata()
                .map(|entry| RecoveryMetadataGuardInput {
                    path: entry.file().path(),
                    observed: entry.bytes(),
                })
                .collect();
            let roots: Vec<_> = inventory
                .roots()
                .map(|root| RecoveryRootInput {
                    path: root.path(),
                    missing_ancestor: match root.observation() {
                        SourceRootObservation::Available { .. } => None,
                        SourceRootObservation::Missing { ancestor_path, .. } => {
                            Some(ancestor_path.as_str())
                        }
                    },
                })
                .collect();
            let missing: Vec<_> = inventory.missing_sidecars().collect();
            PrefabComponentApplyRecoveryData::create(
                PrefabComponentApplyRecoveryInput {
                    ready_manifest: self.schema.manifest_bytes(),
                    bundle_path: self.schema.bundle_path().as_str(),
                    bundle_bytes: self.schema.bundle_bytes(),
                    source_universe: self.schema.settings().project.source_universe,
                    selected_profile: inventory.selected_profile(),
                    complete_original_sources: &sources,
                    containing: RecoveryOutputInput {
                        key: self.originals[0].source_key(),
                        output: bytes[0],
                    },
                    destination: RecoveryOutputInput {
                        key: self.originals[1].source_key(),
                        output: bytes[1],
                    },
                    metadata_guards: &metadata,
                    roots: &roots,
                    missing_sidecars: &missing,
                },
                PrefabComponentApplyRecoveryLimits::default(),
                &mut self.budget,
            )
        };
        release(&mut self.budget, count)?;
        let recovery = recovery?;
        let mut identity = [0_u8; 16];
        getrandom::fill(&mut identity).map_err(|_| ComponentApplyLiveError::RecoveryIdentity)?;
        let path = ProjectPath::new(format!(
            ".nara-component-apply-{:032x}.json",
            u128::from_ne_bytes(identity),
        ))
        .expect("fixed ASCII recovery leaf");
        storage
            .prepare(recovery, path, &mut self.budget)
            .map_err(|failure| failure.error.into())
    }

    pub(crate) fn install(
        &mut self,
        authority: &EditorPersistenceAuthority,
        workspace: &mut EditorWorkspace,
        prepared: nara_tooling::PreparedEditorWorkspacePrefabComponentApply,
        encoded: [nara_tooling::EncodedEditorPersistenceCheckpoint; 2],
        destinations: [ComponentApplyPersistenceDestination<'_>; 2],
    ) -> Result<
        (
            AuthoringProjectSessionSnapshot,
            [AuthoringDocumentSourceFact; 2],
        ),
        ComponentApplyLiveError,
    > {
        let ready = self.ready.as_ref().ok_or(ComponentApplyLiveError::Phase)?;
        let Stage::Outputs(owner) = &mut self.stage else {
            return Err(ComponentApplyLiveError::Phase);
        };
        let bounds = prepared.input();
        let documents = [
            bounds.containing_basis().document,
            bounds.destination_basis().document,
        ];
        let bindings = [
            bounds.containing_binding().clone(),
            bounds.destination_binding().clone(),
        ];
        let [first_destination, second_destination] = destinations;
        let [first_encoded, second_encoded] = encoded;
        let ((bound, first_leaf, second_leaf, sources), accepted) =
            owner.publish_ready(|input, budget| {
                ready.validate(budget)?;
                let (first_leaf, first_commit, first_source) = bind_output(
                    first_destination,
                    documents[0],
                    &bindings[0],
                    &self.originals[0],
                    first_encoded,
                    input.outputs[0],
                    budget,
                )?;
                let (second_leaf, second_commit, second_source) = bind_output(
                    second_destination,
                    documents[1],
                    &bindings[1],
                    &self.originals[1],
                    second_encoded,
                    input.outputs[1],
                    budget,
                )?;
                let bound = authority
                    .bind_prefab_component_apply(workspace, prepared, [first_commit, second_commit])
                    .map_err(|_| ComponentApplyLiveError::Workspace)?;
                Ok::<_, ComponentApplyLiveError>((
                    bound,
                    first_leaf,
                    second_leaf,
                    [first_source, second_source],
                ))
            })?;
        let project = self
            .ready
            .take()
            .expect("Ready retains its prepared Project")
            .install(accepted, owner.budget_mut());
        first_leaf.commit();
        second_leaf.commit();
        bound.commit();
        self.pending = false;
        Ok((project, sources))
    }
}

fn verify_live_inventory(
    original: &SourceInventory,
    current: &ComponentApplyPhysicalInventory,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ComponentApplyLiveError> {
    let current = current.inventory();
    current.validate_namespace_continuity(original, budget)?;
    budget.charge_source_bytes(0, 0, original.comparison_work()?, 0)?;
    if !original
        .participants()
        .map(|entry| entry.file())
        .eq(current.participants().map(|entry| entry.file()))
        || !original
            .metadata()
            .map(|entry| entry.file())
            .eq(current.metadata().map(|entry| entry.file()))
        || original.bundle_digest() != current.bundle_digest()
    {
        return Err(ComponentApplyLiveError::Stale);
    }
    Ok(())
}

pub(crate) enum ComponentApplyPersistenceDestination<'a> {
    Scene(EditorDocumentId, &'a mut ScenePersistenceHost),
    Prefab(EditorDocumentId, &'a mut PrefabPersistenceHost),
}

fn bind_output<'a>(
    destination: ComponentApplyPersistenceDestination<'a>,
    document: EditorDocumentId,
    binding: &nara_tooling::EditorDocumentSourceBinding,
    original: &AuthoringDocumentSourceFact,
    encoded: nara_tooling::EncodedEditorPersistenceCheckpoint,
    output: &AcceptedComponentApplyOutput,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<
    (
        PersistenceLeafCommit<'a>,
        EditorPersistenceCommit,
        AuthoringDocumentSourceFact,
    ),
    ComponentApplyLiveError,
> {
    if output.key() != original.source_key()
        || output.path() != original.relative_path()
        || binding.locator() != output.path()
        || encoded.digest() != output.digest()
    {
        return Err(ComponentApplyLiveError::Stale);
    }
    let digest = output.digest();
    let cost = PersistenceLeafValidationCost::checked(digest, output.path().len())
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    let accounted = checked_sum([
        cost.scratch_bytes,
        original
            .checked_retained_bytes()
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
    ])?;
    let work = checked_sum([cost.work_bytes, cost.encoded_bytes])?;
    let read = checked_product(cost.encoded_bytes, 2)?;
    budget.retain_open_handles(1)?;
    budget.admit_handle_peak(cost.transient_handles)?;
    budget.charge_source_bytes(read, 0, work, accounted)?;
    let relative = RelativePath::new(output.path()).map_err(|_| ComponentApplyLiveError::Stale)?;
    let filename = relative
        .components()
        .last()
        .ok_or(ComponentApplyLiveError::Stale)?;
    let file = output.parent().open_child_file(filename)?;
    if !output.file().same_object(&file)? {
        return Err(ComponentApplyLiveError::Stale);
    }
    let source = original
        .with_persistence_observation(file.identity(), file.parent_identity(), digest)
        .map_err(|_| ComponentApplyLiveError::Stale)?;
    let commit = EditorPersistenceCommit::from_observed_file(encoded, &file)
        .map_err(|_| ComponentApplyLiveError::Workspace)?;
    let expected = (
        original.identity(),
        original.parent_identity(),
        original.digest(),
    );
    let leaf = match destination {
        ComponentApplyPersistenceDestination::Scene(id, host) if id == document => {
            host.bind_published_leaf(expected, file, digest)
        }
        ComponentApplyPersistenceDestination::Prefab(id, host) if id == document => {
            host.bind_published_leaf(binding, expected, file, digest)
        }
        _ => return Err(ComponentApplyLiveError::Workspace),
    }
    .map_err(|failure| ComponentApplyLiveError::Leaf(failure.reason))?;
    release(budget, cost.scratch_bytes)?;
    Ok((leaf, commit, source))
}

impl From<AuthoringProjectSourceAdmissionError> for ComponentApplyLiveError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        ComponentApplyProjectReviewError::SourceCapture(error).into()
    }
}
