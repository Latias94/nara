//! Concrete Schema-upgrade payload admission over code-free native recovery transport.

use nara_core::SourceUniverseId;
use nara_fs::DirectoryCapability;
use nara_project::ProjectAuthoringRecoveryDigest;

use super::AuthoringProjectAdmissionBudget;
use super::recovery_storage::{
    LoadedPendingRecoveryStorage, ObservedPendingRecoveryStorage, RecoveryPackageData,
    RecoveryStorageError, RecoveryStorageHost,
};
use super::source_upgrade_recovery::{
    SourceUpgradeRecovery, SourceUpgradeRecoveryError, SourceUpgradeRecoveryLimits,
};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

impl RecoveryPackageData for SourceUpgradeRecovery {
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
        self.manifest().old_disk.bytes()
    }
    fn final_ready_manifest(&self) -> &[u8] {
        self.manifest().successor.bytes()
    }
    fn protected_paths(&self) -> impl Iterator<Item = &str> {
        [self.manifest().path.as_str(), self.bundle().path.as_str()]
            .into_iter()
            .chain(self.members().iter().map(|member| member.path.as_str()))
            .chain(
                self.metadata()
                    .iter()
                    .map(|metadata| metadata.path.as_str()),
            )
            .chain(self.roots().iter().map(|root| root.path.as_str()))
            .chain(self.missing_sidecars().iter().map(String::as_str))
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceUpgradeStorageError {
    #[error("Schema upgrade recovery transport failed")]
    Storage(#[from] RecoveryStorageError),
    #[error("Schema upgrade recovery payload admission failed")]
    Recovery(#[from] SourceUpgradeRecoveryError),
}

pub(super) fn load_pending(
    root: DirectoryCapability,
    limits: SourceUpgradeRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ObservedPendingRecoveryStorage<SourceUpgradeRecovery>, SourceUpgradeStorageError> {
    let retained = budget.retained_candidate_bytes;
    let handles = budget.retained_open_handles;
    let result = (|| {
        let loaded = RecoveryStorageHost::load_pending(root, limits.encoded_bytes, budget)?;
        admit_loaded(loaded, limits, budget)
    })();
    if result.is_err() {
        // Every new read/model/candidate has dropped. Retain cumulative work and the original
        // allowance; this constructor grants no publication evidence or old nominal authority.
        budget.retained_candidate_bytes = retained;
        budget.retained_open_handles = handles;
    }
    result
}

/// Concrete admission of the transport already authenticated by operation inspection.
pub(super) fn admit_loaded(
    loaded: LoadedPendingRecoveryStorage,
    limits: SourceUpgradeRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ObservedPendingRecoveryStorage<SourceUpgradeRecovery>, SourceUpgradeStorageError> {
    let recovery = SourceUpgradeRecovery::decode(loaded.encoded_bytes(), limits, budget)?
        .into_shared(budget)
        .map_err(|failure| SourceUpgradeStorageError::Recovery(failure.error))?;
    loaded
        .bind(recovery, budget)
        .map_err(|failure| SourceUpgradeStorageError::Storage(failure.error))
}
