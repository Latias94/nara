//! Bounded read-only metadata lookups and Deferred target bindings.

use std::{
    collections::VecDeque,
    io::{ErrorKind, Read},
};

use nara_asset::{AssetReferenceDemand, StableAssetId};
use nara_fs::{DirectoryEnumerationLimits, FileKind};

use super::*;

#[cfg(test)]
mod tests;

/// Source binding observations do not certify decoded payload or compound Product membership.
pub(super) enum ReferenceObservation {
    StableLookup(StableLookupObservation),
    ResolvedTarget(ResolvedTargetObservation),
}

/// One demanded global identity observation, separate from its reader and working-copy evidence.
pub(super) struct StableLookupObservation {
    stable_id: StableAssetId,
    // An unresolved result remains provisional; capture still requires recapture.
    record: Option<AssetRecord>,
    metadata_digest: Option<ContentDigest>,
    // An intrinsic source participates in the same global identity universe even without a
    // sidecar. Revalidation must still reject newly conflicting, otherwise unrelated metadata.
    intrinsic: Option<AssetRecord>,
}

/// A Deferred target keeps its canonical reader binding without observing its document body.
/// Its key and sidecar content are validated by the capture's separate metadata source facts.
pub(super) struct ResolvedTargetObservation {
    kind: AuthoringSourceKind,
    project_path: ProjectPath,
    dependency_depth: usize,
}

impl ResolvedTargetObservation {
    fn revalidate(
        &self,
        root: &DirectoryCapability,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        budget.charge_reference(
            self.project_path.as_str().len(),
            self.dependency_depth,
            false,
        )?;
        let scratch = checked_sum([
            size_of::<OpenedAuthoringSource>(),
            self.project_path.as_str().len(),
        ])?;
        budget.charge_source_bytes(0, 0, 0, scratch)?;
        let result = (|| {
            let opened = open_source_file(
                root,
                self.project_path.clone(),
                self.project_path.as_str(),
                self.kind,
                self.dependency_depth,
                budget,
            )?;
            // No historical leaf or parent identity is asserted for an unread body. A new
            // readable leaf under the same admitted binding has the same Deferred meaning.
            probe_target_readability(&opened, budget, false)
        })();
        budget.reconcile_retained_reservation(scratch, 0)?;
        result
    }
}

fn probe_target_readability(
    opened: &OpenedAuthoringSource,
    budget: &mut AuthoringProjectAdmissionBudget,
    first_observation: bool,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    if first_observation {
        budget.charge_open_file(SOURCE_OPEN_HANDLE_PEAK)?;
    } else {
        budget.admit_handle_peak(SOURCE_OPEN_HANDLE_PEAK)?;
    }
    // Read access is a source-binding fact. One stack byte proves access without making the
    // complete body, its length, a digest, or decoded payload semantics part of the receipt.
    budget.charge_source_bytes(usize::from(first_observation), 0, 1, 0)?;
    let mut reader = opened.capability.reader().map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourceHost {
            kind: opened.kind,
            path: opened.project_path.clone(),
            source,
        }
    })?;
    reader.read(&mut [0_u8; 1]).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourceHost {
            kind: opened.kind,
            path: opened.project_path.clone(),
            source: FsError::Io {
                operation: nara_fs::FsOperation::Read,
                source,
            },
        }
    })?;
    Ok(())
}

impl ReferenceObservation {
    pub(super) fn retained_bytes(&self) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        match self {
            Self::StableLookup(observation) => checked_sum([
                size_of::<Self>(),
                observation
                    .record
                    .as_ref()
                    .map_or(Ok(0), asset_record_retained_bytes)?,
                observation
                    .intrinsic
                    .as_ref()
                    .map_or(Ok(0), asset_record_retained_bytes)?,
            ]),
            Self::ResolvedTarget(observation) => {
                checked_sum([size_of::<Self>(), observation.project_path.as_str().len()])
            }
        }
    }
}

