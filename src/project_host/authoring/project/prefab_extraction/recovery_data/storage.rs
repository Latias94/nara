//! Concrete immutable data projection and fresh Pending admission over native transport.

use std::sync::Arc;

use nara_fs::DirectoryCapability;

use super::*;
use crate::project_host::authoring::recovery_storage::{
    LoadedPendingRecoveryStorage, ObservedPendingRecoveryStorage, RecoveryPackageData,
    RecoveryStorageError, RecoveryStorageHost,
};

impl RecoveryPackageData for PrefabExtractionRecovery {
    fn encoded_bytes(&self) -> &[u8] {
        self.encoded_bytes()
    }

    fn digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.digest()
    }

    fn source_universe(&self) -> SourceUniverseId {
        self.source_universe()
    }

    fn old_ready_manifest(&self) -> &[u8] {
        self.old_ready_manifest().bytes()
    }

    fn final_ready_manifest(&self) -> &[u8] {
        self.final_ready_manifest().bytes()
    }

    fn protected_paths(&self) -> impl Iterator<Item = &str> {
        self.payload
            .file_paths()
            .chain(self.roots().iter().map(RecoveryRoot::path))
            .chain(self.missing_sidecars().iter().map(String::as_str))
    }
}

#[derive(Debug)]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PrefabExtractionRecoveryShareFailure
{
    pub(in crate::project_host::authoring::project::prefab_extraction) recovery:
        PrefabExtractionRecovery,
    pub(in crate::project_host::authoring::project::prefab_extraction) error:
        PrefabExtractionRecoveryError,
}

impl PrefabExtractionRecovery {
    /// Moves the admitted allocations once. Refusal returns the original package, and only
    /// the unallocated Arc control reservation is refunded; cumulative work remains spent.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_shared(
        self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Arc<Self>, PrefabExtractionRecoveryShareFailure> {
        let retained = budget.retained_candidate_bytes;
        if let Err(error) = budget.charge_source_bytes(0, 0, 0, 4 * size_of::<usize>()) {
            budget.retained_candidate_bytes = retained;
            return Err(PrefabExtractionRecoveryShareFailure {
                recovery: self,
                error: error.into(),
            });
        }
        Ok(Arc::new(self))
    }
}

#[derive(Debug, thiserror::Error)]
pub(in crate::project_host::authoring::project::prefab_extraction) enum PrefabExtractionStorageLoadError
{
    #[error("Prefab extraction recovery transport failed")]
    Storage(#[from] RecoveryStorageError),
    #[error("Prefab extraction recovery data admission failed")]
    Data(#[from] PrefabExtractionRecoveryError),
}

/// This establishes actual Pending/package custody and concrete data coherence only. Complete
/// original membership, stored extraction semantics and final installation remain separate.
pub(in crate::project_host::authoring::project::prefab_extraction) fn load_pending(
    root: DirectoryCapability,
    limits: PrefabExtractionRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<
    ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    PrefabExtractionStorageLoadError,
> {
    let retained = budget.retained_candidate_bytes;
    let handles = budget.retained_open_handles;
    let result = (|| {
        let loaded = RecoveryStorageHost::load_pending(root, limits.encoded_bytes, budget)?;
        admit_loaded(loaded, limits, budget)
    })();
    if result.is_err() {
        // Every local candidate and native observation has dropped. Refund only their live
        // storage/handles, keeping all earlier owners and cumulative admission work charged.
        budget.retained_candidate_bytes = retained;
        budget.retained_open_handles = handles;
    }
    result
}

/// Consumes the already authenticated read from operation inspection. No second file read or
/// allowance is created, and concrete payload admission remains with this format owner.
pub(in crate::project_host::authoring::project::prefab_extraction) fn admit_loaded(
    loaded: LoadedPendingRecoveryStorage,
    limits: PrefabExtractionRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<
    ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    PrefabExtractionStorageLoadError,
> {
    let recovery = PrefabExtractionRecovery::decode(loaded.encoded_bytes(), limits, budget)?
        .into_shared(budget)
        .map_err(|failure| PrefabExtractionStorageLoadError::Data(failure.error))?;
    loaded
        .bind(recovery, budget)
        .map_err(|failure| PrefabExtractionStorageLoadError::Storage(failure.error))
}
