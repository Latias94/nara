use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    sync::Arc,
};

use nara_fs::{
    DirectoryEnumerationLimits, DirectoryEnumerationSession, FileIdentity, FileKind, FsError,
    RelativePath,
};

use super::{
    AuthoringSourceIndexBasis, PROJECT_MANIFEST_PATH, ProjectSourceIndexDiagnostic,
    ProjectSourceIndexDiagnosticCode, ProjectSourceIndexDiagnosticPhase, ProjectSourceIndexEntry,
    ProjectSourceIndexFailure, ProjectSourceIndexGeneration, ProjectSourceIndexLimits,
    ProjectSourceIndexProgress, ProjectSourceIndexSnapshot, ProjectSourceInspectReason,
    ProjectSourceKind, ProjectSourceOpenCapability, ProjectSourceRootAvailability,
    ProjectSourceRootFact, ProjectSourceRootRole,
    budget::{SourceIndexBudget, SourceIndexBudgetError},
};

pub(super) enum SourceIndexAttemptProgress {
    Pending,
    Published(ProjectSourceIndexSnapshot),
    Failed(ProjectSourceIndexFailure),
}

pub(super) struct AuthoringSourceIndexAttempt {
    basis: AuthoringSourceIndexBasis,
    generation: ProjectSourceIndexGeneration,
    limits: ProjectSourceIndexLimits,
    budget: SourceIndexBudget,
    roots: Vec<ProjectSourceRootFact>,
    entries: BTreeMap<String, ProjectSourceIndexEntry>,
    canonical_paths: BTreeMap<String, String>,
    diagnostics: Vec<ProjectSourceIndexDiagnostic>,
    frontier: VecDeque<QueuedDirectory>,
    scheduled_directories: BTreeMap<String, FileIdentity>,
    directory_identity_paths: HashMap<FileIdentity, String>,
    required_roots: BTreeSet<String>,
    traversed_directories: BTreeMap<String, FileIdentity>,
    active: Option<ActiveDirectory>,
    phase: AttemptPhase,
}

struct QueuedDirectory {
    relative_path: Arc<str>,
    identity: FileIdentity,
    depth: usize,
}

struct ActiveDirectory {
    queued: QueuedDirectory,
    session: DirectoryEnumerationSession,
}

enum AttemptPhase {
    Enumerating,
    Revalidating(RevalidationState),
}

struct RevalidationState {
    authority_checked: bool,
    cursor: usize,
    lineage: Box<[(Arc<str>, FileIdentity)]>,
}

struct ResolvedRoot {
    relative_path: Arc<str>,
    identity: FileIdentity,
    depth: usize,
}

impl AuthoringSourceIndexAttempt {
    pub(super) fn begin(
        basis: AuthoringSourceIndexBasis,
        generation: ProjectSourceIndexGeneration,
        limits: ProjectSourceIndexLimits,
    ) -> Result<Self, ProjectSourceIndexFailure> {
        let mut attempt = Self {
            basis,
            generation,
            limits,
            budget: SourceIndexBudget::new(limits),
            roots: Vec::new(),
            entries: BTreeMap::new(),
            canonical_paths: BTreeMap::new(),
            diagnostics: Vec::new(),
            frontier: VecDeque::new(),
            scheduled_directories: BTreeMap::new(),
            directory_identity_paths: HashMap::new(),
            required_roots: BTreeSet::new(),
            traversed_directories: BTreeMap::new(),
            active: None,
            phase: AttemptPhase::Enumerating,
        };
        attempt.charge_retained(std::mem::size_of::<ProjectSourceIndexSnapshot>())?;
        attempt.record_entry(ProjectSourceIndexEntry::new(
            Arc::from(PROJECT_MANIFEST_PATH),
            ProjectSourceKind::ProjectManifest,
            Some(attempt.basis.manifest_identity),
            ProjectSourceOpenCapability::InspectOnly(ProjectSourceInspectReason::ProjectAuthority),
        ))?;
        attempt.record_entry(ProjectSourceIndexEntry::new(
            Arc::clone(&attempt.basis.bundle_path),
            ProjectSourceKind::AuthoringSchemaBundle,
            Some(attempt.basis.bundle_identity),
            ProjectSourceOpenCapability::InspectOnly(ProjectSourceInspectReason::ProjectAuthority),
        ))?;
        attempt.normalize_declared_roots()?;
        Ok(attempt)
    }

