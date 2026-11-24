//! The actual mixed inventory for a pending component Apply.

use std::{fmt, mem::size_of, sync::Arc};

use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FileIdentity};
use nara_project::{EffectiveProjectSettings, ProjectAuthoringRecoveryDigest};
use nara_scene::SceneFileLimits;

use super::recovery_data::{PrefabComponentApplyRecoveryData, RecoveryBlob};
use crate::project_host::authoring::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
    recovery_storage::{ObservedPendingRecoveryStorage, RecoveryPackageData, RecoveryStorageError},
    source_inventory::{
        SourceClaim, SourceFileFact, SourceInventory, SourceInventoryError, SourceInventoryInput,
        SourceInventoryManifest, SourceRootObservation, product, read_file, release, reserve,
        revalidate_file, sum, with_candidate_storage,
    },
};

#[cfg(all(test, any(windows, target_os = "linux")))]
pub(super) mod tests;

pub(super) struct ComponentApplyPhysicalInventory {
    inventory: SourceInventory,
    output_is_current: [bool; 2],
    settings: EffectiveProjectSettings,
    bundle_fact: SourceFileFact,
    bundle_file: Arc<FileCapability>,
    bundle_parent: DirectoryCapability,
    operation: ProjectAuthoringRecoveryDigest,
    package_identity: FileIdentity,
}

impl fmt::Debug for ComponentApplyPhysicalInventory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentApplyPhysicalInventory")
            .field("operation", &self.operation)
            .field("output_is_current", &self.output_is_current)
            .finish_non_exhaustive()
    }
}

impl ComponentApplyPhysicalInventory {
    pub(super) fn inventory(&self) -> &SourceInventory {
        &self.inventory
    }

    /// In containing/destination order. Byte-identical outputs are already current.
    pub(super) fn output_is_current(&self) -> [bool; 2] {
        self.output_is_current
    }

    pub(super) fn settings(&self) -> &EffectiveProjectSettings {
        &self.settings
    }

    pub(super) fn bundle_file(&self) -> &Arc<FileCapability> {
        &self.bundle_file
    }

    pub(super) fn bundle_parent(&self) -> &DirectoryCapability {
        &self.bundle_parent
    }

    pub(super) fn bundle_digest(&self) -> ContentDigest {
        self.bundle_fact.digest()
    }

    pub(super) fn observation_input<'a>(
        &'a self,
        pending: &'a ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
    ) -> SourceInventoryInput<'a> {
        inventory_input(pending, &self.bundle_fact, &self.settings)
    }

    /// Reobserves the retained manifest, package and bundle throughout publication.
    pub(super) fn validate_authority(
        &self,
        pending: &ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), ComponentApplyPhysicalError> {
        budget.charge_source_bytes(
            0,
            0,
            size_of::<ProjectAuthoringRecoveryDigest>() + size_of::<FileIdentity>(),
            0,
        )?;
        if self.operation != pending.recovery().digest()
            || self.package_identity != pending.recovery_file().identity()
        {
            return Err(SourceInventoryError::AuthorityChanged.into());
        }
        self.inventory.validate_manifest_observation(
            pending.root().identity(),
            pending.capability().identity(),
            pending.digest(),
        )?;
        pending.validate(budget)?;
        revalidate_file(pending.root(), &self.bundle_fact, budget)?;
        Ok(())
    }

    /// Reobserves this same mixed inventory before the publisher changes either leaf.
    pub(super) fn validate(
        &self,
        pending: &ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), ComponentApplyPhysicalError> {
        self.validate_authority(pending, budget)?;
        self.inventory.validate_admitted_observations(
            self.observation_input(pending),
            &[],
            budget,
        )?;
        pending.validate(budget)?;
        Ok(())
    }
}

