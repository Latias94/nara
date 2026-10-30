//! Actual-demand source acquisition shared by independent authoring documents.

use super::*;

mod input;
pub(super) mod prefabs;
pub(super) mod scenes;
#[cfg(any(feature = "tooling", test))]
mod schema_owner;
pub(crate) use prefabs::AuthoringPrefabSourceCapture;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

#[cfg(all(test, any(windows, target_os = "linux")))]
#[path = "capture/tests/input.rs"]
mod input_tests;

/// An immutable working copy paired with its last accepted file receipt.
///
/// Only a demanded source activates its Schema and file checks. Holding this snapshot does
/// not turn unsaved source into a file-backed publication.
#[derive(Clone)]
pub(crate) struct AuthoringPrefabSessionSnapshot {
    source: Option<super::super::document::AuthoringDocumentSourceFact>,
    source_key: nara_scene::SourceDocumentKey,
    project_path: Arc<str>,
    document: Arc<PrefabDocument>,
    basis: Option<AuthoringSchemaBasis>,
    source_matches: bool,
    persistence_available: bool,
    revision: nara_scene::SceneAuthoringRevision,
}

#[derive(Clone)]
pub(crate) struct AuthoringSceneSessionSnapshot {
    source: Option<super::super::document::AuthoringDocumentSourceFact>,
    source_key: nara_scene::SourceDocumentKey,
    project_path: Arc<str>,
    document: Arc<SceneDocument>,
    basis: Option<AuthoringSchemaBasis>,
    source_matches: bool,
    persistence_available: bool,
    revision: nara_scene::SceneAuthoringRevision,
}

impl AuthoringSceneSessionSnapshot {
    /// Measures the complete input before copying its source, key, path or document handle.
    pub(crate) fn capture_retained_bytes(
        source: &super::super::document::AuthoringDocumentSourceFact,
        session: &nara_scene::SceneAuthoringSession,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        Self::input_retained_bytes(
            Some(source),
            source.source_key(),
            source.relative_path(),
            session.document(),
        )
    }

    pub(crate) fn unavailable_retained_bytes(
        source_key: &nara_scene::SourceDocumentKey,
        project_path: &str,
        session: &nara_scene::SceneAuthoringSession,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        Self::input_retained_bytes(None, source_key, project_path, session.document())
    }

    pub(crate) fn prepared_retained_bytes(
        source: &super::super::document::AuthoringDocumentSourceFact,
        view: nara_scene::__private::PreparedSceneSchemaSource<'_>,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        Self::input_retained_bytes(
            Some(source),
            source.source_key(),
            source.relative_path(),
            view.document(),
        )
    }

    fn input_retained_bytes(
        source: Option<&super::super::document::AuthoringDocumentSourceFact>,
        source_key: &nara_scene::SourceDocumentKey,
        project_path: &str,
        document: &SceneDocument,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        checked_sum([
            size_of::<Self>(),
            document.retained_bytes()?,
            project_path.len(),
            source_key.retained_bytes(),
            source.map_or(Ok(0), |source| {
                source
                    .checked_retained_bytes()
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
            })?,
        ])
    }

    pub(crate) fn capture(
        source: &super::super::document::AuthoringDocumentSourceFact,
        session: &nara_scene::SceneAuthoringSession,
    ) -> Self {
        Self {
            source: Some(source.clone()),
            source_key: source.source_key().clone(),
            project_path: Arc::from(source.relative_path()),
            document: session.shared_document(),
            basis: session.authoring_schema_basis(),
            source_matches: source_locator_matches(
                session.authoring_source(),
                source.relative_path(),
            ),
            persistence_available: true,
            revision: session.revision(),
        }
    }

    pub(crate) fn capture_prepared(
        source: &super::super::document::AuthoringDocumentSourceFact,
        view: nara_scene::__private::PreparedSceneSchemaSource<'_>,
    ) -> Result<Self, AuthoringProjectSourceAdmissionError> {
        if source.source_key().kind() != nara_scene::SourceDocumentKind::Scene
            || !source_locator_matches(Some(view.source()), source.relative_path())
        {
            return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
        }
        Ok(Self {
            source: Some(source.clone()),
            source_key: source.source_key().clone(),
            project_path: Arc::from(source.relative_path()),
            document: view.shared_document(),
            basis: Some(view.schema_basis()),
            source_matches: true,
            persistence_available: true,
            revision: view.base_revision(),
        })
    }

    pub(crate) fn without_persistence(mut self) -> Self {
        self.persistence_available = false;
        self
    }

    pub(crate) fn unavailable(
        source_key: nara_scene::SourceDocumentKey,
        project_path: Arc<str>,
        session: &nara_scene::SceneAuthoringSession,
    ) -> Self {
        Self {
            source: None,
            source_key,
            project_path,
            document: session.shared_document(),
            basis: session.authoring_schema_basis(),
            source_matches: false,
            persistence_available: false,
            revision: session.revision(),
        }
    }
}