    pub(super) fn advance(&mut self) -> SourceIndexAttemptProgress {
        let result = if matches!(&self.phase, AttemptPhase::Revalidating(_)) {
            self.advance_revalidation()
        } else {
            self.advance_enumeration()
        };
        match result {
            Ok(Some(snapshot)) => SourceIndexAttemptProgress::Published(snapshot),
            Ok(None) => SourceIndexAttemptProgress::Pending,
            Err(failure) => SourceIndexAttemptProgress::Failed(failure),
        }
    }

    pub(super) const fn progress(&self) -> ProjectSourceIndexProgress {
        let phase = match &self.phase {
            AttemptPhase::Enumerating => ProjectSourceIndexDiagnosticPhase::Enumeration,
            AttemptPhase::Revalidating(_) => ProjectSourceIndexDiagnosticPhase::LineageRevalidation,
        };
        ProjectSourceIndexProgress::new(phase, self.budget.usage())
    }

    #[cfg(test)]
    pub(super) fn is_revalidating(&self) -> bool {
        matches!(&self.phase, AttemptPhase::Revalidating(_))
    }

    fn normalize_declared_roots(&mut self) -> Result<(), ProjectSourceIndexFailure> {
        let mut grouped = BTreeMap::<String, BTreeSet<ProjectSourceRootRole>>::new();
        let mut folded = BTreeMap::<String, String>::new();
        for (path, role) in &self.basis.declared_roots {
            let path = path.to_string();
            let canonical = canonical_key(&path);
            if let Some(existing) = folded.get(&canonical) {
                if existing != &path {
                    return Err(self.failure(
                        ProjectSourceIndexDiagnosticCode::CanonicalPathAmbiguous,
                        ProjectSourceIndexDiagnosticPhase::RootNormalization,
                        Some(Arc::from(path)),
                    ));
                }
            } else {
                folded.insert(canonical, path.clone());
            }
            grouped.entry(path).or_default().insert(*role);
        }

        let mut resolved = Vec::new();
        for (path, roles) in grouped {
            let path: Arc<str> = Arc::from(path);
            let roles = roles.into_iter().collect::<Vec<_>>().into_boxed_slice();
            let relative = match RelativePath::new(path.as_ref()) {
                Ok(relative) => relative,
                Err(_) => {
                    self.record_unavailable_root(
                        Arc::clone(&path),
                        roles,
                        ProjectSourceRootAvailability::Unavailable,
                        ProjectSourceIndexDiagnosticCode::DeclaredRootUnavailable,
                    )?;
                    continue;
                }
            };
            match self.basis.project_root.open_directory(&relative) {
                Ok(directory) => {
                    let identity = directory.identity();
                    if let Some(existing) = self.directory_identity_paths.get(&identity) {
                        if existing != path.as_ref() {
                            return Err(self.failure(
                                ProjectSourceIndexDiagnosticCode::IdentityAliasAmbiguous,
                                ProjectSourceIndexDiagnosticPhase::RootNormalization,
                                Some(Arc::clone(&path)),
                            ));
                        }
                    } else {
                        self.directory_identity_paths
                            .insert(identity, path.to_string());
                    }
                    let fact = ProjectSourceRootFact::new(
                        Arc::clone(&path),
                        roles,
                        ProjectSourceRootAvailability::Available,
                        Some(identity),
                    );
                    self.charge_retained(fact.retained_bytes())?;
                    self.roots.push(fact);
                    self.record_entry(ProjectSourceIndexEntry::new(
                        Arc::clone(&path),
                        ProjectSourceKind::Directory,
                        Some(identity),
                        ProjectSourceOpenCapability::None,
                    ))?;
                    self.required_roots.insert(path.to_string());
                    resolved.push(ResolvedRoot {
                        relative_path: path,
                        identity,
                        depth: relative.len(),
                    });
                }
                Err(error) => {
                    let (availability, code) = if is_not_found(&error) {
                        (
                            ProjectSourceRootAvailability::Missing,
                            ProjectSourceIndexDiagnosticCode::DeclaredRootMissing,
                        )
                    } else {
                        (
                            ProjectSourceRootAvailability::Unavailable,
                            ProjectSourceIndexDiagnosticCode::DeclaredRootUnavailable,
                        )
                    };
                    self.record_unavailable_root(Arc::clone(&path), roles, availability, code)?;
                }
            }
        }

        resolved.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        for (index, root) in resolved.iter().enumerate() {
            let covered = resolved[..index].iter().any(|ancestor| {
                is_same_or_descendant(&root.relative_path, &ancestor.relative_path)
            });
            if !covered {
                self.schedule_directory(
                    Arc::clone(&root.relative_path),
                    root.identity,
                    root.depth,
                )?;
            }
        }
        self.roots
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(())
    }

