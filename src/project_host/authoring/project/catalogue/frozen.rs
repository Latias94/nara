//! Configuration-specific catalogue construction from an already admitted physical inventory.

use crate::project_host::authoring::source_inventory::SourceInventory;

use super::*;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

impl ReferenceCatalogue {
    /// Retains only this actual configuration's catalogue-visible metadata. Resolution uses the
    /// existing scope/kind rules; neither construction nor subsequent lookups enumerate disk.
    /// The caller keeps the inventory's original native custody and operation allowance alive.
    pub(in crate::project_host::authoring::project) fn from_inventory(
        root: DirectoryCapability,
        inventory: &SourceInventory,
        configuration: usize,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, AuthoringProjectSourceAdmissionError> {
        let policy = inventory
            .configurations()
            .get(configuration)
            .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?;
        let retained_before = budget.retained_candidate_bytes;
        let result = (|| {
            let bindings = policy.catalogue_bindings();
            let binding_bytes = bindings.iter().try_fold(0_usize, |bytes, binding| {
                let kind_bytes = match &binding.expected_kind {
                    Some(AssetSourceKind::Other(kind)) => kind.len(),
                    _ => 0,
                };
                checked_sum([bytes, size_of::<SourceDirectoryBinding>(), kind_bytes])
            })?;
            let mut storage = FrozenCatalogueStorage {
                database_bytes: 0,
                temporary_bytes: checked_sum([size_of::<Self>(), binding_bytes])?,
            };
            // Bindings share their admitted directory/scope Arcs. Only the new Vec slots and
            // any owned kind text are copied, after the continuing allowance accepts them.
            budget.charge_source_bytes(0, 0, binding_bytes, storage.temporary_bytes)?;
            let mut catalogue = Self {
                root,
                bindings: bindings.to_vec(),
                indexed: true,
                database: ProjectAssetDatabase::default(),
                metadata: BTreeMap::new(),
                source_bindings: BTreeMap::new(),
                lookups: BTreeMap::new(),
                resolved_targets: BTreeMap::new(),
                resolved_identities: HashMap::new(),
                frozen_storage: None,
            };
            for metadata in inventory.metadata() {
                // Physical admission in another profile cannot hide a declaration which
                // this actual catalogue would scan and reject. Use the same rule as live IO.
                let Some(actual_binding) = Self::planned_metadata_binding(
                    bindings,
                    metadata.record().path(),
                    metadata.authored_path(),
                    budget,
                )?
                else {
                    continue;
                };
                // Inspect every visibility entry under the original cumulative work meter.
                // A native-only Scripts admission never supplies a catalogue binding.
                budget.charge_source_bytes(
                    0,
                    0,
                    checked_product(metadata.visibility().len(), size_of::<usize>())?,
                    0,
                )?;
                let visibility = metadata
                    .visibility()
                    .iter()
                    .find(|visibility| {
                        visibility.configuration_index() == configuration
                            && visibility.catalogue_binding_index().is_some()
                    })
                    .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?;
                if visibility.catalogue_binding_index() != Some(actual_binding) {
                    return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
                }
                let binding = visibility
                    .catalogue_binding_index()
                    .and_then(|index| bindings.get(index))
                    .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?;
                let record = metadata.record();
                let file = metadata.file();
                budget.charge_source_bytes(
                    0,
                    0,
                    checked_sum([
                        record.path().as_str().len(),
                        file.path().len(),
                        binding.directory().as_str().len(),
                    ])?,
                    0,
                )?;
                let local = binding
                    .scope()
                    .relative_path(record.path())
                    .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?;
                if visibility.scope() != binding.scope()
                    || binding.scope().prefix().map(|path| path.as_str())
                        != Some(binding.directory().as_str())
                    || file.path().strip_suffix(".meta") != Some(record.path().as_str())
                    || file.encoded_bytes() != metadata.bytes().len() as u64
                {
                    return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
                }
                let encoded_bytes = usize::try_from(file.encoded_bytes())
                    .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
                let depth = local.bytes().filter(|byte| *byte == b'/').count();
                budget.charge_reference(file.path().len(), depth, false)?;
                // The inventory has already deduplicated physical metadata across overlapping
                // roots. Keep its one record and actual declaration binding, regardless of the
                // traversal directory's expected query kind. Database conflicts keep their owner.
                let database_before = budget.retained_candidate_bytes;
                admit_asset_record(&mut catalogue.database, record, budget)?;
                storage.database_bytes = checked_sum([
                    storage.database_bytes,
                    budget
                        .retained_candidate_bytes
                        .checked_sub(database_before)
                        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
                ])?;
                let temporary_bytes = checked_sum([
                    source_fact_path_retained_bytes(file.path())?,
                    2 * size_of::<usize>(),
                    size_of::<(AssetSourcePath, Arc<AuthoringSourceFact>)>(),
                    size_of::<(AssetSourcePath, SourceDirectoryBinding)>(),
                    checked_product(record.path().as_str().len(), 2)?,
                    // Conservative node allowance for both independently owned maps.
                    2 * 192,
                    match &binding.expected_kind {
                        Some(AssetSourceKind::Other(kind)) => kind.len(),
                        _ => 0,
                    },
                ])?;
                budget.charge_source_bytes(0, 0, 0, temporary_bytes)?;
                storage.temporary_bytes = checked_sum([storage.temporary_bytes, temporary_bytes])?;
                let source = AuthoringSourceFact::native_metadata(
                    ProjectPath::new(file.path())
                        .map_err(AuthoringProjectSourceAdmissionError::JoinedSourcePath)?,
                    file.identity(),
                    file.parent_identity(),
                    file.digest(),
                    encoded_bytes,
                    depth,
                );
                catalogue
                    .source_bindings
                    .insert(record.path().clone(), binding.clone());
                catalogue
                    .metadata
                    .insert(record.path().clone(), Arc::new(source));
            }
            catalogue.frozen_storage = Some(storage);
            Ok(catalogue)
        })();
        if result.is_err() {
            // A refused construction drops every new allocation. Keep performed work charged,
            // while releasing only this constructor's storage, never the inventory's custody.
            let temporary = budget
                .retained_candidate_bytes
                .checked_sub(retained_before)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            budget.reconcile_retained_reservation(temporary, 0)?;
        }
        result
    }
}
