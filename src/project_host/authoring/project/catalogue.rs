//! Bounded stable lookup observations alongside physical source receipts.

use std::{collections::VecDeque, io::ErrorKind};

use nara_asset::StableAssetId;
use nara_fs::{DirectoryEnumerationLimits, FileKind};

use super::*;

#[cfg(test)]
mod tests;

/// One demanded global identity observation, separate from its reader and working-copy evidence.
pub(super) struct StableLookupObservation {
    stable_id: StableAssetId,
    // An unresolved result remains provisional; capture still requires recapture.
    target: Option<AssetSourcePath>,
}

/// The actual directory capability binding and expected kind of a source query.
/// Neither field is part of canonical source identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceDirectoryBinding {
    pub(super) directory: Arc<ProjectPath>,
    pub(super) scope: Arc<AssetPathScope>,
    pub(super) expected_kind: Option<AssetSourceKind>,
}

impl SourceDirectoryBinding {
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
}

impl StableLookupObservation {
    pub(super) fn retained_bytes(&self) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        checked_sum([
            size_of::<Self>(),
            self.target.as_ref().map_or(0, |path| path.as_str().len()),
        ])
    }
}

pub(super) struct ReferenceCatalogue {
    root: DirectoryCapability,
    bindings: Vec<SourceDirectoryBinding>,
    indexed: bool,
    database: ProjectAssetDatabase,
    metadata: BTreeMap<AssetSourcePath, Arc<AuthoringSourceFact>>,
    source_bindings: BTreeMap<AssetSourcePath, SourceDirectoryBinding>,
    lookups: BTreeMap<StableAssetId, StableLookupObservation>,
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
            .filter_map(|lookup| self.metadata.get(lookup.target.as_ref()?))
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
        Ok(path.map(|path| ResolvedResourceSource { path, binding }))
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
            let target = self.indexed_source_path(stable_id);
            budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([
                    size_of::<StableLookupObservation>(),
                    target.map_or(0, |path| path.as_str().len()),
                    64,
                ])?,
            )?;
            let observation = StableLookupObservation {
                stable_id,
                target: target.cloned(),
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
    fn indexed_source_path(&self, stable_id: StableAssetId) -> Option<&AssetSourcePath> {
        let record = self.database.record_for_stable_id(stable_id)?;
        self.metadata.get(record.path())?;
        Some(record.path())
    }

    pub(super) fn into_lookup_observations(
        self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Arc<[StableLookupObservation]>, AuthoringProjectSourceAdmissionError> {
        // Entries move their paths; the temporary vector and final Arc coexist during transfer.
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                checked_product(self.lookups.len(), 2 * size_of::<StableLookupObservation>())?,
                2 * size_of::<usize>(),
            ])?,
        )?;
        Ok(self.lookups.into_values().collect::<Vec<_>>().into())
    }

    pub(super) fn revalidate_lookups(
        root: &DirectoryCapability,
        bindings: Vec<SourceDirectoryBinding>,
        observations: &[StableLookupObservation],
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
        current.ensure_indexed(budget)?;
        for observation in observations {
            let target = current.indexed_source_path(observation.stable_id);
            if target != observation.target.as_ref() {
                // None is an observed unresolved lookup, never a durable absence receipt.
                let path = observation
                    .target
                    .as_ref()
                    .or(target)
                    .expect("different optional lookup targets include at least one path");
                return Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
                    path: path.clone(),
                });
            }
        }
        for source in current.lookup_sources() {
            revalidate_source_fact(root, source, budget)?;
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
        charge_database_record(&record, budget)?;
        // Overlapping configured directories can observe the same canonical source twice.
        // Both the sidecar declaration and exact source observation must still agree.
        if self.database.record_for_path(record.path()) == Some(&record) {
            if self
                .metadata
                .get(record.path())
                .is_some_and(|indexed| indexed.same_file_observation(&read.source))
            {
                return Ok(());
            }
            return Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
                path: record.path().clone(),
            });
        }
        self.database.insert(record).map_err(|source| {
            AuthoringProjectSourceAdmissionError::AssetIdentity {
                path: canonical_path.clone(),
                source,
            }
        })?;
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
            if !indexed.same_file_observation(source) {
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
    ) -> Result<Option<(AssetRecord, Arc<AuthoringSourceFact>)>, AuthoringProjectSourceAdmissionError>
    {
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
            .expect("indexed metadata has one record")
            .clone();
        charge_database_record(&record, budget)?;
        budget.charge_source_bytes(0, 0, 0, source_fact_retained_bytes(&read.source)?)?;
        Ok(Some((record, read.source)))
    }
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

pub(super) fn insert_asset_record(
    database: &mut ProjectAssetDatabase,
    record: AssetRecord,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let path = record.path().clone();
    database
        .insert(record)
        .map_err(|source| AuthoringProjectSourceAdmissionError::AssetIdentity { path, source })
}
