//! Ordinary native admission of the complete published generation under the same Pending.
//!
//! The original live or recovered publication stays owned. This proves a new physical inventory,
//! not a Ready manifest, future Project, saved installation or a new Schema basis.

use super::super::{
    pending::{PendingConfigurationError, validate_configuration},
    recovery_data::RecoverySourceEncoding,
};
use super::*;
use crate::project_host::authoring::{
    document::AuthoringDocumentSourceFact,
    source_inventory::{
        OpenSource, SourceClaim, SourceEncoding, SourceInventoryInput, SourceInventoryManifest,
        revalidate_file_facts, with_candidate_storage, with_temporary_storage,
    },
};

#[derive(Debug)]
#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct ProjectPrefabExtractionFutureInventory
{
    pub(super) publication: ProjectPrefabExtractionPublication,
    pub(super) inventory: SourceInventory,
}

#[derive(Debug, thiserror::Error)]
pub(in crate::project_host::authoring::project::prefab_extraction) enum ProjectPrefabExtractionFutureInventoryError
{
    #[error("future extraction inventory requires completed actual files and checkpoints")]
    Phase,
    #[error("extraction publication observations changed")]
    Publication(#[from] ProjectPrefabExtractionPublicationError),
    #[error("extraction Pending configuration changed")]
    Configuration(#[from] PendingConfigurationError),
    #[error("complete future extraction native inventory admission failed")]
    Inventory(#[from] SourceInventoryError),
    #[error("future extraction inventory differs from its fixed complete outputs and guards")]
    Generation,
}

impl From<AuthoringProjectAdmissionError> for ProjectPrefabExtractionFutureInventoryError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self::Inventory(error.into())
    }
}

#[derive(Debug)]
#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct ProjectPrefabExtractionFutureInventoryFailure
{
    publication: ProjectPrefabExtractionPublication,
    error: ProjectPrefabExtractionFutureInventoryError,
}

impl ProjectPrefabExtractionFutureInventoryFailure {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_parts(
        self,
    ) -> (
        ProjectPrefabExtractionPublication,
        ProjectPrefabExtractionFutureInventoryError,
    ) {
        (self.publication, self.error)
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn owner(
        &self,
    ) -> &ProjectPrefabExtractionPublication {
        &self.publication
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn error(
        &self,
    ) -> &ProjectPrefabExtractionFutureInventoryError {
        &self.error
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn retry(
        self,
    ) -> Result<ProjectPrefabExtractionFutureInventory, Self> {
        self.publication.into_future_inventory()
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_owner(
        self,
    ) -> ProjectPrefabExtractionPublication {
        self.publication
    }
}

impl ProjectPrefabExtractionPublication {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_future_inventory(
        mut self,
    ) -> Result<ProjectPrefabExtractionFutureInventory, ProjectPrefabExtractionFutureInventoryFailure>
    {
        let result = (|| {
            if self.phase() != ProjectPrefabExtractionPublicationPhase::CheckpointsComplete
                || self.cleanup_pending()
                || self.uncertain()
            {
                return Err(ProjectPrefabExtractionFutureInventoryError::Phase);
            }
            // Validate actual Pending and every completed file result, not the obsolete original
            // Ready/absent-destination observation. A completed cursor does no new writes.
            self.step()?;
            let inputs = self.origin.inputs()?;
            capture(inputs, &self.entries)
        })();
        match result {
            Ok(inventory) => Ok(ProjectPrefabExtractionFutureInventory {
                publication: self,
                inventory,
            }),
            Err(error) => Err(ProjectPrefabExtractionFutureInventoryFailure {
                publication: self,
                error,
            }),
        }
    }
}

impl ProjectPrefabExtractionFutureInventory {
    #[cfg(feature = "tooling")]
    pub(in crate::project_host::authoring::project::prefab_extraction) fn session_inputs_mut(
        &mut self,
    ) -> (&SourceInventory, &mut ProjectExtractionContext) {
        (&self.inventory, self.publication.session_context_mut())
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn publication_mut(
        &mut self,
    ) -> &mut ProjectPrefabExtractionPublication {
        &mut self.publication
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn inventory(
        &self,
    ) -> &SourceInventory {
        &self.inventory
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn publication(
        &self,
    ) -> &ProjectPrefabExtractionPublication {
        &self.publication
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn host_usage(
        &mut self,
    ) -> Result<AuthoringProjectAdmissionUsage, ProjectPrefabExtractionFutureInventoryError> {
        Ok(self.publication.host_usage()?)
    }

    /// Reobserves this same admitted inventory. No second inventory or basis is constructed.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn validate(
        &mut self,
    ) -> Result<(), ProjectPrefabExtractionFutureInventoryError> {
        let PublicationInputs {
            pending,
            bundle,
            budget,
            ..
        } = self.publication.origin.inputs()?;
        validate_inventory(&self.inventory, pending, bundle, budget)
    }
}

pub(super) fn validate_inventory(
    inventory: &SourceInventory,
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    bundle: PublicationBundle<'_>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ProjectPrefabExtractionFutureInventoryError> {
    validate_bundle(pending, bundle, budget)?;
    with_temporary_storage(
        budget,
        |budget| -> Result<(), ProjectPrefabExtractionFutureInventoryError> {
            let before = budget.retained_candidate_bytes;
            let (manifest, settings, settings_bytes) = configuration(pending, budget)?;
            // Validation parses the actual manifest through the shared namespace walker. The
            // configuration value above only checks the operation's exact Pending commitment.
            let parser_bytes = budget
                .retained_candidate_bytes
                .checked_sub(before)
                .and_then(|bytes| bytes.checked_sub(settings_bytes))
                .ok_or(SourceInventoryError::AccountingOverflow)?;
            drop(manifest);
            release(budget, parser_bytes)?;
            let bytes = product(
                inventory.captured_open_sources().len(),
                size_of::<OpenSource<'_>>(),
            )?;
            budget.charge_source_bytes(0, 0, bytes, bytes)?;
            let open: Vec<_> = inventory
                .captured_open_sources()
                .map(OpenSource::Bound)
                .collect();
            inventory.validate_admitted_observations(
                source_input(pending, bundle, Some(&settings)),
                &open,
                budget,
            )?;
            Ok(())
        },
    )?;
    validate_bundle(pending, bundle, budget)?;
    Ok(())
}

fn capture(
    inputs: PublicationInputs<'_>,
    entries: &[Entry],
) -> Result<SourceInventory, ProjectPrefabExtractionFutureInventoryError> {
    let PublicationInputs {
        pending,
        inventory: original,
        budget,
        bundle,
        ..
    } = inputs;
    with_candidate_storage(budget, |budget| {
        let wrapper = size_of::<ProjectPrefabExtractionFutureInventoryError>();
        budget.charge_source_bytes(0, 0, wrapper, wrapper)?;
        validate_bundle(pending, bundle, budget)?;
        let (manifest, settings, settings_bytes) = configuration(pending, budget)?;
        let count = sum(pending.recovery().complete_original_sources().len(), 1)?;
        let slots = product(count, size_of::<SourceClaim<'_>>())?;
        budget.charge_source_bytes(0, 0, slots, slots)?;
        let recovery = pending.recovery();
        let mut claims = Vec::with_capacity(count);
        claims.extend(
            recovery
                .complete_original_sources()
                .iter()
                .map(|source| SourceClaim {
                    path: source.path(),
                    kind: source.key().kind(),
                    explicit_native: source.explicit_native(),
                }),
        );
        let comparisons = (usize::BITS - count.leading_zeros()) as usize + 1;
        budget.charge_source_bytes(
            0,
            0,
            product(sum(recovery.q().path().len(), 1)?, comparisons)?,
            0,
        )?;
        let q = claims.partition_point(|claim| claim.path < recovery.q().path());
        claims.insert(
            q,
            SourceClaim {
                path: recovery.q().path(),
                kind: SourceDocumentKind::Prefab,
                explicit_native: false,
            },
        );

        let (open, open_bytes) = capture_open(original, recovery, entries, budget)?;
        let open_slots = product(open.len(), size_of::<OpenSource<'_>>())?;
        budget.charge_source_bytes(0, 0, open_slots, open_slots)?;
        let borrowed: Vec<_> = open.iter().map(OpenSource::Bound).collect();
        let inventory = SourceInventory::capture_admitted(
            source_input(pending, bundle, Some(&settings)),
            manifest,
            &borrowed,
            &claims,
            &[],
            original.scene_limits(),
            budget,
        )?;
        drop(borrowed);
        // The admitted inventory now owns its exact complete open facts. The borrowed input
        // wrappers have no second continuing role once ordinary capture has accepted them.
        drop(open);
        drop(claims);
        drop(settings);
        release(
            budget,
            sum(sum(sum(slots, open_slots)?, open_bytes)?, settings_bytes)?,
        )?;
        validate_generation(&inventory, original, recovery, entries, budget)?;
        validate_bundle(pending, bundle, budget)?;
        Ok(inventory)
    })
}

fn configuration(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<
    (
        SourceInventoryManifest,
        nara_project::EffectiveProjectSettings,
        usize,
    ),
    ProjectPrefabExtractionFutureInventoryError,
> {
    let mut manifest = SourceInventoryManifest::parse(pending.manifest_bytes(), budget)?;
    validate_configuration(pending, &mut manifest, budget)?;
    let bytes = product(pending.manifest_bytes().len(), 128)?;
    budget.charge_source_bytes(
        0,
        pending.manifest_bytes().len(),
        sum(
            pending.manifest_bytes().len(),
            pending.recovery().selected_profile().map_or(0, str::len),
        )?,
        bytes,
    )?;
    let settings = manifest
        .value_mut()
        .resolve_profile(pending.recovery().selected_profile())
        .map_err(|_| ProjectPrefabExtractionFutureInventoryError::Generation)?;
    Ok((manifest, settings, bytes))
}

fn source_input<'a>(
    pending: &'a ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    bundle: PublicationBundle<'a>,
    settings: Option<&'a nara_project::EffectiveProjectSettings>,
) -> SourceInventoryInput<'a> {
    SourceInventoryInput {
        root: pending.root(),
        manifest_bytes: pending.manifest_bytes(),
        manifest_identity: pending.capability().identity(),
        manifest_digest: pending.digest(),
        bundle_path: bundle.path,
        bundle_identity: bundle.identity,
        bundle_digest: bundle.digest,
        selected_settings: settings,
    }
}

fn validate_bundle(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    bundle: PublicationBundle<'_>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ProjectPrefabExtractionFutureInventoryError> {
    with_temporary_storage(budget, |budget| {
        pending
            .validate(budget)
            .map_err(ProjectPrefabExtractionPublicationError::from)?;
        revalidate_file_facts(
            pending.root(),
            bundle.path,
            bundle.identity,
            bundle.parent,
            bundle.digest,
            budget,
        )?;
        Ok(())
    })
}

fn capture_open(
    original: &SourceInventory,
    recovery: &PrefabExtractionRecovery,
    entries: &[Entry],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(Vec<AuthoringDocumentSourceFact>, usize), ProjectPrefabExtractionFutureInventoryError>
{
    // The original inventory already owns the complete admitted source facts. A recovered
    // origin has no open facts; no Editor IDs or synthetic Workspace participate here.
    let facts = original.captured_open_sources();
    let slots = product(facts.len(), size_of::<AuthoringDocumentSourceFact>())?;
    budget.charge_source_bytes(0, 0, slots, slots)?;
    let mut open = Vec::with_capacity(facts.len());
    let mut retained = slots;
    let originals = recovery.complete_original_sources();
    let lookups = (usize::BITS - originals.len().leading_zeros()) as usize + 1;
    for fact in facts {
        let bytes = fact
            .checked_retained_bytes()
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        budget.charge_source_bytes(
            0,
            0,
            sum(bytes, product(fact.relative_path().len(), lookups)?)?,
            bytes,
        )?;
        retained = sum(retained, bytes)?;
        let index = originals
            .binary_search_by(|source| source.path().cmp(fact.relative_path()))
            .map_err(|_| ProjectPrefabExtractionFutureInventoryError::Generation)?;
        let value = if originals[index].output().is_some() {
            let result = original_result(entries, index, budget)?;
            if !result.accepted_checkpoint() {
                return Err(ProjectPrefabExtractionFutureInventoryError::Phase);
            }
            let (identity, parent, digest) = result.observation();
            fact.with_persistence_observation(identity, parent, digest)
                .map_err(|_| ProjectPrefabExtractionFutureInventoryError::Generation)?
        } else {
            fact.clone()
        };
        open.push(value);
    }
    Ok((open, retained))
}

fn original_result<'a>(
    entries: &'a [Entry],
    index: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<&'a ExtractionFileResult, ProjectPrefabExtractionFutureInventoryError> {
    let originals = entries
        .get(2..)
        .ok_or(ProjectPrefabExtractionFutureInventoryError::Phase)?;
    let lookups = (usize::BITS - originals.len().leading_zeros()) as usize + 1;
    budget.charge_source_bytes(0, 0, product(lookups, size_of::<Entry>())?, 0)?;
    let position = originals
        .binary_search_by_key(&index, |entry| match entry.member {
            ExtractionFileMember::Original(index) => index,
            _ => usize::MAX,
        })
        .map_err(|_| ProjectPrefabExtractionFutureInventoryError::Generation)?;
    originals[position]
        .result
        .as_ref()
        .ok_or(ProjectPrefabExtractionFutureInventoryError::Phase)
}

fn validate_generation(
    future: &SourceInventory,
    original: &SourceInventory,
    recovery: &PrefabExtractionRecovery,
    entries: &[Entry],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ProjectPrefabExtractionFutureInventoryError> {
    future.validate_namespace_continuity(original, budget)?;
    budget.charge_source_bytes(
        0,
        0,
        sum(future.comparison_work()?, original.comparison_work()?)?,
        0,
    )?;
    if future.participants().len() != sum(original.participants().len(), 1)?
        || future.metadata().len() != sum(original.metadata().len(), 1)?
        || original.participants().len() != recovery.complete_original_sources().len()
    {
        return Err(ProjectPrefabExtractionFutureInventoryError::Generation);
    }
    let lookups = (usize::BITS
        - future
            .participants()
            .len()
            .max(future.metadata().len())
            .leading_zeros()) as usize
        + 1;
    for (index, stored) in recovery.complete_original_sources().iter().enumerate() {
        budget.charge_source_bytes(
            0,
            0,
            product(sum(stored.path().len(), 1)?, product(lookups, 2)?)?,
            0,
        )?;
        let before = original
            .participant(stored.path())
            .ok_or(ProjectPrefabExtractionFutureInventoryError::Generation)?;
        let after = future
            .participant(stored.path())
            .ok_or(ProjectPrefabExtractionFutureInventoryError::Generation)?;
        if let Some(output) = stored.output() {
            let result = original_result(entries, index, budget)?;
            if after.source_key() != before.source_key()
                || after.encoding() != before.encoding()
                || after.explicit_native() != before.explicit_native()
                || !after
                    .native_source()
                    .matches_source_identity_update(before.native_source(), after.file().identity())
                || !matches_file(after.file(), result)
                || !matches_blob(output, after.file().digest(), after.bytes())
            {
                return Err(ProjectPrefabExtractionFutureInventoryError::Generation);
            }
        } else if after != before {
            return Err(ProjectPrefabExtractionFutureInventoryError::Generation);
        }
    }
    for before in original.metadata() {
        budget.charge_source_bytes(
            0,
            0,
            product(sum(before.record().path().as_str().len(), 1)?, lookups)?,
            0,
        )?;
        if future.metadata_for_source(before.record().path().as_str()) != Some(before) {
            return Err(ProjectPrefabExtractionFutureInventoryError::Generation);
        }
    }
    budget.charge_source_bytes(
        0,
        0,
        product(sum(recovery.q().path().len(), 1)?, product(lookups, 2)?)?,
        0,
    )?;
    let q = future
        .participant(recovery.q().path())
        .ok_or(ProjectPrefabExtractionFutureInventoryError::Generation)?;
    let metadata = future
        .metadata_for_source(recovery.q().path())
        .ok_or(ProjectPrefabExtractionFutureInventoryError::Generation)?;
    let q_result = entries
        .first()
        .and_then(|entry| entry.result.as_ref())
        .ok_or(ProjectPrefabExtractionFutureInventoryError::Phase)?;
    let meta_result = entries
        .get(1)
        .and_then(|entry| entry.result.as_ref())
        .ok_or(ProjectPrefabExtractionFutureInventoryError::Phase)?;
    let encoding = match q.encoding() {
        SourceEncoding::Json => RecoverySourceEncoding::Json,
        SourceEncoding::Ron => RecoverySourceEncoding::Ron,
    };
    if q.source_key() != recovery.q().key()
        || encoding != recovery.q().encoding()
        || !matches_file(q.file(), q_result)
        || !matches_blob(recovery.q().output(), q.file().digest(), q.bytes())
        || metadata.file().path() != recovery.q_sidecar().path()
        || !matches_file(metadata.file(), meta_result)
        || !matches_blob(
            recovery.q_sidecar().output(),
            metadata.file().digest(),
            metadata.bytes(),
        )
        || metadata.record().path().as_str() != recovery.q().path()
        || metadata.record().source_kind() != &AssetSourceKind::Prefab
    {
        return Err(ProjectPrefabExtractionFutureInventoryError::Generation);
    }
    for (configuration, (after, before)) in future
        .configurations()
        .iter()
        .zip(original.configurations())
        .enumerate()
    {
        let filter_work = after
            .participant_paths()
            .try_fold(0, |bytes, path| sum(bytes, path.len()))?;
        budget.charge_source_bytes(
            0,
            0,
            sum(
                filter_work,
                product(metadata.visibility().len(), size_of::<usize>())?,
            )?,
            0,
        )?;
        if after
            .participant_paths()
            .filter(|path| *path != recovery.q().path())
            .ne(before.participant_paths())
        {
            return Err(ProjectPrefabExtractionFutureInventoryError::Generation);
        }
        // Use the same catalogue rule as original environment admission. The new native
        // participant's physical inclusion does not make it visible to every catalogue.
        let planned = ReferenceCatalogue::planned_metadata_binding(
            after.catalogue_bindings(),
            metadata.record().path(),
            metadata.authored_path(),
            budget,
        )
        .map_err(ProjectPrefabExtractionPublicationError::from)?;
        let actual = metadata
            .visibility()
            .iter()
            .find(|visibility| visibility.configuration_index() == configuration)
            .and_then(|visibility| visibility.catalogue_binding_index());
        if planned != actual {
            return Err(ProjectPrefabExtractionFutureInventoryError::Generation);
        }
    }
    Ok(())
}

fn matches_file(
    file: &crate::project_host::authoring::source_inventory::SourceFileFact,
    result: &ExtractionFileResult,
) -> bool {
    (file.identity(), file.parent_identity(), file.digest()) == result.observation()
}

fn matches_blob(
    blob: &super::super::recovery_data::RecoveryBlob,
    digest: ContentDigest,
    bytes: &[u8],
) -> bool {
    digest == ContentDigest::from_parts(blob.digest().length(), *blob.digest().as_bytes())
        && bytes == blob.bytes()
}