/// Immutable data custody. Dependencies retain the complete production compiler result;
/// the selected root separately retains effective data before native presentation filtering.
pub(crate) struct AuthoringSceneSource {
    native_source: AdmittedNativeDocumentSource,
    source: Arc<AuthoringSourceFact>,
    document: Arc<SceneDocument>,
    compiled: Option<nara_scene::ExpandedSceneCandidate>,
    root_effective: Option<Arc<SceneDocument>>,
    revision: Option<nara_scene::SceneAuthoringRevision>,
    diagnostics: DiagnosticReport,
    failure: Option<nara_scene::PrefabExpansionFailure>,
}

impl AuthoringSceneSource {
    pub(crate) fn native_source(&self) -> &AdmittedNativeDocumentSource {
        &self.native_source
    }
    pub(crate) fn source(&self) -> &AuthoringSourceFact {
        &self.source
    }
    pub(crate) fn document(&self) -> &SceneDocument {
        &self.document
    }
    pub(crate) fn effective_document(&self) -> Option<&SceneDocument> {
        self.compiled
            .as_ref()
            .map(|candidate| candidate.document())
            .or(self.root_effective.as_deref())
    }
    pub(crate) fn session_revision(&self) -> Option<nara_scene::SceneAuthoringRevision> {
        self.revision
    }
    pub(crate) fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
}

pub(crate) struct OwnedAuthoringSceneCapture {
    pub(super) schema: Arc<AuthoringSchemaReceipt>,
    pub(super) scene_limits: SceneFileLimits,
    pub(super) source: super::super::document::AuthoringDocumentSourceFact,
    pub(super) document: Arc<SceneDocument>,
    pub(super) revision: nara_scene::SceneAuthoringRevision,
    pub(super) basis: AuthoringSchemaBasis,
    pub(super) readiness: AuthoringProjectReadiness,
    pub(super) adapter_retained: usize,
    pub(super) open_prefabs: Box<[AuthoringPrefabSessionSnapshot]>,
    pub(super) open_scenes: Box<[AuthoringSceneSessionSnapshot]>,
    pub(super) inputs: AuthoringSourceCaptureInputs,
}

/// One finite, nominally bound allowance admitted before the Host copies capture inputs.
/// It is consumed by one Scene capture or Prefab conversion request.
pub(crate) struct AuthoringSourceCaptureInputs {
    pub(super) basis: AuthoringSchemaBasis,
    pub(super) host_retained_bytes: usize,
    pub(super) worker_retained_bytes: usize,
}

impl AuthoringSourceCaptureInputs {
    fn charge_inputs(
        &self,
        actual_open_inputs: usize,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        self.worker_retained_bytes
            .checked_sub(actual_open_inputs)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let retained_bytes = checked_sum([self.host_retained_bytes, self.worker_retained_bytes])?;
        checked_retained_replacement(
            budget.retained_candidate_bytes,
            0,
            retained_bytes,
            budget.limits.retained_candidate_bytes().get(),
        )?;
        budget.charge_source_bytes(0, 0, 0, retained_bytes)?;
        Ok(())
    }
}

impl OwnedAuthoringSceneCapture {
    pub(crate) fn with_open_scenes(mut self, scenes: Box<[AuthoringSceneSessionSnapshot]>) -> Self {
        self.open_scenes = scenes;
        self
    }

