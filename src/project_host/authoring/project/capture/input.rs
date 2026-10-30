//! Project startup Input facts; executable state remains with the Runtime Input owner.

use nara_input::{ActionMapFileCandidate, ActionMapFileLimits};

use super::*;

struct CapturedInputMetadata {
    candidate: AssetMetaIdentityCandidate,
    source: Arc<AuthoringSourceFact>,
    retained_bytes: usize,
}

pub(super) fn capture(
    acquisition: &mut prefabs::AuthoringPrefabAcquisition<'_>,
    selected: &ProjectPath,
    database: &mut ProjectAssetDatabase,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let root = acquisition.schema.project_root.share();
    let budget = &mut *acquisition.budget;
    budget.charge_reference(selected.as_str().len(), 0, false)?;
    let limits = ActionMapFileLimits::default();
    let source_limit = limits.encoded_bytes().get();
    let (read_limit, _) = budget.read_limit(limits.encoded_bytes().get())?;
    let limits = limits.with_encoded_bytes(
        ByteLimit::new(read_limit)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
    );
    let temporary = limits
        .predecode_work_bytes()
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    let source_storage = source_fact_path_retained_bytes(selected.as_str())?;
    // Reserve both the encoded/decoder peak and the actual source observation before its path
    // copy, file read or source-fact allocation. Encoded/work counters cannot admit memory.
    budget.charge_source_bytes(0, 0, 0, checked_sum([temporary, source_storage])?)?;
    let opened = open_source_file(
        &root,
        selected.clone(),
        selected.as_str(),
        AuthoringSourceKind::InputActionMap,
        0,
        budget,
    )?;
    ensure_unclaimed_source_identity(&acquisition.identities, &opened)?;
    let read = read_unique_source(opened, source_limit, budget)?;
    // The format owner supplies the complete temporary storage ceiling. Four actual JSON
    // walks are work performed; the storage ceiling itself is not a work counter.
    budget.charge_source_bytes(0, 0, checked_product(read.bytes.len(), 4)?, 0)?;
    let file = ActionMapFileCandidate::decode_json_bytes_with_limits(&read.bytes, limits).map_err(
        |source| AuthoringProjectSourceAdmissionError::InputFormat {
            path: selected.clone(),
            source,
        },
    )?;
    let product = file.product();
    budget.charge_source_bytes(0, file.definition().cost().retained_bytes(), 0, 0)?;
    // Authoring retains the original source observation, not another executable definition.
    drop(file);
    drop(read.bytes);
    budget.reconcile_retained_reservation(temporary, 0)?;

    let record_storage = checked_sum([
        size_of::<AssetRecord>(),
        selected.as_str().len(),
        "input-action-map".len(),
    ])?;
    budget.charge_source_bytes(0, 0, 0, record_storage)?;
    let path = AssetSourcePath::new(selected.as_str()).map_err(|source| {
        AuthoringProjectSourceAdmissionError::DocumentSourcePath {
            path: selected.clone(),
            source,
        }
    })?;
    let record = AssetRecord::new(
        product.source(),
        path,
        AssetSourceKind::Other("input-action-map".to_owned()),
    );
    charge_database_record(&record, budget)?;
    let metadata = read_metadata(&root, selected, &record, budget)?;
    acquisition.catalogue.admit_intrinsic_source(
        &record,
        metadata
            .as_ref()
            .map(|metadata| (metadata.candidate.identity(), metadata.source.as_ref())),
        budget,
    )?;
    insert_asset_record(database, record)?;
    acquisition
        .budget
        .reconcile_retained_reservation(record_storage, 0)?;
    include_fact(acquisition, read.source, LoadedIdentity::InputActionMap)?;
    if let Some(metadata) = metadata {
        drop(metadata.candidate);
        acquisition
            .budget
            .reconcile_retained_reservation(metadata.retained_bytes, 0)?;
        include_fact(acquisition, metadata.source, LoadedIdentity::AssetMetadata)?;
    }
    Ok(())
}