/// The actual directory capability binding and expected kind of a source query.
/// Neither field is part of canonical source identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::project_host::authoring) struct SourceDirectoryBinding {
    pub(super) directory: Arc<ProjectPath>,
    pub(super) scope: Arc<AssetPathScope>,
    pub(super) expected_kind: Option<AssetSourceKind>,
}

impl SourceDirectoryBinding {
    pub(in crate::project_host::authoring) fn directory(&self) -> &ProjectPath {
        &self.directory
    }

    pub(in crate::project_host::authoring) fn scope(&self) -> &AssetPathScope {
        &self.scope
    }

    pub(super) fn new(directory: ProjectPath, expected_kind: Option<AssetSourceKind>) -> Self {
        let prefix = AssetSourcePath::new(directory.as_str())
            .expect("an admitted project directory is a canonical source prefix");
        Self {
            directory: Arc::new(directory),
            scope: Arc::new(AssetPathScope::under(prefix)),
            expected_kind,
        }
    }
}

#[derive(Clone)]
pub(super) struct ResolvedResourceSource {
    pub(super) path: AssetPath,
    pub(super) binding: SourceDirectoryBinding,
    pub(super) demand: AssetReferenceDemand,
}

pub(super) struct ReferenceCatalogue {
    root: DirectoryCapability,
    bindings: Vec<SourceDirectoryBinding>,
    indexed: bool,
    database: ProjectAssetDatabase,
    metadata: BTreeMap<AssetSourcePath, Arc<AuthoringSourceFact>>,
    source_bindings: BTreeMap<AssetSourcePath, SourceDirectoryBinding>,
    lookups: BTreeMap<StableAssetId, StableLookupObservation>,
    resolved_targets: BTreeMap<AssetSourcePath, ResolvedTargetObservation>,
    // Capture-local collision rejection only. These copied observations never attest that
    // a subsequently opened leaf is the historical object, and do not escape for revalidation.
    resolved_identities: HashMap<FileIdentity, AssetSourcePath>,
}

impl ReferenceCatalogue {
    pub(super) fn project_bindings(
        settings: &EffectiveProjectSettings,
    ) -> Vec<SourceDirectoryBinding> {
        vec![
            SourceDirectoryBinding::new(settings.paths.assets.clone(), None),
            SourceDirectoryBinding::new(
                settings.paths.prefabs.clone(),
                Some(AssetSourceKind::Prefab),
            ),
            SourceDirectoryBinding::new(
                settings.paths.scenes.clone(),
                Some(AssetSourceKind::Scene),
            ),
        ]
    }

    pub(super) fn new(
        root: DirectoryCapability,
        bindings: Vec<SourceDirectoryBinding>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, AuthoringProjectSourceAdmissionError> {
        for binding in &bindings {
            budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([
                    size_of::<AssetPathScope>(),
                    2 * size_of::<usize>(),
                    binding.directory.as_str().len(),
                    size_of::<SourceDirectoryBinding>(),
                ])?,
            )?;
        }
        Ok(Self {
            root,
            bindings,
            indexed: false,
            database: ProjectAssetDatabase::default(),
            metadata: BTreeMap::new(),
            source_bindings: BTreeMap::new(),
            lookups: BTreeMap::new(),
            resolved_targets: BTreeMap::new(),
            resolved_identities: HashMap::new(),
        })
    }

    pub(super) fn assets(&self) -> SourceDirectoryBinding {
        self.bindings
            .iter()
            .find(|binding| binding.expected_kind.is_none())
            .expect("the source owner binds an Assets query directory")
            .clone()
    }

    pub(super) fn prefabs(&self) -> SourceDirectoryBinding {
        self.bindings
            .iter()
            .find(|binding| binding.expected_kind == Some(AssetSourceKind::Prefab))
            .expect("the source owner binds a Prefab query directory")
            .clone()
    }

    pub(super) const fn is_indexed(&self) -> bool {
        self.indexed
    }

    pub(super) fn lookup_sources(&self) -> impl Iterator<Item = &Arc<AuthoringSourceFact>> {
        self.lookups
            .values()
            .filter_map(|lookup| self.metadata.get(lookup.record.as_ref()?.path()))
    }