    pub(crate) fn prepare(
        self,
    ) -> Result<AuthoringDocumentCapture, AuthoringProjectSourceAdmissionError> {
        let mut budget = self.schema.budget;
        let actual_open_inputs = checked_sum([
            prefabs::open_prefab_inputs_retained_bytes(&self.open_prefabs)?,
            scenes::open_scene_inputs_retained_bytes(&self.open_scenes)?,
        ])?;
        self.inputs.charge_inputs(actual_open_inputs, &mut budget)?;
        let (input_retained, containing) = prefabs::charge_capture_inputs(
            &self.source,
            self.document.retained_bytes()?,
            self.adapter_retained,
            &[],
            &mut budget,
        )?;
        let mut capture = capture_scene(
            &self.schema,
            self.scene_limits,
            &self.document,
            &self.basis,
            &containing,
            self.source.native_source(),
            self.readiness,
            &mut budget,
            &self.open_prefabs,
            &self.open_scenes,
            AuthoringCapturePreview::Prepare,
            None,
        )?;
        // Selected documents transfer into composition ownership during loading. The other
        // working copies remain request inputs until preparation physically returns. Host
        // observations remain charged because their custody outlives this worker request.
        let selected_retained = capture
            .prefabs
            .iter()
            .filter(|prefab| prefab.session_revision().is_some())
            .try_fold(0_usize, |bytes, prefab| {
                checked_sum([bytes, prefab.document().retained_bytes()?])
            })?;
        let selected_scene_retained = capture
            .scenes
            .iter()
            .filter(|scene| scene.session_revision().is_some())
            .try_fold(0, |bytes, scene| {
                checked_sum([bytes, scene.document.retained_bytes()?])
            })?;
        let finished_worker_inputs = input_retained
            .checked_add(self.inputs.worker_retained_bytes)
            .and_then(|bytes| bytes.checked_sub(selected_retained))
            .and_then(|bytes| bytes.checked_sub(selected_scene_retained))
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        drop(self.document);
        drop(self.open_prefabs);
        drop(self.open_scenes);
        drop(self.source);
        budget.reconcile_retained_reservation(finished_worker_inputs, 0)?;
        capture.usage = budget.usage();
        capture.host_retained_bytes = self.inputs.host_retained_bytes;
        capture.root_revision = Some(self.revision);
        Ok(capture)
    }
}

impl AuthoringPrefabSessionSnapshot {
    pub(crate) fn capture_retained_bytes(
        source: &super::super::document::AuthoringDocumentSourceFact,
        session: &nara_scene::PrefabAuthoringSession,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        Self::input_retained_bytes(
            Some(source),
            source.source_key(),
            source.relative_path(),
            session.document(),
        )
    }

    pub(crate) fn unavailable_retained_bytes(
        source_key: &nara_scene::SourceDocumentKey,
        project_path: &str,
        session: &nara_scene::PrefabAuthoringSession,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        Self::input_retained_bytes(None, source_key, project_path, session.document())
    }

    pub(crate) fn prepared_retained_bytes(
        source: &super::super::document::AuthoringDocumentSourceFact,
        view: nara_scene::__private::PreparedPrefabSchemaSource<'_>,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        Self::input_retained_bytes(
            Some(source),
            source.source_key(),
            source.relative_path(),
            view.document(),
        )
    }

    fn input_retained_bytes(
        source: Option<&super::super::document::AuthoringDocumentSourceFact>,
        source_key: &nara_scene::SourceDocumentKey,
        project_path: &str,
        document: &PrefabDocument,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        checked_sum([
            size_of::<Self>(),
            document.retained_bytes()?,
            project_path.len(),
            source_key.retained_bytes(),
            source.map_or(Ok(0), |source| {
                source
                    .checked_retained_bytes()
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
            })?,
        ])
    }

    /// Retains an actual working source under its unpublished classification successor.
    pub(crate) fn capture_prepared(
        source: &super::super::document::AuthoringDocumentSourceFact,
        view: nara_scene::__private::PreparedPrefabSchemaSource<'_>,
    ) -> Result<Self, AuthoringProjectSourceAdmissionError> {
        if source.source_key().kind() != nara_scene::SourceDocumentKind::Prefab
            || !source_locator_matches(Some(view.source()), source.relative_path())
        {
            return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
        }
        Ok(Self {
            source: Some(source.clone()),
            source_key: source.source_key().clone(),
            project_path: Arc::from(source.relative_path()),
            document: view.shared_document(),
            basis: Some(view.schema_basis()),
            source_matches: true,
            persistence_available: true,
            revision: view.base_revision(),
        })
    }

    pub(crate) fn capture(
        source: &super::super::document::AuthoringDocumentSourceFact,
        session: &nara_scene::PrefabAuthoringSession,
    ) -> Self {
        Self {
            source: Some(source.clone()),
            source_key: source.source_key().clone(),
            project_path: Arc::from(source.relative_path()),
            document: session.shared_document(),
            basis: session.authoring_schema_basis(),
            source_matches: source_locator_matches(
                session.authoring_source(),
                source.relative_path(),
            ),
            persistence_available: true,
            revision: session.revision(),
        }
    }

    pub(crate) fn source_key(&self) -> &nara_scene::SourceDocumentKey {
        &self.source_key
    }

    pub(crate) const fn revision(&self) -> nara_scene::SceneAuthoringRevision {
        self.revision
    }

    pub(crate) fn without_persistence(mut self) -> Self {
        self.persistence_available = false;
        self
    }

    pub(crate) fn unavailable(
        source_key: nara_scene::SourceDocumentKey,
        project_path: Arc<str>,
        session: &nara_scene::PrefabAuthoringSession,
    ) -> Self {
        Self {
            source: None,
            source_key,
            project_path,
            document: session.shared_document(),
            basis: session.authoring_schema_basis(),
            source_matches: false,
            persistence_available: false,
            revision: session.revision(),
        }
    }
}

