//! Finite Scene data closure over the production schema-aware Prefab compiler.

use super::super::catalogue::ResolvedResourceSource;
use super::*;

type PendingResources = BTreeMap<AssetSourcePath, (ResolvedResourceSource, usize)>;

pub(super) struct CompiledScene {
    pub(super) source: AuthoringSceneSource,
    pub(super) preview: Option<CapturedAuthoringPreview>,
    pub(super) diagnostics: DiagnosticReport,
    pub(super) failure: Option<nara_scene::PrefabExpansionFailure>,
}

pub(crate) fn open_scene_inputs_retained_bytes(
    scenes: &[AuthoringSceneSessionSnapshot],
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    scenes.iter().try_fold(0, |bytes, scene| {
        checked_sum([
            bytes,
            AuthoringSceneSessionSnapshot::input_retained_bytes(
                scene.source.as_ref(),
                &scene.source_key,
                &scene.project_path,
                &scene.document,
            )?,
        ])
    })
}

pub(super) fn compile_root(
    acquisition: &mut prefabs::AuthoringPrefabAcquisition<'_>,
    document: &SceneDocument,
    source: &Arc<AuthoringSourceFact>,
    native_source: &AdmittedNativeDocumentSource,
    pending: &mut PendingResources,
    preview: AuthoringCapturePreview,
) -> Result<CompiledScene, AuthoringProjectSourceAdmissionError> {
    // The borrowed root input and the retained closure allocation overlap until preparation
    // returns. Admit the complete second allocation before cloning any document data.
    acquisition.budget.charge_source_bytes(
        0,
        0,
        0,
        checked_sum([
            document.retained_bytes()?,
            size_of::<AuthoringSceneSource>(),
            source.project_path().as_str().len(),
            native_source
                .retained_bytes()
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        ])?,
    )?;
    compile(
        acquisition,
        AuthoringSceneSource {
            native_source: native_source.clone(),
            source: Arc::clone(source),
            document: Arc::new(document.clone()),
            compiled: None,
            root_effective: None,
            revision: None,
            diagnostics: DiagnosticReport::default(),
            failure: None,
        },
        0,
        pending,
        Some(preview),
    )
}

pub(super) fn compile_dependency(
    acquisition: &mut prefabs::AuthoringPrefabAcquisition<'_>,
    source: AuthoringSceneSource,
    depth: usize,
    pending: &mut PendingResources,
) -> Result<CompiledScene, AuthoringProjectSourceAdmissionError> {
    compile(acquisition, source, depth, pending, None)
}

