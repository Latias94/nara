//! Concrete actual-demand Prefab acquisition, independent of native preview.

use super::*;
use crate::project_host::authoring::document::AuthoringDocumentSourceFact;
use nara_scene::{PrefabExpansionWorkingSet, SourceDocumentKind};

/// Owned finite background inputs; constructing this request does not read dependencies.
/// The source owner retains its actual nominal root document and request separately.
pub(crate) struct AuthoringPrefabCaptureRequest {
    schema: Arc<AuthoringSchemaReceipt>,
    scene_limits: SceneFileLimits,
    source: AuthoringDocumentSourceFact,
    basis: AuthoringSchemaBasis,
    adapter_retained: usize,
    open_prefabs: Box<[AuthoringPrefabSessionSnapshot]>,
    inputs: AuthoringSourceCaptureInputs,
    root_input_retained: usize,
    budget: AuthoringProjectAdmissionBudget,
}

impl AuthoringProjectSessionSnapshot {
    /// Retain current open Prefabs, including a Prefab containing root. Only actual demand
    /// activates a snapshot; a finite demand of the containing source uses its working copy.
    pub(crate) fn capture_prefab_conversion_request(
        &self,
        source: &AuthoringDocumentSourceFact,
        basis: &AuthoringSchemaBasis,
        kind: SourceDocumentKind,
        inputs: AuthoringSourceCaptureInputs,
        open_prefabs: Box<[AuthoringPrefabSessionSnapshot]>,
    ) -> Result<AuthoringPrefabCaptureRequest, AuthoringProjectSourceAdmissionError> {
        if basis != &self.startup_scene.schema_basis() || &inputs.basis != basis {
            return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
        }
        if source.source_key().kind() != kind {
            return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                crate::project_host::authoring::AuthoringDocumentAdmissionError::DocumentKindMismatch,
            ));
        }
        Ok(AuthoringPrefabCaptureRequest {
            schema: Arc::clone(&self.schema),
            scene_limits: self.scene_limits,
            source: source.clone(),
            basis: basis.clone(),
            adapter_retained: adapter_receipts_retained_bytes(&self.adapters)?,
            open_prefabs,
            inputs,
            root_input_retained: 0,
            budget: self.schema.budget,
        })
    }
}

impl AuthoringPrefabCaptureRequest {
    /// Registers additional root/request retention not already admitted in `inputs`.
    /// The prepared result has its own compiler-reservation transfer at completion.
    pub(crate) fn retain_root_input(&mut self, bytes: usize) {
        self.root_input_retained = self.root_input_retained.max(bytes);
    }

    /// Borrows one finite request while the source owner compiles actual selected demand.
    pub(crate) fn acquire(
        &mut self,
    ) -> Result<AuthoringPrefabAcquisition<'_>, AuthoringProjectSourceAdmissionError> {
        self.budget = self.schema.budget;
        // Conversion keeps both input reservations conservatively charged through its result.
        self.inputs.charge_inputs(
            open_prefab_inputs_retained_bytes(&self.open_prefabs)?,
            &mut self.budget,
        )?;
        let (_, containing) = charge_capture_inputs(
            &self.source,
            self.root_input_retained,
            self.adapter_retained,
            &[],
            &mut self.budget,
        )?;
        let mut acquisition = AuthoringPrefabAcquisition::new(
            &self.schema,
            self.scene_limits,
            &self.basis,
            containing,
            self.source.native_source(),
            &mut self.budget,
            &self.open_prefabs,
        )?;
        acquisition.requests = Some(HashMap::new());
        Ok(acquisition)
    }
}

pub(crate) struct AuthoringPrefabSourceCapture {
    prefabs: Box<[AuthoringPrefabSource]>,
    requests: HashMap<AssetRef, ResolvedPrefabSource>,
    unresolved_dependencies: Box<[AuthoringUnresolvedDependency]>,
    validation: AuthoringSourceValidation,
    usage: AuthoringProjectAdmissionUsage,
}