    /// Bare paths keep their Assets-relative meaning. Stable selections retain the actual
    /// directory which observed the sidecar, including native data outside Assets.
    pub(super) fn resolve_resource(
        &mut self,
        reference: &AssetRef,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Option<ResolvedResourceSource>, AuthoringProjectSourceAdmissionError> {
        if let AssetRef::Path(path) = reference {
            budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([
                    size_of::<ResolvedResourceSource>(),
                    path.as_str().len(),
                    self.assets().directory.as_str().len(),
                ])?,
            )?;
            return Ok(Some(ResolvedResourceSource {
                path: path.clone(),
                binding: self.assets(),
                demand: AssetReferenceDemand::Immediate,
            }));
        }
        self.ensure_indexed(budget)?;
        let id = match reference {
            AssetRef::StableId(id) => *id,
            AssetRef::Product(product) => product.source(),
            AssetRef::Path(_) => unreachable!(),
        };
        let Some(record) = self.database.record_for_stable_id(id) else {
            let binding = self.assets();
            self.resolve(reference, &binding, budget)?;
            return Ok(None);
        };
        if record.source_kind() == &AssetSourceKind::Scene && !reference.is_primary() {
            return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                super::super::AuthoringDocumentAdmissionError::NativeSourceMetadataMismatch,
            ));
        }
        let binding = self
            .source_bindings
            .get(record.path())
            .expect("an indexed record retains its actual directory binding");
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                size_of::<ResolvedResourceSource>(),
                record.path().as_str().len(),
                binding.directory.as_str().len() * 2,
            ])?,
        )?;
        let mut binding = binding.clone();
        binding.expected_kind = Some(record.source_kind().clone());
        let path = self.resolve(reference, &binding, budget)?;
        Ok(path.map(|path| ResolvedResourceSource {
            path,
            binding,
            demand: AssetReferenceDemand::Immediate,
        }))
    }

    pub(super) fn resolve_prefab(
        &mut self,
        reference: &AssetRef,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Option<ResolvedResourceSource>, AuthoringProjectSourceAdmissionError> {
        if !reference.is_primary() {
            return Ok(None);
        }
        if let AssetRef::Path(path) = reference {
            budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([size_of::<ResolvedResourceSource>(), path.as_str().len()])?,
            )?;
            return Ok(Some(ResolvedResourceSource {
                path: path.clone(),
                binding: self.prefabs(),
                demand: AssetReferenceDemand::Immediate,
            }));
        }
        let selected = self.resolve_resource(reference, budget)?;
        if selected
            .as_ref()
            .is_some_and(|selected| selected.binding.expected_kind != Some(AssetSourceKind::Prefab))
        {
            return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                super::super::AuthoringDocumentAdmissionError::NativeSourceMetadataMismatch,
            ));
        }
        Ok(selected)
    }

    pub(super) fn resolve(
        &mut self,
        reference: &AssetRef,
        binding: &SourceDirectoryBinding,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Option<AssetPath>, AuthoringProjectSourceAdmissionError> {
        if matches!(
            binding.expected_kind,
            Some(AssetSourceKind::Prefab | AssetSourceKind::Scene)
        ) && !reference.is_primary()
        {
            return Ok(None);
        }
        let stable_id = match reference {
            AssetRef::Path(path) => return Ok(Some(path.clone())),
            AssetRef::StableId(stable_id) => *stable_id,
            AssetRef::Product(product) => product.source(),
        };
        self.ensure_indexed(budget)?;
        if !self.lookups.contains_key(&stable_id) {
            let record = self.indexed_record(stable_id);
            budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([
                    size_of::<StableLookupObservation>(),
                    record.map_or(Ok(0), asset_record_retained_bytes)?,
                    64,
                ])?,
            )?;
            let observation = StableLookupObservation {
                stable_id,
                record: record.cloned(),
                metadata_digest: record.map(|record| self.metadata[record.path()].digest()),
                intrinsic: None,
            };
            self.lookups.insert(stable_id, observation);
        }
        Ok(self
            .resolve_indexed(reference, binding)
            .and_then(|path| binding.scope.relativize(path)))
    }

    fn ensure_indexed(
        &mut self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        if !self.indexed {
            for index in 0..self.bindings.len() {
                self.index_directory(index, budget)?;
            }
            self.indexed = true;
        }
        Ok(())
    }

    /// Joins an intrinsic source to the existing catalogue, retaining its actual optional
    /// metadata observation and a global identity check for subsequent source validation.
    pub(super) fn admit_intrinsic_source(
        &mut self,
        record: &AssetRecord,
        metadata: Option<(&nara_asset::AssetMetaIdentity, &AuthoringSourceFact)>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        self.ensure_indexed(budget)?;
        if let Some((identity, _)) = metadata {
            let path_matches = self.bindings.iter().any(|binding| {
                binding.scope.relative_path(record.path()) == Some(identity.path().as_str())
            }) || AssetPathScope::ROOT.relative_path(record.path())
                == Some(identity.path().as_str());
            if identity.stable_id() != record.stable_id()
                || identity.source_kind() != record.source_kind()
                || !path_matches
            {
                return Err(
                    AuthoringProjectSourceAdmissionError::IntrinsicSourceMetadataMismatch {
                        path: record.path().clone(),
                    },
                );
            }
        }
        if let Some(indexed) = self.metadata.get(record.path())
            && !metadata.is_some_and(|(_, source)| same_metadata_content(indexed, source))
        {
            return Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
                path: record.path().clone(),
            });
        }
        self.validate_intrinsic_record(record)?;

        let stable_id = record.stable_id();
        if let Some(observation) = self.lookups.get_mut(&stable_id) {
            if observation.intrinsic.is_none() {
                budget.charge_source_bytes(0, 0, 0, asset_record_retained_bytes(record)?)?;
                observation.intrinsic = Some(record.clone());
            }
        } else {
            let indexed = self.indexed_record(stable_id);
            budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([
                    size_of::<StableLookupObservation>(),
                    indexed.map_or(Ok(0), asset_record_retained_bytes)?,
                    asset_record_retained_bytes(record)?,
                    64,
                ])?,
            )?;
            let metadata_digest = indexed.map(|record| self.metadata[record.path()].digest());
            let indexed = indexed.cloned();
            self.lookups.insert(
                stable_id,
                StableLookupObservation {
                    stable_id,
                    record: indexed,
                    metadata_digest,
                    intrinsic: Some(record.clone()),
                },
            );
        }
        admit_asset_record(&mut self.database, record, budget)?;
        Ok(())
    }

    fn validate_intrinsic_record(
        &self,
        record: &AssetRecord,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        if self.database.record_for_path(record.path()) == Some(record) {
            return Ok(());
        }
        self.database.validate_insert(record).map_err(|source| {
            AuthoringProjectSourceAdmissionError::AssetIdentity {
                path: record.path().clone(),
                source,
            }
        })
    }

    /// Attaches actual metadata after a successful Prefab lookup, preserving source identity.
    pub(super) fn bind_prefab_source(
        &self,
        reference: &AssetRef,
        source: ResolvedPrefabSource,
    ) -> Result<ResolvedPrefabSource, AuthoringProjectSourceAdmissionError> {
        let stable_id = match reference {
            AssetRef::Path(_) => return Ok(source),
            AssetRef::StableId(stable_id) => *stable_id,
            AssetRef::Product(product) => product.source(),
        };
        let record = self
            .database
            .record_for_stable_id(stable_id)
            .expect("resolved stable Prefab lookup has admitted metadata");
        source.with_source_asset(record).map_err(|_| {
            AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
                path: record.path().clone(),
            }
        })
    }

    fn resolve_indexed(
        &self,
        reference: &AssetRef,
        binding: &SourceDirectoryBinding,
    ) -> Option<&AssetSourcePath> {
        if matches!(
            binding.expected_kind,
            Some(AssetSourceKind::Prefab | AssetSourceKind::Scene)
        ) && !reference.is_primary()
        {
            return None;
        }
        let stable_id = match reference {
            AssetRef::StableId(id) => *id,
            AssetRef::Product(product) => product.source(),
            AssetRef::Path(_) => return None,
        };
        self.database
            .record_for_stable_id(stable_id)
            .and_then(|record| {
                self.metadata.get(record.path())?;
                (binding.scope.relative_path(record.path()).is_some()
                    && binding
                        .expected_kind
                        .as_ref()
                        .is_none_or(|kind| record.source_kind() == kind))
                .then(|| record.path())
            })
    }

    /// Stable identity is global to the admitted catalogue. A lookup observation must not
    /// inherit the fallback alias scope used before an unresolved source has an actual reader.
    fn indexed_record(&self, stable_id: StableAssetId) -> Option<&AssetRecord> {
        let record = self.database.record_for_stable_id(stable_id)?;
        self.metadata.get(record.path())?;
        Some(record)
    }

    pub(super) fn check_resolved_target_identity(
        &self,
        identity: FileIdentity,
        project_path: &ProjectPath,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        if self
            .resolved_identities
            .get(&identity)
            .is_some_and(|path| path.as_str() != project_path.as_str())
        {
            return Err(
                AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                    path: project_path.clone(),
                },
            );
        }
        Ok(())
    }

    /// Keeps a Deferred source's binding and proves read access without decoding its body.
    pub(super) fn observe_resolved_target(
        &mut self,
        opened: OpenedAuthoringSource,
        record: &AssetRecord,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        if opened.project_path.as_str() != record.path().as_str() {
            return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
        }
        probe_target_readability(&opened, budget, true)?;
        if let Some(previous) = self.resolved_targets.get(record.path()) {
            if previous.kind != opened.kind {
                return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                    kind: opened.kind,
                    path: opened.project_path,
                });
            }
            return Ok(());
        }
        self.check_resolved_target_identity(opened.identity, &opened.project_path)?;
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                size_of::<(AssetSourcePath, ResolvedTargetObservation)>(),
                size_of::<(FileIdentity, AssetSourcePath)>(),
                record.path().as_str().len(),
                record.path().as_str().len(),
                opened.project_path.as_str().len(),
                128,
            ])?,
        )?;
        self.resolved_identities
            .insert(opened.identity, record.path().clone());
        self.resolved_targets.insert(
            record.path().clone(),
            ResolvedTargetObservation {
                kind: opened.kind,
                project_path: opened.project_path,
                dependency_depth: opened.dependency_depth,
            },
        );
        Ok(())
    }

    pub(super) fn revalidate_resolved_targets(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        for target in self.resolved_targets.values() {
            target.revalidate(&self.root, budget)?;
        }
        Ok(())
    }

    pub(super) fn into_reference_observations(
        self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Arc<[ReferenceObservation]>, AuthoringProjectSourceAdmissionError> {
        // Entries move their paths; the temporary vector and final Arc coexist during transfer.
        let count = checked_sum([self.lookups.len(), self.resolved_targets.len()])?;
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                checked_product(count, 2 * size_of::<ReferenceObservation>())?,
                2 * size_of::<usize>(),
            ])?,
        )?;
        Ok(self
            .lookups
            .into_values()
            .map(ReferenceObservation::StableLookup)
            .chain(
                self.resolved_targets
                    .into_values()
                    .map(ReferenceObservation::ResolvedTarget),
            )
            .collect::<Vec<_>>()
            .into())
    }

    pub(super) fn revalidate_observations(
        root: &DirectoryCapability,
        bindings: Vec<SourceDirectoryBinding>,
        observations: &[ReferenceObservation],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        if observations.is_empty() {
            return Ok(());
        }
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                bindings
                    .iter()
                    .map(|binding| binding.directory.as_str().len())
                    .sum(),
                size_of::<Self>(),
            ])?,
        )?;
        let mut current = Self::new(root.share(), bindings, budget)?;
        if observations
            .iter()
            .any(|observation| matches!(observation, ReferenceObservation::StableLookup(_)))
        {
            current.ensure_indexed(budget)?;
        }
        for observation in observations {
            let observation = match observation {
                ReferenceObservation::StableLookup(observation) => observation,
                ReferenceObservation::ResolvedTarget(target) => {
                    target.revalidate(root, budget)?;
                    continue;
                }
            };
            if let Some(record) = &observation.intrinsic {
                current.validate_intrinsic_record(record)?;
            }
            let record = current.indexed_record(observation.stable_id);
            if record != observation.record.as_ref()
                || record.map(|record| current.metadata[record.path()].digest())
                    != observation.metadata_digest
            {
                // None is an observed unresolved lookup, never a durable absence receipt.
                let path = observation
                    .record
                    .as_ref()
                    .or(record)
                    .expect("different optional lookup targets include at least one path");
                return Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
                    path: path.path().clone(),
                });
            }
            if let Some(record) = record {
                revalidate_metadata_content(root, &current.metadata[record.path()], budget)?;
            }
        }
        Ok(())
    }

    fn index_directory(
        &mut self,
        index: usize,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let root_path = &self.bindings[index].directory;
        budget.charge_reference(root_path.as_str().len(), 0, false)?;
        let relative = RelativePath::new(root_path.as_str()).map_err(|source| {
            AuthoringProjectSourceAdmissionError::SourceDirectoryPath {
                kind: AuthoringSourceKind::AssetMetadata,
                source,
            }
        })?;
        let directory = match self.root.open_directory(&relative) {
            Ok(directory) => directory,
            Err(FsError::Io { source, .. }) if source.kind() == ErrorKind::NotFound => {
                return Ok(());
            }
            Err(source) => {
                return Err(AuthoringProjectSourceAdmissionError::SourceDirectoryHost {
                    kind: AuthoringSourceKind::AssetMetadata,
                    source,
                });
            }
        };
        let mut pending = VecDeque::from([(String::new(), 0_usize)]);
        while let Some((prefix, depth)) = pending.pop_front() {
            budget.charge_reference(prefix.len(), depth, false)?;
            // One directory and its native cursor replace the source read's handle allowance.
            budget.charge_open_file(SOURCE_OPEN_HANDLE_PEAK + 2)?;
            let child = if prefix.is_empty() {
                directory.share()
            } else {
                let path = RelativePath::new(&prefix).map_err(|source| {
                    AuthoringProjectSourceAdmissionError::SourceDirectoryPath {
                        kind: AuthoringSourceKind::AssetMetadata,
                        source,
                    }
                })?;
                directory.open_directory(&path).map_err(|source| {
                    AuthoringProjectSourceAdmissionError::SourceDirectoryHost {
                        kind: AuthoringSourceKind::AssetMetadata,
                        source,
                    }
                })?
            };
            let limits = DirectoryEnumerationLimits::new(
                budget.limits.unique_files().get(),
                budget.limits.aggregate_path_bytes().get(),
                budget.limits.unique_files().get(),
            );
            let mut enumeration = child
                .begin_child_enumeration(limits)
                .map_err(catalogue_host_error)?;
            loop {
                let batch = enumeration.next_batch(64).map_err(catalogue_host_error)?;
                for entry in batch.entries() {
                    let name = entry
                        .name()
                        .to_str()
                        .ok_or(AuthoringProjectSourceAdmissionError::CatalogueNameInvalid)?;
                    let path_len =
                        checked_sum([prefix.len(), name.len(), usize::from(!prefix.is_empty())])?;
                    budget.charge_reference(path_len, depth, false)?;
                    // Count every observed child, including ordinary files, against bounded ingress.
                    budget.charge_open_file(SOURCE_OPEN_HANDLE_PEAK + 2)?;
                    budget.charge_source_bytes(
                        0,
                        0,
                        0,
                        checked_sum([path_len, size_of::<String>()])?,
                    )?;
                    let path = if prefix.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{prefix}/{name}")
                    };
                    match entry.kind() {
                        FileKind::Directory => {
                            let next_depth = depth
                                .checked_add(1)
                                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
                            budget.charge_reference(0, next_depth, false)?;
                            pending.push_back((path, next_depth));
                        }
                        FileKind::Regular if path.ends_with(".meta") => {
                            self.index_metadata(&directory, index, &path, depth, budget)?;
                        }
                        _ => {}
                    }
                }
                for _ in batch.rejected_entry_facts() {
                    budget.charge_open_file(SOURCE_OPEN_HANDLE_PEAK + 2)?;
                }
                if batch.is_complete() {
                    break;
                }
            }
        }
        Ok(())
    }

    fn index_metadata(
        &mut self,
        directory: &DirectoryCapability,
        index: usize,
        metadata_path: &str,
        depth: usize,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let source_path = metadata_path
            .strip_suffix(".meta")
            .expect("selected metadata suffix");
        let project_path = join_project_path(&self.bindings[index].directory, metadata_path)?;
        let path = AssetPath::new(source_path).map_err(|source| {
            AuthoringProjectSourceAdmissionError::DocumentSourcePath {
                path: project_path.clone(),
                source,
            }
        })?;
        let read = read_unique_source(
            open_source_file(
                directory,
                project_path,
                metadata_path,
                AuthoringSourceKind::AssetMetadata,
                depth,
                budget,
            )?,
            AssetMetaFileLimits::default().encoded_bytes().get(),
            budget,
        )?;
        let encoded_bytes = read.bytes.len();
        let exact_limit = ByteLimit::new(read.read_limit)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes_owned_with_limits(
            read.bytes.into_boxed_slice(),
            AssetMetaFileLimits::default().with_encoded_bytes(exact_limit),
        )
        .map_err(
            |source| AuthoringProjectSourceAdmissionError::AssetMetadataFormat {
                path: self.bindings[index].scope.resolve(&path),
                source,
            },
        )?;
        let canonical_path = self.bindings[index].scope.resolve(&path);
        // A traversal root locates the observed file. The sidecar's local spelling binds
        // through an actual configured scope, independently of its source kind or owner.
        let scope = self
            .bindings
            .iter()
            .find(|binding| {
                binding.scope.relative_path(&canonical_path)
                    == Some(metadata.identity().path().as_str())
            })
            .ok_or_else(
                || AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch {
                    requested: path.clone(),
                    declared: metadata.identity().path().clone(),
                },
            )?;
        let source_binding = scope.clone();
        let record = metadata.identity().clone().into_record(&scope.scope);
        if record.path() != &canonical_path {
            return Err(
                AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch {
                    requested: path,
                    declared: metadata.identity().path().clone(),
                },
            );
        }
        charge_asset_metadata(&read.source, encoded_bytes, &metadata, &record, budget)?;
        // Overlapping configured directories can observe the same canonical source twice.
        // Both the sidecar declaration and captured content must still agree.
        if self.database.record_for_path(record.path()) == Some(&record) {
            if self
                .metadata
                .get(record.path())
                .is_some_and(|indexed| same_metadata_content(indexed, &read.source))
            {
                return Ok(());
            }
            return Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
                path: record.path().clone(),
            });
        }
        admit_asset_record(&mut self.database, &record, budget)?;
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                size_of::<(AssetSourcePath, Arc<AuthoringSourceFact>)>(),
                size_of::<(AssetSourcePath, SourceDirectoryBinding)>(),
                canonical_path.as_str().len(),
                canonical_path.as_str().len(),
                64,
            ])?,
        )?;
        self.source_bindings
            .insert(canonical_path.clone(), source_binding);
        self.metadata.insert(canonical_path, read.source);
        Ok(())
    }

    pub(super) fn verify_metadata(
        &self,
        path: &AssetPath,
        binding: &SourceDirectoryBinding,
        source: &AuthoringSourceFact,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let canonical_path = binding.scope.resolve(path);
        if let Some(indexed) = self.metadata.get(&canonical_path) {
            if !same_metadata_content(indexed, source) {
                return Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
                    path: canonical_path,
                });
            }
        }
        Ok(())
    }

    pub(super) fn admit_prefab_metadata(
        &self,
        directory: &DirectoryCapability,
        path: &AssetPath,
        binding: &SourceDirectoryBinding,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<
        Option<(&AssetRecord, Arc<AuthoringSourceFact>)>,
        AuthoringProjectSourceAdmissionError,
    > {
        let canonical_path = binding.scope.resolve(path);
        let Some(indexed) = self.metadata.get(&canonical_path) else {
            return Ok(None);
        };
        let metadata_path = path.meta_path();
        let read = read_unique_source(
            open_source_file(
                directory,
                join_project_path(&binding.directory, &metadata_path)?,
                &metadata_path,
                AuthoringSourceKind::AssetMetadata,
                indexed.dependency_depth(),
                budget,
            )?,
            AssetMetaFileLimits::default().encoded_bytes().get(),
            budget,
        )?;
        self.verify_metadata(path, binding, &read.source)?;
        let record = self
            .database
            .record_for_path(&canonical_path)
            .expect("indexed metadata has one record");
        budget.charge_source_bytes(0, 0, 0, source_fact_retained_bytes(&read.source)?)?;
        Ok(Some((record, read.source)))
    }
}