fn compile(
    acquisition: &mut prefabs::AuthoringPrefabAcquisition<'_>,
    mut source: AuthoringSceneSource,
    depth: usize,
    pending: &mut PendingResources,
    preview: Option<AuthoringCapturePreview>,
) -> Result<CompiledScene, AuthoringProjectSourceAdmissionError> {
    acquisition.compiler_limit = remaining_compiler_work(
        acquisition.budget,
        acquisition.compiler_reservation,
        nara_scene::PrefabExpansionLimits::default().working_bytes(),
    )?;
    resize_compiler_work(
        acquisition.budget,
        &mut acquisition.compiler_reservation,
        acquisition.compiler_limit.get(),
    )?;
    let options = nara_scene::PrefabExpansionOptions::default()
        .with_max_depth(acquisition.budget.limits.traversal_depth().get())
        .with_limits(
            nara_scene::PrefabExpansionLimits::default()
                .with_working_bytes(acquisition.compiler_limit),
        );
    let mut compiled = nara_scene::__private::try_expand_authoring_prefabs_with_loader(
        &source.document,
        acquisition.basis,
        options,
        None,
        |reference, work| acquisition.load(reference, work),
    )?;
    if compiled.budget_exceeded() {
        return Err(AuthoringProjectSourceAdmissionError::CompilationBudget(
            compiled.diagnostics,
        ));
    }
    resize_compiler_work(
        acquisition.budget,
        &mut acquisition.compiler_reservation,
        compiled
            .checked_work_bytes()
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
    )?;
    let next_depth = depth
        .checked_add(1)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    for declared in compiled.effective_asset_references() {
        let reference = declared.asset_ref();
        if let Some(selection) = acquisition
            .catalogue
            .resolve_resource(reference, acquisition.budget)?
        {
            acquisition
                .budget
                .charge_reference(selection.path.as_str().len(), next_depth, true)?;
            let canonical_len = checked_sum([
                selection.binding.directory.as_str().len(),
                1,
                selection.path.as_str().len(),
            ])?;
            acquisition.budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([
                    canonical_len,
                    size_of::<(AssetSourcePath, (ResolvedResourceSource, usize))>(),
                    64,
                ])?,
            )?;
            let canonical = selection.binding.scope.resolve(&selection.path);
            pending.entry(canonical).or_insert((selection, next_depth));
        } else {
            acquisition.budget.charge_reference(16, next_depth, true)?;
            acquisition.budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([
                    size_of::<AuthoringUnresolvedDependency>(),
                    asset_ref_retained_bytes(reference)?,
                ])?,
            )?;
            acquisition
                .unresolved_dependencies
                .push(AuthoringUnresolvedDependency {
                    kind: AuthoringUnresolvedDependencyKind::Asset,
                    reference: reference.clone(),
                    dependency_depth: next_depth,
                });
        }
    }
    acquisition.budget.charge_diagnostics(
        compiled.diagnostics.len(),
        compiled.diagnostics.retained_bytes(),
    )?;
    let mut result_retained = compiled.diagnostics.retained_bytes();
    let mut diagnostics = std::mem::take(&mut compiled.diagnostics);
    if let Some(failure) = &compiled.failure {
        result_retained = checked_sum([
            result_retained,
            composition_failure_retained_bytes(failure)?,
        ])?;
    }
    let mut failure = compiled.failure.take();
    let native_preview = if matches!(preview, Some(AuthoringCapturePreview::Prepare))
        && compiled.candidate.is_some()
    {
        let effective = compiled
            .candidate
            .as_ref()
            .expect("checked candidate")
            .document();
        acquisition
            .budget
            .charge_source_bytes(0, 0, 0, effective.retained_bytes()?)?;
        source.root_effective = Some(Arc::new(effective.clone()));
        acquisition.budget.charge_source_bytes(
            0,
            0,
            0,
            source.native_source.key().retained_bytes(),
        )?;
        let omission_reservation = preview_omission_copy_retained_bytes(&compiled)?;
        acquisition
            .budget
            .charge_source_bytes(0, 0, 0, omission_reservation)?;
        let (candidate, omissions) = compiled
            .into_native_preview()
            .map_err(AuthoringProjectSourceAdmissionError::CompilationBudget)?;
        let candidate = candidate.bind_source(source.native_source.key().clone());
        let omission_retained = preview_omissions_retained_bytes(&omissions)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        acquisition
            .budget
            .reconcile_retained_reservation(omission_reservation, omission_retained)?;
        result_retained = checked_sum([
            result_retained,
            crate::scene_retention::direct_startup_scene_retained_bytes(&candidate)
                .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?
                .checked_sub(source.native_source.key().retained_bytes())
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        ])?;
        Some(CapturedAuthoringPreview {
            candidate,
            omissions,
        })
    } else {
        if let Some(candidate) = compiled.candidate.take() {
            acquisition.budget.charge_source_bytes(
                0,
                0,
                0,
                source.native_source.key().retained_bytes(),
            )?;
            let candidate = candidate.bind_source(source.native_source.key().clone());
            result_retained = checked_sum([
                result_retained,
                crate::scene_retention::direct_startup_scene_retained_bytes(&candidate)
                    .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?
                    .checked_sub(source.native_source.key().retained_bytes())
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
            ])?;
            source.compiled = Some(candidate);
        }
        drop(compiled);
        None
    };
    retain_compiler_result(
        acquisition.budget,
        &mut acquisition.compiler_reservation,
        result_retained,
    )?;
    if preview.is_none() {
        source.diagnostics = std::mem::take(&mut diagnostics);
        source.failure = failure.take();
    }
    Ok(CompiledScene {
        source,
        preview: native_preview,
        diagnostics,
        failure,
    })
}