    fn record_unavailable_root(
        &mut self,
        path: Arc<str>,
        roles: Box<[ProjectSourceRootRole]>,
        availability: ProjectSourceRootAvailability,
        code: ProjectSourceIndexDiagnosticCode,
    ) -> Result<(), ProjectSourceIndexFailure> {
        let fact = ProjectSourceRootFact::new(Arc::clone(&path), roles, availability, None);
        self.charge_retained(fact.retained_bytes())?;
        self.roots.push(fact);
        let reason = match availability {
            ProjectSourceRootAvailability::Missing => {
                ProjectSourceInspectReason::MissingDeclaredRoot
            }
            ProjectSourceRootAvailability::Unavailable => {
                ProjectSourceInspectReason::UnavailableDeclaredRoot
            }
            ProjectSourceRootAvailability::Available => unreachable!(),
        };
        self.record_entry(ProjectSourceIndexEntry::new(
            Arc::clone(&path),
            ProjectSourceKind::Directory,
            None,
            ProjectSourceOpenCapability::InspectOnly(reason),
        ))?;
        self.record_diagnostic(
            code,
            ProjectSourceIndexDiagnosticPhase::RootNormalization,
            Some(path),
        )
    }

    fn advance_enumeration(
        &mut self,
    ) -> Result<Option<ProjectSourceIndexSnapshot>, ProjectSourceIndexFailure> {
        let mut entries_work = 0_usize;
        let mut directories_work = 0_usize;

        while entries_work < self.limits.max_entries_per_update
            && directories_work < self.limits.max_directories_per_update
        {
            if self.active.is_none() {
                let Some(queued) = self.frontier.pop_front() else {
                    self.ensure_required_roots_traversed()?;
                    let lineage = std::mem::take(&mut self.traversed_directories)
                        .into_iter()
                        .map(|(path, identity)| (Arc::from(path), identity))
                        .collect::<Vec<_>>()
                        .into_boxed_slice();
                    self.phase = AttemptPhase::Revalidating(RevalidationState {
                        authority_checked: false,
                        cursor: 0,
                        lineage,
                    });
                    return Ok(None);
                };
                directories_work += 1;
                if !self.begin_directory(queued)? {
                    continue;
                }
            }

            let allowance = self.limits.max_entries_per_update - entries_work;
            let active_path = Arc::clone(
                &self
                    .active
                    .as_ref()
                    .expect("active directory was installed")
                    .queued
                    .relative_path,
            );
            let batch = {
                let active = self
                    .active
                    .as_mut()
                    .expect("active directory was installed");
                active.session.next_batch(allowance)
            };
            let (entries, rejected, complete) = match batch {
                Ok(batch) => batch.into_parts(),
                Err(_) => {
                    return Err(self.failure(
                        ProjectSourceIndexDiagnosticCode::EnumerationIncomplete,
                        ProjectSourceIndexDiagnosticPhase::Enumeration,
                        Some(active_path),
                    ));
                }
            };
            let consumed = entries.len().checked_add(rejected.len()).ok_or_else(|| {
                self.failure(
                    ProjectSourceIndexDiagnosticCode::EntryBudgetExceeded,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    self.active
                        .as_ref()
                        .map(|active| Arc::clone(&active.queued.relative_path)),
                )
            })?;
            if consumed == 0 && !complete {
                return Err(self.failure(
                    ProjectSourceIndexDiagnosticCode::EnumerationIncomplete,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    self.active
                        .as_ref()
                        .map(|active| Arc::clone(&active.queued.relative_path)),
                ));
            }
            entries_work = entries_work.checked_add(consumed).ok_or_else(|| {
                self.failure(
                    ProjectSourceIndexDiagnosticCode::EntryBudgetExceeded,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    None,
                )
            })?;
            let parent = Arc::clone(
                &self
                    .active
                    .as_ref()
                    .expect("the batch has an active directory")
                    .queued
                    .relative_path,
            );
            for entry in entries {
                self.record_observation(&parent, entry)?;
            }
            for rejection in rejected {
                self.record_rejection(&parent, rejection.name().and_then(|name| name.to_str()))?;
            }
            if complete {
                self.active = None;
            }
        }
        Ok(None)
    }

