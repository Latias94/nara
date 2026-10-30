//! Bounded identity lookup for references whose source path is not embedded in the document.

use nara_asset::{AssetMetaFileLimits, AssetMetaIdentityCandidate};
use nara_fs::{DirectoryEnumerationLimits, FileKind};

use super::*;

#[cfg(all(test, feature = "project-workflow"))]
mod measurement_tests;

#[derive(Default, Clone)]
pub(super) struct ReferenceCatalogue {
    pub(super) metadata: BTreeMap<AssetSourcePath, CataloguedSource>,
    pub(super) work_bytes: usize,
    complete: bool,
}

#[derive(Clone)]
pub(super) struct CataloguedSource {
    pub(super) digest: ContentDigest,
    pub(super) reader: usize,
}

pub(crate) struct DeliveryCatalogue {
    pub(super) roots: Vec<nara_project::ProjectPath>,
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
        let roots_work = roots.iter().try_fold(
            size_of::<nara_project::ProjectPath>() * roots.len(),
            |bytes, root| {
                checked_plan_sum(
                    ProjectContentBudgetKind::WorkBytes,
                    [bytes, string_retained_bytes(root.as_str())?],
                )
            },
        )?;
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
        let root_count = self
            .catalogue_roots
            .as_ref()
            .expect("content loading binds the manifest directories before decoding")
            .len();
        self.catalogue
            .get_or_insert_with(ReferenceCatalogue::default);
        #[cfg(all(test, feature = "project-workflow"))]
        measurement_tests::record(|counts| counts.catalogue_builds += 1);
        for root_index in 0..root_count {
            let path_bytes = string_retained_bytes(
                self.catalogue_roots
                    .as_ref()
                    .expect("catalogue roots remain bound")[root_index]
                    .as_str(),
            )?;
            let path_work = self.reserve_work(path_bytes)?;
            let path = self
                .catalogue_roots
                .as_ref()
                .expect("catalogue roots remain bound")[root_index]
                .clone();
            let directory = self.open_directory(&path)?;
            let result = self.scan_metadata_directory(&directory, root_index);
            drop((directory, path));
            self.subtract_budget(ProjectContentBudgetKind::OpenHandles, 1);
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, path_work.reserved);
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
        // A nested configured root can enumerate the same actual metadata through two readers.
        // Bind its authored local path against the location actually observed by this reader.
        // Source kinds and installed owners do not select an identity namespace or activate data.
        let observed_bytes = scoped_path_bytes(self.catalogue_scope(root_index), &path)?;
        let scratch = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                observed_bytes,
                observed_bytes,
                2 * size_of::<AssetSourcePath>(),
            ],
        )?;
        let reservation = self.reserve_work(scratch)?;
        let result = (|| {
            let observed = self.catalogue_scope(root_index).resolve(&path);
            let owner_index = (0..self
                .catalogue_roots
                .as_ref()
                .expect("source roots are bound")
                .len())
                .find(|index| {
                    self.catalogue_scope(*index).relative_path(&observed)
                        == Some(identity.path().as_str())
                })
                .ok_or_else(|| identity_conflict(&observed))?;
            let record = identity.into_record(self.catalogue_scope(owner_index));
            if record.path() != &observed {
                return Err(identity_conflict(&observed));
            }
            if let Some(indexed) = self
                .catalogue
                .as_ref()
                .and_then(|catalogue| catalogue.metadata.get(&observed))
            {
                if indexed.digest != digest || indexed.reader != owner_index {
                    return Err(ProjectContentError::with_identifier(
                        ProjectContentErrorKind::HostAuthorityRejected,
                        "project.content.asset-metadata-source-changed",
                        "Asset metadata changed after reference discovery; retry content loading",
                        "asset",
                        observed.as_str(),
                    ));
                }
                return if self.asset_database.record_for_path(&observed) == Some(&record) {
                    Ok(())
                } else {
                    Err(identity_conflict(&observed))
                };
            }
            self.reserve_asset_database_work(&record)?;
            self.reserve_catalogue_work(checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [
                    size_of::<(AssetSourcePath, CataloguedSource)>(),
                    observed.as_str().len(),
                    BTREE_ENTRY_OVERHEAD,
                ],
            )?)?;
            self.asset_database
                .insert(record)
                .map_err(|_| identity_conflict(&observed))?;
            self.catalogue
                .as_mut()
                .expect("indexing owns a catalogue")
                .metadata
                .insert(
                    observed,
                    CataloguedSource {
                        digest,
                        reader: owner_index,
                    },
                );
            Ok(())
        })();
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
        result
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
        selected_scene: &AssetSourcePath,
        scenes: &[ProjectSceneContent],
        prefabs: &BTreeMap<AssetSourcePath, LoadedPrefabContent>,
        images: &[ProjectImageContent],
        tilesets: &[ProjectTileSetContent],
        domains: &[Box<dyn domains::ErasedCollection>],
    ) -> Result<ProjectAssetDatabase, ProjectContentError> {
        let paths = || {
            std::iter::once(selected_scene)
                .chain(scenes.iter().map(ProjectSceneContent::path))
                .chain(prefabs.values().map(|prefab| &prefab.path))
                .chain(images.iter().map(ProjectImageContent::path))
                .chain(tilesets.iter().map(ProjectTileSetContent::path))
                .chain(
                    domains
                        .iter()
                        .flat_map(|domain| domain.records().iter().map(AssetRecord::path)),
                )
        };
        let mut bytes = size_of::<ProjectSourceContent>() + ARC_CONTROL_BLOCK_BYTES;
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
        root: &nara_project::ProjectPath,
    ) -> Result<(AssetPath, WorkReservation), ProjectContentError> {
        if !reference.is_primary() {
            return Err(unsupported_product_selection(reference));
        }
        if let AssetRef::Path(path) = reference {
            let work = self.reserve_work(string_retained_bytes(path.as_str())?)?;
            return Ok((path.clone(), work));
        }
        self.ensure_reference_catalogue()?;
        let unresolved = || {
            ProjectContentError::single(
                ProjectContentErrorKind::AssetMetaMismatch,
                "project.content.stable-asset-reference-unresolved",
                "Stable asset reference is missing from the admitted catalogue or has the wrong source kind",
            )
        };
        let local_bytes = {
            let scope = self.scope_for_root(root).ok_or_else(unresolved)?;
            let record = reference
                .validate_with_database_as(self.asset_database.resolver(scope), kind)
                .map_err(|_| unresolved())?;
            let alias = scope.relative_path(record.path()).ok_or_else(unresolved)?;
            string_retained_bytes(alias)?
        };
        let work = self.reserve_work(local_bytes)?;
        let scope = self.scope_for_root(root).ok_or_else(unresolved)?;
        let record = reference
            .validate_with_database_as(self.asset_database.resolver(scope), kind)
            .map_err(|_| unresolved())?;
        let path = scope.relativize(record.path()).ok_or_else(unresolved)?;
        Ok((path, work))
    }

    /// Resolves native source data through the sidecar's actual reader binding.
    /// Bare Path declarations retain the caller's explicit alias directory.
    pub(super) fn resolve_native_source(
        &mut self,
        reference: &AssetRef,
        kind: &AssetSourceKind,
        alias_root: &nara_project::ProjectPath,
    ) -> Result<(NativeSourceSelection, WorkReservation), ProjectContentError> {
        if !reference.is_primary() {
            return Err(unsupported_product_selection(reference));
        }
        let reader = match reference {
            AssetRef::Path(_) => self
                .catalogue_roots
                .as_ref()
                .and_then(|roots| roots.iter().position(|root| root == alias_root))
                .expect("the native alias directory has an admitted reader"),
            AssetRef::StableId(_) | AssetRef::Product(_) => {
                self.ensure_reference_catalogue()?;
                let id = match reference {
                    AssetRef::StableId(id) => *id,
                    AssetRef::Product(product) => product.source(),
                    AssetRef::Path(_) => unreachable!(),
                };
                let record = self
                    .asset_database
                    .record_for_stable_id(id)
                    .filter(|record| record.source_kind() == kind)
                    .ok_or_else(unresolved_source)?;
                self.catalogue
                    .as_ref()
                    .and_then(|catalogue| catalogue.metadata.get(record.path()))
                    .map(|source| source.reader)
                    .ok_or_else(unresolved_source)?
            }
        };
        let scope = self.catalogue_scope(reader);
        let (canonical_bytes, alias_bytes) = match reference {
            AssetRef::Path(path) => (scoped_path_bytes(scope, path)?, path.as_str().len()),
            AssetRef::StableId(_) | AssetRef::Product(_) => {
                let id = match reference {
                    AssetRef::StableId(id) => *id,
                    AssetRef::Product(product) => product.source(),
                    AssetRef::Path(_) => unreachable!(),
                };
                let canonical = self
                    .asset_database
                    .record_for_stable_id(id)
                    .expect("the stable native source was admitted above")
                    .path();
                (
                    canonical.as_str().len(),
                    scope
                        .relative_path(canonical)
                        .ok_or_else(unresolved_source)?
                        .len(),
                )
            }
        };
        let work = self.reserve_work(checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                size_of::<NativeSourceSelection>(),
                canonical_bytes,
                alias_bytes,
                2 * STRING_ALLOCATION_OVERHEAD,
            ],
        )?)?;
        let scope = self.catalogue_scope(reader);
        let (path, alias) = match reference {
            AssetRef::Path(alias) => (scope.resolve(alias), alias.clone()),
            AssetRef::StableId(_) | AssetRef::Product(_) => {
                let id = match reference {
                    AssetRef::StableId(id) => *id,
                    AssetRef::Product(product) => product.source(),
                    AssetRef::Path(_) => unreachable!(),
                };
                let path = self
                    .asset_database
                    .record_for_stable_id(id)
                    .expect("the stable native source was admitted above")
                    .path();
                (
                    path.clone(),
                    scope
                        .relativize(path)
                        .expect("the reader relation was admitted above"),
                )
            }
        };
        Ok((
            NativeSourceSelection {
                path,
                reader,
                alias,
            },
            work,
        ))
    }

    pub(super) fn discover_source_reference(
        &mut self,
        assets: &DirectoryCapability,
        reference: &AssetRef,
    ) -> Result<(AssetRecord, WorkReservation), ProjectContentError> {
        match reference {
            AssetRef::Path(path) => {
                let scratch = self
                    .asset_resolution()
                    .path_resolution_work_bytes(path)
                    .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
                let reservation = self.reserve_work(scratch)?;
                let present = self.asset_resolution().record_for_path(path).is_some();
                self.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
                if !present {
                    self.catalogue
                        .get_or_insert_with(ReferenceCatalogue::default);
                    let meta_path = path.meta_path();
                    self.index_metadata(assets, 0, meta_path.as_str())?;
                }
            }
            AssetRef::StableId(_) | AssetRef::Product(_) => self.ensure_reference_catalogue()?,
        }
        let scratch = self
            .asset_resolution()
            .resolution_work_bytes(reference)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let reservation = self.reserve_work(scratch)?;
        let result = (|| {
            let record_bytes = self
                .asset_resolution()
                .resolve_ref(reference)
                .map_err(|_| unresolved_source())
                .and_then(asset_record_retained_bytes)?;
            let record_work = self.reserve_work(record_bytes)?;
            let record = self
                .asset_resolution()
                .resolve_ref(reference)
                .map_err(|_| unresolved_source())?
                .clone();
            Ok((record, record_work))
        })();
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
        result
    }

    pub(super) fn verify_catalogued_metadata(
        &self,
        path: &AssetSourcePath,
        root: &nara_project::ProjectPath,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        if let Some(expected) = self
            .catalogue
            .as_ref()
            .and_then(|catalogue| catalogue.metadata.get(path))
        {
            #[cfg(all(test, feature = "project-workflow"))]
            measurement_tests::record(|counts| counts.metadata_revalidations += 1);
            if self
                .scope_for_root(root)
                .and_then(|scope| scope.relative_path(path))
                .is_none()
                || expected.digest != digest
            {
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

    pub(super) fn admit_catalogued_native_metadata(
        &mut self,
        directory: &DirectoryCapability,
        root: &nara_project::ProjectPath,
        path: &str,
        source_path: &AssetSourcePath,
        source_key: &SourceDocumentKey,
    ) -> Result<(), ProjectContentError> {
        if !self
            .catalogue
            .as_ref()
            .is_some_and(|catalogue| catalogue.metadata.contains_key(source_path))
        {
            return Ok(());
        }
        let record = self
            .asset_database
            .record_for_path(source_path)
            .expect("indexed native metadata has its actual source record");
        source_key.validate_asset_source(record).map_err(|_| {
            ProjectContentError::with_identifier(
                ProjectContentErrorKind::AssetMetaMismatch,
                if source_key.kind() == SourceDocumentKind::Prefab {
                    "project.content.prefab-source-association-invalid"
                } else {
                    "project.content.scene-source-association-invalid"
                },
                "Native document metadata conflicts with its admitted source",
                "source",
                source_path.as_str(),
            )
        })?;
        let path_work = self.reserve_work(checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [path.len(), ".meta".len(), STRING_ALLOCATION_OVERHEAD],
        )?)?;
        let meta_path = format!("{path}.meta");
        let (_, digest) = self.read_native_file(
            directory,
            &meta_path,
            AssetMetaFileLimits::default().encoded_bytes().get(),
        )?;
        self.verify_catalogued_metadata(source_path, root, digest)?;
        self.record_member(
            ProjectContentMemberKind::AssetMetadata,
            root,
            &meta_path,
            digest,
        )?;
        drop(meta_path);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, path_work.reserved);
        Ok(())
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
                size_of::<AssetRecord>()
                    + 3 * (size_of::<AssetSourcePath>() + BTREE_ENTRY_OVERHEAD),
            )
        })
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

pub(super) fn identity_conflict(path: &impl fmt::Display) -> ProjectContentError {
    ProjectContentError::with_identifier(
        ProjectContentErrorKind::AssetMetaMismatch,
        "project.content.asset-identity-conflict",
        "Asset identity conflicts within the admitted content sources",
        "asset",
        &path.to_string(),
    )
}

pub(super) fn scoped_path_bytes(
    scope: &AssetPathScope,
    alias: &AssetPath,
) -> Result<usize, ProjectContentError> {
    checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            scope.prefix().map_or(0, |prefix| prefix.as_str().len() + 1),
            alias.as_str().len(),
        ],
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

fn unresolved_source() -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::AssetMetaMismatch,
        "project.content.stable-asset-reference-unresolved",
        "Asset reference is missing from the admitted source identities",
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
