//! Schema-upgrade recovery generations, successor proofs and inventory disposal.

use std::{mem::size_of, sync::Arc};

use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError};
use nara_project::{
    EffectiveProjectSettings, ProjectAuthoringRecoveryDigest, ProjectAuthoringState,
    ProjectManifest,
};
use nara_scene::SceneFileLimits;

use super::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError, AuthoringSchemaReceipt,
    source_inventory::{
        OpenSource, SharedSourceInventoryCustody, SourceClaim, SourceEncoding, SourceFileFact,
        SourceInventory, SourceInventoryError, SourceInventoryInput, SourceInventoryManifest,
        SourceParticipant, SourceRootObservation, product, read_file, release, reserve,
        revalidate_file, sum, with_candidate_storage, with_temporary_storage,
    },
    source_upgrade_recovery::{
        RecoveryMemberState, RecoveryRootObservation, RecoverySourceEncoding, SourceUpgradeRecovery,
    },
};

#[cfg(any(feature = "tooling", feature = "project-tooling", test))]
use super::recovery_storage::{ObservedPendingRecoveryStorage, RecoveryStorageError};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

/// This process's complete mixed observation under an actual Pending manifest. The enclosing
/// recovery owner keeps `ObservedPendingRecoveryStorage` alive; this owns the current bundle leaf
/// and all source/metadata/ancestor custody. Matching old/new bytes grants neither publication
/// evidence nor ordinary Ready admission, and never executes the converter again.
#[cfg(any(feature = "tooling", feature = "project-tooling", test))]
pub(super) struct RecoveredSourceUpgradeInventory {
    inventory: SourceInventory,
    bundle_fact: SourceFileFact,
    bundle_file: Arc<FileCapability>,
    bundle_parent: DirectoryCapability,
    operation: ProjectAuthoringRecoveryDigest,
    recovery_identity: FileIdentity,
}

#[cfg(any(feature = "tooling", feature = "project-tooling", test))]
impl RecoveredSourceUpgradeInventory {
    pub(super) fn capture(
        pending: &ObservedPendingRecoveryStorage<SourceUpgradeRecovery>,
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        with_candidate_storage(budget, |budget| {
            let wrapper_bytes = size_of::<Self>() - size_of::<SourceInventory>();
            reserve(budget, wrapper_bytes)?;
            pending.validate(budget)?;
            let recovery = pending.recovery();
            let bundle = recovery.bundle();
            let maximum = bundle
                .old_disk
                .bytes()
                .len()
                .max(bundle.successor.bytes().len());
            let before_read = budget.retained_candidate_bytes;
            let (fact, bytes, bundle_file, file_custody_bytes) =
                read_file(pending.root(), &bundle.path, None, maximum, budget)?;
            let read_bytes = budget
                .retained_candidate_bytes
                .checked_sub(before_read)
                .ok_or(SourceInventoryError::AccountingOverflow)?;
            let input = SourceInventoryInput {
                root: pending.root(),
                manifest_bytes: pending.manifest_bytes(),
                manifest_identity: pending.capability().identity(),
                manifest_digest: pending.digest(),
                bundle_path: &bundle.path,
                bundle_identity: fact.identity(),
                bundle_digest: fact.digest(),
                selected_settings: None,
            };
            let inventory = capture_pending_inventory(
                input,
                &bytes,
                recovery,
                RecoveryGeneration::Mixed,
                &[],
                scene_limits,
                budget,
            )?;
            validate_recovery_members(&inventory, recovery, RecoveryGeneration::Mixed, budget)?;
            let bundle_parent = inventory
                .shared_parent(&bundle.path)
                .ok_or(SourceInventoryError::SourceChanged)?;
            if fact.parent_identity() != Some(bundle_parent.identity()) {
                return Err(SourceInventoryError::SourceChanged.into());
            }
            // Keep the original read leaf continuously alive while checking its current name,
            // after the complete scan and before transferring observations to publication.
            revalidate_file(pending.root(), &fact, budget)?;
            pending.validate(budget)?;
            drop(bytes);
            // The actual bundle capability and exact fact survive this read. Return only its
            // temporary body/copy storage without touching cumulative IO/work or the live handle.
            release(
                budget,
                read_bytes
                    .checked_sub(file_custody_bytes)
                    .and_then(|bytes| {
                        bytes.checked_sub(size_of::<SourceFileFact>() + fact.path().len())
                    })
                    .ok_or(SourceInventoryError::AccountingOverflow)?,
            )?;
            Ok(Self {
                inventory,
                bundle_fact: fact,
                bundle_file,
                bundle_parent,
                operation: recovery.digest(),
                recovery_identity: pending.recovery_file().identity(),
            })
        })
    }