    fn begin_directory(
        &mut self,
        queued: QueuedDirectory,
    ) -> Result<bool, ProjectSourceIndexFailure> {
        if let Err(error) = self.budget.charge_directory(queued.depth) {
            return Err(self.budget_failure(
                error,
                ProjectSourceIndexDiagnosticPhase::Enumeration,
                Some(Arc::clone(&queued.relative_path)),
            ));
        }
        let relative = RelativePath::new(queued.relative_path.as_ref()).map_err(|_| {
            self.failure(
                ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                ProjectSourceIndexDiagnosticPhase::Enumeration,
                Some(Arc::clone(&queued.relative_path)),
            )
        })?;
        let directory = self
            .basis
            .project_root
            .open_directory(&relative)
            .map_err(|_| {
                self.failure(
                    ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(Arc::clone(&queued.relative_path)),
                )
            })?;
        if directory.identity() != queued.identity {
            return Err(self.failure(
                ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                ProjectSourceIndexDiagnosticPhase::Enumeration,
                Some(Arc::clone(&queued.relative_path)),
            ));
        }
        let session = match directory.begin_child_enumeration(DirectoryEnumerationLimits::new(
            self.limits.max_entries,
            1_024,
            self.limits.max_diagnostics,
        )) {
            Ok(session) => session,
            Err(error) if isolated_enumeration_unavailable(&error) => {
                self.mark_directory_unavailable(&queued.relative_path)?;
                self.record_diagnostic(
                    ProjectSourceIndexDiagnosticCode::DirectoryUnavailable,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(Arc::clone(&queued.relative_path)),
                )?;
                return Ok(false);
            }
            Err(_) => {
                return Err(self.failure(
                    ProjectSourceIndexDiagnosticCode::EnumerationIncomplete,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(Arc::clone(&queued.relative_path)),
                ));
            }
        };
        self.traversed_directories
            .insert(queued.relative_path.to_string(), queued.identity);
        self.active = Some(ActiveDirectory { queued, session });
        Ok(true)
    }

    fn mark_directory_unavailable(
        &mut self,
        relative_path: &Arc<str>,
    ) -> Result<(), ProjectSourceIndexFailure> {
        let declared_root = if let Some(root) = self
            .roots
            .iter_mut()
            .find(|root| root.relative_path.as_ref() == relative_path.as_ref())
        {
            root.availability = ProjectSourceRootAvailability::Unavailable;
            self.required_roots.remove(relative_path.as_ref());
            true
        } else {
            false
        };
        if let Some(entry) = self.entries.get_mut(relative_path.as_ref()) {
            entry.open_capability = ProjectSourceOpenCapability::InspectOnly(if declared_root {
                ProjectSourceInspectReason::UnavailableDeclaredRoot
            } else {
                ProjectSourceInspectReason::UnsupportedEntry
            });
        }
        Ok(())
    }

