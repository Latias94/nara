//! Actual Pending configuration, complete originals and independent fixed-destination facts.

use super::super::pending::validate_configuration;
use super::super::recovery_data::{
    RecoveryBlob, RecoveryFileState, RecoveryRootObservation, RecoverySourceDisposition,
    RecoverySourceEncoding,
};
use super::*;
use crate::project_host::authoring::source_inventory::{
    ObservedFileClaim, SourceClaim, SourceEncoding, SourceFileObservation, SourceInventoryInput,
    SourceInventoryManifest, SourceRootObservation, read_file, revalidate_file,
};

pub(super) fn capture(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    scene_limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<CapturedPhysicalInventory, RecoveredPrefabExtractionInventoryError> {
    // Both callers entered a candidate/temporary scope before this first reservation. Every
    // newly opened object and parser local drops before that enclosing scope refunds failure.
    reserve(
        budget,
        size_of::<PhysicalInventory>() - size_of::<SourceInventory>(),
    )?;
    pending.validate(budget)?;
    let recovery = pending.recovery();
    let bundle = recovery.unchanged_bundle();
    let retained_before_read = budget.retained_candidate_bytes;
    let (fact, bytes, bundle_file, custody_bytes) = read_file(
        pending.root(),
        bundle.path(),
        None,
        bundle.blob().bytes().len(),
        budget,
    )?;
    let temporary_read_bytes = budget
        .retained_candidate_bytes
        .checked_sub(retained_before_read)
        .and_then(|bytes| bytes.checked_sub(custody_bytes))
        .and_then(|bytes| bytes.checked_sub(size_of::<SourceFileFact>() + fact.path().len()))
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    budget.charge_source_bytes(0, 0, bytes.len(), 0)?;
    if !matches_blob(bundle.blob(), fact.digest(), &bytes) {
        return Err(RecoveredPrefabExtractionInventoryError::Configuration);
    }
    let mut manifest = SourceInventoryManifest::parse(pending.manifest_bytes(), budget)?;
    validate_configuration(pending, &mut manifest, budget)?;

    // Resolve the stored selection from the actual Pending model after restoring its state.
    // This settings owner remains live through native policy construction; no Ready receipt
    // or copied source candidate is constructed to satisfy the ordinary scanner's interface.
    let settings_bytes = product(pending.manifest_bytes().len(), 128)?;
    budget.charge_source_bytes(
        0,
        pending.manifest_bytes().len(),
        sum(
            pending.manifest_bytes().len(),
            recovery.selected_profile().map_or(0, str::len),
        )?,
        settings_bytes,
    )?;
    let settings = manifest
        .value_mut()
        .resolve_profile(recovery.selected_profile())
        .map_err(|_| RecoveredPrefabExtractionInventoryError::Configuration)?;
    if settings.authoring_state != manifest.value_mut().authoring.state
        || settings.profile_name.as_deref() != recovery.selected_profile()
    {
        return Err(RecoveredPrefabExtractionInventoryError::Configuration);
    }
    let input = SourceInventoryInput {
        root: pending.root(),
        manifest_bytes: pending.manifest_bytes(),
        manifest_identity: pending.capability().identity(),
        manifest_digest: pending.digest(),
        bundle_path: bundle.path(),
        bundle_identity: fact.identity(),
        bundle_digest: fact.digest(),
        selected_settings: Some(&settings),
    };
    let slots = product(
        recovery.complete_original_sources().len(),
        size_of::<SourceClaim<'_>>(),
    )?;
    budget.charge_source_bytes(0, 0, slots, slots)?;
    // Concrete package admission owns sorted, unique original paths, including originals
    // outside catalogue roots. Q is intentionally absent from this native participant list.
    let claims: Vec<_> = recovery
        .complete_original_sources()
        .iter()
        .map(|source| SourceClaim {
            path: source.path(),
            kind: source.key().kind(),
            explicit_native: source.explicit_native(),
        })
        .collect();
    let mut observations = [
        ObservedFileClaim {
            path: recovery.q().path(),
            maximum_bytes: recovery.q().output().bytes().len(),
        },
        ObservedFileClaim {
            path: recovery.q_sidecar().path(),
            maximum_bytes: recovery.q_sidecar().output().bytes().len(),
        },
    ];
    budget.charge_source_bytes(
        0,
        0,
        sum(observations[0].path.len(), observations[1].path.len())?,
        0,
    )?;
    observations.sort_unstable_by(|left, right| left.path.cmp(right.path));
    let inventory = SourceInventory::capture_admitted(
        input,
        manifest,
        &[],
        &claims,
        &observations,
        scene_limits,
        budget,
    )?;
    drop(claims);
    drop(settings);
    release(budget, sum(slots, settings_bytes)?)?;
    validate_members(&inventory, recovery, budget)?;

    let bundle_parent = inventory
        .shared_parent(bundle.path())
        .ok_or(SourceInventoryError::SourceChanged)?;
    if fact.parent_identity() != Some(bundle_parent.identity()) {
        return Err(SourceInventoryError::SourceChanged.into());
    }
    revalidate_file(pending.root(), &fact, budget)?;
    // Each caller revalidates Pending after its remaining model admission or comparison.
    Ok(CapturedPhysicalInventory {
        physical: PhysicalInventory {
            inventory,
            bundle_fact: fact,
            bundle_file,
            bundle_parent,
            operation: recovery.digest(),
            recovery_identity: pending.recovery_file().identity(),
        },
        bundle_bytes: bytes,
        temporary_read_bytes,
    })
}

pub(super) fn validate(
    physical: &PhysicalInventory,
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), RecoveredPrefabExtractionInventoryError> {
    pending.validate(budget)?;
    with_temporary_storage(budget, |budget| {
        revalidate_file(pending.root(), &physical.bundle_fact, budget)?;
        let mut manifest = SourceInventoryManifest::parse(pending.manifest_bytes(), budget)?;
        validate_configuration(pending, &mut manifest, budget)?;
        let settings_bytes = product(pending.manifest_bytes().len(), 128)?;
        budget.charge_source_bytes(
            0,
            pending.manifest_bytes().len(),
            pending.manifest_bytes().len(),
            settings_bytes,
        )?;
        let settings = manifest
            .value_mut()
            .resolve_profile(pending.recovery().selected_profile())
            .map_err(|_| RecoveredPrefabExtractionInventoryError::Configuration)?;
        physical.inventory.validate_admitted_observations(
            SourceInventoryInput {
                root: pending.root(),
                manifest_bytes: pending.manifest_bytes(),
                manifest_identity: pending.capability().identity(),
                manifest_digest: pending.digest(),
                bundle_path: pending.recovery().unchanged_bundle().path(),
                bundle_identity: physical.bundle_fact.identity(),
                bundle_digest: physical.bundle_fact.digest(),
                selected_settings: Some(&settings),
            },
            &[],
            budget,
        )?;
        Ok::<(), RecoveredPrefabExtractionInventoryError>(())
    })?;
    pending.validate(budget)?;
    Ok(())
}

fn validate_members(
    inventory: &SourceInventory,
    recovery: &PrefabExtractionRecovery,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), RecoveredPrefabExtractionInventoryError> {
    budget.charge_source_bytes(0, 0, inventory.comparison_work()?, 0)?;
    if inventory.participants().len() != recovery.complete_original_sources().len()
        || inventory.metadata().len() != recovery.metadata_guards().len()
        || inventory.roots().len() != recovery.roots().len()
        || inventory.missing_sidecars().len() != recovery.missing_sidecars().len()
        || inventory.captured_open_sources().len() != 0
    {
        return Err(RecoveredPrefabExtractionInventoryError::OriginalInventory);
    }
    for (actual, original) in inventory
        .participants()
        .zip(recovery.complete_original_sources())
    {
        let encoding = match actual.encoding() {
            SourceEncoding::Json => RecoverySourceEncoding::Json,
            SourceEncoding::Ron => RecoverySourceEncoding::Ron,
        };
        let current = original.classify(Some(actual.file().digest()));
        // This selects the committed data blob only. It proves no replacement event.
        let blob = match current {
            RecoveryFileState::Original | RecoveryFileState::Unchanged => original.original_disk(),
            RecoveryFileState::Output
                if original.disposition() != RecoverySourceDisposition::Guard =>
            {
                original
                    .output()
                    .ok_or(RecoveredPrefabExtractionInventoryError::OriginalInventory)?
            }
            _ => return Err(RecoveredPrefabExtractionInventoryError::OriginalInventory),
        };
        budget.charge_source_bytes(
            0,
            0,
            sum(original.path().len(), original.key().retained_bytes())?,
            0,
        )?;
        if actual.file().path() != original.path()
            || actual.source_key() != original.key()
            || encoding != original.encoding()
            || actual.explicit_native() != original.explicit_native()
            || !matches_blob(blob, actual.file().digest(), actual.bytes())
        {
            return Err(RecoveredPrefabExtractionInventoryError::OriginalInventory);
        }
    }
    let depth = product(
        (usize::BITS - inventory.metadata().len().leading_zeros()) as usize + 1,
        16,
    )?;
    for guard in recovery.metadata_guards() {
        budget.charge_source_bytes(0, 0, product(guard.path().len(), depth)?, 0)?;
        // The inventory orders source paths; the package orders sidecar paths. Use the
        // existing source index because a prefix can reverse those two orders.
        let actual = guard
            .path()
            .strip_suffix(".meta")
            .and_then(|path| inventory.metadata_for_source(path))
            .ok_or(RecoveredPrefabExtractionInventoryError::OriginalInventory)?;
        if actual.file().path() != guard.path()
            || !matches_blob(guard.observed(), actual.file().digest(), actual.bytes())
        {
            return Err(RecoveredPrefabExtractionInventoryError::OriginalInventory);
        }
    }
    for (actual, expected) in inventory.roots().zip(recovery.roots()) {
        let same = match (actual.observation(), expected.observation()) {
            (SourceRootObservation::Available { .. }, RecoveryRootObservation::Present) => true,
            (
                SourceRootObservation::Missing { ancestor_path, .. },
                RecoveryRootObservation::Missing { ancestor },
            ) => ancestor_path == ancestor,
            _ => false,
        };
        if actual.path() != expected.path() || !same {
            return Err(RecoveredPrefabExtractionInventoryError::OriginalInventory);
        }
    }
    if inventory
        .missing_sidecars()
        .ne(recovery.missing_sidecars().iter().map(String::as_str))
    {
        return Err(RecoveredPrefabExtractionInventoryError::OriginalInventory);
    }
    for (path, blob) in [
        (recovery.q().path(), recovery.q().output()),
        (recovery.q_sidecar().path(), recovery.q_sidecar().output()),
    ] {
        match inventory.file_observation(path) {
            Some(SourceFileObservation::Missing { .. }) => {}
            Some(SourceFileObservation::Present { file, bytes })
                if matches_blob(blob, file.digest(), bytes) => {}
            _ => return Err(RecoveredPrefabExtractionInventoryError::Destination),
        }
    }
    Ok(())
}

fn matches_blob(blob: &RecoveryBlob, digest: ContentDigest, bytes: &[u8]) -> bool {
    let expected = blob.digest();
    digest == ContentDigest::from_parts(expected.length(), *expected.as_bytes())
        && bytes == blob.bytes()
}
