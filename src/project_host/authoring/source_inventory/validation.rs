//! Reobserve a frozen source corpus without constructing another decoded/admitted corpus.

use super::*;

impl SourceInventory {
    /// Checks retained namespace/configuration facts between two admitted inventories.
    /// Participants, metadata and raw leaf observations belong to the concrete operation's
    /// transition checks; an old observed destination can now be an ordinary participant.
    pub(in crate::project_host::authoring) fn validate_namespace_continuity(
        &self,
        original: &Self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceInventoryError> {
        // Count only the facts compared below, without visiting source bodies or participant
        // paths. Debit the original cumulative allowance before any equality scan; refusal
        // retains this charge and neither inventory acquires storage or filesystem authority.
        budget.charge_source_bytes(
            0,
            0,
            sum(
                self.namespace_comparison_work()?,
                original.namespace_comparison_work()?,
            )?,
            0,
        )?;
        if self.root_identity != original.root_identity {
            return Err(SourceInventoryError::AuthorityChanged);
        }
        if self.selected_profile != original.selected_profile
            || self.configurations.len() != original.configurations.len()
            || !self
                .configurations
                .iter()
                .zip(&original.configurations)
                .all(|(current, original)| current.same_definition(original))
            || self.roots != original.roots
            || self.directories != original.directories
            || self.missing_sidecars != original.missing_sidecars
            || !self
                .custody
                .directories
                .iter()
                .map(|(path, directory)| (path, directory.identity()))
                .eq(original
                    .custody
                    .directories
                    .iter()
                    .map(|(path, directory)| (path, directory.identity())))
        {
            return Err(SourceInventoryError::MembershipChanged);
        }
        Ok(())
    }

    fn namespace_comparison_work(&self) -> Result<usize, SourceInventoryError> {
        let mut work = sum(
            size_of::<Self>(),
            self.selected_profile.as_deref().map_or(0, str::len),
        )?;
        for configuration in &self.configurations {
            work = sum(work, configuration.definition_comparison_work()?)?;
        }
        for (path, root) in &self.roots {
            work = sum(work, size_of::<String>() + size_of::<SourceRoot>())?;
            work = sum(work, sum(path.len(), root.path.len())?)?;
            if let SourceRootObservation::Missing { ancestor_path, .. } = &root.observation {
                work = sum(work, ancestor_path.len())?;
            }
        }
        for path in self
            .directories
            .keys()
            .chain(self.missing_sidecars.keys())
            .chain(self.custody.directories.keys())
        {
            work = sum(
                work,
                sum(path.len(), size_of::<String>() + size_of::<FileIdentity>())?,
            )?;
        }
        Ok(work)
    }

    pub(super) fn validate_observations(
        &self,
        schema: &AuthoringSchemaReceipt,
        complete_open_sources: &[OpenSource<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceInventoryError> {
        if self.selected_profile.as_deref() != schema.settings().profile_name.as_deref() {
            return Err(SourceInventoryError::MembershipChanged);
        }
        with_temporary_storage(budget, |budget| {
            let root = schema.project_root();
            for (path, retained, digest) in [
                (
                    super::super::PROJECT_MANIFEST_PATH,
                    schema.manifest.capability(),
                    schema.manifest_digest(),
                ),
                (
                    schema.bundle_path().as_str(),
                    schema.bundle.capability(),
                    schema.bundle_digest(),
                ),
            ] {
                revalidate_file_facts(
                    &root,
                    path,
                    retained.identity(),
                    retained.parent_identity(),
                    digest,
                    budget,
                )?;
            }
            let input = SourceInventoryInput {
                root: &root,
                manifest_bytes: schema.manifest_bytes(),
                manifest_identity: schema.manifest_identity(),
                manifest_digest: schema.manifest_digest(),
                bundle_path: schema.bundle_path().as_str(),
                bundle_identity: schema.bundle_identity(),
                bundle_digest: schema.bundle_digest(),
                selected_settings: Some(schema.settings()),
            };
            if schema.settings().authoring_state != ProjectAuthoringState::Ready {
                return Err(SourceInventoryError::SourceAuthoringPending);
            }
            self.validate_admitted_observations(input, complete_open_sources, budget)
        })
    }

    /// Reobserves an incumbent corpus under authority already admitted by its concrete owner.
    /// It grants no Ready/Pending permission, semantic transition or new source generation.
    /// Retained objects and equal bytes preserve their original native interpretation; only
    /// namespace/configuration scratch and finite current probes coexist with their custody.
    pub(in crate::project_host::authoring) fn validate_admitted_observations(
        &self,
        input: SourceInventoryInput<'_>,
        complete_open_sources: &[OpenSource<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceInventoryError> {
        self.validate_manifest_observation(
            input.root.identity(),
            input.manifest_identity,
            input.manifest_digest,
        )?;
        if self.bundle_identity != input.bundle_identity
            || self.bundle_digest != input.bundle_digest
        {
            return Err(SourceInventoryError::AuthorityChanged);
        }
        if self.selected_profile.as_deref()
            != input
                .selected_settings
                .and_then(|settings| settings.profile_name.as_deref())
        {
            return Err(SourceInventoryError::MembershipChanged);
        }
        with_temporary_storage(budget, |budget| {
            let root = input.root;
            let manifest = SourceInventoryManifest::parse(input.manifest_bytes, budget)?;
            let mut capture = InventoryCapture::new(input, &[], &[], budget)?;
            capture.resolve_configurations(manifest)?;
            capture.include_open_sources(complete_open_sources)?;
            capture
                .budget
                .charge_source_bytes(0, 0, self.comparison_work()?, 0)?;
            if capture.open_sources != self.open_sources || capture.roots != self.roots {
                return Err(SourceInventoryError::MembershipChanged);
            }
            // Known external members and metadata source spellings participate in the same
            // alias/case checks as newly enumerated entries. Their original custody stays live.
            for fact in self
                .participants
                .values()
                .map(|source| &source.file)
                .chain(self.metadata.values().map(|metadata| &metadata.file))
                .chain(self.file_observations.values().filter_map(
                    |observation| match observation {
                        SourceFileObservation::Present { file, .. } => Some(file),
                        SourceFileObservation::Missing { .. } => None,
                    },
                ))
            {
                let mut scanned = false;
                for observed in self.roots.values() {
                    capture.budget.charge_source_bytes(
                        0,
                        0,
                        sum(fact.path.len(), observed.path.len())?,
                        0,
                    )?;
                    scanned |= below(&fact.path, &observed.path).is_some();
                }
                if !scanned {
                    capture.observe_spelling(&fact.path)?;
                    capture.observe_identity(fact.identity, &fact.path)?;
                }
            }
            for metadata in self.metadata.values() {
                capture.observe_spelling(metadata.record.path().as_str())?;
            }
            for (path, observation) in &self.file_observations {
                if matches!(observation, SourceFileObservation::Missing { .. }) {
                    capture.observe_spelling(path)?;
                }
            }
            // Reuse the actual complete namespace walker, including overlap, truncation,
            // hard-link, spelling and native handle admission. No new file body is decoded.
            capture.enumerate()?;
            if capture.directories != self.directories {
                return Err(SourceInventoryError::MembershipChanged);
            }
            for fact in self
                .participants
                .values()
                .map(|source| &source.file)
                .chain(self.metadata.values().map(|metadata| &metadata.file))
                .chain(self.file_observations.values().filter_map(
                    |observation| match observation {
                        SourceFileObservation::Present { file, .. } => Some(file),
                        SourceFileObservation::Missing { .. } => None,
                    },
                ))
            {
                let mut scanned = false;
                for observed in self.roots.values() {
                    capture.budget.charge_source_bytes(
                        0,
                        0,
                        sum(fact.path.len(), observed.path.len())?,
                        0,
                    )?;
                    scanned |= below(&fact.path, &observed.path).is_some();
                }
                if scanned && !capture.entries.contains_key(&fact.path) {
                    return Err(SourceInventoryError::MembershipChanged);
                }
            }
            let lookups = (usize::BITS
                - self
                    .participants
                    .len()
                    .max(self.metadata.len())
                    .max(self.file_observations.len())
                    .leading_zeros()) as usize
                + 1;
            for (path, entry) in &capture.entries {
                capture.budget.charge_source_bytes(
                    0,
                    0,
                    product(path.len(), product(lookups, 16)?)?,
                    0,
                )?;
                if entry.kind != FileKind::Regular {
                    continue;
                }
                if let Some(observation) = self.file_observations.get(path) {
                    if !matches!(observation, SourceFileObservation::Present { file, .. }
                        if file.identity == entry.identity)
                    {
                        return Err(SourceInventoryError::MembershipChanged);
                    }
                    continue;
                }
                if let Some(source_path) = path.strip_suffix(".meta") {
                    if self
                        .metadata
                        .get(source_path)
                        .is_none_or(|metadata| metadata.file.identity != entry.identity)
                    {
                        return Err(SourceInventoryError::MembershipChanged);
                    }
                } else if let Some(participant) = self.participants.get(path) {
                    if participant.file.identity != entry.identity {
                        return Err(SourceInventoryError::MembershipChanged);
                    }
                } else if source_file_kind(
                    path,
                    &capture.roots,
                    self.metadata.get(path).map(|metadata| &metadata.record),
                    capture.budget,
                )?
                .is_some()
                {
                    return Err(SourceInventoryError::MembershipChanged);
                }
            }
            for (path, kind) in &capture.claims {
                capture.budget.charge_source_bytes(
                    0,
                    0,
                    product(path.len(), product(lookups, 16)?)?,
                    0,
                )?;
                if self
                    .participants
                    .get(path)
                    .is_none_or(|participant| participant.kind() != *kind)
                {
                    return Err(SourceInventoryError::MembershipChanged);
                }
            }
            // Equal bytes at the same retained objects preserve their admitted kind, canonical
            // identity, metadata and per-profile interpretation. Hash every original body once;
            // do not rerun its parser, native admission or source construction to prove equality.
            revalidate_source_observations(
                &root,
                &capture.roots,
                &self.custody,
                self.participants
                    .values()
                    .map(|source| &source.file)
                    .chain(self.metadata.values().map(|metadata| &metadata.file)),
                &self.missing_sidecars,
                &self.file_observations,
                capture.budget,
            )
        })
    }
}

pub(super) fn source_file_kind(
    path: &str,
    roots: &BTreeMap<String, SourceRoot>,
    record: Option<&AssetRecord>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Option<SourceDocumentKind>, SourceInventoryError> {
    let named = if path.ends_with(".scene.json") || path.ends_with(".scene.ron") {
        Some(SourceDocumentKind::Scene)
    } else if path.ends_with(".prefab.json") || path.ends_with(".prefab.ron") {
        Some(SourceDocumentKind::Prefab)
    } else {
        None
    };
    let encoded = path.ends_with(".json") || path.ends_with(".ron");
    let (mut scenes, mut prefabs) = (false, false);
    for root in roots.values() {
        budget.charge_source_bytes(0, 0, sum(path.len(), root.path.len())?, 0)?;
        if below(path, &root.path).is_some() {
            scenes |= root.roles.scenes;
            prefabs |= root.roles.prefabs;
        }
    }
    if let Some(record) = record {
        let kind = document_kind(record.source_kind());
        if (named.is_some() && named != kind) || (encoded && (scenes || prefabs) && kind.is_none())
        {
            return Err(SourceInventoryError::KindConflict);
        }
        return Ok(kind);
    }
    if named.is_some() {
        return Ok(named);
    }
    if !encoded {
        return Ok(None);
    }
    match (scenes, prefabs) {
        (true, false) => Ok(Some(SourceDocumentKind::Scene)),
        (false, true) => Ok(Some(SourceDocumentKind::Prefab)),
        (false, false) => Ok(None),
        (true, true) => Err(SourceInventoryError::KindConflict),
    }
}

pub(super) fn revalidate_source_observations<'a>(
    root: &DirectoryCapability,
    roots: &BTreeMap<String, SourceRoot>,
    custody: &InventoryCustody,
    facts: impl Iterator<Item = &'a SourceFileFact>,
    missing_sidecars: &BTreeMap<String, FileIdentity>,
    file_observations: &BTreeMap<String, SourceFileObservation>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceInventoryError> {
    for observed in roots.values() {
        if observe_root(root, &observed.path, budget)? != observed.observation {
            return Err(SourceInventoryError::SourceChanged);
        }
    }
    for (path, retained) in &custody.directories {
        let relative = relative_path(path, budget)?;
        budget.admit_handle_peak(HANDLE_PEAK)?;
        let current = open_directory_exact(root, &relative, budget)?;
        if !retained.same_object(&current)? {
            return Err(SourceInventoryError::SourceChanged);
        }
    }
    for fact in facts {
        revalidate_file(root, fact, budget)?;
    }
    for (path, observation) in file_observations {
        match observation {
            SourceFileObservation::Present { file, .. } => revalidate_file(root, file, budget)?,
            SourceFileObservation::Missing { parent_identity } => {
                // Every retained non-root directory was reopened above. Bind this absence to
                // that same continuously held parent and ancestor chain before probing its name.
                let parent = custody
                    .shared_directory(split_parent(path).0)
                    .ok_or(SourceInventoryError::SourceChanged)?;
                if parent.identity() != *parent_identity {
                    return Err(SourceInventoryError::SourceChanged);
                }
                let relative = relative_path(path, budget)?;
                budget.charge_open_file(HANDLE_PEAK)?;
                match open_file_exact(root, &relative, budget) {
                    Err(SourceInventoryError::Host(error)) if is_missing(&error) => {}
                    Ok(_) => return Err(SourceInventoryError::MembershipChanged),
                    Err(error) => return Err(error),
                }
            }
        }
    }
    for (path, parent) in missing_sidecars {
        let relative = relative_path(path, budget)?;
        match open_file_exact(root, &relative, budget) {
            Err(SourceInventoryError::Host(error)) if is_missing(&error) => {}
            Ok(_) => return Err(SourceInventoryError::MembershipChanged),
            Err(error) => return Err(error),
        }
        let source_path = path
            .strip_suffix(".meta")
            .ok_or(SourceInventoryError::KindConflict)?;
        let relative = relative_path(source_path, budget)?;
        let source = open_file_exact(root, &relative, budget)?;
        if source.parent_identity() != Some(*parent) {
            return Err(SourceInventoryError::SourceChanged);
        }
    }
    Ok(())
}