fn read_metadata(
    root: &DirectoryCapability,
    selected: &ProjectPath,
    record: &AssetRecord,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Option<CapturedInputMetadata>, AuthoringProjectSourceAdmissionError> {
    let path_bytes = checked_sum([
        selected.as_str().len(),
        ".meta".len(),
        size_of::<ProjectPath>(),
    ])?;
    budget.charge_source_bytes(0, 0, 0, path_bytes)?;
    let path = ProjectPath::new(format!("{}.meta", selected.as_str()))
        .map_err(AuthoringProjectSourceAdmissionError::JoinedSourcePath)?;
    budget.charge_reference(path.as_str().len(), 0, false)?;
    let source_storage = source_fact_path_retained_bytes(path.as_str())?;
    budget.charge_source_bytes(0, 0, 0, source_storage)?;
    let opened = open_source_file(
        root,
        path.clone(),
        path.as_str(),
        AuthoringSourceKind::AssetMetadata,
        0,
        budget,
    );
    let opened = match opened {
        Ok(opened) => opened,
        Err(error) if not_found(&error) => {
            drop(path);
            budget.reconcile_retained_reservation(path_bytes, 0)?;
            budget.reconcile_retained_reservation(source_storage, 0)?;
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let limits = AssetMetaFileLimits::default();
    let source_limit = limits.encoded_bytes().get();
    let (read_limit, _) = budget.read_limit(limits.encoded_bytes().get())?;
    let limits = limits.with_encoded_bytes(
        ByteLimit::new(read_limit)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
    );
    let shape = limits.shape();
    // Finite metadata parsing uses the existing Asset owner, including its identity-first
    // decoder. Reserve parser/encoded overlap and sparse structural nodes before construction.
    let temporary = checked_sum([
        checked_product(limits.encoded_bytes().get(), 4)?,
        checked_product(shape.nodes().get(), 512)?,
        checked_product(shape.total_string_bytes().get(), 2)?,
    ])?;
    budget.charge_source_bytes(0, 0, 0, temporary)?;
    let read = read_unique_source(opened, source_limit, budget)?;
    budget.charge_source_bytes(0, 0, checked_product(read.bytes.len(), 3)?, 0)?;
    let candidate = AssetMetaIdentityCandidate::decode_json_bytes_owned_with_limits(
        read.bytes.into_boxed_slice(),
        limits,
    )
    .map_err(
        |source| AuthoringProjectSourceAdmissionError::AssetMetadataFormat {
            path: record.path().clone(),
            source,
        },
    )?;
    let retained = candidate
        .retained_bytes()
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    budget.reconcile_retained_reservation(temporary, retained)?;
    budget.charge_source_bytes(0, retained, 0, 0)?;
    drop(path);
    budget.reconcile_retained_reservation(path_bytes, 0)?;
    Ok(Some(CapturedInputMetadata {
        candidate,
        source: read.source,
        retained_bytes: retained,
    }))
}

fn include_fact(
    acquisition: &mut prefabs::AuthoringPrefabAcquisition<'_>,
    source: Arc<AuthoringSourceFact>,
    identity_kind: LoadedIdentity,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let path = source.project_path();
    if let Some(existing) = acquisition.source_facts.get(path.as_str()) {
        return if existing.same_file_observation(&source) {
            Ok(())
        } else {
            Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                kind: source.kind(),
                path: path.clone(),
            })
        };
    }
    if acquisition.identities.contains_key(&source.identity()) {
        return Err(
            AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict { path: path.clone() },
        );
    }
    acquisition.budget.charge_source_bytes(
        0,
        0,
        0,
        checked_sum([
            path.as_str().len(),
            size_of::<(FileIdentity, LoadedIdentity)>(),
            size_of::<(String, Arc<AuthoringSourceFact>)>(),
            128,
        ])?,
    )?;
    acquisition
        .identities
        .insert(source.identity(), identity_kind);
    acquisition
        .source_facts
        .insert(path.as_str().to_owned(), source);
    Ok(())
}

/// Existing facts validate present sidecars. An absent optional sidecar is also an observation:
/// its later appearance changes the startup member set and must invalidate the old capture.
pub(super) fn revalidate_metadata_absence<'a>(
    root: &DirectoryCapability,
    sources: impl Iterator<Item = &'a AuthoringSourceFact> + Clone,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    for input in sources
        .clone()
        .filter(|source| source.kind() == AuthoringSourceKind::InputActionMap)
    {
        if sources.clone().any(|source| {
            source.kind() == AuthoringSourceKind::AssetMetadata
                && source.project_path().as_str().strip_suffix(".meta")
                    == Some(input.project_path().as_str())
        }) {
            continue;
        }
        let path_bytes = checked_sum([
            input.project_path().as_str().len(),
            ".meta".len(),
            size_of::<ProjectPath>(),
        ])?;
        budget.charge_source_bytes(0, 0, 0, path_bytes)?;
        let path = ProjectPath::new(format!("{}.meta", input.project_path().as_str()))
            .map_err(AuthoringProjectSourceAdmissionError::JoinedSourcePath)?;
        budget.charge_reference(path.as_str().len(), 0, false)?;
        match open_source_file(
            root,
            path.clone(),
            path.as_str(),
            AuthoringSourceKind::AssetMetadata,
            0,
            budget,
        ) {
            Ok(_) => {
                return Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                    kind: AuthoringSourceKind::AssetMetadata,
                    path,
                });
            }
            Err(error) if not_found(&error) => {}
            Err(error) => return Err(error),
        }
        drop(path);
        budget.reconcile_retained_reservation(path_bytes, 0)?;
    }
    Ok(())
}

fn not_found(error: &AuthoringProjectSourceAdmissionError) -> bool {
    matches!(
        error,
        AuthoringProjectSourceAdmissionError::SourceHost {
            source: FsError::Io { source, .. }, ..
        } if source.kind() == std::io::ErrorKind::NotFound
    )
}
