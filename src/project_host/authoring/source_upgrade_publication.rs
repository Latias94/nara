//! Reviewed or freshly recovered source/bundle publication under confirmed Pending storage.
//!
//! Each step owns at most one file publisher. The original inventory's shared custody keeps
//! every expected leaf and ancestor alive. Only replacements performed by this owner retain
//! historical publication evidence; already-current outputs receive a fresh read observation.
//! Final saved observations read the actual resulting names. Complete membership belongs to
//! the inventory owner.
//! No source conversion, Ready publication or Workspace installation happens here.

use std::{mem::size_of, sync::Arc};

use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, RelativeComponent,
    RelativePath,
};
use nara_project::ProjectAuthoringRecoveryDigest;
use nara_scene::{SceneFileLimits, SourceDocumentKey};

#[cfg(feature = "tooling")]
use super::project::source_upgrade::PreparedAuthoredSourceUpgrade;
use super::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError, AuthoringSchemaReceipt,
    document::{AuthoringDocumentAdmissionError, AuthoringDocumentSourceFact},
    source_upgrade_inventory::{
        RecoveredSourceUpgradeInventory, SharedSourceUpgradeInventoryCustody,
        SourceUpgradeInventory, SourceUpgradeInventoryError,
    },
    source_upgrade_ready::RETAINED_HANDLE_CAPACITY as READY_HANDLE_CAPACITY,
    source_upgrade_recovery::SourceUpgradeRecovery,
    source_upgrade_storage::{
        ObservedPendingRecoveryStorage, PENDING_HANDLE_CAPACITY,
        RETAINED_HANDLE_CAPACITY as STORAGE_HANDLE_CAPACITY, RecoveryStorageError,
    },
};
#[cfg(test)]
use crate::project_host::file_publication::CleanupTestControl;
use crate::project_host::file_publication::{
    FilePublicationBinding, FilePublicationEvidence, FilePublicationFailureStage,
    FilePublicationHost, FilePublicationOutcome, FilePublicationRejection,
};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

const NATIVE_SCRATCH: usize = 2 * 65_536;
const HANDLE_PEAK: usize = 16;
// One observed source and at most one failed temporary-file cleanup can outlive a step.
// The parent shares the inventory's retained handle. Reserve both slots before binding.
const ACTIVE_HANDLE_RESERVATION: usize = 2;

/// Actual current inputs and stored deterministic outputs. Bodies are immutable and shared
/// with the recovery storage owner; no second serialized or decoded package is retained.
pub(super) struct SourceUpgradePublication {
    root: DirectoryCapability,
    bundle: BundleCustody,
    originals: SharedSourceUpgradeInventoryCustody,
    recovery: Arc<SourceUpgradeRecovery>,
    entries: Vec<Entry>,
    next: usize,
    active: Option<FilePublicationHost>,
    active_retained: usize,
    #[cfg(test)]
    fail_next_write: Option<CleanupTestControl>,
}

/// A live review keeps its admitted receipt; recovery pins only its freshly observed leaf.
/// Neither sharing path duplicates a native handle or turns an observation into a receipt.
enum BundleCustody {
    Live(Arc<AuthoringSchemaReceipt>),
    Recovered(Arc<FileCapability>),
}

impl BundleCustody {
    fn file(&self) -> &FileCapability {
        match self {
            Self::Live(schema) => schema.bundle.capability(),
            Self::Recovered(file) => file,
        }
    }
}

struct Entry {
    expected: Observation,
    source: Option<AuthoringDocumentSourceFact>,
    state: EntryState,
    byte_limit: u64,
}

#[derive(Clone, Copy)]
struct Observation {
    identity: FileIdentity,
    parent: Option<FileIdentity>,
    digest: ContentDigest,
}

enum EntryState {
    Pending,
    /// A successful replacement is historical evidence, kept separately from a fresh read.
    Published(FilePublicationEvidence),
    Unchanged,
}