fn same_metadata_content(expected: &AuthoringSourceFact, actual: &AuthoringSourceFact) -> bool {
    expected.kind() == actual.kind()
        && expected.project_path() == actual.project_path()
        && expected.digest() == actual.digest()
        && expected.encoded_bytes() == actual.encoded_bytes()
}

fn revalidate_metadata_content(
    root: &DirectoryCapability,
    source: &AuthoringSourceFact,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let retained_before = budget.retained_candidate_bytes;
    let result = (|| {
        budget.admit_handle_peak(SOURCE_OPEN_HANDLE_PEAK)?;
        let work = source
            .encoded_bytes()
            .checked_add(1)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let scratch = checked_sum([
            64 * 1024,
            size_of::<OpenedAuthoringSource>(),
            source.project_path().as_str().len(),
        ])?;
        budget.charge_source_bytes(0, 0, work, scratch)?;
        let opened = open_source_file(
            root,
            source.project_path().clone(),
            source.project_path().as_str(),
            source.kind(),
            source.dependency_depth(),
            budget,
        )?;
        opened
            .capability
            .verify_digest(source.digest(), source.digest().length())
            .map_err(|error| match error {
                FsError::DigestMismatch { .. } | FsError::ByteLimitExceeded { .. } => {
                    AuthoringProjectSourceAdmissionError::SourceChanged {
                        kind: source.kind(),
                        path: source.project_path().clone(),
                    }
                }
                source_error => AuthoringProjectSourceAdmissionError::SourceHost {
                    kind: source.kind(),
                    path: source.project_path().clone(),
                    source: source_error,
                },
            })
    })();
    let temporary = budget
        .retained_candidate_bytes
        .checked_sub(retained_before)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    budget.reconcile_retained_reservation(temporary, 0)?;
    result
}