    pub(super) const fn inventory(&self) -> &SourceInventory {
        &self.inventory
    }

    /// The same continuously retained leaf whose actual bytes selected Old/New/Unchanged.
    pub(super) const fn bundle_file(&self) -> &Arc<FileCapability> {
        &self.bundle_file
    }

    pub(super) const fn bundle_digest(&self) -> ContentDigest {
        self.inventory.bundle_digest()
    }

    /// Shared from inventory custody; all of this directory's ancestors remain retained there.
    pub(super) const fn bundle_parent(&self) -> &DirectoryCapability {
        &self.bundle_parent
    }

    /// Source/metadata/ancestor custody only. The enclosing owner separately transfers the
    /// actual bundle Arc and Pending storage when discarding this mixed inventory's corpus.
    pub(super) fn shared_custody(&self) -> SharedSourceInventoryCustody {
        self.inventory.shared_custody()
    }

    /// Freshly scans the complete set again, including every profile and negative observation.
    /// Old-to-New progress, equal bytes at another leaf, or any membership change requires a new
    /// observation; confirmation cannot silently adopt files changed since this capture.
    pub(super) fn validate(
        &self,
        pending: &ObservedPendingRecoveryStorage<SourceUpgradeRecovery>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        if self.operation != pending.recovery().digest()
            || self.recovery_identity != pending.recovery_file().identity()
        {
            return Err(SourceInventoryError::AuthorityChanged.into());
        }
        self.inventory.validate_manifest_observation(
            pending.root().identity(),
            pending.capability().identity(),
            pending.digest(),
        )?;
        pending.validate(budget)?;
        with_temporary_storage(budget, |budget| {
            revalidate_file(pending.root(), &self.bundle_fact, budget)?;
            let mut manifest = SourceInventoryManifest::parse(pending.manifest_bytes(), budget)?;
            let (settings, _) = resolve_recovery_settings(
                pending.manifest_bytes().len(),
                manifest.value_mut(),
                pending.recovery(),
                None,
                budget,
            )?;
            self.inventory
                .validate_admitted_observations(
                    SourceInventoryInput {
                        root: pending.root(),
                        manifest_bytes: pending.manifest_bytes(),
                        manifest_identity: pending.capability().identity(),
                        manifest_digest: pending.digest(),
                        bundle_path: &pending.recovery().bundle().path,
                        bundle_identity: self.bundle_fact.identity(),
                        bundle_digest: self.bundle_fact.digest(),
                        selected_settings: Some(&settings),
                    },
                    &[],
                    budget,
                )
                .map_err(SourceUpgradeInventoryError::from)
        })?;
        pending.validate(budget)?;
        Ok(())
    }
}

/// A complete successor observation under the real, still-Pending manifest. Construction stays
/// private to this owner; it grants neither ordinary Open nor final Ready publication authority.
/// The enclosing operation separately revalidates its Ready read gate and owns publication lanes.
pub(super) struct VerifiedSourceUpgradeInventory {
    inventory: SourceInventory,
    schema: Arc<AuthoringSchemaReceipt>,
    operation: ProjectAuthoringRecoveryDigest,
    retained_bytes: usize,
}