    fn record_observation(
        &mut self,
        parent: &str,
        observation: nara_fs::DirectoryEntryObservation,
    ) -> Result<(), ProjectSourceIndexFailure> {
        let Some(name) = observation.name().to_str() else {
            self.charge_observed_entry(0, Some(Arc::from(parent)))?;
            return self.record_diagnostic(
                ProjectSourceIndexDiagnosticCode::EntryUnsupported,
                ProjectSourceIndexDiagnosticPhase::Enumeration,
                Some(Arc::from(parent)),
            );
        };
        let path = join_path(parent, name);
        self.charge_observed_entry(path.len(), Some(Arc::from(path.as_str())))?;
        let identity = observation.identity();
        let kind = self.classify_source(&path, observation.kind());
        let open_capability = open_capability(kind, identity);
        self.insert_entry(ProjectSourceIndexEntry::new(
            Arc::from(path.as_str()),
            kind,
            identity,
            open_capability,
        ))?;

        if observation.kind() == FileKind::Directory {
            let Some(identity) = identity else {
                return self.record_diagnostic(
                    ProjectSourceIndexDiagnosticCode::DirectoryUnavailable,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(Arc::from(path)),
                );
            };
            let relative = RelativePath::new(&path).map_err(|_| {
                self.failure(
                    ProjectSourceIndexDiagnosticCode::CanonicalPathAmbiguous,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(Arc::from(path.as_str())),
                )
            })?;
            self.schedule_directory(Arc::from(path), identity, relative.len())?;
        }
        Ok(())
    }

    fn record_rejection(
        &mut self,
        parent: &str,
        name: Option<&str>,
    ) -> Result<(), ProjectSourceIndexFailure> {
        let path = name.map(|name| join_path(parent, name));
        self.charge_observed_entry(
            path.as_deref().map_or(0, str::len),
            path.as_deref().map(Arc::from),
        )?;
        if let Some(path) = &path {
            self.insert_entry(ProjectSourceIndexEntry::new(
                Arc::from(path.as_str()),
                ProjectSourceKind::Unsupported,
                None,
                ProjectSourceOpenCapability::InspectOnly(
                    ProjectSourceInspectReason::UnsupportedEntry,
                ),
            ))?;
        }
        self.record_diagnostic(
            ProjectSourceIndexDiagnosticCode::EntryUnsupported,
            ProjectSourceIndexDiagnosticPhase::Enumeration,
            path.map(Arc::from).or_else(|| Some(Arc::from(parent))),
        )
    }

    fn schedule_directory(
        &mut self,
        relative_path: Arc<str>,
        identity: FileIdentity,
        depth: usize,
    ) -> Result<(), ProjectSourceIndexFailure> {
        if depth > self.limits.max_depth {
            return Err(self.failure(
                ProjectSourceIndexDiagnosticCode::DepthBudgetExceeded,
                ProjectSourceIndexDiagnosticPhase::Enumeration,
                Some(relative_path),
            ));
        }
        if let Some(existing) = self.scheduled_directories.get(relative_path.as_ref()) {
            if *existing != identity {
                return Err(self.failure(
                    ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(relative_path),
                ));
            }
            return Ok(());
        }
        if let Some(existing_path) = self.directory_identity_paths.get(&identity) {
            if existing_path != relative_path.as_ref() {
                return Err(self.failure(
                    ProjectSourceIndexDiagnosticCode::IdentityAliasAmbiguous,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(relative_path),
                ));
            }
        } else {
            self.directory_identity_paths
                .insert(identity, relative_path.to_string());
        }
        self.scheduled_directories
            .insert(relative_path.to_string(), identity);
        self.frontier.push_back(QueuedDirectory {
            relative_path,
            identity,
            depth,
        });
        Ok(())
    }

