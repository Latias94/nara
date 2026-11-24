//! Deliver the two existing component Apply outputs and then their fixed Ready manifest.

use std::{mem::size_of, sync::Arc};

use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, RelativePath};
use nara_project::DEFAULT_MANIFEST_BYTE_LIMIT;
use nara_scene::SourceDocumentKey;

use super::{
    physical::{ComponentApplyPhysicalError, ComponentApplyPhysicalInventory},
    recovery_data::{PrefabComponentApplyRecoveryData, RecoverySource},
};
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
        AuthoringProjectAdmissionUsage, PROJECT_MANIFEST_PATH,
        publication_admission::with_cleanup_admission,
        recovery_storage::{ObservedPendingRecoveryStorage, RecoveryPackageData},
        source_inventory::{
            SourceInventoryError, product, release, reserve, revalidate_file, sum,
            with_candidate_storage,
        },
    },
    file_publication::{
        AcceptedFilePublication, FilePublicationEvidence, FilePublicationFailureStage,
        FilePublicationHost, FilePublicationOutcome, FilePublicationRejection,
    },
};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

const NATIVE_SCRATCH: usize = 2 * 65_536;
const HANDLE_PEAK: usize = 16;
const ACTIVE_HANDLES: usize = 3;

pub(super) struct AcceptedComponentApplyOutput {
    path: String,
    key: SourceDocumentKey,
    file: Arc<FileCapability>,
    parent: DirectoryCapability,
    digest: ContentDigest,
    evidence: Option<FilePublicationEvidence>,
}

impl AcceptedComponentApplyOutput {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn key(&self) -> &SourceDocumentKey {
        &self.key
    }

    pub(super) fn file(&self) -> &Arc<FileCapability> {
        &self.file
    }

    pub(super) fn parent(&self) -> &DirectoryCapability {
        &self.parent
    }

    pub(super) fn digest(&self) -> ContentDigest {
        self.digest
    }

    pub(super) fn evidence(&self) -> Option<&FilePublicationEvidence> {
        self.evidence.as_ref()
    }
}

pub(super) struct ComponentApplyReadyInput<'a> {
    pub(super) pending: &'a ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
    pub(super) physical: &'a ComponentApplyPhysicalInventory,
    pub(super) outputs: [&'a AcceptedComponentApplyOutput; 2],
}

pub(super) struct ComponentApplyPublication {
    pending: ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
    physical: ComponentApplyPhysicalInventory,
    budget: AuthoringProjectAdmissionBudget,
    accepted: [Option<AcceptedComponentApplyOutput>; 2],
    position: usize,
    active: Option<ActivePublication>,
    uncertain: Option<FilePublicationEvidence>,
    ready_published: bool,
}

impl std::fmt::Debug for ComponentApplyPublication {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ComponentApplyPublication")
            .field("accepted_outputs", &self.position)
            .field("pending", &self.is_pending())
            .field("cleanup_pending", &self.cleanup_pending())
            .field("uncertain", &self.uncertain())
            .finish_non_exhaustive()
    }
}

impl ComponentApplyPublication {
    pub(super) fn new(
        pending: ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
        physical: ComponentApplyPhysicalInventory,
        budget: AuthoringProjectAdmissionBudget,
    ) -> Self {
        Self {
            pending,
            physical,
            budget,
            accepted: [None, None],
            position: 0,
            active: None,
            uncertain: None,
            ready_published: false,
        }
    }

    pub(super) fn is_pending(&self) -> bool {
        !self.ready_published
    }

    pub(super) fn budget_mut(&mut self) -> &mut AuthoringProjectAdmissionBudget {
        &mut self.budget
    }