fn catalogue_host_error(source: FsError) -> AuthoringProjectSourceAdmissionError {
    AuthoringProjectSourceAdmissionError::SourceDirectoryHost {
        kind: AuthoringSourceKind::AssetMetadata,
        source,
    }
}

pub(super) fn charge_database_record(
    record: &AssetRecord,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    // The indexes own path copies and a case-folded key in addition to the record.
    let bytes = checked_sum([
        record
            .path()
            .as_str()
            .len()
            .checked_mul(8)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        asset_record_retained_bytes(record)?,
        size_of::<AssetSourcePath>() * 3 + 192,
    ])?;
    budget.charge_source_bytes(0, 0, 0, bytes)?;
    Ok(())
}

/// Reuses an exact admitted identity while preserving all database conflict checks.
/// Physical source and metadata observations remain the caller's independent admission.
pub(super) fn admit_asset_record(
    database: &mut ProjectAssetDatabase,
    record: &AssetRecord,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    if database.record_for_path(record.path()) == Some(record) {
        return Ok(());
    }
    // The original database owns UUID, path and case-collision validation. Reserve its complete
    // insertion storage before cloning or building index keys; a rejected insert never replaces
    // a previous record. Reusing a record needs neither another allocation nor a second charge.
    charge_database_record(record, budget)?;
    let path = record.path().clone();
    database
        .insert(record.clone())
        .map_err(|source| AuthoringProjectSourceAdmissionError::AssetIdentity { path, source })
}
