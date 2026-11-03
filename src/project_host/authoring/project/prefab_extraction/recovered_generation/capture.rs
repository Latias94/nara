//! Decode fixed before/after sources and reconstruct actual configuration-local associations.

use super::*;
use crate::project_host::authoring::source_inventory::{product, sum, with_candidate_storage};

pub(super) struct Inputs {
    pub(super) disk: Vec<CapturedExtractionSource>,
    pub(super) request: PrefabExtractionRequest,
    pub(super) outputs: Vec<CapturedExtractionSource>,
    pub(super) extracted: ResolvedPrefabSource,
    pub(super) published: Vec<bool>,
}

pub(super) fn sources(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    observation: &RecoveredPrefabExtractionInventory,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: &mut PrefabExtractionWork,
) -> Result<Inputs, RecoveredPrefabExtractionGenerationError> {
    // Failed locals end before Host storage is refunded. Source Work is cumulative even when
    // a decoder or a catalogue refuses; neither retry nor a later stage receives a fresh cap.
    with_candidate_storage(budget, |budget| {
        let recovery = pending.recovery();
        let inventory = observation.inventory();
        let basis = observation.basis();
        let count = recovery.complete_original_sources().len();
        work.admit_capture_storage(Some(sum(
            size_of::<RecoveredPrefabExtractionGeneration>()
                + size_of::<RecoveredPrefabExtractionGenerationFailure>()
                + size_of::<Inputs>(),
            product(
                count,
                3 * size_of::<CapturedExtractionSource>() + size_of::<bool>(),
            )?,
        )?))
        .map_err(RecoveredPrefabExtractionGenerationError::SourceWork)?;
        let mut disk = Vec::with_capacity(count);
        let mut working = Vec::with_capacity(count);
        let mut outputs = Vec::with_capacity(count);
        let mut published = Vec::with_capacity(count);
        for original in recovery.complete_original_sources() {
            let before = source(
                original.key(),
                original.path(),
                original.encoding().into(),
                original.original_disk().bytes(),
                inventory,
                basis,
                limits,
                budget,
                work,
            )?;
            let current = if same_bytes(
                original.original_disk().bytes(),
                original.reviewed_working().bytes(),
                budget,
            )? {
                share(&before, work)?
            } else {
                source(
                    original.key(),
                    original.path(),
                    original.encoding().into(),
                    original.reviewed_working().bytes(),
                    inventory,
                    basis,
                    limits,
                    budget,
                    work,
                )?
            };
            let after = match original.output() {
                None => share(&current, work)?,
                Some(output)
                    if same_bytes(output.bytes(), original.reviewed_working().bytes(), budget)? =>
                {
                    share(&current, work)?
                }
                Some(output) => source(
                    original.key(),
                    original.path(),
                    original.encoding().into(),
                    output.bytes(),
                    inventory,
                    basis,
                    limits,
                    budget,
                    work,
                )?,
            };
            disk.push(before);
            working.push(current);
            outputs.push(after);
            published.push(original.output().is_some());
        }
        let metadata_bytes = recovery.q_sidecar().output().bytes();
        // Only the small metadata parser is temporary Host storage. Its returned identity and
        // fixed destination record are reserved under source Work before that parser runs.
        work.admit_capture_storage(Some(sum(
            product(metadata_bytes.len(), 4)?,
            size_of::<AssetMetaIdentity>() + size_of::<AssetRecord>(),
        )?))
        .map_err(RecoveredPrefabExtractionGenerationError::SourceWork)?;
        let metadata = recovery.destination_identity(budget)?;
        if metadata.source_kind() != &AssetSourceKind::Prefab {
            return Err(RecoveredPrefabExtractionGenerationError::Destination);
        }
        let policy = inventory
            .configurations()
            .iter()
            .find(|policy| policy.profile_name() == recovery.selected_profile())
            .ok_or(RecoveredPrefabExtractionGenerationError::Destination)?;
        let binding = policy
            .catalogue_bindings()
            .iter()
            .find(|binding| binding.expected_kind == Some(AssetSourceKind::Prefab))
            .ok_or(RecoveredPrefabExtractionGenerationError::Destination)?;
        work.admit_capture_storage(Some(checked_sum([
            size_of::<AssetRecord>() + size_of::<AssetMetaIdentity>(),
            metadata.path().as_str().len(),
            binding
                .scope()
                .prefix()
                .map_or(0, |path| path.as_str().len()),
            metadata.path().as_str().len(),
            1,
        ])?))
        .map_err(RecoveredPrefabExtractionGenerationError::SourceWork)?;
        let record = metadata.clone().into_record(binding.scope());
        if record.path().as_str() != recovery.q().path()
            || record.source_kind() != &AssetSourceKind::Prefab
            || record
                .validate_source_document_key(recovery.q().key())
                .is_err()
        {
            return Err(RecoveredPrefabExtractionGenerationError::Destination);
        }
        let (environments, selected_environment) = environments::capture(
            pending.root().share(),
            recovery.selected_profile(),
            inventory,
            &working,
            environments::ExtractionDestination {
                key: recovery.q().key(),
                record: &record,
                authored_path: metadata.path(),
            },
            budget,
            work,
        )?;
        let q = source(
            recovery.q().key(),
            recovery.q().path(),
            recovery.q().encoding().into(),
            recovery.q().output().bytes(),
            inventory,
            basis,
            limits,
            budget,
            work,
        )?;
        let CapturedExtractionSource::Prefab { source: extracted } = q else {
            return Err(RecoveredPrefabExtractionGenerationError::Destination);
        };
        let extracted = extracted
            .with_source_asset(&record)
            .map_err(|_| RecoveredPrefabExtractionGenerationError::Destination)?;
        work.admit_capture_storage(Some(checked_sum([
            recovery.selection().source().retained_bytes(),
            recovery.selection().root().as_str().len(),
            recovery.q().key().retained_bytes(),
        ])?))
        .map_err(RecoveredPrefabExtractionGenerationError::SourceWork)?;
        Ok(Inputs {
            disk,
            request: PrefabExtractionRequest::new(
                working,
                environments,
                selected_environment,
                basis.clone(),
                recovery.selection().source().clone(),
                recovery.selection().root().clone(),
                nara_scene::__private::PrefabExtractionDestination {
                    key: recovery.q().key().clone(),
                    reference: AssetRef::StableId(record.stable_id()),
                    metadata: record,
                },
            ),
            outputs,
            extracted,
            published,
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn source(
    key: &SourceDocumentKey,
    path: &str,
    encoding: SceneFileEncoding,
    bytes: &[u8],
    inventory: &SourceInventory,
    basis: &AuthoringSchemaBasis,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: &mut PrefabExtractionWork,
) -> Result<CapturedExtractionSource, RecoveredPrefabExtractionGenerationError> {
    let tree = decode_source(key.kind(), encoding, bytes, limits, budget)?;
    let retained = tree.retained_bytes()?;
    work.admit_capture_storage(Some(checked_sum([
        retained,
        key.retained_bytes(),
        4 * size_of::<usize>(),
    ])?))
    .map_err(RecoveredPrefabExtractionGenerationError::SourceWork)?;
    let source = match tree {
        SourceTree::Scene(document) => CapturedExtractionSource::Scene {
            key: key.clone(),
            document: Arc::new(document),
            basis: basis.clone(),
        },
        SourceTree::Prefab(document) => CapturedExtractionSource::Prefab {
            source: associated_prefab(Arc::new(document), key, path, basis, inventory)?,
        },
    };
    budget.reconcile_retained_reservation(retained, 0)?;
    Ok(source)
}

fn share(
    source: &CapturedExtractionSource,
    work: &mut PrefabExtractionWork,
) -> Result<CapturedExtractionSource, RecoveredPrefabExtractionGenerationError> {
    work.admit_capture_storage(Some(source.key().retained_bytes()))
        .map_err(RecoveredPrefabExtractionGenerationError::SourceWork)?;
    Ok(share_source(source))
}

fn same_bytes(
    left: &[u8],
    right: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<bool, RecoveredPrefabExtractionGenerationError> {
    budget.charge_source_bytes(0, 0, sum(left.len(), right.len())?, 0)?;
    Ok(left == right)
}