    fn ensure_required_roots_traversed(&self) -> Result<(), ProjectSourceIndexFailure> {
        if let Some(path) = self
            .required_roots
            .iter()
            .find(|path| !self.traversed_directories.contains_key(path.as_str()))
        {
            return Err(self.failure(
                ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                ProjectSourceIndexDiagnosticPhase::Enumeration,
                Some(Arc::from(path.as_str())),
            ));
        }
        Ok(())
    }

    fn advance_revalidation(
        &mut self,
    ) -> Result<Option<ProjectSourceIndexSnapshot>, ProjectSourceIndexFailure> {
        let (authority_checked, cursor, lineage_len) = match &self.phase {
            AttemptPhase::Revalidating(state) => {
                (state.authority_checked, state.cursor, state.lineage.len())
            }
            AttemptPhase::Enumerating => unreachable!(),
        };
        if !authority_checked {
            self.revalidate_authority()?;
            let AttemptPhase::Revalidating(state) = &mut self.phase else {
                unreachable!();
            };
            state.authority_checked = true;
            return Ok(None);
        }

        let mut next = cursor;
        let end = lineage_len.min(cursor.saturating_add(self.limits.max_directories_per_update));
        while next < end {
            let (path, expected) = match &self.phase {
                AttemptPhase::Revalidating(state) => {
                    let (path, expected) = &state.lineage[next];
                    (Arc::clone(path), *expected)
                }
                AttemptPhase::Enumerating => unreachable!(),
            };
            let relative = RelativePath::new(path.as_ref()).map_err(|_| {
                self.failure(
                    ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                    ProjectSourceIndexDiagnosticPhase::LineageRevalidation,
                    Some(Arc::clone(&path)),
                )
            })?;
            let directory = self
                .basis
                .project_root
                .open_directory(&relative)
                .map_err(|_| {
                    self.failure(
                        ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                        ProjectSourceIndexDiagnosticPhase::LineageRevalidation,
                        Some(Arc::clone(&path)),
                    )
                })?;
            if directory.identity() != expected {
                return Err(self.failure(
                    ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                    ProjectSourceIndexDiagnosticPhase::LineageRevalidation,
                    Some(path),
                ));
            }
            next += 1;
        }

        if next < lineage_len {
            let AttemptPhase::Revalidating(state) = &mut self.phase else {
                unreachable!();
            };
            state.cursor = next;
            return Ok(None);
        }

        let roots = std::mem::take(&mut self.roots).into_boxed_slice();
        let entries = std::mem::take(&mut self.entries)
            .into_values()
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let diagnostics = std::mem::take(&mut self.diagnostics).into_boxed_slice();
        let snapshot = ProjectSourceIndexSnapshot::new(
            &self.basis,
            self.generation,
            roots,
            entries,
            diagnostics,
            self.budget.usage(),
        );
        Ok(Some(snapshot))
    }

    fn revalidate_authority(&self) -> Result<(), ProjectSourceIndexFailure> {
        self.revalidate_file(
            PROJECT_MANIFEST_PATH,
            self.basis.manifest_identity,
            self.basis.manifest_digest,
            ProjectSourceIndexDiagnosticCode::ManifestLineageChanged,
        )?;
        self.revalidate_file(
            &self.basis.bundle_path,
            self.basis.bundle_identity,
            self.basis.bundle_digest,
            ProjectSourceIndexDiagnosticCode::SchemaBundleLineageChanged,
        )
    }

    fn revalidate_file(
        &self,
        path: &str,
        expected_identity: FileIdentity,
        expected_digest: nara_fs::ContentDigest,
        code: ProjectSourceIndexDiagnosticCode,
    ) -> Result<(), ProjectSourceIndexFailure> {
        let relative = RelativePath::new(path).map_err(|_| {
            self.failure(
                code,
                ProjectSourceIndexDiagnosticPhase::LineageRevalidation,
                Some(Arc::from(path)),
            )
        })?;
        let file = self.basis.project_root.open_file(&relative).map_err(|_| {
            self.failure(
                code,
                ProjectSourceIndexDiagnosticPhase::LineageRevalidation,
                Some(Arc::from(path)),
            )
        })?;
        if file.identity() != expected_identity
            || file
                .verify_digest(expected_digest, expected_digest.length())
                .is_err()
        {
            return Err(self.failure(
                code,
                ProjectSourceIndexDiagnosticPhase::LineageRevalidation,
                Some(Arc::from(path)),
            ));
        }
        Ok(())
    }