pub(super) fn retain_compiler_result(
    budget: &mut AuthoringProjectAdmissionBudget,
    reservation: &mut usize,
    retained: usize,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    // The candidate, diagnostics and failure move out of compiler custody. No second copy
    // overlaps the suspended compiler's reservation; only actual new copies were charged above.
    resize_compiler_work(budget, reservation, retained)?;
    *reservation = 0;
    Ok(())
}

fn preview_omission_copy_retained_bytes(
    compiled: &nara_scene::__private::AuthoringPrefabExpansionReport,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    let candidate = compiled
        .candidate
        .as_ref()
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    compiled.omissions.iter().try_fold(
        checked_product(
            compiled.omissions.len(),
            size_of::<nara_scene::__private::AuthoringPreviewOmission>(),
        )?,
        |bytes, fact| {
            let provenance = candidate
                .provenance()
                .get(&fact.entity)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            checked_sum([
                bytes,
                crate::scene_retention::string_retained_bytes(fact.entity.as_str())
                    .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
                crate::scene_retention::string_retained_bytes(fact.component.as_str())
                    .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
                if provenance.projection_path().is_some() {
                    crate::scene_retention::scene_provenance_dynamic_bytes(provenance)
                        .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?
                } else {
                    0
                },
            ])
        },
    )
}

