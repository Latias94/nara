//! Actual-demand source acquisition shared by independent authoring documents.

use super::*;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

/// An immutable working copy paired with its last accepted file receipt.
///
/// Only a demanded source activates its Schema and file checks. Holding this snapshot does
/// not turn unsaved source into a file-backed publication.
#[derive(Clone)]
pub(crate) struct AuthoringPrefabSessionSnapshot {
    source: super::super::document::AuthoringDocumentSourceFact,
    document: Arc<PrefabDocument>,
    basis: Option<AuthoringSchemaBasis>,
    source_matches: bool,
    persistence_available: bool,
    revision: nara_scene::SceneAuthoringRevision,
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
}

impl OwnedAuthoringSceneCapture {
    pub(crate) fn prepare(
        self,
    ) -> Result<AuthoringDocumentCapture, AuthoringProjectSourceAdmissionError> {
        let mut budget = self.schema.budget;
        budget.charge_source_bytes(0, 0, 0, self.adapter_retained)?;
        let mut input_retained =
            checked_sum([self.document.retained_bytes()?, 2 * size_of::<usize>()])?;
        for snapshot in &self.open_prefabs {
            input_retained = checked_sum([
                input_retained,
                size_of::<AuthoringPrefabSessionSnapshot>(),
                snapshot.document.retained_bytes()?,
                snapshot.source.relative_path().len(),
                snapshot
                    .source
                    .native_source()
                    .retained_bytes()
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
            ])?;
        }
        budget.charge_source_bytes(0, 0, 0, input_retained)?;
        budget.charge_reference(self.source.relative_path().len(), 0, false)?;
        budget.charge_open_file(3)?;
        budget.charge_source_bytes(
            self.source.encoded_bytes(),
            0,
            0,
            checked_sum([
                size_of::<AuthoringSourceFact>(),
                size_of::<Arc<AuthoringSourceFact>>(),
                self.source.relative_path().len(),
            ])?,
        )?;
        let native = self.source.native_source();
        let metadata_bytes = if let Some(metadata) = native.metadata_source() {
            budget.charge_open_file(3)?;
            budget.charge_reference(metadata.project_path().as_str().len(), 0, false)?;
            metadata.encoded_bytes()
        } else {
            0
        };
        budget.charge_source_bytes(
            metadata_bytes,
            0,
            0,
            native
                .retained_bytes()
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        )?;
        let containing = AuthoringSourceFact {
            kind: AuthoringSourceKind::Scene,
            project_path: ProjectPath::new(self.source.relative_path().to_owned())
                .map_err(AuthoringProjectSourceAdmissionError::JoinedSourcePath)?,
            identity: self.source.identity(),
            parent_identity: self.source.parent_identity(),
            digest: self.source.digest(),
            encoded_bytes: self.source.encoded_bytes(),
            dependency_depth: 0,
        };
        let mut capture = capture_scene(
            &self.schema,
            self.scene_limits,
            &self.document,
            &self.basis,
            &Arc::new(containing),
            self.source.native_source(),
            self.readiness,
            &mut budget,
            &self.open_prefabs,
        )?;
        // Selected documents transfer into composition ownership during loading. The other
        // working copies remain request inputs until preparation physically returns.
        let selected_retained = capture
            .prefabs
            .iter()
            .filter(|prefab| prefab.session_revision().is_some())
            .try_fold(0_usize, |bytes, prefab| {
                checked_sum([bytes, prefab.document().retained_bytes()?])
            })?;
        let remaining_input = input_retained
            .checked_sub(selected_retained)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        drop(self.document);
        drop(self.open_prefabs);
        budget.reconcile_retained_reservation(remaining_input, 0)?;
        capture.usage = budget.usage();
        capture.root_revision = Some(self.revision);
        Ok(capture)
    }
}

