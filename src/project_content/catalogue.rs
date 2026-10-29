//! Bounded identity lookup for references whose source path is not embedded in the document.

use nara_asset::{AssetMetaFileLimits, AssetMetaIdentityCandidate};
use nara_fs::{DirectoryEnumerationLimits, FileKind};

use super::*;

#[cfg(all(test, feature = "project-workflow"))]
mod measurement_tests;

#[derive(Default, Clone)]
pub(super) struct ReferenceCatalogue {
    metadata: BTreeMap<AssetPath, (usize, ContentDigest)>,
    pub(super) work_bytes: usize,
    complete: bool,
}

pub(crate) struct DeliveryCatalogue {
    pub(super) roots: [nara_project::ProjectPath; 2],
    pub(super) catalogue: ReferenceCatalogue,
    pub(super) asset_database: ProjectAssetDatabase,
    pub(super) asset_database_work_bytes: usize,
    _budget: BudgetTicket,
}

impl LoadContext<'_> {
    pub(super) fn take_delivery_catalogue(&mut self) -> Option<DeliveryCatalogue> {
        self.completed_delivery_catalogue.take()
    }

    fn capture_delivery_catalogue(&mut self) -> Result<(), ProjectContentError> {
        if !self.capture_delivery_catalogue
            || self.completed_delivery_catalogue.is_some()
            || !self
                .catalogue
                .as_ref()
                .is_some_and(|catalogue| catalogue.complete)
        {
            return Ok(());
        }
        let roots = self
            .catalogue_roots
            .as_ref()
            .expect("content loading binds the manifest directories before decoding");
        let catalogue = self
            .catalogue
            .as_ref()
            .expect("complete catalogue remains available during delivery capture");
        let roots_work = roots.iter().try_fold(0usize, |bytes, root| {
            checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [bytes, string_retained_bytes(root.as_str())?],
            )
        })?;
        let mut budget = self.loader.budget_host.reserve();
        budget
            .set(
                ProjectContentBudgetKind::WorkBytes,
                checked_plan_sum(
                    ProjectContentBudgetKind::WorkBytes,
                    [
                        size_of::<DeliveryCatalogue>(),
                        roots_work,
                        catalogue.work_bytes,
                        self.asset_database_work_bytes,
                    ],
                )?,
            )
            .map_err(ProjectContentError::budget)?;
        self.completed_delivery_catalogue = Some(DeliveryCatalogue {
            roots: roots.clone(),
            catalogue: catalogue.clone(),
            asset_database: self.asset_database.clone(),
            asset_database_work_bytes: self.asset_database_work_bytes,
            _budget: budget,
        });
        self.capture_delivery_catalogue = false;
        Ok(())
    }

    pub(super) fn ensure_reference_catalogue(&mut self) -> Result<(), ProjectContentError> {
        if self
            .catalogue
            .as_ref()
            .is_some_and(|catalogue| catalogue.complete)
        {
            return Ok(());
        }
        let roots = self
            .catalogue_roots
            .as_ref()
            .expect("content loading binds the manifest directories before decoding")
            .clone();
        self.catalogue
            .get_or_insert_with(ReferenceCatalogue::default);
        #[cfg(all(test, feature = "project-workflow"))]
        measurement_tests::record(|counts| counts.catalogue_builds += 1);
        for (root_index, path) in roots.iter().enumerate() {
            let directory = self.open_directory(path)?;
            let result = self.scan_metadata_directory(&directory, root_index);
            drop(directory);
            self.subtract_budget(ProjectContentBudgetKind::OpenHandles, 1);
            result?;
        }
        self.catalogue
            .as_mut()
            .expect("catalogue was initialized")
            .complete = true;
        self.capture_delivery_catalogue()?;
        Ok(())
    }

    fn scan_metadata_directory(
        &mut self,
        root: &DirectoryCapability,
        root_index: usize,
    ) -> Result<(), ProjectContentError> {
        let mut pending = VecDeque::from([(String::new(), 0_usize)]);
        self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
        while let Some((prefix, depth)) = pending.pop_front() {
            self.subtract_budget(ProjectContentBudgetKind::QueuedJobs, 1);
            self.observe_depth(depth)?;
            self.add_budget(
                ProjectContentBudgetKind::OpenHandles,
                RELATIVE_OPEN_HANDLE_PEAK,
            )?;
            let directory = if prefix.is_empty() {
                root.share()
            } else {
                root.open_directory(&RelativePath::new(&prefix).map_err(|_| invalid_path(&prefix))?)
                    .map_err(map_fs_error)?
            };
            const BATCH_ENTRIES: usize = 64;
            const MAX_NAME_BYTES: usize = 1_024;
            let limits = DirectoryEnumerationLimits::new(
                self.loader
                    .limits
                    .limit(ProjectContentBudgetKind::DirectoryEntries),
                MAX_NAME_BYTES,
                self.loader
                    .limits
                    .limit(ProjectContentBudgetKind::DirectoryEntries),
            );
            let mut enumeration = directory
                .begin_child_enumeration(limits)
                .map_err(map_fs_error)?;
            #[cfg(all(test, feature = "project-workflow"))]
            measurement_tests::record(|counts| counts.directory_enumerations += 1);
            loop {
                let batch_memory = BATCH_ENTRIES
                    * (MAX_NAME_BYTES
                        + size_of::<nara_fs::DirectoryEntryObservation>()
                        + size_of::<nara_fs::DirectoryEntryRejectedFact>());
                let reservation = self.reserve_work(batch_memory)?;
                let batch = enumeration
                    .next_batch(BATCH_ENTRIES)
                    .map_err(map_fs_error)?;
                #[cfg(all(test, feature = "project-workflow"))]
                measurement_tests::record(|counts| {
                    counts.directory_entries +=
                        batch.entries().len() + batch.rejected_entry_facts().len();
                });
                self.add_budget(
                    ProjectContentBudgetKind::DirectoryEntries,
                    batch.entries().len() + batch.rejected_entry_facts().len(),
                )?;
                for entry in batch.entries() {
                    let name = entry.name().to_str().ok_or_else(|| {
                        ProjectContentError::single(
                            ProjectContentErrorKind::InvalidLogicalPath,
                            "project.content.catalogue-name-invalid",
                            "Asset catalogue entry cannot be represented by a logical UTF-8 path",
                        )
                    })?;
                    let path_len = prefix
                        .len()
                        .checked_add(name.len())
                        .and_then(|length| length.checked_add(usize::from(!prefix.is_empty())))
                        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::PathBytes))?;
                    self.reserve_catalogue_work(
                        path_len
                            .checked_add(size_of::<String>() + BTREE_ENTRY_OVERHEAD)
                            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?,
                    )?;
                    let path = if prefix.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{prefix}/{name}")
                    };
                    self.observe_path(&path)?;
                    match entry.kind() {
                        FileKind::Directory => {
                            self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
                            pending.push_back((
                                path,
                                depth.checked_add(1).ok_or_else(|| {
                                    overflow_budget(ProjectContentBudgetKind::DirectoryDepth)
                                })?,
                            ));
                        }
                        FileKind::Regular if path.ends_with(".meta") => {
                            self.index_metadata(root, root_index, &path)?
                        }
                        _ => {}
                    }
                }
                let complete = batch.is_complete();
                drop(batch);
                self.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
                if complete {
                    break;
                }
            }
            drop((enumeration, directory));
            self.subtract_budget(
                ProjectContentBudgetKind::OpenHandles,
                RELATIVE_OPEN_HANDLE_PEAK,
            );
        }
        Ok(())
    }

    fn index_metadata(
        &mut self,
        root: &DirectoryCapability,
        root_index: usize,
        meta_path: &str,
    ) -> Result<(), ProjectContentError> {
        let source = meta_path
            .strip_suffix(".meta")
            .expect("metadata suffix was selected");
        let path = AssetPath::new(source).map_err(|_| invalid_path(source))?;
        let ceiling = AssetMetaFileLimits::default().encoded_bytes().get();
        let (bytes, digest) = self.read_file(root, meta_path, ceiling)?;
        #[cfg(all(test, feature = "project-workflow"))]
        measurement_tests::record(|counts| counts.identity_metadata_reads += 1);
        let reservation = self.reserve_work(ceiling)?;
        let identity =
            AssetMetaIdentityCandidate::decode_json_bytes_owned(bytes.into_boxed_slice())
                .map_err(|_| invalid_metadata(&path))?
                .into_identity();
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
        self.index_metadata_identity(path, root_index, digest, identity)
    }

    pub(super) fn index_metadata_identity(
        &mut self,
        path: AssetPath,
        root_index: usize,
        digest: ContentDigest,
        identity: nara_asset::AssetMetaIdentity,
    ) -> Result<(), ProjectContentError> {
        if let Some((indexed_root, indexed_digest)) = self
            .catalogue
            .as_ref()
            .and_then(|catalogue| catalogue.metadata.get(&path))
        {
            if *indexed_root != root_index {
                return Err(identity_conflict(&path));
            }
            if *indexed_digest != digest {
                return Err(ProjectContentError::with_identifier(
                    ProjectContentErrorKind::HostAuthorityRejected,
                    "project.content.asset-metadata-source-changed",
                    "Asset metadata changed after reference discovery; retry content loading",
                    "asset",
                    path.as_str(),
                ));
            }
            if self.asset_database.record_for_path(&path) == Some(&AssetRecord::from(identity)) {
                return Ok(());
            }
            return Err(identity_conflict(&path));
        }
        self.reserve_asset_database_work(&AssetRecord::from(identity.clone()))?;
        self.reserve_catalogue_work(
            size_of::<(AssetPath, (usize, ContentDigest))>()
                + path.as_str().len()
                + BTREE_ENTRY_OVERHEAD,
        )?;
        self.asset_database
            .insert_identity(path.clone(), identity)
            .map_err(|_| identity_conflict(&path))?;
        self.catalogue
            .as_mut()
            .expect("indexing owns a catalogue")
            .metadata
            .insert(path, (root_index, digest));
        Ok(())
    }

    fn reserve_catalogue_work(&mut self, bytes: usize) -> Result<(), ProjectContentError> {
        self.add_budget(ProjectContentBudgetKind::WorkBytes, bytes)?;
        let catalogue = self
            .catalogue
            .get_or_insert_with(ReferenceCatalogue::default);
        catalogue.work_bytes = catalogue
            .work_bytes
            .checked_add(bytes)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        Ok(())
    }

    pub(super) fn release_catalogue_work(&mut self) {
        if let Some(catalogue) = self.catalogue.take() {
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, catalogue.work_bytes);
        }
    }

    pub(super) fn reserve_asset_database_work(
        &mut self,
        record: &AssetRecord,
    ) -> Result<(), ProjectContentError> {
        let bytes = asset_record_retained_bytes(record)?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, bytes)?;
        self.asset_database_work_bytes = self
            .asset_database_work_bytes
            .checked_add(bytes)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        Ok(())
    }

    pub(super) fn retain_loaded_asset_identities(
        &mut self,
        prefabs: &BTreeMap<AssetPath, LoadedPrefabContent>,
        images: &[ProjectImageContent],
        tilesets: &[ProjectTileSetContent],
        domains: &[Box<dyn domains::ErasedCollection>],
    ) -> Result<ProjectAssetDatabase, ProjectContentError> {
        let paths = || {
            prefabs
                .keys()
                .chain(images.iter().map(ProjectImageContent::path))
                .chain(tilesets.iter().map(ProjectTileSetContent::path))
                .chain(
                    domains
                        .iter()
                        .flat_map(|domain| domain.records().iter().map(AssetRecord::path)),
                )
        };
        let mut bytes = size_of::<ProjectContentAssets>() + ARC_CONTROL_BLOCK_BYTES;
        for path in paths() {
            if let Some(record) = self.asset_database.record_for_path(path) {
                bytes = bytes
                    .checked_add(asset_record_retained_bytes(record)?)
                    .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?;
            }
        }
        let reservation = self.reserve_work(bytes)?;
        let mut database = ProjectAssetDatabase::default();
        for path in paths() {
            if let Some(record) = self.asset_database.record_for_path(path) {
                database
                    .insert(record.clone())
                    .map_err(|_| identity_conflict(path))?;
            }
        }
        self.commit_work_as_retained(reservation, bytes)?;
        self.asset_database = ProjectAssetDatabase::default();
        self.subtract_budget(
            ProjectContentBudgetKind::WorkBytes,
            self.asset_database_work_bytes,
        );
        self.asset_database_work_bytes = 0;
        Ok(database)
    }

    pub(super) fn resolve_reference_path(
        &mut self,
        reference: &AssetRef,
        kind: &AssetSourceKind,
    ) -> Result<AssetPath, ProjectContentError> {
        if !reference.is_primary() {
            return Err(unsupported_product_selection(reference));
        }
        if let AssetRef::Path(path) = reference {
            return Ok(path.clone());
        }
        self.ensure_reference_catalogue()?;
        reference.validate_with_database_as(&self.asset_database, kind)
            .map(|record| record.path().clone())
            .map_err(|_| ProjectContentError::single(
                ProjectContentErrorKind::AssetMetaMismatch,
                "project.content.stable-asset-reference-unresolved",
                "Stable asset reference is missing from the admitted catalogue or has the wrong source kind",
            ))
    }

    pub(super) fn discover_source_reference(
        &mut self,
        assets: &DirectoryCapability,
        reference: &AssetRef,
    ) -> Result<AssetRecord, ProjectContentError> {
        match reference {
            AssetRef::Path(path) => {
                if self.asset_database.record_for_path(path).is_none() {
                    self.catalogue
                        .get_or_insert_with(ReferenceCatalogue::default);
                    let meta_path = path.meta_path();
                    self.index_metadata(assets, 0, meta_path.as_str())?;
                }
            }
            AssetRef::StableId(_) | AssetRef::Product(_) => self.ensure_reference_catalogue()?,
        }
        self.asset_database
            .resolve_ref(reference)
            .cloned()
            .map_err(|_| {
                ProjectContentError::single(
                    ProjectContentErrorKind::AssetMetaMismatch,
                    "project.content.stable-asset-reference-unresolved",
                    "Asset reference is missing from the admitted source identities",
                )
            })
    }

    pub(super) fn verify_catalogued_metadata(
        &self,
        path: &AssetPath,
        root: &nara_project::ProjectPath,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        if let Some((root_index, expected)) = self
            .catalogue
            .as_ref()
            .and_then(|catalogue| catalogue.metadata.get(path))
        {
            #[cfg(all(test, feature = "project-workflow"))]
            measurement_tests::record(|counts| counts.metadata_revalidations += 1);
            let roots = self
                .catalogue_roots
                .as_ref()
                .expect("catalogue roots remain bound during admission");
            if &roots[*root_index] != root || *expected != digest {
                return Err(ProjectContentError::with_identifier(
                    ProjectContentErrorKind::HostAuthorityRejected,
                    "project.content.asset-metadata-source-changed",
                    "Asset metadata changed after stable reference resolution; retry content loading",
                    "asset",
                    path.as_str(),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn admit_catalogued_prefab_metadata(
        &mut self,
        directory: &DirectoryCapability,
        root: &nara_project::ProjectPath,
        path: &AssetPath,
    ) -> Result<(), ProjectContentError> {
        if !self
            .catalogue
            .as_ref()
            .is_some_and(|catalogue| catalogue.metadata.contains_key(path))
        {
            return Ok(());
        }
        let meta_path = path.meta_path();
        let (_, digest) = self.read_native_file(
            directory,
            &meta_path,
            AssetMetaFileLimits::default().encoded_bytes().get(),
        )?;
        self.verify_catalogued_metadata(path, root, digest)?;
        self.record_member(
            ProjectContentMemberKind::AssetMetadata,
            root,
            &meta_path,
            digest,
        )
    }
}

pub(crate) fn asset_record_retained_bytes(
    record: &AssetRecord,
) -> Result<usize, ProjectContentError> {
    // The database owns four path copies and a case-folded key; Unicode folding can grow the key.
    let kind_bytes = match record.source_kind() {
        AssetSourceKind::Other(kind) => kind.len(),
        _ => 0,
    };
    record
        .path()
        .as_str()
        .len()
        .checked_mul(8)
        .and_then(|bytes| bytes.checked_add(kind_bytes))
        .and_then(|bytes| {
            bytes.checked_add(
                size_of::<AssetRecord>() + 3 * (size_of::<AssetPath>() + BTREE_ENTRY_OVERHEAD),
            )
        })
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

pub(super) fn identity_conflict(path: &AssetPath) -> ProjectContentError {
    ProjectContentError::with_identifier(
        ProjectContentErrorKind::AssetMetaMismatch,
        "project.content.asset-identity-conflict",
        "Asset identity conflicts within the admitted content sources",
        "asset",
        path.as_str(),
    )
}

fn invalid_metadata(path: &AssetPath) -> ProjectContentError {
    ProjectContentError::with_identifier(
        ProjectContentErrorKind::AssetMeta,
        "project.content.asset-meta-invalid",
        "Asset catalogue metadata is invalid",
        "asset",
        path.as_str(),
    )
}

fn invalid_path(path: &str) -> ProjectContentError {
    ProjectContentError::with_identifier(
        ProjectContentErrorKind::InvalidLogicalPath,
        "project.content.catalogue-path-invalid",
        "Asset catalogue path is invalid",
        "asset",
        path,
    )
}
