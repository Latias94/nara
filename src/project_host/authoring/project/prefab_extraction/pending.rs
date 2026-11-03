//! Actual extraction Pending configuration shared by mixed recovery and the future inventory.

use nara_project::ProjectAuthoringState;

use super::recovery_data::PrefabExtractionRecovery;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
    recovery_storage::ObservedPendingRecoveryStorage,
    source_inventory::{
        SourceInventoryError, SourceInventoryManifest, product, sum, with_temporary_storage,
    },
};

#[derive(Debug, thiserror::Error)]
pub(super) enum PendingConfigurationError {
    #[error("extraction Pending differs from its complete committed Ready configuration")]
    Configuration,
    #[error("extraction Pending configuration admission failed")]
    Inventory(#[from] SourceInventoryError),
}

impl From<AuthoringProjectAdmissionError> for PendingConfigurationError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self::Inventory(error.into())
    }
}

pub(super) fn validate_configuration(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    manifest: &mut SourceInventoryManifest,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), PendingConfigurationError> {
    let recovery = pending.recovery();
    let actual = manifest.value_mut();
    let ProjectAuthoringState::Pending(reference) = &actual.authoring.state else {
        return Err(PendingConfigurationError::Configuration);
    };
    budget.charge_source_bytes(
        0,
        0,
        sum(
            reference.recovery_path().as_str().len(),
            recovery.unchanged_bundle().path().len(),
        )?,
        0,
    )?;
    if reference != pending.reference()
        || reference.recovery_digest() != recovery.digest()
        || actual.project.source_universe != recovery.source_universe()
        || actual.authoring.schema_bundle.as_deref() != Some(recovery.unchanged_bundle().path())
    {
        return Err(PendingConfigurationError::Configuration);
    }
    for expected in [
        recovery.old_ready_manifest(),
        recovery.final_ready_manifest(),
    ] {
        with_temporary_storage(budget, |budget| {
            let mut expected_manifest = SourceInventoryManifest::parse(expected.bytes(), budget)?;
            budget.charge_source_bytes(
                0,
                0,
                product(
                    sum(pending.manifest_bytes().len(), expected.bytes().len())?,
                    128,
                )?,
                0,
            )?;
            // Normalize only the private comparison value. Native admission keeps the actual
            // Pending state and recovery reference, and no Ready receipt is constructed here.
            let state =
                std::mem::replace(&mut actual.authoring.state, ProjectAuthoringState::Ready);
            let expected = expected_manifest.value_mut();
            let same =
                expected.authoring.state == ProjectAuthoringState::Ready && actual == expected;
            actual.authoring.state = state;
            if !same {
                return Err(PendingConfigurationError::Configuration);
            }
            Ok(())
        })?;
    }
    Ok(())
}