impl AuthoringPrefabSessionSnapshot {
    pub(crate) fn capture(
        source: &super::super::document::AuthoringDocumentSourceFact,
        session: &nara_scene::PrefabAuthoringSession,
    ) -> Self {
        Self {
            source: source.clone(),
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
        self.source.source_key()
    }

    pub(crate) const fn revision(&self) -> nara_scene::SceneAuthoringRevision {
        self.revision
    }

    pub(crate) fn without_persistence(mut self) -> Self {
        self.persistence_available = false;
        self
    }
}

pub(crate) struct AuthoringDocumentCapture {
    pub(super) readiness: AuthoringProjectReadiness,
    pub(super) diagnostics: DiagnosticReport,
    pub(super) prefabs: Box<[AuthoringPrefabSource]>,
    pub(super) assets: Box<[AuthoringAssetSource]>,
    pub(super) asset_database: Arc<ProjectAssetDatabase>,
    pub(super) unresolved_dependencies: Box<[AuthoringUnresolvedDependency]>,
    pub(super) source_facts: Arc<[Arc<AuthoringSourceFact>]>,
    pub(super) stable_lookups: Arc<[catalogue::StableLookupObservation]>,
    pub(super) usage: AuthoringProjectAdmissionUsage,
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
            revalidate_source_fact(&self.schema.project_root, source, &mut budget)?;
        }
        for source in schema_source_facts(&self.schema) {
            revalidate_source_fact(&self.schema.project_root, &source, &mut budget)?;
        }
        ReferenceCatalogue::revalidate_lookups(
            &self.schema.project_root,
            [
                &self.schema.settings.paths.assets,
                &self.schema.settings.paths.prefabs,
            ],
            &self.stable_lookups,
            &mut budget,
        )?;
        Ok(())
    }
}

impl AuthoringDocumentCapture {
    pub(crate) const fn readiness(&self) -> AuthoringProjectReadiness {
        self.readiness
    }
    pub(crate) fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
    pub(crate) fn prefabs(&self) -> &[AuthoringPrefabSource] {
        &self.prefabs
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
    pub(crate) const fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.usage
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
) -> Result<AuthoringDocumentCapture, AuthoringProjectSourceAdmissionError> {
    let prefab_root = schema.settings.paths.prefabs.clone();
    let asset_root = schema.settings.paths.assets.clone();
    let mut identities = HashMap::new();
    identities.insert(schema.manifest_identity(), LoadedIdentity::SchemaBasis);
    identities.insert(schema.bundle_identity(), LoadedIdentity::SchemaBasis);
    if identities.contains_key(&containing_source.identity()) {
        return Err(
            AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                path: containing_source.project_path().clone(),
            },
        );
    }
    let source_policy = NativeDocumentSourcePolicy::new(schema.settings());
    identities.insert(containing_source.identity(), LoadedIdentity::Scene);
    let mut source_facts = BTreeMap::new();
    source_facts.insert(
        containing_source.project_path().as_str().to_owned(),
        Arc::clone(containing_source),
    );
    include_native_metadata_source(containing_native_source, &mut identities, &mut source_facts)?;