/// Captures existing sources through native admission under the actual Pending manifest.
/// The storage owner has already matched the complete configuration to the fixed Ready bytes.
pub(super) fn capture_pending_inventory(
    pending: &ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
    scene_limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ComponentApplyPhysicalInventory, ComponentApplyPhysicalError> {
    with_candidate_storage(budget, |budget| {
        reserve(
            budget,
            size_of::<ComponentApplyPhysicalInventory>() - size_of::<SourceInventory>(),
        )?;
        pending.validate(budget)?;
        let recovery = pending.recovery();
        let bundle = recovery.unchanged_bundle();
        let retained_before = budget.retained_candidate_bytes;
        let (bundle_fact, bundle_bytes, bundle_file, custody_bytes) = read_file(
            pending.root(),
            bundle.path(),
            None,
            bundle.blob().bytes().len(),
            budget,
        )?;
        let read_scratch = budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .and_then(|bytes| bytes.checked_sub(custody_bytes))
            .and_then(|bytes| {
                bytes.checked_sub(size_of::<SourceFileFact>() + bundle_fact.path().len())
            })
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        budget.charge_source_bytes(0, 0, bundle_bytes.len(), 0)?;
        if !matches_blob(bundle.blob(), &bundle_fact, &bundle_bytes) {
            return Err(ComponentApplyPhysicalError::BundleChanged);
        }
        drop(bundle_bytes);
        release(budget, read_scratch)?;

        let mut manifest = SourceInventoryManifest::parse(pending.manifest_bytes(), budget)?;
        let profile = recovery.selected_profile();
        let settings_bytes = product(pending.manifest_bytes().len(), 128)?;
        budget.charge_source_bytes(
            0,
            pending.manifest_bytes().len(),
            sum(pending.manifest_bytes().len(), profile.map_or(0, str::len))?,
            settings_bytes,
        )?;
        let settings = manifest
            .value_mut()
            .resolve_profile(profile)
            .map_err(|_| ComponentApplyPhysicalError::Configuration)?;
        if settings.authoring_state != manifest.value_mut().authoring.state
            || settings.profile_name.as_deref() != profile
            || settings
                .authoring_schema_bundle
                .as_ref()
                .map(|path| path.as_str())
                != Some(bundle.path())
        {
            return Err(ComponentApplyPhysicalError::Configuration);
        }

        let sources = recovery.complete_original_sources();
        let claim_bytes = product(sources.len(), size_of::<SourceClaim<'_>>())?;
        budget.charge_source_bytes(0, 0, claim_bytes, claim_bytes)?;
        let claims: Vec<_> = sources
            .iter()
            .map(|source| SourceClaim {
                path: source.path(),
                kind: source.key().kind(),
                explicit_native: source.explicit_native(),
            })
            .collect();
        let inventory = SourceInventory::capture_admitted(
            inventory_input(pending, &bundle_fact, &settings),
            manifest,
            &[],
            &claims,
            &[],
            scene_limits,
            budget,
        )?;
        drop(claims);
        release(budget, claim_bytes)?;
        let output_is_current = validate_members(&inventory, recovery, budget)?;
        let bundle_parent = inventory
            .shared_parent(bundle.path())
            .ok_or(SourceInventoryError::SourceChanged)?;
        if bundle_fact.parent_identity() != Some(bundle_parent.identity()) {
            return Err(SourceInventoryError::SourceChanged.into());
        }
        revalidate_file(pending.root(), &bundle_fact, budget)?;
        pending.validate(budget)?;
        Ok(ComponentApplyPhysicalInventory {
            inventory,
            output_is_current,
            settings,
            bundle_fact,
            bundle_file,
            bundle_parent,
            operation: recovery.digest(),
            package_identity: pending.recovery_file().identity(),
        })
    })
}

fn inventory_input<'a>(
    pending: &'a ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
    bundle: &'a SourceFileFact,
    settings: &'a EffectiveProjectSettings,
) -> SourceInventoryInput<'a> {
    SourceInventoryInput {
        root: pending.root(),
        manifest_bytes: pending.manifest_bytes(),
        manifest_identity: pending.capability().identity(),
        manifest_digest: pending.digest(),
        bundle_path: bundle.path(),
        bundle_identity: bundle.identity(),
        bundle_digest: bundle.digest(),
        selected_settings: Some(settings),
    }
}