    fn record_entry(
        &mut self,
        entry: ProjectSourceIndexEntry,
    ) -> Result<(), ProjectSourceIndexFailure> {
        self.charge_observed_entry(
            entry.relative_path().len(),
            Some(Arc::clone(&entry.relative_path)),
        )?;
        self.insert_entry(entry)
    }

    fn insert_entry(
        &mut self,
        entry: ProjectSourceIndexEntry,
    ) -> Result<(), ProjectSourceIndexFailure> {
        let path = entry.relative_path().to_owned();
        let canonical = canonical_key(&path);
        if let Some(existing) = self.canonical_paths.get(&canonical) {
            if existing != &path {
                return Err(self.failure(
                    ProjectSourceIndexDiagnosticCode::CanonicalPathAmbiguous,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(Arc::from(path)),
                ));
            }
        } else {
            self.canonical_paths.insert(canonical, path.clone());
        }
        if let Some(existing) = self.entries.get(&path) {
            if existing.kind() != entry.kind()
                || existing.scoped_identity() != entry.scoped_identity()
            {
                return Err(self.failure(
                    ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged,
                    ProjectSourceIndexDiagnosticPhase::Enumeration,
                    Some(Arc::from(path)),
                ));
            }
            return Ok(());
        }
        self.charge_retained(entry.retained_bytes())?;
        self.entries.insert(path, entry);
        Ok(())
    }

    fn charge_observed_entry(
        &mut self,
        path_bytes: usize,
        relative_source: Option<Arc<str>>,
    ) -> Result<(), ProjectSourceIndexFailure> {
        if let Err(error) = self.budget.charge_entry(path_bytes) {
            return Err(self.budget_failure(
                error,
                ProjectSourceIndexDiagnosticPhase::Enumeration,
                relative_source,
            ));
        }
        Ok(())
    }

    fn charge_retained(&mut self, bytes: usize) -> Result<(), ProjectSourceIndexFailure> {
        if let Err(error) = self.budget.charge_retained(bytes) {
            return Err(self.budget_failure(
                error,
                ProjectSourceIndexDiagnosticPhase::Publication,
                None,
            ));
        }
        Ok(())
    }