    let mut pending_assets = BTreeMap::new();
    let mut unresolved_dependencies = Vec::new();
    let mut catalogue = ReferenceCatalogue::new(
        schema.project_root.share(),
        [asset_root.clone(), prefab_root.clone()],
    );
    let mut prefabs_directory = None;
    let mut prefabs = BTreeMap::new();
    let mut compiler_limit = remaining_compiler_work(
        budget,
        0,
        nara_scene::PrefabExpansionLimits::default().working_bytes(),
    )?;
    let mut compiler_reservation = 0;
    resize_compiler_work(budget, &mut compiler_reservation, compiler_limit.get())?;
    let options = nara_scene::PrefabExpansionOptions::default()
        .with_max_depth(budget.limits.traversal_depth().get())
        .with_limits(
            nara_scene::PrefabExpansionLimits::default().with_working_bytes(compiler_limit),
        );
    let mut compiled = nara_scene::__private::try_expand_authoring_prefabs_with_loader(
        document,
        basis,
        options,
        None,
        |reference, working_set| {
            let suspended = working_set
                .checked_work_bytes()
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            resize_compiler_work(budget, &mut compiler_reservation, suspended)?;
            let depth = working_set.source_depth();
            let loaded = (|| -> Result<Option<nara_scene::ResolvedPrefabSource>, AuthoringProjectSourceAdmissionError> {
                let Some(path) = catalogue.resolve(reference, true, budget)? else {
                    budget.charge_reference(16, depth, true)?;
                    unresolved_dependencies.push(AuthoringUnresolvedDependency {
                        kind: AuthoringUnresolvedDependencyKind::Prefab,
                        reference: reference.clone(), dependency_depth: depth,
                    });
                    return Ok(None);
                };
                budget.charge_reference(path.as_str().len(), depth, true)?;
                if let Some(prefab) = prefabs.get(&path) {
                    let prefab: &AuthoringPrefabSource = prefab;
                    return Ok(Some(prefab.resolved_source()));
                }
                if prefabs_directory.is_none() {
                    match open_source_directory(
                        &schema.project_root, &prefab_root, AuthoringSourceKind::Prefab,
                        budget,
                    ) {
                        Ok(directory) => prefabs_directory = Some(directory),
                        Err(error) if is_missing_source(&error) => {
                            unresolved_dependencies.push(AuthoringUnresolvedDependency {
                                kind: AuthoringUnresolvedDependencyKind::Prefab,
                                reference: reference.clone(), dependency_depth: depth,
                            });
                            return Ok(None);
                        }
                        Err(error) => return Err(error),
                    }
                }
                let project_path = join_project_path(&prefab_root, path.as_str())?;
                let opened = match open_source_file(
                    prefabs_directory.as_ref().expect("requested Prefab directory is retained"),
                    project_path, path.as_str(), AuthoringSourceKind::Prefab, depth,
                    budget,
                ) {
                    Ok(opened) => opened,
                    Err(error) if is_missing_source(&error) => {
                        unresolved_dependencies.push(AuthoringUnresolvedDependency {
                            kind: AuthoringUnresolvedDependencyKind::Prefab,
                            reference: reference.clone(), dependency_depth: depth,
                        });
                        return Ok(None);
                    }
                    Err(error) => return Err(error),
                };
                let native_source = source_policy.admit_budgeted(
                    &schema.project_root, opened.project_path.as_str(),
                    nara_scene::SourceDocumentKind::Prefab, opened.identity, depth, 5,
                    budget,
                )?;
                if native_source.key().origin() == containing_native_source.key().origin()
                    || prefabs.values().any(|prefab: &AuthoringPrefabSource| {
                        prefab.native_source.key().origin() == native_source.key().origin()
                            && prefab.source.project_path() != &opened.project_path
                    })
                {
                    return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                        super::super::AuthoringDocumentAdmissionError::NativeSourceIdentityConflict,
                    ));
                }
                include_native_metadata_source(&native_source, &mut identities, &mut source_facts)?;
                let mut matching_sessions = open_prefabs.iter().filter(|snapshot| {
                    snapshot.source.relative_path() == opened.project_path.as_str()
                        || snapshot.source_key() == native_source.key()
                });
                let selected_session = matching_sessions.next();
                if matching_sessions.next().is_some() {
                    return Err(AuthoringProjectSourceAdmissionError::AmbiguousPrefabSession {
                        path: opened.project_path,
                    });
                }
                let (source, content, limits) = if let Some(snapshot) = selected_session {
                    if snapshot.source_key().kind() != nara_scene::SourceDocumentKind::Prefab
                        || !snapshot.persistence_available
                        || !snapshot.source_matches
                        || snapshot.source_key() != native_source.key()
                        || snapshot.source.relative_path() != opened.project_path.as_str()
                        || snapshot.source.identity() != opened.identity
                        || snapshot.source.parent_identity() != opened.parent_identity
                        || !matching_metadata(
                            snapshot.source.native_source().metadata_source(),
                            native_source.metadata_source(),
                        )
                    {
                        return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                            kind: AuthoringSourceKind::Prefab,
                            path: opened.project_path,
                        });
                    }
                    if snapshot.basis.as_ref() != Some(basis) {
                        return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
                    }
                    if identities.contains_key(&opened.identity) {
                        return Err(AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                            path: opened.project_path,
                        });
                    }
                    let source = AuthoringSourceFact {
                        kind: AuthoringSourceKind::Prefab,
                        project_path: opened.project_path,
                        identity: snapshot.source.identity(),
                        parent_identity: snapshot.source.parent_identity(),
                        digest: snapshot.source.digest(),
                        encoded_bytes: snapshot.source.encoded_bytes(),
                        dependency_depth: depth,
                    };
                    // No decode or encoded input is charged for a shared working document.
                    // Its retained source and final file fence remain part of this capture.
                    budget.reconcile_retained_reservation(snapshot.document.retained_bytes()?, 0)?;
                    charge_composition_source(&source, 0, &snapshot.document, budget)?;
                    budget.charge_source_bytes(
                        source.encoded_bytes(), 0, 0, 0,
                    )?;
                    let source = Arc::new(source);
                    let content = AuthoringPrefabContent::Session {
                        document: Arc::clone(&snapshot.document),
                        revision: snapshot.revision,
                    };
                    identities.insert(source.identity(), LoadedIdentity::Prefab {
                        source: Arc::clone(&source),
                        document: Arc::clone(&snapshot.document),
                        limits: scene_limits,
                    });
                    source_facts.insert(source.project_path().as_str().to_owned(), Arc::clone(&source));
                    (source, content, scene_limits)
                } else if let Some(loaded) = identities.get(&opened.identity) {
                    let LoadedIdentity::Prefab { source, document, limits } = loaded else {
                        return Err(AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                            path: opened.project_path,
                        });
                    };
                    if prefabs.values().any(|prefab: &AuthoringPrefabSource| {
                        prefab.source.identity() == opened.identity && prefab.session_revision().is_some()
                    }) {
                        return Err(AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                            path: opened.project_path,
                        });
                    }
                    (Arc::clone(source), AuthoringPrefabContent::File(Arc::clone(document)), *limits)
                } else {
                    let read = read_unique_source(opened, scene_limits.encoded_bytes().get(), budget)?;
                    let (document, limits) = admit_prefab_composition_source(
                        &read, scene_limits, budget,
                    )?;
                    let source = Arc::clone(&read.source);
                    identities.insert(source.identity(), LoadedIdentity::Prefab {
                        source: Arc::clone(&source), document: Arc::clone(&document), limits,
                    });
                    source_facts.insert(source.project_path().as_str().to_owned(), Arc::clone(&source));
                    (source, AuthoringPrefabContent::File(document), limits)
                };
                let prefab = AuthoringPrefabSource {
                    path: path.clone(), native_source, source, content, basis: basis.clone(), limits,
                };
                let resolved = prefab.resolved_source();
                prefabs.insert(path, prefab);
                Ok(Some(resolved))
            })()?;
            compiler_limit = remaining_compiler_work(budget, compiler_reservation, compiler_limit)?;
            if suspended > compiler_limit.get() {
                return Err(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                        maximum: budget.limits.retained_candidate_bytes().get(),
                    }
                    .into(),
                );
            }
            resize_compiler_work(budget, &mut compiler_reservation, compiler_limit.get())?;
            working_set.restrict_working_bytes(compiler_limit);
            Ok::<_, AuthoringProjectSourceAdmissionError>(loaded)
        },
    )?;
    if compiled.budget_exceeded() {
        return Err(AuthoringProjectSourceAdmissionError::CompilationBudget(
            compiled.diagnostics,
        ));
    }
    resize_compiler_work(
        budget,
        &mut compiler_reservation,
        compiled
            .checked_work_bytes()
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
    )?;
    if compiled.diagnostics.has_errors() {
        readiness = AuthoringProjectReadiness::Degraded;
    }
    budget.charge_diagnostics(
        compiled.diagnostics.len(),
        compiled.diagnostics.retained_bytes(),
    )?;
    enqueue_document_assets(
        compiled.effective_asset_references(),
        0,
        &mut pending_assets,
        &mut unresolved_dependencies,
        &mut catalogue,
        budget,
    )?;
    let diagnostics = std::mem::take(&mut compiled.diagnostics);
    budget.charge_source_bytes(0, 0, 0, diagnostics.retained_bytes())?;
    if let Some(failure) = &compiled.failure {
        budget.charge_source_bytes(0, 0, 0, composition_failure_retained_bytes(failure)?)?;
    }
    let composition_failure = compiled.failure.take();
    let preview = if compiled.candidate.is_some() {
        let key_bytes = containing_native_source.key().retained_bytes();
        budget.charge_source_bytes(0, 0, 0, key_bytes)?;
        let (expanded, omissions) = compiled
            .into_native_preview()
            .map_err(AuthoringProjectSourceAdmissionError::CompilationBudget)?;
        Some(CapturedAuthoringPreview {
            candidate: expanded.bind_source(containing_native_source.key().clone()),
            omissions,
        })
    } else {
        drop(compiled);
        None
    };
    resize_compiler_work(budget, &mut compiler_reservation, 0)?;
    if let Some(preview) = &preview {
        let candidate_bytes =
            crate::scene_retention::direct_startup_scene_retained_bytes(&preview.candidate)
                .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let bytes = candidate_bytes
            .checked_sub(containing_native_source.key().retained_bytes())
            .and_then(|bytes| {
                bytes.checked_add(preview_omissions_retained_bytes(&preview.omissions)?)
            })
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        budget.charge_source_bytes(0, 0, 0, bytes)?;
    }
    drop(prefabs_directory);

    let mut asset_database = ProjectAssetDatabase::default();
    if catalogue.is_indexed() && !prefabs.is_empty() {
        let directory = open_source_directory(
            &schema.project_root,
            &prefab_root,
            AuthoringSourceKind::Prefab,
            budget,
        )?;
        for (path, prefab) in &prefabs {
            if let (Some(source), nara_scene::SourceDocumentOrigin::StableAsset { stable_id }) = (
                prefab.native_source.metadata_source(),
                prefab.native_source.key().origin(),
            ) {
                // Native admission already owns this exact observation and its byte charge.
                // Compare the catalogue evidence without reopening or retaining another fact.
                catalogue.verify_metadata(path, true, source)?;
                let record = AssetRecord::new(*stable_id, path.clone(), AssetSourceKind::Prefab);
                charge_database_record(&record, budget)?;
                insert_asset_record(&mut asset_database, record)?;
                continue;
            }
            if let Some((record, source)) =
                catalogue.admit_prefab_metadata(&directory, path, budget)?
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

    let assets_directory = if pending_assets.is_empty() {
        None
    } else {
        Some(open_source_directory(
            &schema.project_root,
            &asset_root,
            AuthoringSourceKind::AssetSource,
            budget,
        )?)
    };
    let mut assets = BTreeMap::new();
    for (path, depth) in pending_assets {
        let directory = assets_directory
            .as_ref()
            .ok_or(AuthoringProjectSourceAdmissionError::AssetDirectoryAuthorityUnavailable)?;
        let metadata_path = path.meta_path();
        budget.charge_reference(metadata_path.len(), depth, false)?;
        let metadata_project_path = join_project_path(&asset_root, &metadata_path)?;
        let metadata_opened = open_source_file(
            directory,
            metadata_project_path,
            &metadata_path,
            AuthoringSourceKind::AssetMetadata,
            depth,
            budget,
        )?;
        ensure_unclaimed_source_identity(&identities, &metadata_opened)?;
        let metadata_limits = AssetMetaFileLimits::default();
        let metadata_read = read_unique_source(
            metadata_opened,
            metadata_limits.encoded_bytes().get(),
            budget,
        )?;
        catalogue.verify_metadata(&path, false, &metadata_read.source)?;
        let exact_metadata_limit = ByteLimit::new(metadata_read.read_limit)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let metadata_encoded_bytes = metadata_read.bytes.len();
        let metadata_source = metadata_read.source;
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes_owned_with_limits(
            metadata_read.bytes.into_boxed_slice(),
            metadata_limits.with_encoded_bytes(exact_metadata_limit),
        )
        .map_err(
            |source| AuthoringProjectSourceAdmissionError::AssetMetadataFormat {
                path: path.clone(),
                source,
            },
        )?;
        asset_database
            .insert_identity(path.clone(), metadata.identity().clone())
            .map_err(|source| match source {
                AssetDatabaseError::MetaPathMismatch {
                    source_path,
                    meta_path,
                } => AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch {
                    requested: source_path,
                    declared: meta_path,
                },
                source => AuthoringProjectSourceAdmissionError::AssetIdentity {
                    path: path.clone(),
                    source,
                },
            })?;
        let record = AssetRecord::from(metadata.identity().clone());
        charge_database_record(&record, budget)?;
        charge_asset_metadata(
            &metadata_source,
            metadata_encoded_bytes,
            &metadata,
            &record,
            budget,
        )?;
        identities.insert(metadata_source.identity(), LoadedIdentity::AssetMetadata);
        source_facts.insert(
            metadata_source.project_path().as_str().to_owned(),
            Arc::clone(&metadata_source),
        );

        let source_project_path = join_project_path(&asset_root, record.path().as_str())?;
        let source_opened = open_source_file(
            directory,
            source_project_path,
            record.path().as_str(),
            AuthoringSourceKind::AssetSource,
            depth,
            budget,
        )?;
        ensure_unclaimed_source_identity(&identities, &source_opened)?;
        let aggregate_source_limit = budget.maximum();
        let source_read = read_unique_source(source_opened, aggregate_source_limit, budget)?;
        charge_asset_source(&source_read, budget)?;
        let source = Arc::clone(&source_read.source);
        identities.insert(source.identity(), LoadedIdentity::AssetSource);
        source_facts.insert(
            source.project_path().as_str().to_owned(),
            Arc::clone(&source),
        );
        let bytes = Arc::<[u8]>::from(source_read.bytes);
        assets.insert(
            record.path().clone(),
            AuthoringAssetSource {
                record,
                metadata,
                metadata_source,
                source,
                bytes,
            },
        );
    }
    drop(assets_directory);

    let prefab_path_bytes = prefabs.keys().try_fold(0_usize, |total, path| {
        total
            .checked_add(path.as_str().len())
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    })?;
    containing_native_source
        .revalidate(&schema.project_root)
        .map_err(AuthoringProjectSourceAdmissionError::NativeSource)?;
    for prefab in prefabs.values() {
        prefab
            .native_source
            .revalidate(&schema.project_root)
            .map_err(AuthoringProjectSourceAdmissionError::NativeSource)?;
        if let nara_scene::SourceDocumentOrigin::StableAsset { stable_id } =
            prefab.native_source.key().origin()
            && asset_database
                .record_for_stable_id(*stable_id)
                .is_some_and(|record| {
                    record.path() != prefab.path()
                        || record.source_kind() != &AssetSourceKind::Prefab
                })
        {
            return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                super::super::AuthoringDocumentAdmissionError::NativeSourceIdentityConflict,
            ));
        }
    }
    for source in source_facts.values() {
        revalidate_source_fact(&schema.project_root, source, budget)?;
    }
    for source in schema_source_facts(schema) {
        revalidate_source_fact(&schema.project_root, &source, budget)?;
    }
    for source in catalogue.lookup_sources() {
        if let Some(existing) = source_facts.get(source.project_path().as_str()) {
            if !existing.same_file_observation(source) {
                return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                    kind: source.kind(),
                    path: source.project_path().clone(),
                });
            }
        } else {
            revalidate_source_fact(&schema.project_root, source, budget)?;
            if identities.contains_key(&source.identity()) {
                return Err(
                    AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                        path: source.project_path().clone(),
                    },
                );
            }
            identities.insert(source.identity(), LoadedIdentity::AssetMetadata);
            source_facts.insert(
                source.project_path().as_str().to_owned(),
                Arc::clone(source),
            );
        }
    }
    if let nara_scene::SourceDocumentOrigin::StableAsset { stable_id } =
        containing_native_source.key().origin()
        && asset_database.record_for_stable_id(*stable_id).is_some()
    {
        return Err(AuthoringProjectSourceAdmissionError::NativeSource(
            super::super::AuthoringDocumentAdmissionError::NativeSourceIdentityConflict,
        ));
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
        assets: assets.into_values().collect::<Vec<_>>().into_boxed_slice(),
        asset_database: Arc::new(asset_database),
        unresolved_dependencies: unresolved_dependencies.into_boxed_slice(),
        source_facts: source_facts.into(),
        stable_lookups,
        usage,
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