pub(crate) struct AuthoringDocumentCapture {
    pub(super) readiness: AuthoringProjectReadiness,
    pub(super) diagnostics: DiagnosticReport,
    pub(super) prefabs: Box<[AuthoringPrefabSource]>,
    pub(super) scenes: Box<[AuthoringSceneSource]>,
    pub(super) assets: Box<[AuthoringAssetSource]>,
    pub(super) asset_database: Arc<ProjectAssetDatabase>,
    pub(super) asset_scope: Arc<AssetPathScope>,
    pub(super) prefab_scope: Arc<AssetPathScope>,
    pub(super) unresolved_dependencies: Box<[AuthoringUnresolvedDependency]>,
    pub(super) source_facts: Arc<[Arc<AuthoringSourceFact>]>,
    pub(super) stable_lookups: Arc<[catalogue::StableLookupObservation]>,
    pub(super) usage: AuthoringProjectAdmissionUsage,
    pub(super) host_retained_bytes: usize,
    pub(super) preview: Option<CapturedAuthoringPreview>,
    pub(super) composition_failure: Option<nara_scene::PrefabExpansionFailure>,
    pub(super) root_revision: Option<nara_scene::SceneAuthoringRevision>,
    pub(super) authority: Arc<AuthoringSchemaReceipt>,
}

pub(crate) struct CapturedAuthoringPreview {
    pub(crate) candidate: nara_scene::ExpandedSceneCandidate,
    pub(crate) omissions: Vec<nara_scene::__private::AuthoringPreviewOmission>,
}

#[derive(Clone)]
pub(crate) struct AuthoringSourceValidation {
    schema: Arc<AuthoringSchemaReceipt>,
    sources: Arc<[Arc<AuthoringSourceFact>]>,
    stable_lookups: Arc<[catalogue::StableLookupObservation]>,
    retained_capture_bytes: usize,
}

impl AuthoringSourceValidation {
    /// Checks retained schema handles; historical sources are collected separately.
    pub(crate) fn source_object_in_use(&self, candidate: &FileCapability) -> Result<bool, FsError> {
        self.schema.source_object_in_use(candidate)
    }

    /// Keeps source observations available even after decoded capture data is released.
    /// An error invalidates the entire collection, including any files already appended.
    pub(crate) fn collect_reclamation_sources(
        &self,
        output: &mut Vec<FileCapability>,
    ) -> Result<(), FsError> {
        self.schema.collect_reclamation_sources(output)?;
        for source in self.sources.iter() {
            source.collect_reclamation_source(&self.schema.project_root, output)?;
        }
        Ok(())
    }

    pub(super) fn new(
        schema: Arc<AuthoringSchemaReceipt>,
        sources: Arc<[Arc<AuthoringSourceFact>]>,
        stable_lookups: Arc<[catalogue::StableLookupObservation]>,
        retained_capture_bytes: usize,
    ) -> Self {
        Self {
            schema,
            sources,
            stable_lookups,
            retained_capture_bytes,
        }
    }