impl AuthoringPrefabSourceCapture {
    pub(crate) fn prefabs(&self) -> &[AuthoringPrefabSource] {
        &self.prefabs
    }

    /// Uses the original authored request, including every actually demanded alias.
    pub(crate) fn resolve(&self, reference: &AssetRef) -> Option<ResolvedPrefabSource> {
        self.requests.get(reference).cloned()
    }

    pub(crate) fn source_validation(&self) -> AuthoringSourceValidation {
        self.validation.clone()
    }

    pub(crate) const fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.usage
    }

    pub(crate) fn requires_recapture(&self) -> bool {
        !self.unresolved_dependencies.is_empty()
    }
}

/// One loader owner serves full preview and source-only selected conversion.
///
/// The budget reserves the callback's complete suspended view, including any dependency
/// snapshots retained by its source-owner wrapper. Host source caches and receipts retain
/// their own modeled charge. Shared handles are not an excuse to omit either ownership model.
pub(crate) struct AuthoringPrefabAcquisition<'a> {
    pub(super) schema: &'a Arc<AuthoringSchemaReceipt>,
    pub(super) scene_limits: SceneFileLimits,
    pub(super) basis: &'a AuthoringSchemaBasis,
    pub(super) containing_native_source: &'a AdmittedNativeDocumentSource,
    pub(super) budget: &'a mut AuthoringProjectAdmissionBudget,
    pub(super) open_prefabs: &'a [AuthoringPrefabSessionSnapshot],
    pub(super) identities: HashMap<FileIdentity, LoadedIdentity>,
    pub(super) source_facts: BTreeMap<String, Arc<AuthoringSourceFact>>,
    pub(super) unresolved_dependencies: Vec<AuthoringUnresolvedDependency>,
    pub(super) catalogue: ReferenceCatalogue,
    pub(super) prefabs_directory: Option<(Arc<ProjectPath>, DirectoryCapability)>,
    pub(super) prefabs: BTreeMap<AssetSourcePath, AuthoringPrefabSource>,
    pub(super) compiler_limit: ByteLimit,
    pub(super) compiler_reservation: usize,
    source_policy: NativeDocumentSourcePolicy,
    containing_source: Arc<AuthoringSourceFact>,
    requests: Option<HashMap<AssetRef, ResolvedPrefabSource>>,
}

impl<'a> AuthoringPrefabAcquisition<'a> {
    pub(super) fn new(
        schema: &'a Arc<AuthoringSchemaReceipt>,
        scene_limits: SceneFileLimits,
        basis: &'a AuthoringSchemaBasis,
        containing_source: Arc<AuthoringSourceFact>,
        containing_native_source: &'a AdmittedNativeDocumentSource,
        budget: &'a mut AuthoringProjectAdmissionBudget,
        open_prefabs: &'a [AuthoringPrefabSessionSnapshot],
    ) -> Result<Self, AuthoringProjectSourceAdmissionError> {
        let kind = match containing_source.kind() {
            AuthoringSourceKind::Scene => SourceDocumentKind::Scene,
            AuthoringSourceKind::Prefab => SourceDocumentKind::Prefab,
            _ => {
                return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
            }
        };
        if containing_native_source.key().kind() != kind
            || containing_native_source.source_identity() != containing_source.identity()
        {
            return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
        }
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
        identities.insert(
            containing_source.identity(),
            LoadedIdentity::ContainingDocument,
        );
        let mut source_facts = BTreeMap::new();
        source_facts.insert(
            containing_source.project_path().as_str().to_owned(),
            Arc::clone(&containing_source),
        );
        include_native_metadata_source(
            containing_native_source,
            &mut identities,
            &mut source_facts,
        )?;
        let catalogue = ReferenceCatalogue::new(
            schema.project_root.share(),
            ReferenceCatalogue::project_bindings(&schema.settings),
            budget,
        )?;
        let compiler_limit = remaining_compiler_work(
            budget,
            0,
            nara_scene::PrefabExpansionLimits::default().working_bytes(),
        )?;
        let mut compiler_reservation = 0;
        resize_compiler_work(budget, &mut compiler_reservation, compiler_limit.get())?;
        Ok(Self {
            schema,
            scene_limits,
            basis,
            containing_native_source,
            budget,
            open_prefabs,
            identities,
            source_facts,
            unresolved_dependencies: Vec::new(),
            catalogue,
            prefabs_directory: None,
            prefabs: BTreeMap::new(),
            compiler_limit,
            compiler_reservation,
            source_policy: NativeDocumentSourcePolicy::new(schema.settings()),
            containing_source,
            requests: None,
        })
    }