fn validate_members(
    inventory: &SourceInventory,
    recovery: &PrefabComponentApplyRecoveryData,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<[bool; 2], ComponentApplyPhysicalError> {
    budget.charge_source_bytes(0, 0, inventory.comparison_work()?, 0)?;
    if inventory.participants().len() != recovery.complete_original_sources().len()
        || inventory.metadata().len() != recovery.metadata_guards().len()
        || inventory.roots().len() != recovery.roots().len()
        || inventory.captured_open_sources().len() != 0
    {
        return Err(ComponentApplyPhysicalError::InventoryChanged);
    }

    let outputs = recovery.outputs();
    let mut output_is_current = [false; 2];
    for (actual, stored) in inventory
        .participants()
        .zip(recovery.complete_original_sources())
    {
        budget.charge_source_bytes(
            0,
            0,
            sum(
                product(stored.path().len(), 3)?,
                stored.key().retained_bytes(),
            )?,
            0,
        )?;
        if actual.file().path() != stored.path()
            || actual.source_key() != stored.key()
            || actual.encoding() != stored.encoding()
            || actual.explicit_native() != stored.explicit_native()
        {
            return Err(ComponentApplyPhysicalError::InventoryChanged);
        }
        let old = stored.original_disk();
        if let Some(index) = outputs
            .iter()
            .position(|(source, _)| source.path() == stored.path())
        {
            budget.charge_source_bytes(0, 0, product(actual.bytes().len(), 2)?, 0)?;
            if matches_blob(outputs[index].1, actual.file(), actual.bytes()) {
                output_is_current[index] = true;
            } else if !matches_blob(old, actual.file(), actual.bytes()) {
                return Err(ComponentApplyPhysicalError::OutputConflict { index });
            }
        } else if !matches_blob(old, actual.file(), actual.bytes()) {
            return Err(ComponentApplyPhysicalError::InventoryChanged);
        }
    }

    let lookups = product(
        (usize::BITS - inventory.metadata().len().leading_zeros()) as usize + 1,
        16,
    )?;
    for stored in recovery.metadata_guards() {
        budget.charge_source_bytes(0, 0, product(stored.path().len(), lookups)?, 0)?;
        let actual = stored
            .path()
            .strip_suffix(".meta")
            .and_then(|path| inventory.metadata_for_source(path))
            .ok_or(ComponentApplyPhysicalError::InventoryChanged)?;
        if actual.file().path() != stored.path()
            || !matches_blob(stored.observed(), actual.file(), actual.bytes())
        {
            return Err(ComponentApplyPhysicalError::InventoryChanged);
        }
    }
    for (actual, stored) in inventory.roots().zip(recovery.roots()) {
        let ancestor = match actual.observation() {
            SourceRootObservation::Available { .. } => None,
            SourceRootObservation::Missing { ancestor_path, .. } => Some(ancestor_path.as_str()),
        };
        if actual.path() != stored.path() || ancestor != stored.missing_ancestor() {
            return Err(ComponentApplyPhysicalError::InventoryChanged);
        }
    }
    if inventory
        .missing_sidecars()
        .ne(recovery.missing_sidecars().iter().map(String::as_str))
    {
        return Err(ComponentApplyPhysicalError::InventoryChanged);
    }
    Ok(output_is_current)
}

fn matches_blob(expected: &RecoveryBlob, fact: &SourceFileFact, actual: &[u8]) -> bool {
    fact.digest() == expected.digest() && actual == expected.bytes()
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ComponentApplyPhysicalError {
    #[error("component Apply recovery inventory could not be admitted")]
    Inventory(#[from] SourceInventoryError),
    #[error("component Apply Pending storage changed")]
    Pending(#[from] RecoveryStorageError),
    #[error("component Apply Pending configuration differs from the fixed Ready configuration")]
    Configuration,
    #[error("component Apply Schema bundle differs from its exact unchanged bytes")]
    BundleChanged,
    #[error("component Apply inventory or a physical dependency differs from the reviewed project")]
    InventoryChanged,
    #[error("component Apply output {index} is neither its original nor its fixed output")]
    OutputConflict { index: usize },
}

impl From<AuthoringProjectAdmissionError> for ComponentApplyPhysicalError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        SourceInventoryError::Admission(error).into()
    }
}