/// Allocation settlement prepared before Ready for one exact immutable inventory. The shared
/// token keeps every native object alive while the discarded corpus is destroyed. Preparing or
/// dropping this record alone never refunds the inventory, custody or any shared Schema/body.
#[derive(Debug)]
pub(super) struct PreparedSourceUpgradeInventoryDisposal {
    custody: SharedSourceInventoryCustody,
    discarded_bytes: usize,
    inventory_bytes: usize,
}

impl VerifiedSourceUpgradeInventory {
    pub(super) fn shared_custody(&self) -> SharedSourceInventoryCustody {
        self.inventory.shared_custody()
    }

    pub(super) fn capture(
        schema: &Arc<AuthoringSchemaReceipt>,
        recovery: &SourceUpgradeRecovery,
        complete_future_open: &[OpenSource<'_>],
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        with_candidate_storage(budget, |budget| {
            let wrapper_bytes = size_of::<Self>() - size_of::<SourceInventory>();
            reserve(budget, wrapper_bytes)?;
            let root = schema.project_root();
            let input = SourceInventoryInput {
                root: &root,
                manifest_bytes: schema.manifest_bytes(),
                manifest_identity: schema.manifest_identity(),
                manifest_digest: schema.manifest_digest(),
                bundle_path: schema.bundle_path().as_str(),
                bundle_identity: schema.bundle_identity(),
                bundle_digest: schema.bundle_digest(),
                selected_settings: Some(schema.settings()),
            };
            let inventory = capture_pending_inventory(
                input,
                schema.bundle_bytes(),
                recovery,
                RecoveryGeneration::Successor,
                complete_future_open,
                scene_limits,
                budget,
            )?;
            validate_recovery_members(&inventory, recovery, RecoveryGeneration::Successor, budget)?;
            let retained_bytes = sum(wrapper_bytes, inventory.retained_bytes())?;
            Ok(Self {
                inventory,
                schema: Arc::clone(schema),
                operation: recovery.digest(),
                retained_bytes,
            })
        })
    }

    /// Prepares the fixed disposal record while admission can still refuse, before Ready. C is
    /// the custody owner's actual captured reservations; D is the rest of this capture's own
    /// frozen allowance. No later global budget delta or adoption-side cost model is involved.
    pub(super) fn prepare_disposal(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<PreparedSourceUpgradeInventoryDisposal, SourceUpgradeInventoryError> {
        with_candidate_storage(budget, |budget| {
            let discarded = self
                .retained_bytes
                .checked_sub(self.inventory.shared_custody().retained_bytes())
                .ok_or(SourceInventoryError::AccountingOverflow)?;
            let record_bytes = size_of::<PreparedSourceUpgradeInventoryDisposal>();
            let discarded_bytes = sum(discarded, record_bytes)?;
            reserve(budget, record_bytes)?;
            Ok(PreparedSourceUpgradeInventoryDisposal {
                custody: self.inventory.shared_custody(),
                discarded_bytes,
                inventory_bytes: self.retained_bytes,
            })
        })
    }

    /// Consumes this exact inventory and its previously bound disposal record. This performs
    /// no allocation, filesystem access, parsing or admission. The enclosing pre-Ready owner
    /// keeps this pair together; supplying another inventory's record is an internal invariant
    /// violation. All native handles and their C allowance continue through the returned token.
    pub(super) fn into_custody(
        self,
        disposal: PreparedSourceUpgradeInventoryDisposal,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> SharedSourceInventoryCustody {
        assert!(
            self.inventory
                .shared_custody()
                .same_owner(&disposal.custody),
            "inventory disposal belongs to the exact captured custody"
        );
        assert_eq!(
            self.retained_bytes, disposal.inventory_bytes,
            "inventory disposal binds the original capture allowance"
        );
        let PreparedSourceUpgradeInventoryDisposal {
            custody,
            discarded_bytes,
            inventory_bytes: _,
        } = disposal;
        // Destroy every source body, classification fact, index and open-fact copy before
        // releasing D. The moved shared token still owns all capabilities and their storage.
        drop(self);
        budget.retained_candidate_bytes = budget
            .retained_candidate_bytes
            .checked_sub(discarded_bytes)
            .expect("bound inventory corpus and disposal record remain charged until consumed");
        custody
    }

    pub(super) const fn inventory(&self) -> &SourceInventory {
        &self.inventory
    }

    pub(super) fn participants(&self) -> impl ExactSizeIterator<Item = &SourceParticipant> {
        self.inventory.participants()
    }

    /// Rechecks the retained future-open membership without copying its source facts. The
    /// enclosing Workspace guard separately binds the complete live open set through adoption.
    pub(super) fn validate_retained(
        &self,
        schema: &Arc<AuthoringSchemaReceipt>,
        recovery: &SourceUpgradeRecovery,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        with_temporary_storage(budget, |budget| {
            let count = self.inventory.captured_open_sources().len();
            let references = product(count, size_of::<OpenSource<'_>>())?;
            budget.charge_source_bytes(
                0,
                0,
                references,
                sum(size_of::<Vec<OpenSource<'_>>>(), references)?,
            )?;
            let mut sources = Vec::with_capacity(count);
            for source in self.inventory.captured_open_sources() {
                sources.push(OpenSource::Bound(source));
            }
            let result = self.validate(schema, recovery, &sources, budget);
            drop(sources);
            result
        })
    }

    /// Confirm the same operation, receipt lineage and complete future open set. No retained
    /// collection survives confirmation, including refusal; all performed work stays cumulative.
    pub(super) fn validate(
        &self,
        schema: &Arc<AuthoringSchemaReceipt>,
        recovery: &SourceUpgradeRecovery,
        complete_future_open: &[OpenSource<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        if !Arc::ptr_eq(&self.schema, schema) || self.operation != recovery.digest() {
            return Err(SourceInventoryError::AuthorityChanged.into());
        }
        self.inventory.validate_authority(schema)?;
        with_temporary_storage(budget, |budget| {
            let current = Self::capture(
                schema,
                recovery,
                complete_future_open,
                self.inventory.scene_limits(),
                budget,
            )?;
            budget.charge_source_bytes(0, 0, self.inventory.comparison_work()?, 0)?;
            if self.inventory != current.inventory {
                return Err(SourceInventoryError::MembershipChanged.into());
            }
            Ok(())
        })
    }
}

#[derive(Clone, Copy)]
enum RecoveryGeneration {
    Original,
    Successor,
    Mixed,
}

/// Admit the actual Schema-upgrade authority before supplying claims to the physical scanner.
/// The caller's candidate scope starts before this parser/claim scratch, so refusal returns all
/// new storage and handles while retaining cumulative work. No policy callback is involved.
fn capture_pending_inventory(
    input: SourceInventoryInput<'_>,
    bundle_bytes: &[u8],
    recovery: &SourceUpgradeRecovery,
    generation: RecoveryGeneration,
    complete_open: &[OpenSource<'_>],
    scene_limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SourceInventory, SourceUpgradeInventoryError> {
    let mut manifest = SourceInventoryManifest::parse(input.manifest_bytes, budget)?;
    validate_pending_authority(
        input,
        bundle_bytes,
        manifest.value_mut(),
        recovery,
        generation,
        budget,
    )?;
    let (settings, settings_bytes) = resolve_recovery_settings(
        input.manifest_bytes.len(),
        manifest.value_mut(),
        recovery,
        input.selected_settings,
        budget,
    )?;
    let slots = product(recovery.members().len(), size_of::<SourceClaim<'_>>())?;
    let scratch = sum(size_of::<Vec<SourceClaim<'_>>>(), slots)?;
    budget.charge_source_bytes(0, 0, slots, scratch)?;
    let claims: Vec<_> = recovery
        .members()
        .iter()
        .map(|member| SourceClaim {
            path: &member.path,
            kind: member.key.kind(),
            explicit_native: member.explicit_native,
        })
        .collect();
    let inventory = SourceInventory::capture_admitted(
        SourceInventoryInput {
            selected_settings: Some(&settings),
            ..input
        },
        manifest,
        complete_open,
        &claims,
        &[],
        scene_limits,
        budget,
    )?;
    drop(claims);
    release(budget, scratch)?;
    drop(settings);
    release(budget, settings_bytes)?;
    Ok(inventory)
}

fn resolve_recovery_settings(
    manifest_bytes: usize,
    manifest: &ProjectManifest,
    recovery: &SourceUpgradeRecovery,
    supplied: Option<&EffectiveProjectSettings>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(EffectiveProjectSettings, usize), SourceUpgradeInventoryError> {
    let selected = recovery.selected_profile();
    let selector_bytes = sum(
        selected.map_or(0, str::len),
        supplied
            .and_then(|settings| settings.profile_name.as_deref())
            .map_or(0, str::len),
    )?;
    budget.charge_source_bytes(0, 0, selector_bytes, 0)?;
    if supplied.is_some_and(|settings| settings.profile_name.as_deref() != selected) {
        return Err(SourceUpgradeInventoryError::RecoveryInventory);
    }
    // The existing grammar owns this finite settings projection. Reserve its strings and
    // collection slots before resolving, and keep that scratch until physical capture ends.
    let retained = sum(
        size_of::<EffectiveProjectSettings>(),
        product(manifest_bytes, 64)?,
    )?;
    budget.charge_source_bytes(0, 0, manifest_bytes, retained)?;
    let settings = manifest
        .resolve_profile(selected)
        .map_err(|_| SourceUpgradeInventoryError::RecoveryInventory)?;
    Ok((settings, retained))
}

fn validate_pending_authority(
    input: SourceInventoryInput<'_>,
    bundle_bytes: &[u8],
    manifest: &mut ProjectManifest,
    recovery: &SourceUpgradeRecovery,
    generation: RecoveryGeneration,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeInventoryError> {
    let ProjectAuthoringState::Pending(reference) = &manifest.authoring.state else {
        return Err(SourceUpgradeInventoryError::RecoveryInventory);
    };
    if reference.recovery_digest() != recovery.digest() {
        return Err(SourceUpgradeInventoryError::RecoveryInventory);
    }
    let work = sum(
        bundle_bytes.len(),
        sum(
            input.bundle_path.len(),
            reference.recovery_path().as_str().len(),
        )?,
    )?;
    budget.charge_source_bytes(0, 0, work, 0)?;
    let bundle = recovery.bundle();
    let matches_bundle = match generation {
        RecoveryGeneration::Mixed => match bundle.classify(Some(input.bundle_digest)) {
            RecoveryMemberState::Old | RecoveryMemberState::Unchanged => {
                bundle_bytes == bundle.old_disk.bytes()
            }
            RecoveryMemberState::New => bundle_bytes == bundle.successor.bytes(),
            RecoveryMemberState::Missing | RecoveryMemberState::Conflict => false,
        },
        RecoveryGeneration::Successor => {
            let expected = bundle.successor.digest();
            input.bundle_digest
                == ContentDigest::from_parts(expected.length(), *expected.as_bytes())
                && bundle_bytes == bundle.successor.bytes()
        }
        RecoveryGeneration::Original => false,
    };
    if input.bundle_path != bundle.path
        || !matches_bundle
        || manifest.authoring.schema_bundle.as_deref() != Some(recovery.bundle().path.as_str())
        || manifest.project.source_universe != recovery.source_universe()
        || input.selected_settings.is_some_and(|settings| {
            settings.project.source_universe != recovery.source_universe()
                || settings.authoring_state != manifest.authoring.state
        })
    {
        return Err(SourceUpgradeInventoryError::RecoveryInventory);
    }
    validate_recovery_configuration(
        input.manifest_bytes.len(),
        manifest,
        recovery.manifest().old_disk.bytes(),
        budget,
    )?;
    if matches!(generation, RecoveryGeneration::Mixed) {
        // Each temporary model drops and returns its storage before parsing the next one.
        // No Ready receipt or ordinary admission is manufactured from either definition.
        validate_recovery_configuration(
            input.manifest_bytes.len(),
            manifest,
            recovery.manifest().successor.bytes(),
            budget,
        )?;
    }
    Ok(())
}

fn validate_recovery_configuration(
    current_bytes: usize,
    manifest: &mut ProjectManifest,
    expected_bytes: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeInventoryError> {
    with_temporary_storage(budget, |budget| {
        let expected = parse_recovery_manifest(expected_bytes, budget)?;
        budget.charge_source_bytes(0, 0, sum(current_bytes, expected_bytes.len())?, 0)?;
        // Normalize only this temporary parsed value for complete modeled comparison, including
        // every profile. Restore the real Pending state before constructing native policies.
        let pending =
            std::mem::replace(&mut manifest.authoring.state, ProjectAuthoringState::Ready);
        let same =
            expected.authoring.state == ProjectAuthoringState::Ready && *manifest == expected;
        manifest.authoring.state = pending;
        if !same {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
        Ok(())
    })
}

fn parse_recovery_manifest(
    bytes: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ProjectManifest, SourceUpgradeInventoryError> {
    // Match the existing manifest owner's 4,096-node ceiling, reserving resolved profile
    // storage before its parser runs. Strings are charged separately from model slots.
    let slot = sum(
        size_of::<nara_project::ProjectProfileOverlay>(),
        size_of::<EffectiveProjectSettings>(),
    )?;
    let retained = sum(
        size_of::<ProjectManifest>(),
        sum(
            product(bytes.len(), 8)?,
            product(bytes.len().min(4_096), slot)?,
        )?,
    )?;
    budget.charge_source_bytes(0, retained, product(bytes.len(), 4)?, retained)?;
    let load = ProjectManifest::parse_toml_bytes(bytes);
    if load.has_errors() {
        return Err(SourceInventoryError::ManifestInvalid.into());
    }
    load.manifest
        .ok_or(SourceInventoryError::ManifestInvalid)
        .map_err(Into::into)
}

/// Joins a complete ordinary Ready capture to the recovery owner's original disk set.
/// Working-copy conversion and current name revalidation remain with their owning callers.
pub(super) fn validate_original_recovery(
    inventory: &SourceInventory,
    schema: &AuthoringSchemaReceipt,
    recovery: &SourceUpgradeRecovery,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeInventoryError> {
    inventory.validate_authority(schema)?;
    let bytes = sum(
        sum(schema.manifest_bytes().len(), schema.bundle_bytes().len())?,
        sum(
            schema.bundle_path().as_str().len(),
            sum(
                inventory.selected_profile().map_or(0, str::len),
                recovery.selected_profile().map_or(0, str::len),
            )?,
        )?,
    )?;
    budget.charge_source_bytes(0, 0, bytes, 0)?;
    if schema.manifest_bytes() != recovery.manifest().old_disk.bytes()
        || schema.bundle_bytes() != recovery.bundle().old_disk.bytes()
        || schema.bundle_path().as_str() != recovery.bundle().path
        || inventory.selected_profile() != recovery.selected_profile()
    {
        return Err(SourceUpgradeInventoryError::RecoveryInventory);
    }
    validate_recovery_members(inventory, recovery, RecoveryGeneration::Original, budget)
}

fn validate_recovery_members(
    inventory: &SourceInventory,
    recovery: &SourceUpgradeRecovery,
    generation: RecoveryGeneration,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeInventoryError> {
    if inventory.participants().len() != recovery.members().len()
        || inventory.metadata().len() != recovery.metadata().len()
        || inventory.roots().len() != recovery.roots().len()
        || inventory.missing_sidecars().len() != recovery.missing_sidecars().len()
    {
        return Err(SourceUpgradeInventoryError::RecoveryInventory);
    }
    // Charge comparisons before looking at source bodies, complete keys or payload digests.
    // Metadata is indexed by source path, whereas the package sorts sidecar paths. Those
    // orders can differ when one source path prefixes another, so use the existing index.
    let mut work = inventory.comparison_work()?;
    // Include linear search within std's BTree nodes as well as descent, rather than
    // charging one byte comparison per abstract tree level.
    let lookup_depth = product(
        (usize::BITS - inventory.metadata().len().leading_zeros()) as usize + 1,
        16,
    )?;
    for metadata in recovery.metadata() {
        work = sum(work, product(metadata.path.len(), lookup_depth)?)?;
    }
    budget.charge_source_bytes(0, 0, work, 0)?;
    for (participant, member) in inventory.participants().zip(recovery.members()) {
        let encoding = match participant.encoding() {
            SourceEncoding::Json => RecoverySourceEncoding::Json,
            SourceEncoding::Ron => RecoverySourceEncoding::Ron,
        };
        let bytes_match = match generation {
            RecoveryGeneration::Original => participant.bytes() == member.old_disk.bytes(),
            RecoveryGeneration::Successor => participant.bytes() == member.successor.bytes(),
            RecoveryGeneration::Mixed => match member.classify(Some(participant.file().digest())) {
                RecoveryMemberState::Old | RecoveryMemberState::Unchanged => {
                    participant.bytes() == member.old_disk.bytes()
                }
                RecoveryMemberState::New => participant.bytes() == member.successor.bytes(),
                RecoveryMemberState::Missing | RecoveryMemberState::Conflict => false,
            },
        };
        if participant.file().path() != member.path
            || participant.source_key() != &member.key
            || participant.kind() != member.key.kind()
            || encoding != member.encoding
            || participant.explicit_native() != member.explicit_native
            || !bytes_match
        {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
    }
    for proposed in recovery.metadata() {
        let observed = proposed
            .path
            .strip_suffix(".meta")
            .and_then(|source| inventory.metadata_for_source(source))
            .ok_or(SourceUpgradeInventoryError::RecoveryInventory)?;
        let digest = proposed.observed.digest();
        if observed.file().path() != proposed.path
            || observed.file().digest()
                != ContentDigest::from_parts(digest.length(), *digest.as_bytes())
        {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
    }
    for (observed, proposed) in inventory.roots().zip(recovery.roots()) {
        let same = match (observed.observation(), &proposed.observation) {
            (SourceRootObservation::Available { .. }, RecoveryRootObservation::Present) => true,
            (
                SourceRootObservation::Missing { ancestor_path, .. },
                RecoveryRootObservation::Missing {
                    ancestor_path: proposed,
                },
            ) => ancestor_path == proposed,
            _ => false,
        };
        if observed.path() != proposed.path || !same {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
    }
    if inventory
        .missing_sidecars()
        .ne(recovery.missing_sidecars().iter().map(String::as_str))
    {
        return Err(SourceUpgradeInventoryError::RecoveryInventory);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceUpgradeInventoryError {
    #[error("the physical source inventory could not be admitted or confirmed")]
    Inventory(#[from] SourceInventoryError),
    #[cfg(any(feature = "tooling", feature = "project-tooling", test))]
    #[error("the actual Pending manifest or recovery observation changed")]
    PendingStorage(#[source] RecoveryStorageError),
    #[error("the complete inventory differs from the verified recovery declarations")]
    RecoveryInventory,
}

impl From<AuthoringProjectAdmissionError> for SourceUpgradeInventoryError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        SourceInventoryError::Admission(error).into()
    }
}

impl From<FsError> for SourceUpgradeInventoryError {
    fn from(error: FsError) -> Self {
        SourceInventoryError::Host(error).into()
    }
}

#[cfg(any(feature = "tooling", feature = "project-tooling", test))]
impl From<RecoveryStorageError> for SourceUpgradeInventoryError {
    fn from(error: RecoveryStorageError) -> Self {
        match error {
            RecoveryStorageError::Admission(error) => Self::from(error),
            error => Self::PendingStorage(error),
        }
    }
}