    /// Apply this cap to the nominal source request before it starts compiling.
    pub(crate) const fn initial_compiler_limit(&self) -> ByteLimit {
        self.compiler_limit
    }

    pub(crate) fn loaded_prefabs(&self) -> impl Iterator<Item = &AuthoringPrefabSource> {
        self.prefabs.values()
    }

    pub(crate) fn load(
        &mut self,
        reference: &AssetRef,
        working_set: &mut PrefabExpansionWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, AuthoringProjectSourceAdmissionError> {
        let suspended = working_set
            .checked_work_bytes()
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        resize_compiler_work(self.budget, &mut self.compiler_reservation, suspended)?;
        let loaded = self.load_source(reference, working_set.source_depth())?;
        if let (Some(requests), Some(source)) = (&mut self.requests, &loaded) {
            if let Some(previous) = requests.get(reference) {
                if previous.key() != source.key() {
                    return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                        crate::project_host::authoring::AuthoringDocumentAdmissionError::NativeSourceIdentityConflict,
                    ));
                }
            } else {
                self.budget.charge_source_bytes(
                    0,
                    0,
                    0,
                    checked_sum([
                        size_of::<(AssetRef, ResolvedPrefabSource)>(),
                        asset_ref_retained_bytes(reference)?,
                        source.key().retained_bytes(),
                        64,
                    ])?,
                )?;
                requests.insert(reference.clone(), source.clone());
            }
        }
        self.compiler_limit =
            remaining_compiler_work(self.budget, self.compiler_reservation, self.compiler_limit)?;
        if suspended > self.compiler_limit.get() {
            return Err(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                    maximum: self.budget.limits.retained_candidate_bytes().get(),
                }
                .into(),
            );
        }
        resize_compiler_work(
            self.budget,
            &mut self.compiler_reservation,
            self.compiler_limit.get(),
        )?;
        working_set.restrict_working_bytes(self.compiler_limit);
        Ok(loaded)
    }

    fn load_source(
        &mut self,
        reference: &AssetRef,
        depth: usize,
    ) -> Result<Option<ResolvedPrefabSource>, AuthoringProjectSourceAdmissionError> {
        let schema = self.schema;
        let scene_limits = self.scene_limits;
        let basis = self.basis;
        let containing_native_source = self.containing_native_source;
        let containing_source = &self.containing_source;
        let budget = &mut *self.budget;
        let open_prefabs = self.open_prefabs;
        let identities = &mut self.identities;
        let source_facts = &mut self.source_facts;
        let catalogue = &mut self.catalogue;
        let prefabs_directory = &mut self.prefabs_directory;
        let prefabs = &mut self.prefabs;
        let source_policy = &self.source_policy;
        let unresolved_dependencies = &mut self.unresolved_dependencies;
        let Some(selected) = catalogue.resolve_prefab(reference, budget)? else {
            budget.charge_reference(16, depth, true)?;
            unresolved_dependencies.push(AuthoringUnresolvedDependency {
                kind: AuthoringUnresolvedDependencyKind::Prefab,
                reference: reference.clone(),
                dependency_depth: depth,
            });
            return Ok(None);
        };
        let path = selected.path;
        let binding = selected.binding;
        budget.charge_source_bytes(
            0,
            0,
            0,
            checked_sum([binding.directory.as_str().len(), path.as_str().len(), 1])?,
        )?;
        let canonical = binding.scope.resolve(&path);
        budget.charge_reference(path.as_str().len(), depth, true)?;
        if let Some(prefab) = prefabs.get(&canonical) {
            let prefab: &AuthoringPrefabSource = prefab;
            return catalogue
                .bind_prefab_source(reference, prefab.resolved_source())
                .map(Some);
        }
        if prefabs_directory
            .as_ref()
            .is_none_or(|(root, _)| root != &binding.directory)
        {
            match open_source_directory(
                &schema.project_root,
                &binding.directory,
                AuthoringSourceKind::Prefab,
                budget,
            ) {
                Ok(directory) => {
                    *prefabs_directory = Some((Arc::clone(&binding.directory), directory))
                }
                Err(error) if is_missing_source(&error) => {
                    unresolved_dependencies.push(AuthoringUnresolvedDependency {
                        kind: AuthoringUnresolvedDependencyKind::Prefab,
                        reference: reference.clone(),
                        dependency_depth: depth,
                    });
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
        }
        let project_path = join_project_path(&binding.directory, path.as_str())?;
        let opened = match open_source_file(
            prefabs_directory
                .as_ref()
                .map(|(_, directory)| directory)
                .expect("requested Prefab directory is retained"),
            project_path,
            path.as_str(),
            AuthoringSourceKind::Prefab,
            depth,
            budget,
        ) {
            Ok(opened) => opened,
            Err(error) if is_missing_source(&error) => {
                unresolved_dependencies.push(AuthoringUnresolvedDependency {
                    kind: AuthoringUnresolvedDependencyKind::Prefab,
                    reference: reference.clone(),
                    dependency_depth: depth,
                });
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let native_source = source_policy.admit_budgeted(
            &schema.project_root,
            opened.project_path.as_str(),
            nara_scene::SourceDocumentKind::Prefab,
            opened.identity,
            depth,
            5,
            budget,
        )?;
        let is_containing_prefab = containing_source.kind() == AuthoringSourceKind::Prefab
            && containing_source.project_path() == &opened.project_path
            && containing_source.identity() == opened.identity
            && native_source.key() == containing_native_source.key();
        // Equal canonical sources can form a finite selected tree under suppression. A
        // containing Prefab still requires its actual working copy in the selection below.
        if (native_source.key().origin() == containing_native_source.key().origin()
            && !is_containing_prefab)
            || prefabs.values().any(|prefab: &AuthoringPrefabSource| {
                prefab.native_source.key().origin() == native_source.key().origin()
                    && prefab.source.project_path() != &opened.project_path
            })
        {
            return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                crate::project_host::authoring::AuthoringDocumentAdmissionError::NativeSourceIdentityConflict,
            ));
        }
        include_native_metadata_source(&native_source, identities, source_facts)?;
        let mut matching_sessions = open_prefabs.iter().filter(|snapshot| {
            snapshot.project_path.as_ref() == opened.project_path.as_str()
                || snapshot.source_key() == native_source.key()
        });
        let selected_session = matching_sessions.next();
        if matching_sessions.next().is_some() {
            return Err(
                AuthoringProjectSourceAdmissionError::AmbiguousPrefabSession {
                    path: opened.project_path,
                },
            );
        }
        let (source, content, limits) = if let Some(snapshot) = selected_session {
            let Some(snapshot_source) = snapshot.source.as_ref() else {
                return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                    kind: AuthoringSourceKind::Prefab,
                    path: opened.project_path,
                });
            };
            if snapshot.source_key().kind() != nara_scene::SourceDocumentKind::Prefab
                || !snapshot.persistence_available
                || !snapshot.source_matches
                || snapshot.source_key() != native_source.key()
                || snapshot_source.relative_path() != opened.project_path.as_str()
                || snapshot_source.identity() != opened.identity
                || snapshot_source.parent_identity() != opened.parent_identity
                || !matching_metadata(
                    snapshot_source.native_source().metadata_source(),
                    native_source.metadata_source(),
                )
                || (is_containing_prefab
                    && (snapshot_source.digest() != containing_source.digest()
                        || snapshot_source.encoded_bytes() != containing_source.encoded_bytes()
                        || snapshot_source.parent_identity()
                            != containing_source.parent_identity()))
            {
                return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                    kind: AuthoringSourceKind::Prefab,
                    path: opened.project_path,
                });
            }
            if snapshot.basis.as_ref() != Some(basis) {
                return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
            }
            if identities.contains_key(&opened.identity) && !is_containing_prefab {
                return Err(
                    AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                        path: opened.project_path,
                    },
                );
            }
            let source = AuthoringSourceFact {
                kind: AuthoringSourceKind::Prefab,
                project_path: opened.project_path,
                identity: snapshot_source.identity(),
                parent_identity: snapshot_source.parent_identity(),
                digest: snapshot_source.digest(),
                encoded_bytes: snapshot_source.encoded_bytes(),
                dependency_depth: depth,
            };
            // No decode or encoded input is charged for a shared working document.
            // Its retained source and final file fence remain part of this capture.
            budget.reconcile_retained_reservation(snapshot.document.retained_bytes()?, 0)?;
            charge_composition_source(&source, 0, &snapshot.document, budget)?;
            budget.charge_source_bytes(source.encoded_bytes(), 0, 0, 0)?;
            let source = Arc::new(source);
            let content = AuthoringPrefabContent::Session {
                document: Arc::clone(&snapshot.document),
                revision: snapshot.revision,
            };
            identities.insert(source.identity(), LoadedIdentity::Prefab);
            source_facts.insert(
                source.project_path().as_str().to_owned(),
                Arc::clone(&source),
            );
            (source, content, scene_limits)
        } else if identities.contains_key(&opened.identity) {
            return Err(
                AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                    path: opened.project_path,
                },
            );
        } else {
            let read = read_unique_source(opened, scene_limits.encoded_bytes().get(), budget)?;
            let (document, limits) = admit_prefab_composition_source(&read, scene_limits, budget)?;
            let source = Arc::clone(&read.source);
            identities.insert(source.identity(), LoadedIdentity::Prefab);
            source_facts.insert(
                source.project_path().as_str().to_owned(),
                Arc::clone(&source),
            );
            (source, AuthoringPrefabContent::File(document), limits)
        };
        let prefab = AuthoringPrefabSource {
            path,
            binding,
            native_source,
            source,
            content,
            basis: basis.clone(),
            limits,
        };
        let resolved = catalogue.bind_prefab_source(reference, prefab.resolved_source())?;
        prefabs.insert(canonical, prefab);
        Ok(Some(resolved))
    }

    /// Finish without resource hydration or a Scene preview.
    ///
    /// `retained_conversion_bytes` accounts for the complete prepared source token,
    /// including its dependency snapshots; it replaces the compiler reservation. Input
    /// documents remain conservatively charged until this finite request is dropped.
    pub(crate) fn finish_source_capture(
        mut self,
        retained_conversion_bytes: usize,
    ) -> Result<AuthoringPrefabSourceCapture, AuthoringProjectSourceAdmissionError> {
        resize_compiler_work(
            self.budget,
            &mut self.compiler_reservation,
            retained_conversion_bytes,
        )?;
        drop(self.prefabs_directory.take());
        revalidate_sources(
            self.schema,
            self.containing_native_source,
            &self.prefabs,
            &mut self.identities,
            &mut self.source_facts,
            &self.catalogue,
            self.budget,
        )?;
        let path_bytes = self
            .prefabs
            .keys()
            .try_fold(0, |bytes, path| checked_sum([bytes, path.as_str().len()]))?;
        let index_bytes = checked_sum([
            size_of::<AuthoringPrefabSourceCapture>(),
            checked_product(
                self.source_facts.len(),
                2 * size_of::<Arc<AuthoringSourceFact>>(),
            )?,
            checked_product(self.prefabs.len(), size_of::<AuthoringPrefabSource>())?,
            path_bytes,
            unresolved_dependencies_retained_bytes(&self.unresolved_dependencies)?,
            2 * size_of::<usize>(),
        ])?;
        self.budget.charge_source_bytes(0, 0, 0, index_bytes)?;
        let stable_lookups = self.catalogue.into_lookup_observations(self.budget)?;
        let sources = self.source_facts.into_values().collect::<Vec<_>>().into();
        let validation = AuthoringSourceValidation::new(
            Arc::clone(self.schema),
            sources,
            stable_lookups,
            self.budget.usage().retained_candidate_bytes(),
        );
        Ok(AuthoringPrefabSourceCapture {
            prefabs: self
                .prefabs
                .into_values()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            requests: self.requests.unwrap_or_default(),
            unresolved_dependencies: self.unresolved_dependencies.into_boxed_slice(),
            validation,
            usage: self.budget.usage(),
        })
    }
}

