//! Per-source read semantics. Only inherited working sources retain physical custody.

use super::*;

#[derive(Clone, Copy)]
pub(in crate::project_host::authoring::project) enum SourceCaptureMode<'a> {
    Content,
    Physical(&'a FileSourceObservation),
}

pub(in crate::project_host::authoring::project) struct SourceObservations {
    entries: BTreeMap<String, SourceObservation>,
}

enum SourceObservation {
    Original {
        source: Arc<AuthoringSourceFact>,
        observation: FileSourceObservation,
        native: AdmittedNativeDocumentSource,
    },
    ReadContent {
        source: Arc<AuthoringSourceFact>,
        native: Option<AdmittedNativeDocumentSource>,
    },
}

impl SourceObservation {
    fn source(&self) -> &AuthoringSourceFact {
        match self {
            Self::Original { source, .. } | Self::ReadContent { source, .. } => source,
        }
    }

    fn native(&self) -> Option<&AdmittedNativeDocumentSource> {
        match self {
            Self::Original { native, .. } => Some(native),
            Self::ReadContent { native, .. } => native.as_ref(),
        }
    }
}

impl SourceObservations {
    pub(super) fn new(
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, AuthoringProjectSourceAdmissionError> {
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([size_of::<Self>(), 2 * size_of::<usize>()])?,
        )?;
        Ok(Self {
            entries: BTreeMap::new(),
        })
    }

    pub(super) fn insert_original(
        &mut self,
        source: &Arc<AuthoringSourceFact>,
        native: &AdmittedNativeDocumentSource,
        observation: &FileSourceObservation,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        check_native_binding(source, Some(native))?;
        let path = source.project_path().as_str();
        if observation.observation()
            != (source.identity(), source.parent_identity(), source.digest())
        {
            return Err(changed(source));
        }
        if let Some(previous) = self.entries.get_mut(path) {
            if !previous.source().same_file_observation(source)
                || previous.native().map(AdmittedNativeDocumentSource::key) != Some(native.key())
            {
                return Err(changed(source));
            }
            match previous {
                SourceObservation::Original {
                    observation: held, ..
                } => {
                    if !held
                        .matches(observation.file())
                        .map_err(|error| host_error(source, error))?
                    {
                        return Err(changed(source));
                    }
                }
                SourceObservation::ReadContent {
                    source: held,
                    native,
                } => {
                    // Preserve the admitted key allocation and move the existing native payload.
                    // Upgrading custody needs neither a replacement path nor two native copies.
                    let native = native.take().ok_or_else(|| changed(source))?;
                    let held = Arc::clone(held);
                    *previous = SourceObservation::Original {
                        source: held,
                        observation: observation.clone(),
                        native,
                    };
                }
            }
            return Ok(());
        }
        self.charge_entry(source, Some(native), budget)?;
        self.entries.insert(
            path.to_owned(),
            SourceObservation::Original {
                source: Arc::clone(source),
                observation: observation.clone(),
                native: native.clone(),
            },
        );
        if let Some(metadata) = native.metadata_source() {
            self.insert_read_content(metadata, None, budget)?;
        }
        Ok(())
    }

    pub(super) fn insert_read_content(
        &mut self,
        source: &Arc<AuthoringSourceFact>,
        native: Option<&AdmittedNativeDocumentSource>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        check_native_binding(source, native)?;
        let path = source.project_path().as_str();
        if let Some(previous) = self.entries.get(path) {
            if !previous.source().same_file_observation(source)
                || (native.is_some()
                    && previous.native().map(AdmittedNativeDocumentSource::key)
                        != native.map(AdmittedNativeDocumentSource::key))
            {
                return Err(changed(source));
            }
            return Ok(());
        }
        self.charge_entry(source, native, budget)?;
        self.entries.insert(
            path.to_owned(),
            SourceObservation::ReadContent {
                source: Arc::clone(source),
                native: native.cloned(),
            },
        );
        if let Some(metadata) = native.and_then(AdmittedNativeDocumentSource::metadata_source) {
            self.insert_read_content(metadata, None, budget)?;
        }
        Ok(())
    }

    pub(super) fn check_original_binding(
        &self,
        source: &super::super::super::document::AuthoringDocumentSourceFact,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let mismatch = || AuthoringProjectSourceAdmissionError::DocumentSourceMismatch;
        let entry = self
            .entries
            .get(source.relative_path())
            .ok_or_else(mismatch)?;
        let fact = entry.source();
        if (fact.identity(), fact.parent_identity(), fact.digest())
            != (source.identity(), source.parent_identity(), source.digest())
            || entry.native().map(AdmittedNativeDocumentSource::key) != Some(source.source_key())
        {
            return Err(mismatch());
        }
        Ok(())
    }