    fn record_diagnostic(
        &mut self,
        code: ProjectSourceIndexDiagnosticCode,
        phase: ProjectSourceIndexDiagnosticPhase,
        relative_source: Option<Arc<str>>,
    ) -> Result<(), ProjectSourceIndexFailure> {
        let diagnostic = ProjectSourceIndexDiagnostic::new(code, phase, relative_source);
        if let Err(error) = self.budget.charge_diagnostic(&diagnostic) {
            return Err(self.budget_failure(error, phase, None));
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn budget_failure(
        &self,
        error: SourceIndexBudgetError,
        phase: ProjectSourceIndexDiagnosticPhase,
        relative_source: Option<Arc<str>>,
    ) -> ProjectSourceIndexFailure {
        self.failure(error.code, phase, relative_source)
    }

    fn failure(
        &self,
        code: ProjectSourceIndexDiagnosticCode,
        phase: ProjectSourceIndexDiagnosticPhase,
        relative_source: Option<Arc<str>>,
    ) -> ProjectSourceIndexFailure {
        ProjectSourceIndexFailure::new(code, phase, relative_source, self.budget.usage())
    }

    fn classify_source(&self, path: &str, file_kind: FileKind) -> ProjectSourceKind {
        if path == PROJECT_MANIFEST_PATH {
            return ProjectSourceKind::ProjectManifest;
        }
        if path == self.basis.bundle_path.as_ref() {
            return ProjectSourceKind::AuthoringSchemaBundle;
        }
        if file_kind == FileKind::Directory {
            return ProjectSourceKind::Directory;
        }
        if file_kind != FileKind::Regular {
            return ProjectSourceKind::Unsupported;
        }

        let roles = self.roles_for(path);
        let scene_role = roles.contains(&ProjectSourceRootRole::Scenes);
        let prefab_role = roles.contains(&ProjectSourceRootRole::Prefabs);
        if path.ends_with(".scene.json") || path.ends_with(".scene.ron") {
            return ProjectSourceKind::Scene;
        }
        if path.ends_with(".prefab.json") || path.ends_with(".prefab.ron") {
            return ProjectSourceKind::Prefab;
        }
        if scene_role && !prefab_role && (path.ends_with(".json") || path.ends_with(".ron")) {
            return ProjectSourceKind::Scene;
        }
        if prefab_role && !scene_role && (path.ends_with(".json") || path.ends_with(".ron")) {
            return ProjectSourceKind::Prefab;
        }
        if roles.contains(&ProjectSourceRootRole::Assets) {
            return if path.ends_with(".meta") {
                ProjectSourceKind::AssetMeta
            } else {
                ProjectSourceKind::AssetSource
            };
        }
        if roles.contains(&ProjectSourceRootRole::Scripts) {
            return ProjectSourceKind::Script;
        }
        ProjectSourceKind::Unknown
    }

    fn roles_for(&self, path: &str) -> BTreeSet<ProjectSourceRootRole> {
        self.roots
            .iter()
            .filter(|root| is_same_or_descendant(path, root.relative_path()))
            .flat_map(|root| root.roles().iter().copied())
            .collect()
    }
}

fn open_capability(
    kind: ProjectSourceKind,
    identity: Option<FileIdentity>,
) -> ProjectSourceOpenCapability {
    match (kind, identity) {
        (ProjectSourceKind::Scene, Some(_)) => ProjectSourceOpenCapability::Scene,
        (ProjectSourceKind::Prefab, Some(_)) => ProjectSourceOpenCapability::Prefab,
        (ProjectSourceKind::Scene | ProjectSourceKind::Prefab, None) => {
            ProjectSourceOpenCapability::InspectOnly(
                ProjectSourceInspectReason::IdentityUnavailable,
            )
        }
        (ProjectSourceKind::ProjectManifest | ProjectSourceKind::AuthoringSchemaBundle, _) => {
            ProjectSourceOpenCapability::InspectOnly(ProjectSourceInspectReason::ProjectAuthority)
        }
        (
            ProjectSourceKind::AssetSource
            | ProjectSourceKind::AssetMeta
            | ProjectSourceKind::Script,
            _,
        ) => ProjectSourceOpenCapability::InspectOnly(ProjectSourceInspectReason::ReadOnlySource),
        (ProjectSourceKind::Directory, _) => ProjectSourceOpenCapability::None,
        (ProjectSourceKind::Unknown, _) => {
            ProjectSourceOpenCapability::InspectOnly(ProjectSourceInspectReason::UnknownSourceKind)
        }
        (ProjectSourceKind::Unsupported, _) => {
            ProjectSourceOpenCapability::InspectOnly(ProjectSourceInspectReason::UnsupportedEntry)
        }
    }
}

fn canonical_key(path: &str) -> String {
    path.to_lowercase()
}

fn join_path(parent: &str, child: &str) -> String {
    let mut path = String::with_capacity(parent.len() + child.len() + 1);
    path.push_str(parent);
    path.push('/');
    path.push_str(child);
    path
}

fn is_same_or_descendant(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn is_not_found(error: &FsError) -> bool {
    matches!(
        error,
        FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound
    )
}

fn isolated_enumeration_unavailable(error: &FsError) -> bool {
    matches!(
        error,
        FsError::Unsupported { .. }
            | FsError::Unproven { .. }
            | FsError::ReparsePoint { .. }
            | FsError::SymbolicLinkTraversal
            | FsError::CrossVolume
            | FsError::MultipleLinks { .. }
            | FsError::IdentityUnavailable
    )
}