impl SourceUpgradePublication {
    /// The source owner has already proved all proposed values/claims and exact membership.
    /// Confirmation under the live Workspace remains the enclosing operation's responsibility.
    #[cfg(feature = "tooling")]
    pub(super) fn prepare(
        reviewed: &PreparedAuthoredSourceUpgrade,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradePublicationError> {
        with_candidate_storage(budget, |budget| {
            let recovery = reviewed.recovery();
            check_handle_capacity(
                reviewed.inventory(),
                reviewed.workspace().documents().len(),
                PENDING_HANDLE_CAPACITY,
                budget,
            )?;
            let mut entries = prepare_source_entries(
                reviewed.inventory(),
                recovery,
                |index| reviewed.source_file_limits(index),
                budget,
            )?;
            let schema = reviewed.schema();
            entries.push(Entry {
                expected: Observation {
                    identity: schema.bundle_identity(),
                    parent: schema.bundle_parent_identity(),
                    digest: schema.bundle_digest(),
                },
                source: None,
                state: EntryState::Pending,
                byte_limit: nara_reflect::AuthoringSchemaBundleLimits::default()
                    .encoded_bytes()
                    .get() as u64,
            });
            Ok(Self {
                root: schema.project_root(),
                bundle: BundleCustody::Live(Arc::clone(schema)),
                originals: reviewed.inventory().shared_custody(),
                recovery: reviewed.shared_recovery(),
                entries,
                next: 0,
                active: None,
                active_retained: 0,
                #[cfg(test)]
                fail_next_write: None,
            })
        })
    }

    /// Joins an authenticated mixed-generation inventory to its actual Pending observation.
    /// Revalidates that complete inventory under the same operation's cumulative allowance.
    /// Recovery has no open Workspace or original live receipt. Every entry starts from the
    /// current pinned leaf: old members replace from stored bytes, while new/identical members
    /// are only observed by the same publication kernel.
    pub(super) fn prepare_recovered(
        pending: &ObservedPendingRecoveryStorage,
        recovered: &RecoveredSourceUpgradeInventory,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradePublicationError> {
        with_candidate_storage(budget, |budget| {
            recovered
                .validate(pending, budget)
                .map_err(|error| match error {
                    SourceUpgradeInventoryError::Admission(error) => {
                        SourceUpgradePublicationError::Admission(error)
                    }
                    error => SourceUpgradePublicationError::Inventory(error),
                })?;
            let inventory = recovered.inventory();
            check_handle_capacity(inventory, 0, 0, budget)?;
            let mut entries = prepare_source_entries(
                inventory,
                pending.recovery(),
                |_| Some(inventory.scene_limits()),
                budget,
            )?;
            let bundle_file = recovered.bundle_file();
            entries.push(Entry {
                expected: Observation {
                    identity: bundle_file.identity(),
                    parent: bundle_file.parent_identity(),
                    digest: recovered.bundle_digest(),
                },
                source: None,
                state: EntryState::Pending,
                byte_limit: nara_reflect::AuthoringSchemaBundleLimits::default()
                    .encoded_bytes()
                    .get() as u64,
            });
            Ok(Self {
                root: pending.root().share(),
                bundle: BundleCustody::Recovered(Arc::clone(bundle_file)),
                originals: inventory.shared_custody(),
                recovery: pending.shared_recovery(),
                entries,
                next: 0,
                active: None,
                active_retained: 0,
                #[cfg(test)]
                fail_next_write: None,
            })
        })
    }

    pub(super) fn operation(&self) -> ProjectAuthoringRecoveryDigest {
        self.recovery.digest()
    }
    pub(super) fn completed(&self) -> usize {
        self.next
    }
    pub(super) fn is_complete(&self) -> bool {
        self.next == self.entries.len()
    }

    pub(super) fn publication(&self, index: usize) -> Option<&FilePublicationEvidence> {
        match &self.entries.get(index)?.state {
            EntryState::Published(evidence) => Some(evidence),
            _ => None,
        }
    }

    pub(super) fn cleanup_pending(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(FilePublicationHost::has_cleanup)
    }
    pub(super) fn uncertain(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(FilePublicationHost::uncertain)
    }
    pub(super) fn retry_cleanup(&mut self) -> bool {
        self.active
            .as_mut()
            .is_none_or(FilePublicationHost::retry_cleanup)
    }

    #[cfg(test)]
    pub(super) fn test_fail_next_write(&mut self) -> CleanupTestControl {
        let control = CleanupTestControl::blocked();
        self.fail_next_write = Some(control.clone());
        control
    }

    /// Advances one exact source, then the bundle. A refusal never advances the cursor or
    /// discards a publisher with pending cleanup/uncertain evidence. The enclosing operation
    /// keeps old Session writes fenced after any confirmed Pending transition.
    pub(super) fn publish_next(
        &mut self,
        pending: &ObservedPendingRecoveryStorage,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> SourceUpgradePublicationStep {
        let index = self.next;
        if self.is_complete() {
            return SourceUpgradePublicationStep::Complete;
        }
        if let Err(error) = self.validate_pending(pending, budget) {
            return SourceUpgradePublicationStep::Refused { index, error };
        }
        let (path, bytes, target) = member_output(&self.recovery, index);
        let expected = self.entries[index].expected;
        let original = if index < self.recovery.members().len() {
            self.originals.file(path)
        } else {
            Some(self.bundle.file())
        };
        let Some(original) = original else {
            return SourceUpgradePublicationStep::Refused {
                index,
                error: SourceUpgradePublicationError::SourceChanged,
            };
        };
        let Some(parent) = self.originals.parent(path) else {
            return SourceUpgradePublicationStep::Refused {
                index,
                error: SourceUpgradePublicationError::SourceChanged,
            };
        };
        if expected.digest == target {
            let result = verify_named(&self.root, path, original, parent, expected, budget);
            return match result {
                Ok(file) => {
                    drop(file);
                    self.entries[index].state = EntryState::Unchanged;
                    self.next += 1;
                    SourceUpgradePublicationStep::Observed { index }
                }
                Err(error) => SourceUpgradePublicationStep::Refused { index, error },
            };
        }
        if self.active.is_none() {
            let retained_before = budget.retained_candidate_bytes;
            let handles_before = budget.retained_open_handles;
            let active_retained = match mul(path.len(), 4)
                .and_then(|bytes| add(size_of::<FilePublicationHost>(), bytes))
            {
                Ok(bytes) => bytes,
                Err(error) => return SourceUpgradePublicationStep::Refused { index, error },
            };
            let result = with_path(path, budget, |budget| {
                budget.retain_open_handles(ACTIVE_HANDLE_RESERVATION)?;
                let old = verify_named(&self.root, path, original, parent, expected, budget)?;
                let length = usize::try_from(expected.digest.length())
                    .map_err(|_| SourceUpgradePublicationError::Accounting)?;
                budget.charge_source_bytes(0, 0, add(length, 1)?, active_retained)?;
                let target = RelativeComponent::new(path.rsplit('/').next().unwrap_or(path))
                    .map_err(|_| SourceUpgradePublicationError::SourceChanged)?;
                let publisher = FilePublicationHost::from_existing(
                    parent.share(),
                    target,
                    old,
                    expected.digest,
                    self.entries[index].byte_limit,
                );
                if Some(publisher.parent_identity()) != expected.parent {
                    return Err(SourceUpgradePublicationError::SourceChanged);
                }
                Ok(publisher)
            });
            match result {
                Ok(publisher) => {
                    self.active = Some(publisher);
                    self.active_retained = active_retained;
                }
                Err(error) => {
                    let released = budget
                        .retained_candidate_bytes
                        .checked_sub(retained_before)
                        .expect("failed binding retains no publisher");
                    release(budget, released);
                    let closed = budget
                        .retained_open_handles
                        .checked_sub(handles_before)
                        .expect("failed binding has dropped its native handles");
                    budget.release_open_handles(closed);
                    return SourceUpgradePublicationStep::Refused { index, error };
                }
            }
        }
        if let Err(error) = verify_named(&self.root, path, original, parent, expected, budget) {
            return SourceUpgradePublicationStep::Refused { index, error };
        }
        let reservation = match reserve_publication(expected, bytes.len(), path, budget) {
            Ok(reservation) => reservation,
            Err(error) => return SourceUpgradePublicationStep::Refused { index, error },
        };
        #[cfg(test)]
        if let Some(control) = self.fail_next_write.take() {
            self.active
                .as_mut()
                .expect("the next step retains its publisher")
                .test_fail_after_write_with_control(&control);
        }
        let outcome = self
            .active
            .as_mut()
            .expect("this step owns its publisher")
            .publish(bytes);
        release(budget, reservation);
        match outcome {
            FilePublicationOutcome::Published(accepted) => {
                let (evidence, source) = accepted.into_parts();
                self.entries[index].state = EntryState::Published(evidence);
                // The inventory continues to pin originals. Output evidence is historical;
                // a later current saved observation is established by its own actual read.
                drop(source);
                drop(self.active.take());
                budget.release_open_handles(ACTIVE_HANDLE_RESERVATION);
                release(budget, self.active_retained);
                self.active_retained = 0;
                self.next += 1;
                SourceUpgradePublicationStep::Published { index }
            }
            FilePublicationOutcome::Rejected(reason) => {
                SourceUpgradePublicationStep::Rejected { index, reason }
            }
            FilePublicationOutcome::Failed(stage) => {
                SourceUpgradePublicationStep::Failed { index, stage }
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                SourceUpgradePublicationStep::Uncertain { index, evidence }
            }
        }
    }

    fn validate_pending(
        &self,
        pending: &ObservedPendingRecoveryStorage,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradePublicationError> {
        if self.operation() != pending.recovery().digest()
            || self.root.identity() != pending.root().identity()
        {
            return Err(SourceUpgradePublicationError::Operation);
        }
        pending
            .validate(budget)
            .map_err(SourceUpgradePublicationError::Pending)
    }

    /// Rechecks the actual outputs individually. Complete source/metadata/root membership is
    /// deliberately a separate proof from `VerifiedSourceUpgradeInventory`.
    pub(super) fn validate_complete(
        &self,
        pending: &ObservedPendingRecoveryStorage,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradePublicationError> {
        self.validate_pending(pending, budget)?;
        if !self.is_complete() {
            return Err(SourceUpgradePublicationError::Incomplete);
        }
        for (index, _) in self.entries.iter().enumerate() {
            let (path, _, digest) = member_output(&self.recovery, index);
            drop(observe_output(&self.root, path, digest, budget)?);
            budget.release_open_handles(1);
        }
        Ok(())
    }

    /// Opens a real current leaf for the pre-Ready persistence guard. This is a read observation,
    /// distinct from the independently retained successful replacement evidence.
    pub(super) fn observe_source(
        &self,
        index: usize,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<ObservedSourceUpgradeFile, SourceUpgradePublicationError> {
        with_candidate_storage(budget, |budget| {
            if !self.is_complete() {
                return Err(SourceUpgradePublicationError::Incomplete);
            }
            let entry = self
                .entries
                .get(index)
                .ok_or(SourceUpgradePublicationError::Incomplete)?;
            let original = entry
                .source
                .as_ref()
                .ok_or(SourceUpgradePublicationError::SourceKind)?;
            let (path, _, digest) = member_output(&self.recovery, index);
            let file = observe_output(&self.root, path, digest, budget)?;
            let retained = original
                .checked_retained_bytes()
                .ok_or(SourceUpgradePublicationError::Accounting)?;
            budget.charge_source_bytes(
                0,
                0,
                retained,
                add(retained, size_of::<ObservedSourceUpgradeFile>())?,
            )?;
            let source = original
                .with_persistence_observation(file.identity(), file.parent_identity(), digest)
                .map_err(SourceUpgradePublicationError::Source)?;
            Ok(ObservedSourceUpgradeFile { file, source })
        })
    }
}

pub(super) struct ObservedSourceUpgradeFile {
    pub(super) file: FileCapability,
    pub(super) source: AuthoringDocumentSourceFact,
}

impl ObservedSourceUpgradeFile {
    pub(super) fn source_key(&self) -> &SourceDocumentKey {
        self.source.source_key()
    }
}

#[must_use = "keep the owner, actual file outcomes and any pending cleanup for reconciliation"]
pub(super) enum SourceUpgradePublicationStep {
    Complete,
    Observed {
        index: usize,
    },
    Published {
        index: usize,
    },
    Refused {
        index: usize,
        error: SourceUpgradePublicationError,
    },
    Rejected {
        index: usize,
        reason: FilePublicationRejection,
    },
    Failed {
        index: usize,
        stage: FilePublicationFailureStage,
    },
    Uncertain {
        index: usize,
        evidence: FilePublicationEvidence,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceUpgradePublicationError {
    #[error("the actual Pending observation belongs to another operation or capability root")]
    Operation,
    #[error("some reviewed sources or the bundle have not been published")]
    Incomplete,
    #[error("this publication member is not a Scene or Prefab source")]
    SourceKind,
    #[error("the observed file name, parent, identity or content changed")]
    SourceChanged,
    #[error("source publication exceeded its shared admission allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("source publication accounting overflowed")]
    Accounting,
    #[error("native document identity admission failed")]
    Source(#[source] AuthoringDocumentAdmissionError),
    #[error("the Pending operation could not be revalidated")]
    Pending(#[source] RecoveryStorageError),
    #[error("the recovered source inventory does not match the Pending operation")]
    Inventory(#[source] SourceUpgradeInventoryError),
}

fn check_handle_capacity(
    inventory: &SourceUpgradeInventory,
    open_documents: usize,
    additional_pending_handles: usize,
    budget: &AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradePublicationError> {
    // Before publication, check the known final overlap: current + verified + confirming
    // inventories, open replacement leaves and the storage/Ready owners. Recovery has no
    // open documents and its actual Pending handles are already admitted. Future handles are
    // not opened, reserved or recorded as an observed peak.
    let final_stage = add(
        mul(inventory.retained_handle_count(), 2)?,
        add(
            open_documents,
            add(
                additional_pending_handles,
                add(READY_HANDLE_CAPACITY, HANDLE_PEAK)?,
            )?,
        )?,
    )?;
    let storage_stage = add(STORAGE_HANDLE_CAPACITY, HANDLE_PEAK)?;
    let source_stage = add(
        additional_pending_handles,
        add(ACTIVE_HANDLE_RESERVATION, HANDLE_PEAK)?,
    )?;
    budget.check_handle_capacity(final_stage.max(storage_stage).max(source_stage))?;
    Ok(())
}

fn prepare_source_entries(
    inventory: &SourceUpgradeInventory,
    recovery: &SourceUpgradeRecovery,
    file_limits: impl Fn(usize) -> Option<SceneFileLimits>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<Entry>, SourceUpgradePublicationError> {
    if inventory.participants().len() != recovery.members().len() {
        return Err(SourceUpgradePublicationError::SourceChanged);
    }
    let count = add(recovery.members().len(), 1)?;
    budget.charge_source_bytes(
        0,
        0,
        count,
        add(
            size_of::<SourceUpgradePublication>(),
            mul(count, size_of::<Entry>())?,
        )?,
    )?;
    let mut entries = Vec::with_capacity(count);
    for (index, participant) in inventory.participants().enumerate() {
        let file = participant.file();
        let native = participant.native_source();
        let retained = add(
            size_of::<AuthoringDocumentSourceFact>(),
            add(
                file.path().len(),
                native
                    .retained_bytes()
                    .ok_or(SourceUpgradePublicationError::Accounting)?,
            )?,
        )?;
        budget.charge_source_bytes(0, 0, retained, retained)?;
        let source = AuthoringDocumentSourceFact::from_native_observation(
            file.path(),
            file.identity(),
            file.parent_identity(),
            file.digest(),
            native,
        )
        .map_err(SourceUpgradePublicationError::Source)?;
        entries.push(Entry {
            expected: Observation {
                identity: file.identity(),
                parent: file.parent_identity(),
                digest: file.digest(),
            },
            source: Some(source),
            state: EntryState::Pending,
            byte_limit: file_limits(index)
                .ok_or(SourceUpgradePublicationError::SourceKind)?
                .encoded_bytes()
                .get() as u64,
        });
    }
    Ok(entries)
}

fn member_output(recovery: &SourceUpgradeRecovery, index: usize) -> (&str, &[u8], ContentDigest) {
    let (path, bytes) = match recovery.members().get(index) {
        Some(member) => (member.path.as_str(), &member.successor),
        None => (
            recovery.bundle().path.as_str(),
            &recovery.bundle().successor,
        ),
    };
    let digest = bytes.digest();
    (
        path,
        bytes.bytes(),
        ContentDigest::from_parts(digest.length(), *digest.as_bytes()),
    )
}

fn verify_named(
    root: &DirectoryCapability,
    path: &str,
    original: &FileCapability,
    parent: &DirectoryCapability,
    expected: Observation,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<FileCapability, SourceUpgradePublicationError> {
    with_path(path, budget, |budget| {
        let length = usize::try_from(expected.digest.length())
            .map_err(|_| SourceUpgradePublicationError::Accounting)?;
        budget.charge_source_bytes(length, 0, add(length, 1)?, 0)?;
        let relative =
            RelativePath::new(path).map_err(|_| SourceUpgradePublicationError::SourceChanged)?;
        let file = root
            .open_file_exact(&relative)
            .map_err(|_| SourceUpgradePublicationError::SourceChanged)?;
        if original.identity() != expected.identity
            || Some(parent.identity()) != expected.parent
            || !file.same_object(original).unwrap_or(false)
            || file.parent_identity() != expected.parent
            || file
                .verify_digest(expected.digest, expected.digest.length())
                .is_err()
        {
            return Err(SourceUpgradePublicationError::SourceChanged);
        }
        Ok(file)
    })
}

/// A fresh current content observation. No historical FileIdentity is compared after its
/// original leaf custody ended; the returned live capability supplies the new observation.
fn observe_output(
    root: &DirectoryCapability,
    path: &str,
    digest: ContentDigest,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<FileCapability, SourceUpgradePublicationError> {
    with_candidate_storage(budget, |budget| {
        with_path(path, budget, |budget| {
            let length = usize::try_from(digest.length())
                .map_err(|_| SourceUpgradePublicationError::Accounting)?;
            budget.charge_source_bytes(length, 0, add(length, 1)?, 0)?;
            let relative = RelativePath::new(path)
                .map_err(|_| SourceUpgradePublicationError::SourceChanged)?;
            budget.retain_open_handles(1)?;
            let file = root
                .open_file_exact(&relative)
                .map_err(|_| SourceUpgradePublicationError::SourceChanged)?;
            if file.verify_digest(digest, digest.length()).is_err() {
                return Err(SourceUpgradePublicationError::SourceChanged);
            }
            Ok(file)
        })
    })
}

fn with_path<T>(
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
    operation: impl FnOnce(
        &mut AuthoringProjectAdmissionBudget,
    ) -> Result<T, SourceUpgradePublicationError>,
) -> Result<T, SourceUpgradePublicationError> {
    budget.charge_source_bytes(0, 0, path.len(), 0)?;
    let path_shape =
        RelativePath::preflight(path).map_err(|_| SourceUpgradePublicationError::SourceChanged)?;
    budget.charge_reference(path.len(), path_shape.components(), true)?;
    budget.admit_handle_peak(HANDLE_PEAK)?;
    let scratch = add(
        NATIVE_SCRATCH,
        add(mul(path.len(), 4)?, mul(path_shape.components(), 128)?)?,
    )?;
    let before_scratch = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, 0, scratch) {
        let refused = budget
            .retained_candidate_bytes
            .checked_sub(before_scratch)
            .expect("a refused scratch admission retains no allocation");
        release(budget, refused);
        return Err(error.into());
    }
    let result = operation(budget);
    release(budget, scratch);
    result
}

fn reserve_publication(
    expected: Observation,
    new: usize,
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<usize, SourceUpgradePublicationError> {
    with_candidate_storage(budget, |budget| {
        let cost = FilePublicationBinding::Existing {
            identity: expected.identity,
            digest: expected.digest,
        }
        .checked_content_cost(new)
        .ok_or(SourceUpgradePublicationError::Accounting)?;
        let work = add(cost.content_work_bytes, add(path.len(), 1024)?)?;
        let reservation = add(NATIVE_SCRATCH, add(new, add(mul(path.len(), 4)?, 4096)?)?)?;
        budget.charge_open_file(HANDLE_PEAK)?;
        budget.charge_source_bytes(cost.encoded_read_bytes, 0, work, reservation)?;
        Ok(reservation)
    })
}

fn with_candidate_storage<T>(
    budget: &mut AuthoringProjectAdmissionBudget,
    prepare: impl FnOnce(
        &mut AuthoringProjectAdmissionBudget,
    ) -> Result<T, SourceUpgradePublicationError>,
) -> Result<T, SourceUpgradePublicationError> {
    let before = budget.retained_candidate_bytes;
    let handles_before = budget.retained_open_handles;
    let result = prepare(budget);
    if result.is_err() {
        let released = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .expect("refused preparation retains no new ownership");
        release(budget, released);
        let closed = budget
            .retained_open_handles
            .checked_sub(handles_before)
            .expect("refused preparation has dropped its native handles");
        budget.release_open_handles(closed);
    }
    result
}

fn release(budget: &mut AuthoringProjectAdmissionBudget, bytes: usize) {
    budget.retained_candidate_bytes = budget
        .retained_candidate_bytes
        .checked_sub(bytes)
        .expect("a publication scratch reservation remains live until its owner returns");
}
fn add(left: usize, right: usize) -> Result<usize, SourceUpgradePublicationError> {
    left.checked_add(right)
        .ok_or(SourceUpgradePublicationError::Accounting)
}
fn mul(left: usize, right: usize) -> Result<usize, SourceUpgradePublicationError> {
    left.checked_mul(right)
        .ok_or(SourceUpgradePublicationError::Accounting)
}
