//! Frozen catalogue associations over one physical source table.

use nara_asset::{AssetProductRef, ImportedProductId};
use nara_scene::__private::{
    PrefabExtractionAssets, PrefabExtractionBinding, PrefabExtractionEnvironment,
};

use super::*;

pub(super) struct ExtractionDestination<'a> {
    pub(super) key: &'a SourceDocumentKey,
    pub(super) record: &'a AssetRecord,
    pub(super) authored_path: &'a AssetPath,
}

pub(super) fn capture(
    root: DirectoryCapability,
    selected_profile: Option<&str>,
    inventory: &SourceInventory,
    sources: &[CapturedExtractionSource],
    destination: ExtractionDestination<'_>,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: &mut PrefabExtractionWork,
) -> Result<(Vec<PrefabExtractionEnvironment>, usize), PrefabExtractionProjectError> {
    work.admit_capture_storage(
        inventory
            .configurations()
            .len()
            .checked_mul(size_of::<PrefabExtractionEnvironment>()),
    )
    .map_err(PrefabExtractionProjectError::SourceWork)?;
    let mut environments = Vec::with_capacity(inventory.configurations().len());
    let mut selected = None;
    for (configuration_index, policy) in inventory.configurations().iter().enumerate() {
        // A future sidecar may be outside this catalogue, admitted to it, or invalidate its
        // declaration rules. Validate even empty configurations before omitting Core members.
        let visible_q = ReferenceCatalogue::planned_metadata_binding(
            policy.catalogue_bindings(),
            destination.record.path(),
            destination.authored_path,
            budget,
        )?
        .is_some();
        // Empty configurations still belong to the inventory and revalidation. They have no
        // original source interpretation to compile, and must not acquire fabricated members.
        if policy.participant_paths().len() == 0 {
            for metadata in inventory.metadata() {
                ReferenceCatalogue::planned_metadata_binding(
                    policy.catalogue_bindings(),
                    metadata.record().path(),
                    metadata.authored_path(),
                    budget,
                )?;
            }
            continue;
        }
        let mut catalogue = ReferenceCatalogue::from_inventory(
            root.share(),
            inventory,
            configuration_index,
            budget,
        )?;
        let name_bytes = policy
            .profile_name()
            .map_or("default".len(), |name| "profile:".len() + name.len());
        work.admit_capture_storage(Some(checked_sum([
            name_bytes,
            checked_product(policy.participant_paths().len(), size_of::<usize>())?,
        ])?))
        .map_err(PrefabExtractionProjectError::SourceWork)?;
        let name = policy
            .profile_name()
            .map_or_else(|| "default".to_owned(), |name| format!("profile:{name}"));
        let mut members = Vec::with_capacity(policy.participant_paths().len());
        let mut participants = policy.participant_paths().peekable();
        let binding_capacity = checked_product(policy.participant_paths().len(), 3)?;
        work.admit_capture_storage(
            binding_capacity.checked_mul(size_of::<PrefabExtractionBinding>()),
        )
        .map_err(PrefabExtractionProjectError::SourceWork)?;
        let mut before = Vec::with_capacity(binding_capacity);
        for (index, member) in inventory.participants().enumerate() {
            let Some(path) = participants.peek() else {
                break;
            };
            work.admit_capture_work(
                1,
                Some(checked_sum([member.file().path().len(), path.len()])?),
            )
            .map_err(PrefabExtractionProjectError::SourceWork)?;
            // The retained configuration's sorted membership is the authority, not the source
            // kind, a global alias union or whichever configuration first found this file.
            if *path != member.file().path() {
                continue;
            }
            participants.next();
            members.push(index);
            if member.kind() != SourceDocumentKind::Prefab {
                continue;
            }
            let scratch = member.file().path().len();
            budget.charge_source_bytes(0, 0, scratch, scratch)?;
            let result = (|| {
                let canonical = AssetSourcePath::new(member.file().path())
                    .map_err(|_| PrefabExtractionProjectError::Destination)?;
                let prefab_scope = catalogue.prefabs();
                if let Some(path) = prefab_scope.scope().relative_path(&canonical) {
                    work.admit_capture_storage(Some(path.len()))
                        .map_err(PrefabExtractionProjectError::SourceWork)?;
                    let reference = AssetRef::path(path)
                        .map_err(|_| PrefabExtractionProjectError::Destination)?;
                    admit_lookup(
                        &mut catalogue,
                        reference,
                        sources[index].key(),
                        &canonical,
                        &mut before,
                        budget,
                        work,
                    )?;
                }
                if let Some(record) = catalogue.database().record_for_path(&canonical) {
                    let stable_id = record.stable_id();
                    for reference in [AssetRef::StableId(stable_id), primary(stable_id)?] {
                        admit_lookup(
                            &mut catalogue,
                            reference,
                            sources[index].key(),
                            &canonical,
                            &mut before,
                            budget,
                            work,
                        )?;
                    }
                }
                Ok::<_, PrefabExtractionProjectError>(())
            })();
            budget.reconcile_retained_reservation(scratch, 0)?;
            result?;
        }
        if participants.next().is_some() {
            return Err(PrefabExtractionProjectError::OpenSet);
        }
        let after_capacity = checked_sum([before.len(), 3])?;
        work.admit_capture_storage(Some(checked_product(
            after_capacity,
            size_of::<PrefabExtractionBinding>(),
        )?))
        .map_err(PrefabExtractionProjectError::SourceWork)?;
        let mut after = Vec::with_capacity(after_capacity);
        for binding in &before {
            push(&mut after, &binding.reference, &binding.source, work)?;
        }
        if visible_q {
            for reference in [
                AssetRef::StableId(destination.record.stable_id()),
                primary(destination.record.stable_id())?,
            ] {
                push(&mut after, &reference, destination.key, work)?;
            }
            let prefab_scope = catalogue.prefabs();
            if let Some(path) = prefab_scope
                .scope()
                .relative_path(destination.record.path())
            {
                work.admit_capture_storage(Some(path.len()))
                    .map_err(PrefabExtractionProjectError::SourceWork)?;
                let reference =
                    AssetRef::path(path).map_err(|_| PrefabExtractionProjectError::Destination)?;
                push(&mut after, &reference, destination.key, work)?;
            }
        }
        // This is a real configuration-local database. Stable lookups bypass alias scope, so
        // copying the all-profile union here would falsely make invisible records available.
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                2 * size_of::<ProjectAssetDatabase>(),
                8 * size_of::<usize>(),
                checked_product(
                    catalogue
                        .assets()
                        .scope()
                        .prefix()
                        .map_or(0, |path| path.as_str().len()),
                    2,
                )?,
            ])?,
        )?;
        for record in catalogue.database().records() {
            catalogue::charge_database_record(record, budget)?;
        }
        let mut after_database = catalogue.database().clone();
        if visible_q {
            admit_asset_record(&mut after_database, destination.record, budget)?;
        }
        let scope = catalogue.assets().scope().clone();
        let before_database = catalogue.into_database(budget)?;
        if policy.profile_name() == selected_profile {
            selected = Some(environments.len());
        }
        environments.push(PrefabExtractionEnvironment {
            name,
            sources: members,
            before,
            after,
            assets_before: Some(PrefabExtractionAssets {
                database: Arc::new(before_database),
                scope: scope.clone(),
            }),
            assets_after: Some(PrefabExtractionAssets {
                database: Arc::new(after_database),
                scope,
            }),
        });
    }
    Ok((
        environments,
        selected.ok_or(PrefabExtractionProjectError::OpenSet)?,
    ))
}