    fn charge_entry(
        &self,
        source: &AuthoringSourceFact,
        native: Option<&AdmittedNativeDocumentSource>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([
                size_of::<(String, SourceObservation)>(),
                source.project_path().as_str().len(),
                64,
                native
                    .map_or(Some(0), AdmittedNativeDocumentSource::retained_bytes)
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
            ])?,
        )?;
        Ok(())
    }

    pub(in crate::project_host::authoring::project) fn physical_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| matches!(entry, SourceObservation::Original { .. }))
            .count()
    }

    pub(in crate::project_host::authoring::project) fn source_object_in_use(
        &self,
        candidate: &FileCapability,
    ) -> Result<bool, FsError> {
        for entry in self.entries.values() {
            if let SourceObservation::Original { observation, .. } = entry
                && candidate.same_object(observation.file())?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn validate(
        &self,
        schema: &AuthoringSchemaReceipt,
        source: &AuthoringSourceFact,
        original: Option<&FileSourceObservation>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        self.read_current(schema, source, original, budget)
            .map(drop)
    }

    fn read_current(
        &self,
        schema: &AuthoringSchemaReceipt,
        source: &AuthoringSourceFact,
        original: Option<&FileSourceObservation>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<FileCapability, AuthoringProjectSourceAdmissionError> {
        let entry = self
            .entries
            .get(source.project_path().as_str())
            .ok_or_else(|| changed(source))?;
        if !entry.source().same_file_observation(source) {
            return Err(changed(source));
        }
        let mode = match entry {
            SourceObservation::Original { observation, .. } => {
                SourceCaptureMode::Physical(observation)
            }
            SourceObservation::ReadContent { .. } => {
                original.map_or(SourceCaptureMode::Content, SourceCaptureMode::Physical)
            }
        };
        let current = revalidate_source(&schema.project_root, source, mode, budget)?;
        if let (SourceObservation::Original { .. }, Some(original)) = (entry, original)
            && (original.observation()
                != (source.identity(), source.parent_identity(), source.digest())
                || !original
                    .matches(&current)
                    .map_err(|error| host_error(source, error))?)
        {
            return Err(changed(source));
        }
        if let Some(expected) = entry.native() {
            NativeDocumentSourcePolicy::new(schema.settings()).revalidate_read_content(
                &schema.project_root,
                source.project_path().as_str(),
                &current,
                expected,
                source.dependency_depth(),
                budget,
            )?;
        }
        Ok(current)
    }

    pub(in crate::project_host::authoring::project) fn collect_reclamation_source(
        &self,
        schema: &AuthoringSchemaReceipt,
        source: &AuthoringSourceFact,
        original: Option<&FileSourceObservation>,
        inventory: Option<&SharedSourceUpgradeInventoryCustody>,
        budget: &mut AuthoringProjectAdmissionBudget,
        output: &mut Vec<FileCapability>,
    ) -> Result<(), FsError> {
        let unproven = || FsError::Unproven {
            operation: nara_fs::FsOperation::InspectHandle,
            proof: "the current authoring source exclusion could not be verified",
        };
        // These temporary live exclusions belong to the finite disposal check. They do not
        // extend ordinary capture with N persistent file handles or historical continuity.
        budget
            .charge_source_bytes(0, 0, 0, size_of::<FileCapability>())
            .map_err(|_| unproven())?;
        budget.retain_open_handles(1).map_err(|_| unproven())?;
        let current = self
            .read_current(schema, source, original, budget)
            .map_err(|error| match error {
                AuthoringProjectSourceAdmissionError::SourceHost { source, .. } => source,
                _ => unproven(),
            })?;
        if !source_observation_matches_custody(source, &current, inventory)? {
            return Err(unproven());
        }
        output.push(current);
        Ok(())
    }
}

pub(in crate::project_host::authoring::project) fn check_document_observation(
    source: &super::super::super::document::AuthoringDocumentSourceFact,
    observation: &FileSourceObservation,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    if observation.observation() != (source.identity(), source.parent_identity(), source.digest()) {
        return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
    }
    Ok(())
}

pub(super) fn capture_source_observation(
    budget: &mut AuthoringProjectAdmissionBudget,
    cost: FileSourceObservationCost,
    factory: impl FnOnce() -> Result<FileSourceObservation, AuthoringProjectSourceAdmissionError>,
) -> Result<FileSourceObservation, AuthoringProjectSourceAdmissionError> {
    let retained_before = budget.retained_candidate_bytes;
    let handles_before = budget.retained_open_handles;
    let result = (|| {
        // This owner-turn handoff checks live binding only. The accepted digest is historical;
        // the finite capture worker hashes current content before it publishes any result.
        budget.charge_source_bytes(0, 0, 1, cost.retained_bytes)?;
        budget.retain_open_handles(cost.retained_handle_capacity)?;
        budget.charge_open_file(SOURCE_OPEN_HANDLE_PEAK)?;
        factory()
    })();
    if result.is_err() {
        let temporary = budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        budget.reconcile_retained_reservation(temporary, 0)?;
        budget.release_open_handles(
            budget
                .retained_open_handles
                .checked_sub(handles_before)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        );
    }
    result
}

fn revalidate_source(
    root: &DirectoryCapability,
    source: &AuthoringSourceFact,
    mode: SourceCaptureMode<'_>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<FileCapability, AuthoringProjectSourceAdmissionError> {
    budget.admit_handle_peak(SOURCE_OPEN_HANDLE_PEAK)?;
    let preflight =
        RelativePath::preflight(source.project_path().as_str()).map_err(|_| changed(source))?;
    budget.charge_source_bytes(
        0,
        0,
        exact_open_work(preflight.components(), source.project_path().as_str().len()),
        0,
    )?;
    let file = root
        .open_file_exact(
            &RelativePath::new(source.project_path().as_str()).map_err(|_| changed(source))?,
        )
        .map_err(|error| host_error(source, error))?;
    if let SourceCaptureMode::Physical(observation) = mode {
        if observation.observation()
            != (source.identity(), source.parent_identity(), source.digest())
            || !observation
                .matches(&file)
                .map_err(|error| host_error(source, error))?
        {
            return Err(changed(source));
        }
    }
    let read_limit = source
        .encoded_bytes()
        .checked_add(1)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    budget.charge_source_bytes(0, 0, read_limit, read_limit)?;
    let result = (|| {
        let bytes = file
            .read_to_end_bounded(read_limit as u64)
            .map_err(|error| match error {
                FsError::ByteLimitExceeded { .. } => changed(source),
                error => host_error(source, error),
            })?;
        if bytes.len() != source.encoded_bytes()
            || ContentDigest::of_bytes(&bytes) != source.digest()
        {
            return Err(changed(source));
        }
        Ok(())
    })();
    budget.reconcile_retained_reservation(read_limit, 0)?;
    result?;
    Ok(file)
}

fn changed(source: &AuthoringSourceFact) -> AuthoringProjectSourceAdmissionError {
    AuthoringProjectSourceAdmissionError::SourceChanged {
        kind: source.kind(),
        path: source.project_path().clone(),
    }
}

fn check_native_binding(
    source: &AuthoringSourceFact,
    native: Option<&AdmittedNativeDocumentSource>,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let kind = match source.kind() {
        AuthoringSourceKind::Scene => nara_scene::SourceDocumentKind::Scene,
        AuthoringSourceKind::Prefab => nara_scene::SourceDocumentKind::Prefab,
        AuthoringSourceKind::ProjectManifest | AuthoringSourceKind::SchemaBundle => {
            return Err(changed(source));
        }
        AuthoringSourceKind::AssetSource
        | AuthoringSourceKind::AssetMetadata
        | AuthoringSourceKind::InputActionMap => {
            return if native.is_none() {
                Ok(())
            } else {
                Err(changed(source))
            };
        }
    };
    let native = native.ok_or_else(|| changed(source))?;
    if native.key().kind() != kind || native.source_identity() != source.identity() {
        return Err(changed(source));
    }
    match native.key().origin() {
        nara_scene::SourceDocumentOrigin::ProjectLocation { path, .. }
            if path.as_str() == source.project_path().as_str() =>
        {
            Ok(())
        }
        nara_scene::SourceDocumentOrigin::StableAsset { .. }
            if native.metadata_source().is_some_and(|metadata| {
                metadata.project_path().as_str().strip_suffix(".meta")
                    == Some(source.project_path().as_str())
            }) =>
        {
            Ok(())
        }
        _ => Err(changed(source)),
    }
}

fn host_error(
    source: &AuthoringSourceFact,
    error: FsError,
) -> AuthoringProjectSourceAdmissionError {
    AuthoringProjectSourceAdmissionError::SourceHost {
        kind: source.kind(),
        path: source.project_path().clone(),
        source: error,
    }
}
