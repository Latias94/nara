//! Finite authored Scene capture through the production Prefab compiler.

use super::*;

impl LoadContext<'_> {
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
    ) -> Result<ExpandedSceneCandidate, ProjectContentError> {
        let mut compiler_work_limit = self
            .remaining_expansion_work(0, self.loader.limits.prefab_expansion().working_bytes())?;
        let expansion_limits = self
            .loader
            .limits
            .prefab_expansion()
            .with_working_bytes(compiler_work_limit);
        let mut work = self.reserve_work(prefab_expansion_work_plan(expansion_limits)?)?;
        let options = nara_scene::PrefabExpansionOptions::default();
        let options = options.with_limits(expansion_limits).with_max_depth(
            options.max_depth.min(
                self.loader
                    .limits
                    .limit(ProjectContentBudgetKind::DirectoryDepth),
            ),
        );
        let validation = self.validation;
        let expansion = validation.bindings().try_expand_prefabs_with_loader(
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
        let expanded = expansion.candidate.ok_or_else(|| {
            map_scene_diagnostics(
                ProjectContentErrorKind::PrefabExpansion,
                expansion.diagnostics,
            )
        })?;
        let retained_data = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [
                size_of::<ExpandedSceneCandidate>(),
                scene_retained_bytes(expanded.document())?,
                crate::scene_retention::scene_provenance_retained_bytes(expanded.provenance())
                    .map_err(|_| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?,
            ],
        )?;
        self.resize_work(&mut work, retained_data)?;
        self.commit_work_as_retained(work, retained_data)?;
        // Containing identity belongs to the Host, independently of the reusable compiler cap.
        let identity_work = self.reserve_work(source.retained_bytes())?;
        let candidate = expanded.bind_source(source.clone());
        for (_, origin) in candidate.provenance().iter() {
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
            let expanded = self.compile_scene(
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
                expanded: Arc::new(expanded),
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