    pub(crate) fn validate(self) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let mut budget = self.schema.budget;
        self.validate_with_budget(&mut budget)
    }

    /// Admits additional Host observations against this validation's complete retained capture.
    /// Call before copying the observations; shared source facts retain their existing charge.
    pub(crate) fn with_additional_retained_bytes(
        mut self,
        bytes: usize,
    ) -> Result<Self, AuthoringProjectSourceAdmissionError> {
        self.retained_capture_bytes = checked_retained_replacement(
            self.retained_capture_bytes,
            0,
            bytes,
            self.schema.budget.limits.retained_candidate_bytes().get(),
        )?;
        Ok(self)
    }

    /// Reuses the same observation checks under an operation's cumulative allowance.
    pub(super) fn validate_with_budget(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let retained = self.sources.iter().try_fold(
            checked_product(self.sources.len(), size_of::<Arc<AuthoringSourceFact>>())?,
            |bytes, source| checked_sum([bytes, source_fact_retained_bytes(source)?]),
        )?;
        let retained = self.stable_lookups.iter().try_fold(
            checked_sum([retained, 2 * size_of::<usize>()])?,
            |bytes, observation| checked_sum([bytes, observation.retained_bytes()?]),
        )?;
        budget.charge_source_bytes(
            0,
            0,
            0,
            retained.max(
                self.retained_capture_bytes
                    .saturating_sub(budget.usage().retained_candidate_bytes()),
            ),
        )?;
        for source in self.sources.iter() {
            revalidate_source_fact(&self.schema.project_root, source, budget)?;
        }
        input::revalidate_metadata_absence(
            &self.schema.project_root,
            self.sources.iter().map(Arc::as_ref),
            budget,
        )?;
        for source in schema_source_facts(&self.schema) {
            revalidate_source_fact(&self.schema.project_root, &source, budget)?;
        }
        ReferenceCatalogue::revalidate_lookups(
            &self.schema.project_root,
            ReferenceCatalogue::project_bindings(&self.schema.settings),
            &self.stable_lookups,
            budget,
        )?;
        Ok(())
    }
}

pub(super) fn charge_open_prefab_inputs(
    open_prefabs: &[AuthoringPrefabSessionSnapshot],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    budget.charge_source_bytes(
        0,
        0,
        0,
        prefabs::open_prefab_inputs_retained_bytes(open_prefabs)?,
    )?;
    Ok(())
}

impl AuthoringDocumentCapture {
    /// Checks retained schema handles; historical sources are collected separately.
    pub(crate) fn source_object_in_use(&self, candidate: &FileCapability) -> Result<bool, FsError> {
        self.authority.source_object_in_use(candidate)
    }

    /// An error invalidates the entire collection, including any files already appended.
    pub(crate) fn collect_reclamation_sources(
        &self,
        output: &mut Vec<FileCapability>,
    ) -> Result<(), FsError> {
        self.authority.collect_reclamation_sources(output)?;
        for source in self.source_facts.iter() {
            source.collect_reclamation_source(&self.authority.project_root, output)?;
        }
        Ok(())
    }

    pub(crate) const fn readiness(&self) -> AuthoringProjectReadiness {
        self.readiness
    }
    pub(crate) fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
    pub(crate) fn prefabs(&self) -> &[AuthoringPrefabSource] {
        &self.prefabs
    }
    pub(crate) fn scenes(&self) -> &[AuthoringSceneSource] {
        &self.scenes
    }
    pub(crate) fn assets(&self) -> &[AuthoringAssetSource] {
        &self.assets
    }
    pub(crate) fn source_facts(&self) -> impl ExactSizeIterator<Item = &AuthoringSourceFact> {
        self.source_facts.iter().map(Arc::as_ref)
    }
    pub(crate) fn asset_database(&self) -> Arc<ProjectAssetDatabase> {
        Arc::clone(&self.asset_database)
    }
    pub(crate) fn asset_scope(&self) -> Arc<AssetPathScope> {
        Arc::clone(&self.asset_scope)
    }
    pub(crate) fn prefab_scope(&self) -> Arc<AssetPathScope> {
        Arc::clone(&self.prefab_scope)
    }
    pub(crate) fn asset_resolution(&self) -> AssetReferenceResolver<'_> {
        self.asset_database.resolver(&self.asset_scope)
    }
    pub(crate) fn prefab_resolution(&self) -> AssetReferenceResolver<'_> {
        self.asset_database.resolver(&self.prefab_scope)
    }
    pub(crate) const fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.usage
    }

    #[cfg(test)]
    pub(crate) const fn host_retained_bytes(&self) -> usize {
        self.host_retained_bytes
    }

    /// Replaces only Host retention, preserving all admitted source and compiler output.
    /// Admit input/output overlap before copying, then settle after unused inputs are dropped.
    /// Failure leaves both the complete usage and the previous Host reservation unchanged.
    pub(crate) fn replace_host_retained_bytes(
        &mut self,
        bytes: usize,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        let retained = checked_retained_replacement(
            self.usage.retained_candidate_bytes,
            self.host_retained_bytes,
            bytes,
            self.authority
                .budget
                .limits
                .retained_candidate_bytes()
                .get(),
        )?;
        self.usage.retained_candidate_bytes = retained;
        self.host_retained_bytes = bytes;
        Ok(())
    }

    pub(crate) fn take_preview(&mut self) -> Option<CapturedAuthoringPreview> {
        self.preview.take()
    }

    pub(crate) fn composition_failure(&self) -> Option<&nara_scene::PrefabExpansionFailure> {
        self.composition_failure.as_ref()
    }

    pub(crate) fn requires_recapture(&self) -> bool {
        !self.unresolved_dependencies.is_empty()
    }

    pub(crate) const fn root_revision(&self) -> Option<nara_scene::SceneAuthoringRevision> {
        self.root_revision
    }

    pub(crate) fn source_validation(&self) -> AuthoringSourceValidation {
        AuthoringSourceValidation::new(
            Arc::clone(&self.authority),
            Arc::clone(&self.source_facts),
            Arc::clone(&self.stable_lookups),
            self.usage.retained_candidate_bytes(),
        )
    }
}