fn primary(source: nara_asset::StableAssetId) -> Result<AssetRef, PrefabExtractionProjectError> {
    AssetProductRef::new(source, ImportedProductId::PRIMARY)
        .map(AssetRef::Product)
        .map_err(|_| PrefabExtractionProjectError::Destination)
}

fn admit_lookup(
    catalogue: &mut ReferenceCatalogue,
    reference: AssetRef,
    key: &SourceDocumentKey,
    expected_path: &AssetSourcePath,
    bindings: &mut Vec<PrefabExtractionBinding>,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: &mut PrefabExtractionWork,
) -> Result<(), PrefabExtractionProjectError> {
    let Some(resolved) = catalogue.resolve_prefab(&reference, budget)? else {
        return Ok(());
    };
    let bytes = checked_sum([
        resolved.path.as_str().len(),
        resolved.binding.directory().as_str().len(),
        1,
    ])?;
    budget.charge_source_bytes(0, 0, bytes, bytes)?;
    let result = (|| {
        let actual_path = resolved.binding.scope().resolve(&resolved.path);
        if &actual_path != expected_path {
            return Err(PrefabExtractionProjectError::Destination);
        }
        push(bindings, &reference, key, work)
    })();
    budget.reconcile_retained_reservation(bytes, 0)?;
    resolved.release(budget)?;
    result
}

fn push(
    bindings: &mut Vec<PrefabExtractionBinding>,
    reference: &AssetRef,
    key: &SourceDocumentKey,
    work: &mut PrefabExtractionWork,
) -> Result<(), PrefabExtractionProjectError> {
    let bytes = checked_sum([
        key.retained_bytes(),
        match reference {
            AssetRef::Path(path) => path.as_str().len(),
            _ => 0,
        },
    ])?;
    // The caller already admitted three slots per physical Prefab, and three more for Q.
    // Only these variable-size copies remain; push cannot grow the admitted table.
    work.admit_capture_storage(Some(bytes))
        .map_err(PrefabExtractionProjectError::SourceWork)?;
    bindings.push(PrefabExtractionBinding {
        reference: reference.clone(),
        source: key.clone(),
    });
    Ok(())
}