pub(super) fn load_resource(
    acquisition: &mut prefabs::AuthoringPrefabAcquisition<'_>,
    selection: &ResolvedResourceSource,
    depth: usize,
    open_scenes: &[AuthoringSceneSessionSnapshot],
    database: &mut ProjectAssetDatabase,
    assets: &mut BTreeMap<AssetSourcePath, AuthoringAssetSource>,
) -> Result<Option<AuthoringSceneSource>, AuthoringProjectSourceAdmissionError> {
    let binding = &selection.binding;
    let path = &selection.path;
    let (directory, record, metadata, metadata_source) =
        load_resource_metadata(acquisition, selection, depth, database)?;
    let budget = &mut *acquisition.budget;
    let scene = record.source_kind() == &AssetSourceKind::Scene;
    let kind = if scene {
        AuthoringSourceKind::Scene
    } else {
        AuthoringSourceKind::AssetSource
    };
    let opened = open_source_file(
        &directory,
        join_project_path(&binding.directory, path.as_str())?,
        path.as_str(),
        kind,
        depth,
        budget,
    )?;
    ensure_unclaimed_source_identity(&acquisition.identities, &opened)?;
    if !scene {
        let read = read_unique_source(opened, budget.maximum(), budget)?;
        charge_asset_source(&read, budget)?;
        acquisition
            .identities
            .insert(read.source.identity(), LoadedIdentity::AssetSource);
        acquisition.source_facts.insert(
            read.source.project_path().as_str().to_owned(),
            Arc::clone(&read.source),
        );
        assets.insert(
            record.path().clone(),
            AuthoringAssetSource {
                record,
                metadata,
                metadata_source,
                source: read.source,
                bytes: Arc::from(read.bytes),
            },
        );
        return Ok(None);
    }
    let native = NativeDocumentSourcePolicy::new(acquisition.schema.settings()).admit_budgeted(
        &acquisition.schema.project_root,
        opened.project_path.as_str(),
        nara_scene::SourceDocumentKind::Scene,
        opened.identity,
        depth,
        5,
        budget,
    )?;
    if let Some(observed) = native.metadata_source() {
        if !observed.same_file_observation(&metadata_source) {
            return Err(changed(observed));
        }
    }
    let mut matches = open_scenes.iter().filter(|snapshot| {
        snapshot.project_path.as_ref() == opened.project_path.as_str()
            || &snapshot.source_key == native.key()
    });
    let selected = matches.next();
    if matches.next().is_some() {
        return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
            kind,
            path: opened.project_path,
        });
    }
    let (source, document, revision) = if let Some(snapshot) = selected {
        let Some(snapshot_source) = snapshot.source.as_ref() else {
            return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                kind,
                path: opened.project_path,
            });
        };
        if !snapshot.source_matches
            || !snapshot.persistence_available
            || snapshot_source.source_key() != native.key()
            || snapshot_source.relative_path() != opened.project_path.as_str()
            || snapshot_source.identity() != opened.identity
            || snapshot_source.parent_identity() != opened.parent_identity
            || !matching_metadata(
                snapshot_source.native_source().metadata_source(),
                native.metadata_source(),
            )
        {
            return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                kind,
                path: opened.project_path,
            });
        }
        if snapshot.basis.as_ref() != Some(acquisition.basis) {
            return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
        }
        let fact_bytes = checked_sum([
            size_of::<AuthoringSourceFact>(),
            size_of::<Arc<AuthoringSourceFact>>(),
            opened.project_path.as_str().len(),
        ])?;
        budget.charge_source_bytes(snapshot_source.encoded_bytes(), 0, 0, fact_bytes)?;
        let source = Arc::new(AuthoringSourceFact {
            kind,
            project_path: opened.project_path,
            identity: snapshot_source.identity(),
            parent_identity: snapshot_source.parent_identity(),
            digest: snapshot_source.digest(),
            encoded_bytes: snapshot_source.encoded_bytes(),
            dependency_depth: depth,
        });
        // Move the input's modeled document custody into the captured closure.
        budget.reconcile_retained_reservation(snapshot.document.retained_bytes()?, 0)?;
        budget.charge_source_bytes(0, 0, 0, snapshot.document.retained_bytes()?)?;
        (
            source,
            Arc::clone(&snapshot.document),
            Some(snapshot.revision),
        )
    } else {
        let read = read_unique_source(
            opened,
            acquisition.scene_limits.encoded_bytes().get(),
            budget,
        )?;
        let reservation = reserve_composition_decode(&read, acquisition.scene_limits, budget)?;
        let limits = reservation.limits;
        let path = read.source.project_path().as_str();
        let candidate = if path.ends_with(".ron") {
            nara_scene::SceneDocumentCandidate::decode_ron_bytes_with_limits(&read.bytes, limits)
        } else if path.ends_with(".json") {
            nara_scene::SceneDocumentCandidate::decode_json_bytes_with_limits(&read.bytes, limits)
        } else {
            return Err(
                AuthoringProjectSourceAdmissionError::UnsupportedSourceEncoding {
                    kind,
                    path: read.source.project_path().clone(),
                },
            );
        };
        let candidate = candidate.map_err(|source| reservation.format_error(&read, source))?;
        let document = candidate.into_composition_source().map_err(|source| {
            AuthoringProjectSourceAdmissionError::SourceStructure {
                kind,
                path: read.source.project_path().clone(),
                source,
            }
        })?;
        let bytes = document.retained_bytes()?;
        budget.reconcile_retained_reservation(
            reservation.bytes,
            checked_sum([
                bytes,
                source_fact_retained_bytes(&read.source)?,
                size_of::<Arc<SceneDocument>>(),
            ])?,
        )?;
        budget.charge_source_bytes(0, bytes, checked_sum([bytes, read.bytes.len()])?, 0)?;
        (read.source, Arc::new(document), None)
    };
    budget.charge_source_bytes(
        0,
        0,
        0,
        checked_sum([
            size_of::<AuthoringSceneSource>(),
            source.project_path().as_str().len(),
            64,
        ])?,
    )?;
    acquisition
        .identities
        .insert(source.identity(), LoadedIdentity::ContainingDocument);
    acquisition.source_facts.insert(
        source.project_path().as_str().to_owned(),
        Arc::clone(&source),
    );
    Ok(Some(AuthoringSceneSource {
        native_source: native,
        source,
        document,
        compiled: None,
        root_effective: None,
        revision,
        diagnostics: DiagnosticReport::default(),
        failure: None,
    }))
}