#[derive(Clone, Copy)]
pub(super) enum AuthoringCapturePreview {
    Prepare,
    Omit,
}

pub(super) fn capture_scene(
    schema: &Arc<AuthoringSchemaReceipt>,
    scene_limits: SceneFileLimits,
    document: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    containing_source: &Arc<AuthoringSourceFact>,
    containing_native_source: &AdmittedNativeDocumentSource,
    mut readiness: AuthoringProjectReadiness,
    budget: &mut AuthoringProjectAdmissionBudget,
    open_prefabs: &[AuthoringPrefabSessionSnapshot],
    open_scenes: &[AuthoringSceneSessionSnapshot],
    preview: AuthoringCapturePreview,
    project_input: Option<&ProjectPath>,
) -> Result<AuthoringDocumentCapture, AuthoringProjectSourceAdmissionError> {
    let mut acquisition = prefabs::AuthoringPrefabAcquisition::new(
        schema,
        scene_limits,
        basis,
        Arc::clone(containing_source),
        containing_native_source,
        budget,
        open_prefabs,
    )?;
    resize_compiler_work(acquisition.budget, &mut acquisition.compiler_reservation, 0)?;
    let mut pending = BTreeMap::new();
    let mut captured_scenes = BTreeMap::new();
    let root = scenes::compile_root(
        &mut acquisition,
        document,
        containing_source,
        containing_native_source,
        &mut pending,
        preview,
    )?;
    let diagnostics = root.diagnostics;
    let composition_failure = root.failure;
    let preview = root.preview;
    if diagnostics.has_errors() {
        readiness = AuthoringProjectReadiness::Degraded;
    }
    captured_scenes.insert(
        containing_source.project_path().as_str().to_owned(),
        root.source,
    );
    let mut asset_database = ProjectAssetDatabase::default();
    let mut assets = BTreeMap::new();
    while let Some((canonical, (selection, depth))) = pending.pop_first() {
        if captured_scenes.contains_key(canonical.as_str()) {
            if asset_database.record_for_path(&canonical).is_none() {
                let (_, record, _, _) = scenes::load_resource_metadata(
                    &mut acquisition,
                    &selection,
                    depth,
                    &mut asset_database,
                )?;
                if record.source_kind() != &AssetSourceKind::Scene {
                    return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
                }
            }
            continue;
        }
        if assets.contains_key(&canonical) {
            continue;
        }
        if let Some(scene) = scenes::load_resource(
            &mut acquisition,
            &selection,
            depth,
            open_scenes,
            &mut asset_database,
            &mut assets,
        )? {
            let compiled =
                scenes::compile_dependency(&mut acquisition, scene, depth, &mut pending)?;
            if compiled.source.diagnostics.has_errors() {
                readiness = AuthoringProjectReadiness::Degraded;
            }
            captured_scenes.insert(canonical.as_str().to_owned(), compiled.source);
        }
    }
    // Only project startup capture activates global project inputs. Ordinary document capture
    // remains driven by its selected Scene and actual dependencies.
    if let Some(path) = project_input {
        input::capture(&mut acquisition, path, &mut asset_database)?;
    }
    let prefabs::AuthoringPrefabAcquisition {
        budget,
        mut identities,
        mut source_facts,
        unresolved_dependencies,
        catalogue,
        prefabs_directory,
        prefabs,
        ..
    } = acquisition;
    drop(prefabs_directory);

    let asset_scope = Arc::clone(&catalogue.assets().scope);
    let prefab_scope = Arc::clone(&catalogue.prefabs().scope);
    if catalogue.is_indexed() && !prefabs.is_empty() {
        for prefab in prefabs.values() {
            let path = prefab.path();
            let binding = &prefab.binding;
            if let (Some(source), nara_scene::SourceDocumentOrigin::StableAsset { stable_id }) = (
                prefab.native_source.metadata_source(),
                prefab.native_source.key().origin(),
            ) {
                // Native admission already owns this exact observation and its byte charge.
                // Compare the catalogue evidence without reopening or retaining another fact.
                catalogue.verify_metadata(path, binding, source)?;
                let record = AssetRecord::new(
                    *stable_id,
                    binding.scope.resolve(path),
                    AssetSourceKind::Prefab,
                );
                charge_database_record(&record, budget)?;
                insert_asset_record(&mut asset_database, record)?;
                continue;
            }
            let directory = open_source_directory(
                &schema.project_root,
                &binding.directory,
                AuthoringSourceKind::Prefab,
                budget,
            )?;
            if let Some((record, source)) =
                catalogue.admit_prefab_metadata(&directory, path, binding, budget)?
            {
                if identities.contains_key(&source.identity()) {
                    return Err(
                        AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                            path: source.project_path().clone(),
                        },
                    );
                }
                insert_asset_record(&mut asset_database, record)?;
                identities.insert(source.identity(), LoadedIdentity::AssetMetadata);
                source_facts.insert(source.project_path().as_str().to_owned(), source);
            }
        }
    }

    let prefab_path_bytes = prefabs.keys().try_fold(0_usize, |total, path| {
        total
            .checked_add(path.as_str().len())
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    })?;
    for prefab in prefabs.values() {
        if let nara_scene::SourceDocumentOrigin::StableAsset { stable_id } =
            prefab.native_source.key().origin()
            && asset_database
                .record_for_stable_id(*stable_id)
                .is_some_and(|record| {
                    record.path().as_str() != prefab.source().project_path().as_str()
                        || record.source_kind() != &AssetSourceKind::Prefab
                })
        {
            return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                super::super::AuthoringDocumentAdmissionError::NativeSourceIdentityConflict,
            ));
        }
    }
    prefabs::revalidate_sources(
        schema,
        containing_native_source,
        &prefabs,
        &mut identities,
        &mut source_facts,
        &catalogue,
        budget,
    )?;
    input::revalidate_metadata_absence(
        &schema.project_root,
        source_facts.values().map(Arc::as_ref),
        budget,
    )?;
    for scene in captured_scenes.values() {
        scene
            .native_source
            .revalidate(&schema.project_root)
            .map_err(AuthoringProjectSourceAdmissionError::NativeSource)?;
        if let nara_scene::SourceDocumentOrigin::StableAsset { stable_id } =
            scene.native_source.key().origin()
            && asset_database
                .record_for_stable_id(*stable_id)
                .is_some_and(|record| {
                    record.path().as_str() != scene.source.project_path().as_str()
                        || record.source_kind() != &AssetSourceKind::Scene
                })
        {
            return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                super::super::AuthoringDocumentAdmissionError::NativeSourceIdentityConflict,
            ));
        }
    }
    if !unresolved_dependencies.is_empty() {
        readiness = AuthoringProjectReadiness::Degraded;
    }
    let unresolved_dependency_retained =
        unresolved_dependencies_retained_bytes(&unresolved_dependencies)?;
    budget.charge_source_bytes(0, 0, 0, unresolved_dependency_retained)?;
    let source_facts = source_facts.into_values().collect::<Vec<_>>();
    budget.charge_source_bytes(
        0,
        0,
        0,
        snapshot_index_retained_bytes(
            source_facts.len(),
            prefabs.len(),
            assets.len(),
            unresolved_dependencies.len(),
            prefab_path_bytes,
        )?,
    )?;
    let stable_lookups = catalogue.into_lookup_observations(budget)?;
    let usage = budget.usage();
    Ok(AuthoringDocumentCapture {
        readiness,
        diagnostics,
        prefabs: prefabs.into_values().collect::<Vec<_>>().into_boxed_slice(),
        scenes: captured_scenes
            .into_values()
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        assets: assets.into_values().collect::<Vec<_>>().into_boxed_slice(),
        asset_database: Arc::new(asset_database),
        asset_scope,
        prefab_scope,
        unresolved_dependencies: unresolved_dependencies.into_boxed_slice(),
        source_facts: source_facts.into(),
        stable_lookups,
        usage,
        host_retained_bytes: 0,
        preview,
        composition_failure,
        root_revision: None,
        authority: Arc::clone(schema),
    })
}