    pub(super) fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.budget.usage()
    }

    pub(super) fn accepted_output(&self, index: usize) -> Option<&AcceptedComponentApplyOutput> {
        self.accepted.get(index).and_then(Option::as_ref)
    }

    pub(super) fn cleanup_pending(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.publisher.has_cleanup())
    }

    pub(super) fn uncertain(&self) -> bool {
        self.uncertain.is_some()
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.publisher.uncertain())
    }

    pub(super) fn uncertain_evidence(&self) -> Option<&FilePublicationEvidence> {
        self.uncertain.as_ref()
    }

    pub(super) fn retry_cleanup(&mut self) -> Result<bool, ComponentApplyPublicationError> {
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.publisher.has_cleanup())
        else {
            return Ok(true);
        };
        Ok(with_cleanup_admission(&mut self.budget, || {
            active.publisher.retry_cleanup()
        })?)
    }

    #[cfg(feature = "tooling")]
    pub(super) fn into_cleanup_publisher(self) -> Option<FilePublicationHost> {
        self.active
            .map(|active| active.publisher)
            .filter(FilePublicationHost::has_cleanup)
    }

    pub(super) fn can_close(&self) -> bool {
        !self.cleanup_pending() && !self.uncertain()
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn close(self) -> Result<(), Self> {
        if !self.can_close() {
            return Err(self);
        }
        Ok(())
    }

    /// Accept one output at a time in containing/destination order.
    pub(super) fn advance(&mut self) -> Result<Option<usize>, ComponentApplyPublicationError> {
        self.require_pending()?;
        let index = self.position;
        if index == self.accepted.len() {
            return Ok(None);
        }
        let inventory = self.physical.inventory();
        let limit = inventory.scene_limits().encoded_bytes().get();
        let outputs = self.pending.recovery().outputs();
        if index == 0 {
            if outputs
                .iter()
                .any(|(_, output)| output.bytes().len() > limit)
            {
                return Err(ComponentApplyPublicationError::Failed(
                    FilePublicationFailureStage::PayloadTooLarge,
                ));
            }
            self.physical.validate(&self.pending, &mut self.budget)?;
        } else {
            self.physical
                .validate_authority(&self.pending, &mut self.budget)?;
        }

        let (member, output) = outputs[index];
        let path = member.path();
        let root = self.pending.root();
        let participant = inventory
            .participant(path)
            .ok_or(SourceInventoryError::SourceChanged)?;
        let captured = participant.file();
        let file = inventory
            .shared_file(path)
            .ok_or(SourceInventoryError::SourceChanged)?;
        let parent = inventory
            .shared_parent(path)
            .ok_or(SourceInventoryError::SourceChanged)?;
        let output_is_current = self.physical.output_is_current()[index];
        let digest = if output_is_current {
            output.digest()
        } else {
            member.original_disk().digest()
        };
        if participant.source_key() != member.key()
            || captured.identity() != file.identity()
            || captured.parent_identity() != Some(parent.identity())
            || captured.digest() != digest
        {
            return Err(SourceInventoryError::SourceChanged.into());
        }

        if !output_is_current && self.active.is_none() {
            self.active = Some(ActivePublication::bind(
                root,
                path,
                &file,
                &parent,
                digest,
                limit as u64,
                &mut self.budget,
            )?);
        }
        let result = with_candidate_storage(&mut self.budget, |budget| {
            reserve_output(member, !output_is_current, budget)?;
            let (file, digest, evidence) = if output_is_current {
                revalidate_file(root, captured, budget)?;
                (file, captured.digest(), None)
            } else {
                let active = self
                    .active
                    .as_mut()
                    .ok_or(ComponentApplyPublicationError::Phase)?;
                let accepted =
                    active.publish(root, path, output.bytes(), budget, &mut self.uncertain)?;
                let (evidence, source) = accepted.into_parts();
                let (file, accepted_digest) = source.into_parts();
                debug_assert_eq!(accepted_digest, output.digest());
                (Arc::new(file), accepted_digest, Some(evidence))
            };
            Ok::<_, ComponentApplyPublicationError>(AcceptedComponentApplyOutput {
                path: path.to_owned(),
                key: member.key().clone(),
                file,
                parent,
                digest,
                evidence,
            })
        })?;
        self.release_active();
        self.accepted[index] = Some(result);
        self.position += 1;
        Ok(Some(index))
    }

    /// Runs fallible Ready preparation against both accepted files and the continuing allowance.
    /// Successful preparation owns its new resource reservations.
    pub(super) fn prepare_ready<T, E>(
        &mut self,
        prepare: impl FnOnce(
            ComponentApplyReadyInput<'_>,
            &mut AuthoringProjectAdmissionBudget,
        ) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<ComponentApplyPublicationError>,
    {
        self.require_pending()?;
        let [Some(containing), Some(destination)] = &self.accepted else {
            return Err(ComponentApplyPublicationError::Phase.into());
        };
        let retained = self.budget.retained_candidate_bytes;
        let handles = self.budget.retained_open_handles;
        let result = prepare(
            ComponentApplyReadyInput {
                pending: &self.pending,
                physical: &self.physical,
                outputs: [containing, destination],
            },
            &mut self.budget,
        );
        if result.is_err() {
            self.budget.retained_candidate_bytes = retained;
            self.budget.retained_open_handles = handles;
        }
        result
    }

    /// Binds caller-owned Ready state, then reobserves the saved generation before publication.
    pub(super) fn publish_ready<T, E>(
        &mut self,
        prepare: impl FnOnce(
            ComponentApplyReadyInput<'_>,
            &mut AuthoringProjectAdmissionBudget,
        ) -> Result<T, E>,
    ) -> Result<(T, AcceptedFilePublication), E>
    where
        E: From<ComponentApplyPublicationError>,
    {
        self.require_pending()?;
        if self.position != self.accepted.len() {
            return Err(ComponentApplyPublicationError::Phase.into());
        }
        if self.active.is_none() {
            self.active = Some(ActivePublication::bind(
                self.pending.root(),
                PROJECT_MANIFEST_PATH,
                self.pending.capability(),
                self.pending.root(),
                self.pending.digest(),
                DEFAULT_MANIFEST_BYTE_LIMIT,
                &mut self.budget,
            )?);
        }
        let retained = self.budget.retained_candidate_bytes;
        let handles = self.budget.retained_open_handles;
        let prepared = self.prepare_ready(prepare)?;
        match self.publish_ready_manifest() {
            Ok(publication) => Ok((prepared, publication)),
            Err(error) => {
                drop(prepared);
                self.budget.retained_candidate_bytes = retained;
                self.budget.retained_open_handles = handles;
                Err(error.into())
            }
        }
    }

    fn publish_ready_manifest(
        &mut self,
    ) -> Result<AcceptedFilePublication, ComponentApplyPublicationError> {
        let [Some(containing), Some(destination)] = &self.accepted else {
            return Err(ComponentApplyPublicationError::Phase);
        };
        self.physical.inventory().validate_admitted_replacements(
            self.physical.observation_input(&self.pending),
            [containing, destination]
                .map(|output| (output.path(), output.file().as_ref(), output.digest())),
            &mut self.budget,
        )?;
        self.physical
            .validate_authority(&self.pending, &mut self.budget)?;
        let active = self
            .active
            .as_mut()
            .ok_or(ComponentApplyPublicationError::Phase)?;
        let accepted = with_candidate_storage(&mut self.budget, |budget| {
            active.publish(
                self.pending.root(),
                PROJECT_MANIFEST_PATH,
                self.pending.recovery().final_ready_manifest(),
                budget,
                &mut self.uncertain,
            )
        })?;
        self.ready_published = true;
        self.release_active();
        Ok(accepted)
    }

    fn require_pending(&self) -> Result<(), ComponentApplyPublicationError> {
        if self.uncertain() {
            return Err(ComponentApplyPublicationError::Uncertain);
        }
        if self.cleanup_pending() {
            return Err(ComponentApplyPublicationError::CleanupPending);
        }
        if !self.is_pending() {
            return Err(ComponentApplyPublicationError::Phase);
        }
        Ok(())
    }

    fn release_active(&mut self) {
        if let Some(active) = self.active.take() {
            let retained = active.retained_bytes;
            drop(active);
            self.budget.release_open_handles(ACTIVE_HANDLES);
            self.budget
                .reconcile_retained_reservation(retained, 0)
                .expect("retained component Apply publisher reservation");
        }
    }
}

struct ActivePublication {
    publisher: FilePublicationHost,
    parent_path: Option<RelativePath>,
    retained_bytes: usize,
}

impl ActivePublication {
    #[allow(clippy::too_many_arguments)]
    fn bind(
        root: &DirectoryCapability,
        path: &str,
        file: &FileCapability,
        parent: &DirectoryCapability,
        digest: ContentDigest,
        limit: u64,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, ComponentApplyPublicationError> {
        with_candidate_storage(budget, |budget| {
            let retained_bytes = sum(size_of::<Self>() + 4096, product(path.len(), 4)?)?;
            reserve(budget, retained_bytes)?;
            budget.retain_open_handles(ACTIVE_HANDLES)?;
            let parent_path = path
                .rsplit_once('/')
                .map(|(parent, _)| RelativePath::new(parent))
                .transpose()
                .map_err(|_| SourceInventoryError::SourceChanged)?;
            let bytes = usize::try_from(digest.length())
                .map_err(|_| SourceInventoryError::AccountingOverflow)?;
            let scratch = sum(NATIVE_SCRATCH, product(path.len(), 4)?)?;
            budget.charge_open_file(HANDLE_PEAK)?;
            budget.charge_reference(path.len(), path_depth(path), false)?;
            budget.charge_source_bytes(bytes, 0, sum(bytes, 1)?, scratch)?;
            let publisher = FilePublicationHost::bind_existing(root, path, file, digest, limit)
                .map_err(ComponentApplyPublicationError::Failed)?;
            release(budget, scratch)?;
            if Some(publisher.parent_identity()) != file.parent_identity()
                || publisher.parent_identity() != parent.identity()
            {
                return Err(SourceInventoryError::SourceChanged.into());
            }
            Ok(Self {
                publisher,
                parent_path,
                retained_bytes,
            })
        })
    }

    fn publish(
        &mut self,
        root: &DirectoryCapability,
        path: &str,
        bytes: &[u8],
        budget: &mut AuthoringProjectAdmissionBudget,
        uncertain: &mut Option<FilePublicationEvidence>,
    ) -> Result<AcceptedFilePublication, ComponentApplyPublicationError> {
        let cost = self
            .publisher
            .binding()
            .checked_content_cost(bytes.len())
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        let path_work = product(path.len(), 4)?;
        let scratch = sum(NATIVE_SCRATCH, sum(bytes.len(), path_work)?)?;
        for _ in 0..2 {
            budget.charge_open_file(HANDLE_PEAK)?;
            budget.charge_reference(path.len(), path_depth(path), false)?;
        }
        budget.retain_open_handles(1)?;
        budget.charge_source_bytes(
            cost.encoded_read_bytes,
            0,
            sum(cost.content_work_bytes, path_work)?,
            scratch,
        )?;
        let parent_path = self.parent_path.as_ref();
        let current_parent = |expected: &DirectoryCapability| match parent_path {
            Some(path) => root.open_directory_exact(path).is_ok_and(|current| {
                current.identity() == expected.identity()
                    && current.same_object(expected).unwrap_or(false)
            }),
            None => {
                root.identity() == expected.identity()
                    && root.same_object(expected).unwrap_or(false)
            }
        };
        let outcome = self.publisher.publish_guarded(bytes, |parent, _| {
            if !current_parent(parent) {
                return Err(FilePublicationRejection::SourceBindingMismatch);
            }
            Ok(current_parent)
        });
        budget
            .reconcile_retained_reservation(scratch, 0)
            .expect("reserved component Apply publication scratch");
        match outcome {
            FilePublicationOutcome::Published(accepted) => Ok(accepted),
            FilePublicationOutcome::Rejected(reason) => {
                Err(ComponentApplyPublicationError::Rejected(reason))
            }
            FilePublicationOutcome::Failed(stage) => {
                Err(ComponentApplyPublicationError::Failed(stage))
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                *uncertain = Some(evidence);
                Err(ComponentApplyPublicationError::Uncertain)
            }
        }
    }
}

fn path_depth(path: &str) -> usize {
    path.bytes().filter(|&byte| byte == b'/').count()
}

fn reserve_output(
    member: &RecoverySource,
    needs_file: bool,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceInventoryError> {
    let path_and_key = sum(member.path().len(), member.key().retained_bytes())?;
    let file = usize::from(needs_file) * (size_of::<FileCapability>() + 2 * size_of::<usize>());
    let storage = sum(file, path_and_key)?;
    budget.charge_source_bytes(0, 0, path_and_key, storage)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ComponentApplyPublicationError {
    #[error("component Apply physical authority changed")]
    Physical(#[from] ComponentApplyPhysicalError),
    #[error("component Apply source inventory changed")]
    Inventory(#[from] SourceInventoryError),
    #[error("component Apply publication exceeded its allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("component Apply could not publish an existing leaf at {0:?}")]
    Failed(FilePublicationFailureStage),
    #[error("component Apply publication was rejected: {0:?}")]
    Rejected(FilePublicationRejection),
    #[error("component Apply has an unresolved publication outcome")]
    Uncertain,
    #[error("component Apply retains temporary-file cleanup")]
    CleanupPending,
    #[error("component Apply is not at the requested Pending publication step")]
    Phase,
}
