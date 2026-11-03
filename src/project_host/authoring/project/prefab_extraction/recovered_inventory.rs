//! Complete physical observations and one unchanged Schema admission under actual Pending.
//!
//! This prerequisite retains current native custody and a nominal authoring basis. It does not
//! verify the stored extraction pair, issue replacement evidence, publish files or admit Ready.

use std::{fmt, mem::size_of, sync::Arc};

use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError};
use nara_project::ProjectAuthoringRecoveryDigest;
use nara_scene::{AuthoringSchemaBasis, SceneFileLimits};

use super::recovery_data::PrefabExtractionRecovery;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
    recovery_storage::{ObservedPendingRecoveryStorage, RecoveryStorageError},
    schema_models::{SchemaModelsError, SelectedSchemaModels, prepare_schema_models},
    source_inventory::{
        SharedSourceInventoryCustody, SourceFileFact, SourceInventory, SourceInventoryError,
        product, release, reserve, sum, with_candidate_storage, with_temporary_storage,
    },
};

mod physical;

/// The enclosing recovery owner must keep the actual Pending storage alive alongside this
/// observation. Data classification below cannot stand in for a historical publication receipt.
pub(super) struct RecoveredPrefabExtractionInventory {
    physical: PhysicalInventory,
    models: SelectedSchemaModels,
}

struct PhysicalInventory {
    inventory: SourceInventory,
    bundle_fact: SourceFileFact,
    bundle_file: Arc<FileCapability>,
    bundle_parent: DirectoryCapability,
    operation: ProjectAuthoringRecoveryDigest,
    recovery_identity: FileIdentity,
}

/// Only the initial capture uses these verified bytes to admit models. Continuing validation
/// reuses the retained observations without capturing another corpus or nominal basis.
struct CapturedPhysicalInventory {
    physical: PhysicalInventory,
    bundle_bytes: Vec<u8>,
    temporary_read_bytes: usize,
}

impl fmt::Debug for RecoveredPrefabExtractionInventory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveredPrefabExtractionInventory")
            .field("inventory", &self.physical.inventory)
            .field("operation", &self.physical.operation)
            .finish_non_exhaustive()
    }
}

impl RecoveredPrefabExtractionInventory {
    pub(super) fn capture(
        pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, RecoveredPrefabExtractionInventoryError> {
        with_candidate_storage(budget, |budget| {
            reserve(budget, size_of::<Self>() - size_of::<PhysicalInventory>())?;
            let CapturedPhysicalInventory {
                physical,
                bundle_bytes,
                temporary_read_bytes,
            } = physical::capture(pending, scene_limits, budget)?;
            let models = prepare_schema_models(&bundle_bytes, budget)?;
            // Keep the first read's actual capability and every ancestor. Only its temporary
            // body/copy reservation ends here; the exact fact and all previous work remain charged.
            drop(bundle_bytes);
            release(budget, temporary_read_bytes)?;
            pending.validate(budget)?;
            Ok(Self { physical, models })
        })
    }

    pub(super) fn inventory(&self) -> &SourceInventory {
        &self.physical.inventory
    }

    pub(super) fn models(&self) -> &SelectedSchemaModels {
        &self.models
    }

    pub(super) fn basis(&self) -> &AuthoringSchemaBasis {
        &self.models.basis
    }

    /// The same live leaf whose exact bytes supplied this process's model admission.
    pub(super) fn bundle_file(&self) -> &Arc<FileCapability> {
        &self.physical.bundle_file
    }

    pub(super) fn bundle_digest(&self) -> ContentDigest {
        self.physical.inventory.bundle_digest()
    }

    /// Shared from complete inventory custody, which retains this parent's own ancestors.
    pub(super) fn bundle_parent(&self) -> &DirectoryCapability {
        &self.physical.bundle_parent
    }

    pub(super) fn shared_custody(&self) -> SharedSourceInventoryCustody {
        self.physical.inventory.shared_custody()
    }

    /// Requires an unchanged complete current observation, including both Q leaves separately.
    /// Publication progress requires a new capture; validation preserves these original models
    /// and their basis, checking current names/content without a second full native corpus.
    pub(super) fn validate(
        &self,
        pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), RecoveredPrefabExtractionInventoryError> {
        budget.charge_source_bytes(
            0,
            0,
            size_of::<ProjectAuthoringRecoveryDigest>() + size_of::<FileIdentity>(),
            0,
        )?;
        if self.physical.operation != pending.recovery().digest()
            || self.physical.recovery_identity != pending.recovery_file().identity()
        {
            return Err(SourceInventoryError::AuthorityChanged.into());
        }
        self.physical.inventory.validate_manifest_observation(
            pending.root().identity(),
            pending.capability().identity(),
            pending.digest(),
        )?;
        physical::validate(&self.physical, pending, budget)
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum RecoveredPrefabExtractionInventoryError {
    #[error("Prefab extraction recovery physical inventory failed")]
    Inventory(#[from] SourceInventoryError),
    #[error("Prefab extraction recovery Pending storage changed")]
    Pending(#[from] RecoveryStorageError),
    #[error("Prefab extraction recovery unchanged Schema admission failed")]
    Schema(#[from] SchemaModelsError),
    #[error("Prefab extraction recovery differs from the actual Pending configuration")]
    Configuration,
    #[error("Prefab extraction recovery differs from the complete original physical declarations")]
    OriginalInventory,
    #[error("Prefab extraction recovery Q or its sidecar conflicts with the fixed output")]
    Destination,
}

impl From<AuthoringProjectAdmissionError> for RecoveredPrefabExtractionInventoryError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        SourceInventoryError::Admission(error).into()
    }
}

impl From<FsError> for RecoveredPrefabExtractionInventoryError {
    fn from(error: FsError) -> Self {
        SourceInventoryError::Host(error).into()
    }
}

impl From<super::pending::PendingConfigurationError> for RecoveredPrefabExtractionInventoryError {
    fn from(error: super::pending::PendingConfigurationError) -> Self {
        match error {
            super::pending::PendingConfigurationError::Configuration => Self::Configuration,
            super::pending::PendingConfigurationError::Inventory(error) => Self::Inventory(error),
        }
    }
}