/// Actual selected metadata is needed even when a self-edge reuses the captured root data.
pub(super) fn load_resource_metadata(
    acquisition: &mut prefabs::AuthoringPrefabAcquisition<'_>,
    selection: &ResolvedResourceSource,
    depth: usize,
    database: &mut ProjectAssetDatabase,
) -> Result<
    (
        DirectoryCapability,
        AssetRecord,
        AssetMetaIdentityCandidate,
        Arc<AuthoringSourceFact>,
    ),
    AuthoringProjectSourceAdmissionError,
> {
    let binding = &selection.binding;
    let path = &selection.path;
    let budget = &mut *acquisition.budget;
    let directory = open_source_directory(
        &acquisition.schema.project_root,
        &binding.directory,
        AuthoringSourceKind::AssetSource,
        budget,
    )?;
    let metadata_path = path.meta_path();
    let opened = open_source_file(
        &directory,
        join_project_path(&binding.directory, &metadata_path)?,
        &metadata_path,
        AuthoringSourceKind::AssetMetadata,
        depth,
        budget,
    )?;
    let read = read_unique_source(
        opened,
        AssetMetaFileLimits::default().encoded_bytes().get(),
        budget,
    )?;
    acquisition
        .catalogue
        .verify_metadata(path, binding, &read.source)?;
    if let Some(existing) = acquisition
        .source_facts
        .get(read.source.project_path().as_str())
    {
        if !existing.same_file_observation(&read.source) {
            return Err(changed(&read.source));
        }
    } else if acquisition.identities.contains_key(&read.source.identity()) {
        return Err(
            AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                path: read.source.project_path().clone(),
            },
        );
    }
    let encoded = read.bytes.len();
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes_owned_with_limits(
        read.bytes.into_boxed_slice(),
        AssetMetaFileLimits::default().with_encoded_bytes(
            ByteLimit::new(read.read_limit)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        ),
    )
    .map_err(
        |source| AuthoringProjectSourceAdmissionError::AssetMetadataFormat {
            path: binding.scope.resolve(path),
            source,
        },
    )?;
    if metadata.identity().path() != path {
        return Err(
            AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch {
                requested: path.clone(),
                declared: metadata.identity().path().clone(),
            },
        );
    }
    if binding
        .expected_kind
        .as_ref()
        .is_some_and(|kind| kind != metadata.identity().source_kind())
    {
        return Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged {
            path: binding.scope.resolve(path),
        });
    }
    let record = metadata.identity().clone().into_record(&binding.scope);
    charge_asset_metadata(&read.source, encoded, &metadata, &record, budget)?;
    charge_database_record(&record, budget)?;
    insert_asset_record(database, record.clone())?;
    let metadata_source = read.source;
    acquisition
        .identities
        .insert(metadata_source.identity(), LoadedIdentity::AssetMetadata);
    acquisition.source_facts.insert(
        metadata_source.project_path().as_str().to_owned(),
        Arc::clone(&metadata_source),
    );
    Ok((directory, record, metadata, metadata_source))
}

fn changed(source: &AuthoringSourceFact) -> AuthoringProjectSourceAdmissionError {
    AuthoringProjectSourceAdmissionError::SourceChanged {
        kind: source.kind(),
        path: source.project_path().clone(),
    }
}