pub(super) fn charge_capture_inputs(
    source: &AuthoringDocumentSourceFact,
    document_bytes: usize,
    adapter_retained: usize,
    open_prefabs: &[AuthoringPrefabSessionSnapshot],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(usize, Arc<AuthoringSourceFact>), AuthoringProjectSourceAdmissionError> {
    budget.charge_source_bytes(0, 0, 0, adapter_retained)?;
    let input_retained = checked_sum([
        document_bytes,
        2 * size_of::<usize>(),
        open_prefab_inputs_retained_bytes(open_prefabs)?,
    ])?;
    budget.charge_source_bytes(0, 0, 0, input_retained)?;
    budget.charge_reference(source.relative_path().len(), 0, false)?;
    budget.charge_open_file(3)?;
    budget.charge_source_bytes(
        source.encoded_bytes(),
        0,
        0,
        checked_sum([
            size_of::<AuthoringSourceFact>(),
            size_of::<Arc<AuthoringSourceFact>>(),
            source.relative_path().len(),
        ])?,
    )?;
    let native = source.native_source();
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
    let kind = match source.source_key().kind() {
        SourceDocumentKind::Scene => AuthoringSourceKind::Scene,
        SourceDocumentKind::Prefab => AuthoringSourceKind::Prefab,
    };
    Ok((
        input_retained,
        Arc::new(AuthoringSourceFact {
            kind,
            project_path: ProjectPath::new(source.relative_path().to_owned())
                .map_err(AuthoringProjectSourceAdmissionError::JoinedSourcePath)?,
            identity: source.identity(),
            parent_identity: source.parent_identity(),
            digest: source.digest(),
            encoded_bytes: source.encoded_bytes(),
            dependency_depth: 0,
        }),
    ))
}

pub(in crate::project_host::authoring::project) fn open_prefab_inputs_retained_bytes(
    open_prefabs: &[AuthoringPrefabSessionSnapshot],
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    let mut input_retained = 0;
    for snapshot in open_prefabs {
        input_retained = checked_sum([
            input_retained,
            AuthoringPrefabSessionSnapshot::input_retained_bytes(
                snapshot.source.as_ref(),
                &snapshot.source_key,
                &snapshot.project_path,
                &snapshot.document,
            )?,
        ])?;
    }
    Ok(input_retained)
}

pub(super) fn revalidate_sources(
    schema: &AuthoringSchemaReceipt,
    containing_native_source: &AdmittedNativeDocumentSource,
    prefabs: &BTreeMap<AssetSourcePath, AuthoringPrefabSource>,
    identities: &mut HashMap<FileIdentity, LoadedIdentity>,
    source_facts: &mut BTreeMap<String, Arc<AuthoringSourceFact>>,
    catalogue: &ReferenceCatalogue,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    containing_native_source
        .revalidate(&schema.project_root)
        .map_err(AuthoringProjectSourceAdmissionError::NativeSource)?;
    for prefab in prefabs.values() {
        prefab
            .native_source
            .revalidate(&schema.project_root)
            .map_err(AuthoringProjectSourceAdmissionError::NativeSource)?;
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
    Ok(())
}
