//! Lookup candidates for stable references; loaded metadata remains the closure authority.

use std::{collections::VecDeque, io::ErrorKind};

use nara_fs::{DirectoryEnumerationLimits, FileKind};

use super::*;

#[cfg(test)]
mod tests;

pub(super) struct ReferenceCatalogue {
    root: DirectoryCapability,
    directories: [ProjectPath; 2],
    indexed: bool,
    database: ProjectAssetDatabase,
    metadata: BTreeMap<AssetPath, (bool, Arc<AuthoringSourceFact>)>,
}

impl ReferenceCatalogue {
    pub(super) fn new(root: DirectoryCapability, directories: [ProjectPath; 2]) -> Self {
        Self {
            root,
            directories,
            indexed: false,
            database: ProjectAssetDatabase::default(),
            metadata: BTreeMap::new(),
        }
    }

    pub(super) const fn is_indexed(&self) -> bool {
        self.indexed
    }

    pub(super) fn lookup_sources(&self) -> impl Iterator<Item = &Arc<AuthoringSourceFact>> {
        self.metadata.values().map(|(_, source)| source)
    }

    pub(super) fn resolve(
        &mut self,
        reference: &AssetRef,
        prefab: bool,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Option<AssetPath>, AuthoringProjectSourceAdmissionError> {
        if let AssetRef::Path(path) = reference {
            return Ok(Some(path.clone()));
        }
        if !self.indexed {
            for prefab in [false, true] {
                self.index_directory(prefab, budget)?;
            }
            self.indexed = true;
        }
        Ok(self
            .database
            .resolve_ref(reference)
            .ok()
            .and_then(|record| {
                let (indexed_prefab, _) = self.metadata.get(record.path())?;
                (*indexed_prefab == prefab
                    && (!prefab || record.source_kind() == &AssetSourceKind::Prefab))
                    .then(|| record.path().clone())
            }))
    }

    fn index_directory(
        &mut self,
        prefab: bool,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let root_path = &self.directories[usize::from(prefab)];
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
                            self.index_metadata(&directory, prefab, &path, depth, budget)?;
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
        prefab: bool,
        metadata_path: &str,
        depth: usize,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let source_path = metadata_path
            .strip_suffix(".meta")
            .expect("selected metadata suffix");
        let project_path =
            join_project_path(&self.directories[usize::from(prefab)], metadata_path)?;
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
                path: path.clone(),
                source,
            },
        )?;
        let record = AssetRecord::from(metadata.identity().clone());
        charge_asset_metadata(&read.source, encoded_bytes, &metadata, &record, budget)?;
        charge_database_record(&record, budget)?;
        self.database
            .insert_identity(path.clone(), metadata.into_identity())
            .map_err(
                |source| AuthoringProjectSourceAdmissionError::AssetIdentity {
                    path: path.clone(),
                    source,
                },
            )?;
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                size_of::<(AssetPath, (bool, Arc<AuthoringSourceFact>))>(),
                path.as_str().len(),
                64,
            ])?,
        )?;
        self.metadata.insert(path, (prefab, read.source));
        Ok(())
    }

    pub(super) fn verify_metadata(
        &self,
        path: &AssetPath,
        prefab: bool,
        source: &AuthoringSourceFact,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        if let Some((indexed_prefab, indexed)) = self.metadata.get(path) {
            if *indexed_prefab != prefab
                || indexed.identity() != source.identity()
                || indexed.digest() != source.digest()
            {
                return Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
                    path: path.clone(),
                });
            }
        }
        Ok(())
    }

    pub(super) fn admit_prefab_metadata(
        &self,
        directory: &DirectoryCapability,
        path: &AssetPath,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Option<(AssetRecord, Arc<AuthoringSourceFact>)>, AuthoringProjectSourceAdmissionError>
    {
        let Some((_, indexed)) = self.metadata.get(path) else {
            return Ok(None);
        };
        let metadata_path = path.meta_path();
        let read = read_unique_source(
            open_source_file(
                directory,
                join_project_path(&self.directories[1], &metadata_path)?,
                &metadata_path,
                AuthoringSourceKind::AssetMetadata,
                indexed.dependency_depth(),
                budget,
            )?,
            AssetMetaFileLimits::default().encoded_bytes().get(),
            budget,
        )?;
        self.verify_metadata(path, true, &read.source)?;
        let record = self
            .database
            .record_for_path(path)
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
        size_of::<AssetPath>() * 3 + 192,
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