fn composition_failure_retained_bytes(
    failure: &nara_scene::PrefabExpansionFailure,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    let mut bytes = size_of::<nara_scene::PrefabExpansionFailure>();
    if let Some(location) = failure.location() {
        checked_add_to(
            &mut bytes,
            crate::scene_retention::string_retained_bytes(location.path().anchor.as_str())
                .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        )?;
        checked_add_to(
            &mut bytes,
            checked_product(
                location.path().steps.capacity(),
                size_of::<nara_scene::PrefabInstancePathSegment>(),
            )?,
        )?;
        if let nara_scene::PrefabDeclarationSource::Prefab(key) = location.source() {
            checked_add_to(&mut bytes, key.retained_bytes())?;
        }
    }
    Ok(bytes)
}

fn schema_source_facts(schema: &AuthoringSchemaReceipt) -> [AuthoringSourceFact; 2] {
    [
        AuthoringSourceFact {
            kind: AuthoringSourceKind::ProjectManifest,
            project_path: ProjectPath::new(super::super::PROJECT_MANIFEST_PATH)
                .expect("the project manifest uses a valid fixed path"),
            identity: schema.manifest_identity(),
            parent_identity: schema.manifest_parent_identity(),
            digest: schema.manifest_digest(),
            encoded_bytes: schema.manifest_bytes().len(),
            dependency_depth: 0,
        },
        AuthoringSourceFact {
            kind: AuthoringSourceKind::SchemaBundle,
            project_path: schema.bundle_path().clone(),
            identity: schema.bundle_identity(),
            parent_identity: schema.bundle_parent_identity(),
            digest: schema.bundle_digest(),
            encoded_bytes: schema.bundle_bytes().len(),
            dependency_depth: 0,
        },
    ]
}

