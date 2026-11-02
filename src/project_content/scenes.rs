//! Finite authored Scene capture through the production Prefab compiler.

use super::*;

impl LoadContext<'_> {
    /// Resolves a future target without interpreting its payload or claiming imported membership.
    /// Metadata remains an exact source receipt; the target body deliberately does not.
    pub(super) fn capture_resolved_source_target(
        &mut self,
        reference: &AssetRef,
        source: AssetRecord,
        source_work: WorkReservation,
        sources: &mut SourceCandidates,
    ) -> Result<(), ProjectContentError> {
        if source.stable_id().as_uuid().is_nil() {
            return Err(catalogue::identity_conflict(source.path()));
        }
        let kind = source.source_kind();
        if kind != &AssetSourceKind::Scene
            && kind != &AssetSourceKind::Image
            && !matches!(kind, AssetSourceKind::Other(kind) if kind == "tileset")
            && !self.domains.supports(kind)
        {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::UnsupportedAssetKind,
                "project.content.asset-kind-unsupported",
                "Project Scene content references an unsupported asset kind",
                "asset",
                source.path().as_str(),
            ));
        }
        if !self.domains.supports(kind) && !reference.is_primary() {
            return Err(unsupported_product_selection(reference));
        }
        // Compiled asset-domain importers, Images and TileSets read through the actual Assets
        // capability. Native Scene targets use the metadata's admitted reader instead.
        if kind != &AssetSourceKind::Scene
            && self.asset_scope.relative_path(source.path()).is_none()
        {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.content.asset-source-outside-scope",
                "Asset source is outside the admitted resource directory",
                "asset",
                source.path().as_str(),
            ));
        }
        if let Some(previous) = sources.resolved_sources.get(source.path()) {
            if previous != &source {
                return Err(catalogue::identity_conflict(source.path()));
            }
            drop(source);
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, source_work.reserved);
            return Ok(());
        }
        let reader = self
            .catalogue
            .as_ref()
            .and_then(|catalogue| catalogue.metadata.get(source.path()))
            .map(|observed| observed.reader)
            .ok_or_else(|| catalogue::identity_conflict(source.path()))?;
        let root = &self
            .catalogue_roots
            .as_ref()
            .expect("reference discovery retains the actual source readers")[reader];
        let alias = self
            .catalogue_scope(reader)
            .relative_path(source.path())
            .ok_or_else(|| catalogue::identity_conflict(source.path()))?;
        let path_work = self.reserve_work(checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                string_retained_bytes(root.as_str())?,
                string_retained_bytes(alias)?,
                string_retained_bytes(alias)?,
                ".meta".len(),
            ],
        )?)?;
        let root = self
            .catalogue_roots
            .as_ref()
            .expect("reference discovery retains the actual source readers")[reader]
            .clone();
        let alias = self
            .catalogue_scope(reader)
            .relativize(source.path())
            .expect("the actual reader relation was checked before allocation");
        let meta_path = alias.meta_path();
        let directory = self.open_directory(&root)?;
        let result = (|| {
            let (metadata, digest) = self.read_native_file(
                &directory,
                &meta_path,
                AssetMetaFileLimits::default().encoded_bytes().get(),
            )?;
            // The catalogue used the production identity decoder. Exact byte equality retains
            // that admitted binding without a second metadata interpretation.
            self.verify_catalogued_metadata(source.path(), &root, digest)?;
            self.record_member(
                ProjectContentMemberKind::AssetMetadata,
                &root,
                &meta_path,
                digest,
            )?;
            drop(metadata);
            self.check_source_readable(&directory, alias.as_str())
        })();
        drop((directory, meta_path, alias, root));
        self.subtract_budget(ProjectContentBudgetKind::OpenHandles, 1);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, path_work.reserved);
        result?;

        let entry_work = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                size_of::<(AssetSourcePath, AssetRecord)>(),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(source.path().as_str())?,
            ],
        )?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, entry_work)?;
        checked_add_to(
            &mut sources.resolved_source_work_bytes,
            checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [entry_work, source_work.reserved],
            )?,
            ProjectContentBudgetKind::WorkBytes,
        )?;
        sources
            .resolved_sources
            .insert(source.path().clone(), source);
        Ok(())
    }

    fn check_source_readable(
        &mut self,
        directory: &DirectoryCapability,
        alias: &str,
    ) -> Result<(), ProjectContentError> {
        let relative = self.observe_path(alias)?;
        self.add_budget(ProjectContentBudgetKind::Files, 1)?;
        self.add_budget(
            ProjectContentBudgetKind::OpenHandles,
            EXACT_RELATIVE_OPEN_HANDLE_PEAK,
        )?;
        let work = self.reserve_work(EXACT_RELATIVE_OPEN_WORK_BYTES)?;
        let opened = directory.open_file_exact(&relative);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, work.reserved);
        let result = opened.and_then(|file| {
            let mut byte = [0];
            let mut reader = file.reader()?;
            // A successful bounded probe, including EOF for an empty file, proves readability.
            // Its value is not interpreted, hashed or retained as a payload-validity claim.
            io::Read::read(&mut reader, &mut byte)
                .map(|_| ())
                .map_err(|source| FsError::Io {
                    operation: FsOperation::Read,
                    source,
                })
        });
        self.subtract_budget(
            ProjectContentBudgetKind::OpenHandles,
            EXACT_RELATIVE_OPEN_HANDLE_PEAK,
        );
        result.map_err(map_fs_error)
    }

    pub(super) fn retain_resolved_source_identities(
        &mut self,
        database: &mut ProjectAssetDatabase,
        sources: BTreeMap<AssetSourcePath, AssetRecord>,
        mut work_bytes: usize,
    ) -> Result<(), ProjectContentError> {
        for (path, source) in sources {
            if let Some(previous) = database.record_for_path(&path) {
                if previous != &source {
                    return Err(catalogue::identity_conflict(&path));
                }
            } else {
                // The discovery clone's reservation now takes custody of the actual database
                // entry. A future target gains identity, never a decoded collection member.
                let retained = asset_record_retained_bytes(&source)?;
                self.commit_work_as_retained(WorkReservation { reserved: retained }, retained)?;
                database
                    .insert(source)
                    .map_err(|_| catalogue::identity_conflict(&path))?;
                work_bytes = work_bytes
                    .checked_sub(retained)
                    .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
            }
        }
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, work_bytes);
        Ok(())
    }

    pub(super) fn reconcile_native_metadata(
        &mut self,
        path: &AssetSourcePath,
        source: &SourceDocumentKey,
    ) -> Result<(), ProjectContentError> {
        let Some(reader) = self
            .catalogue
            .as_ref()
            .and_then(|catalogue| catalogue.metadata.get(path))
            .map(|source| source.reader)
        else {
            return Ok(());
        };
        let root = &self
            .catalogue_roots
            .as_ref()
            .expect("source roots remain bound")[reader];
        let alias = self
            .catalogue_scope(reader)
            .relative_path(path)
            .ok_or_else(|| catalogue::identity_conflict(path))?;
        let work = self.reserve_work(checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                string_retained_bytes(root.as_str())?,
                string_retained_bytes(alias)?,
            ],
        )?)?;
        let root = self
            .catalogue_roots
            .as_ref()
            .expect("source roots remain bound")[reader]
            .clone();
        let alias = self
            .catalogue_scope(reader)
            .relativize(path)
            .expect("the captured source relation was checked above");
        let directory = self.open_directory(&root)?;
        let result =
            self.admit_catalogued_native_metadata(&directory, &root, alias.as_str(), path, source);
        drop((directory, root, alias));
        self.subtract_budget(ProjectContentBudgetKind::OpenHandles, 1);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, work.reserved);
        result
    }

    pub(super) fn compile_scene(
        &mut self,
        scene: &SceneDocument,
        source: &SourceDocumentKey,
        settings: &nara_project::EffectiveProjectSettings,
        loaded_prefabs: &mut BTreeMap<AssetSourcePath, LoadedPrefabContent>,
        prefab_cache_work: &mut usize,
    ) -> Result<CompiledSceneCandidate, ProjectContentError> {
        let mut compiler_work_limit = self
            .remaining_expansion_work(0, self.loader.limits.scene_compilation().working_bytes())?;
        let expansion_limits = self
            .loader
            .limits
            .scene_compilation()
            .with_working_bytes(compiler_work_limit);
        let mut work = self.reserve_work(prefab_expansion_work_plan(expansion_limits)?)?;
        let options = nara_scene::SceneCompilationOptions::default();
        let options = options.with_limits(expansion_limits).with_max_depth(
            options.max_depth.min(
                self.loader
                    .limits
                    .limit(ProjectContentBudgetKind::DirectoryDepth),
            ),
        );
        let validation = self.validation;
        let expansion = validation.bindings().try_compile_scene_with_loader(
            scene,
            options,
            None,
            |reference, working_set| {
                let suspended = working_set
                    .checked_work_bytes()
                    .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
                self.resize_work(&mut work, suspended)?;
                self.add_budget(ProjectContentBudgetKind::DependencyEdges, 1)?;
                let (selection, selection_work) = self.resolve_native_source(
                    reference,
                    &AssetSourceKind::Prefab,
                    &settings.paths.prefabs,
                )?;
                self.observe_path(selection.path.as_str())?;
                let (resolved, resolved_work) = if let Some(loaded) =
                    loaded_prefabs.get(&selection.path)
                {
                    let resolved_work = self.reserve_work(loaded.source.retained_bytes())?;
                    let resolved = resolved_prefab_source(loaded, &self.asset_database)?;
                    (resolved, resolved_work)
                } else {
                    let cache_work = checked_plan_sum(
                        ProjectContentBudgetKind::WorkBytes,
                        [
                            size_of::<(AssetSourcePath, LoadedPrefabContent)>(),
                            BTREE_ENTRY_OVERHEAD,
                            string_retained_bytes(selection.path.as_str())?,
                            string_retained_bytes(selection.path.as_str())?,
                        ],
                    )?;
                    self.add_budget(ProjectContentBudgetKind::WorkBytes, cache_work)?;
                    checked_add_to(
                        prefab_cache_work,
                        cache_work,
                        ProjectContentBudgetKind::WorkBytes,
                    )?;
                    let root_work = self.reserve_work(string_retained_bytes(
                        self.catalogue_roots
                            .as_ref()
                            .expect("source roots remain bound")[selection.reader]
                            .as_str(),
                    )?)?;
                    let root = self
                        .catalogue_roots
                        .as_ref()
                        .expect("source roots remain bound")[selection.reader]
                        .clone();
                    let directory = self.open_directory(&root)?;
                    self.add_budget(ProjectContentBudgetKind::InFlightJobs, 1)?;
                    let loaded = self.load_prefab(
                        &directory,
                        &root,
                        &selection.alias,
                        &selection.path,
                        settings.project.native_document_identity,
                        settings.project.source_universe,
                    );
                    self.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
                    drop((directory, root));
                    self.subtract_budget(ProjectContentBudgetKind::OpenHandles, 1);
                    self.subtract_budget(ProjectContentBudgetKind::WorkBytes, root_work.reserved);
                    let loaded = loaded?;
                    let resolved_work = self.reserve_work(loaded.source.retained_bytes())?;
                    let resolved = resolved_prefab_source(&loaded, &self.asset_database)?;
                    loaded_prefabs.insert(selection.path.clone(), loaded);
                    (resolved, resolved_work)
                };
                drop(selection);
                self.subtract_budget(ProjectContentBudgetKind::WorkBytes, selection_work.reserved);
                // The resumed compiler's full promise takes custody of the returned key clone.
                self.subtract_budget(ProjectContentBudgetKind::WorkBytes, resolved_work.reserved);
                let limit = self.remaining_expansion_work(work.reserved, compiler_work_limit)?;
                if suspended > limit.get() {
                    return Err(ProjectContentError::budget(
                        ProjectContentBudgetError::synthetic(
                            ProjectContentBudgetKind::WorkBytes,
                            suspended,
                            limit.get(),
                        ),
                    ));
                }
                self.resize_work(&mut work, limit.get())?;
                working_set.restrict_working_bytes(limit);
                compiler_work_limit = limit;
                Ok::<_, ProjectContentError>(Some(resolved))
            },
        )?;
        self.source_upgrade_required |= expansion.source_upgrade_required();
        let compiled = expansion.candidate.ok_or_else(|| {
            map_scene_diagnostics(
                ProjectContentErrorKind::SceneCompilation,
                expansion.diagnostics,
            )
        })?;
        let retained_data = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [
                size_of::<CompiledSceneCandidate>(),
                compiled_scene_retained_bytes(&compiled)?,
            ],
        )?;
        self.resize_work(&mut work, retained_data)?;
        self.commit_work_as_retained(work, retained_data)?;
        // Containing identity belongs to the Host, independently of the reusable compiler cap.
        let identity_work = self.reserve_work(source.retained_bytes())?;
        let candidate = compiled.bind_source(source.clone());
        for (_, origin) in candidate.canonical_provenance().iter() {
            if let Some(path) = origin.projection_path() {
                self.observe_depth(path.steps().len())?;
            }
        }
        self.commit_work_as_retained(identity_work, identity_work.reserved)?;
        Ok(candidate)
    }

    pub(super) fn enqueue_scene(
        &mut self,
        sources: &mut SourceCandidates,
        record: &AssetRecord,
    ) -> Result<(), ProjectContentError> {
        let path = record.path();
        if sources.seen_scenes.contains(path) {
            return Ok(());
        }
        let reader = self
            .catalogue
            .as_ref()
            .and_then(|catalogue| catalogue.metadata.get(path))
            .map(|observed| observed.reader)
            .ok_or_else(|| catalogue::identity_conflict(path))?;
        let alias = self
            .catalogue_scope(reader)
            .relative_path(path)
            .ok_or_else(|| catalogue::identity_conflict(path))?;
        let index_work = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                size_of::<AssetSourcePath>(),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(path.as_str())?,
            ],
        )?;
        let pending_work = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                size_of::<(AssetSourcePath, PendingSceneSource)>(),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(path.as_str())?,
                string_retained_bytes(alias)?,
            ],
        )?;
        let work = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [index_work, pending_work],
        )?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, work)?;
        self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
        sources.scene_index_work_bytes = sources
            .scene_index_work_bytes
            .checked_add(index_work)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let alias = self
            .catalogue_scope(reader)
            .relativize(path)
            .expect("the source binding was checked before admission");
        sources.seen_scenes.insert(path.clone());
        sources
            .scenes
            .insert(path.clone(), PendingSceneSource { reader, alias });
        Ok(())
    }

    pub(super) fn capture_scene_dependency(
        &mut self,
        directory: &DirectoryCapability,
        source_root: &nara_project::ProjectPath,
        settings: &nara_project::EffectiveProjectSettings,
        path: &AssetPath,
        loaded_prefabs: &mut BTreeMap<AssetSourcePath, LoadedPrefabContent>,
        prefab_cache_work: &mut usize,
    ) -> Result<ProjectSceneContent, ProjectContentError> {
        self.add_budget(ProjectContentBudgetKind::InFlightJobs, 1)?;
        let result = (|| {
            let (bytes, digest) = self.read_native_file(
                directory,
                path.as_str(),
                self.loader.limits.scene_file().encoded_bytes().get(),
            )?;
            self.record_member(
                ProjectContentMemberKind::Scene,
                source_root,
                path.as_str(),
                digest,
            )?;
            let source = self.admit_native_source(
                directory,
                source_root,
                path.as_str(),
                SourceDocumentKind::Scene,
                settings.project.native_document_identity,
                settings.project.source_universe,
            )?;
            let document = self.decode_scene(path.as_str(), &bytes)?;
            let compiled = self.compile_scene(
                &document,
                &source,
                settings,
                loaded_prefabs,
                prefab_cache_work,
            )?;
            let path_work = self.reserve_work(checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [
                    source_root.as_str().len(),
                    1,
                    path.as_str().len(),
                    STRING_ALLOCATION_OVERHEAD,
                ],
            )?)?;
            let canonical = self
                .scope_for_root(source_root)
                .expect("the source reader retains its actual scope")
                .resolve(path);
            self.commit_work_as_retained(path_work, path_work.reserved)?;
            Ok(ProjectSceneContent {
                path: canonical,
                source,
                document: Arc::new(document),
                compiled: Arc::new(compiled),
            })
        })();
        self.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
        result
    }
}

pub(super) fn pending_scene_source_work_bytes(
    canonical: &AssetSourcePath,
    alias: &AssetPath,
) -> Result<usize, ProjectContentError> {
    checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            size_of::<(AssetSourcePath, PendingSceneSource)>(),
            BTREE_ENTRY_OVERHEAD,
            string_retained_bytes(canonical.as_str())?,
            string_retained_bytes(alias.as_str())?,
        ],
    )
}