pub(super) fn source_locator_matches(source: Option<&AuthoringDocumentSource>, path: &str) -> bool {
    matches!(source.map(AuthoringDocumentSource::as_asset_ref),
        Some(AssetRef::Path(source_path)) if source_path.as_str() == path)
}

fn matching_metadata(
    expected: Option<&Arc<AuthoringSourceFact>>,
    actual: Option<&Arc<AuthoringSourceFact>>,
) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(expected), Some(actual)) => expected.same_file_observation(actual),
        _ => false,
    }
}

fn preview_omissions_retained_bytes(
    omissions: &Vec<nara_scene::__private::AuthoringPreviewOmission>,
) -> Option<usize> {
    use nara_scene::__private::AuthoringPreviewOmissionLocation;
    omissions.iter().try_fold(
        omissions
            .capacity()
            .checked_mul(size_of::<nara_scene::__private::AuthoringPreviewOmission>())?,
        |bytes, fact| {
            let (entity, component, provenance) = match fact.location() {
                AuthoringPreviewOmissionLocation::DirectEntityComponent { entity, component } => {
                    (entity, Some(component), None)
                }
                AuthoringPreviewOmissionLocation::ProjectedEntityComponent {
                    entity,
                    component,
                    provenance,
                } => (entity, Some(component), Some(provenance)),
                AuthoringPreviewOmissionLocation::NestedPrefab { entity } => (entity, None, None),
            };
            bytes
                .checked_add(crate::scene_retention::string_retained_bytes(entity.as_str()).ok()?)?
                .checked_add(component.map_or(Some(0), |component| {
                    crate::scene_retention::string_retained_bytes(component.as_str()).ok()
                })?)?
                .checked_add(provenance.map_or(Some(0), |provenance| {
                    crate::scene_retention::scene_provenance_dynamic_bytes(provenance).ok()
                })?)
        },
    )
}
