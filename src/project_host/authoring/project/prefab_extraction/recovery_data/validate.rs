//! Concrete data coherence. Semantic extraction and complete saved-generation proof remain
//! with the enclosing Project/Core owners under their actual unchanged Schema admission.

use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetMetaIdentityCandidate, AssetPathScope, AssetSourceKind, AssetSourcePath};
use nara_fs::RelativePath;
use nara_project::{NativeDocumentIdentity, ProjectAuthoringState, ProjectManifest};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleLimits, ComponentFieldSchema, ComponentSchema,
    ComponentShapeMember, ComponentValue,
};
use nara_scene::{
    __private::{canonical_prefab_source_matches, canonical_scene_source_matches},
    SourceDocumentKind, SourceDocumentOrigin,
};

use super::admission::{
    adaptive_reservation, aggregate, check_aggregate, check_counts, ordering_work,
    with_small_document, with_temporary,
};
use super::*;
use crate::project_host::authoring::source_decode::{SourceDecodeError, SourceTree, decode_source};

pub(super) fn validate_payload(
    payload: &RecoveryPayload,
    limits: PrefabExtractionRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), PrefabExtractionRecoveryError> {
    check_counts(
        payload.complete_original_sources.len(),
        payload.metadata_guards.len(),
        payload.roots.len(),
        payload.missing_sidecars.len(),
        limits,
    )?;
    let bytes = aggregate(payload)?;
    check_aggregate(bytes, limits)?;
    budget.charge_source_bytes(bytes, bytes, checked_mul(bytes, 4)?, 0)?;
    for source in &payload.complete_original_sources {
        if (source.disposition == RecoverySourceDisposition::Guard) != source.output.is_none() {
            return Err(PrefabExtractionRecoveryError::DispositionInvalid);
        }
    }
    validate_paths(payload, budget)?;
    for blob in payload.blobs() {
        blob.validate()?;
    }
    let manifest_bytes = checked_add(
        payload.old_ready_manifest.bytes.len(),
        payload.final_ready_manifest.bytes.len(),
    )?;
    with_small_document(manifest_bytes, budget, |budget| {
        let old = parse_ready(payload.old_ready_manifest.bytes())?;
        let final_ready = parse_ready(payload.final_ready_manifest.bytes())?;
        // Formatting is intentionally irrelevant to configuration equality. The fixed final
        // bytes can be the original manifest, preserving its comments and authored spelling.
        if old != final_ready
            || old.project.source_universe != payload.source_universe
            || old.authoring.schema_bundle.as_deref()
                != Some(payload.unchanged_bundle.path.as_str())
        {
            return Err(PrefabExtractionRecoveryError::ConfigurationMismatch);
        }
        validate_keys(payload, &old, budget)?;
        validate_configurations_and_metadata(payload, &old, manifest_bytes, budget)
    })?;
    validate_bundle(&payload.unchanged_bundle.blob, budget)?;
    for source in &payload.complete_original_sources {
        let selected = source.key == payload.selection.source;
        validate_source(
            &source.path,
            &source.key,
            source.encoding,
            source.original_disk.bytes(),
            None,
            None,
            limits.scene,
            budget,
        )?;
        let save_current = source.disposition == RecoverySourceDisposition::SaveCurrent;
        let expected = if save_current {
            source.output.as_ref().map(RecoveryBlob::bytes)
        } else {
            None
        };
        let found = validate_source(
            &source.path,
            &source.key,
            source.encoding,
            source.reviewed_working.bytes(),
            expected,
            selected.then_some(&payload.selection.root),
            limits.scene,
            budget,
        )
        .map_err(|error| match error {
            PrefabExtractionRecoveryError::NonCanonicalOutput if save_current => {
                PrefabExtractionRecoveryError::SaveCurrentMismatch
            }
            other => other,
        })?;
        if selected && !found {
            return Err(PrefabExtractionRecoveryError::SelectionInvalid);
        }
        if let Some(output) = source.output.as_ref().filter(|_| !save_current) {
            validate_source(
                &source.path,
                &source.key,
                source.encoding,
                output.bytes(),
                Some(output.bytes()),
                None,
                limits.scene,
                budget,
            )?;
        }
    }
    validate_source(
        &payload.q.path,
        &payload.q.key,
        payload.q.encoding,
        payload.q.output.bytes(),
        Some(payload.q.output.bytes()),
        None,
        limits.scene,
        budget,
    )?;
    Ok(())
}

fn parse_ready(bytes: &[u8]) -> Result<ProjectManifest, PrefabExtractionRecoveryError> {
    let loaded = ProjectManifest::parse_toml_bytes(bytes);
    if loaded.has_errors() {
        return Err(PrefabExtractionRecoveryError::ManifestInvalid);
    }
    let manifest = loaded
        .manifest
        .ok_or(PrefabExtractionRecoveryError::ManifestInvalid)?;
    if manifest.authoring.state != ProjectAuthoringState::Ready {
        return Err(PrefabExtractionRecoveryError::ManifestInvalid);
    }
    Ok(manifest)
}

fn validate_path(
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), PrefabExtractionRecoveryError> {
    if path.contains('\\') || path.split('/').any(|part| matches!(part, "" | "." | "..")) {
        return Err(PrefabExtractionRecoveryError::PathInvalid);
    }
    let checked =
        RelativePath::preflight(path).map_err(|_| PrefabExtractionRecoveryError::PathInvalid)?;
    budget.charge_reference(path.len(), checked.components(), false)?;
    budget.charge_source_bytes(0, 0, path.len(), 0)?;
    Ok(())
}

fn sorted<'a>(paths: impl Iterator<Item = &'a str>) -> Result<(), PrefabExtractionRecoveryError> {
    let mut previous = None;
    for path in paths {
        if previous.is_some_and(|previous| previous >= path) {
            return Err(PrefabExtractionRecoveryError::PathConflict);
        }
        previous = Some(path);
    }
    Ok(())
}

fn below(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn sidecar_of(path: &str, source: &str) -> bool {
    path.strip_suffix(".meta") == Some(source)
}

fn first_missing_component<'a>(
    path: &'a str,
    ancestor: &str,
) -> Result<&'a str, PrefabExtractionRecoveryError> {
    let suffix = if ancestor.is_empty() {
        path
    } else {
        path.strip_prefix(ancestor)
            .and_then(|suffix| suffix.strip_prefix('/'))
            .ok_or(PrefabExtractionRecoveryError::RootConflict)?
    };
    let component_end = path.len() - suffix.len() + suffix.find('/').unwrap_or(suffix.len());
    Ok(&path[..component_end])
}

fn validate_paths(
    payload: &RecoveryPayload,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), PrefabExtractionRecoveryError> {
    let paths = || {
        payload
            .file_paths()
            .chain(payload.missing_sidecars.iter().map(String::as_str))
            .chain(payload.roots.iter().map(|root| root.path.as_str()))
            .chain(
                payload
                    .roots
                    .iter()
                    .filter_map(|root| match &root.observation {
                        RecoveryRootObservation::Missing { ancestor } if !ancestor.is_empty() => {
                            Some(ancestor.as_str())
                        }
                        _ => None,
                    }),
            )
    };
    let mut prefix_bytes = 0;
    let mut prefixes = 0;
    for path in paths() {
        validate_path(path, budget)?;
        for end in path
            .match_indices('/')
            .map(|(end, _)| end)
            .chain(std::iter::once(path.len()))
        {
            prefix_bytes = checked_add(prefix_bytes, end)?;
            prefixes = checked_add(prefixes, 1)?;
        }
    }
    let folded_bytes = checked_mul(prefix_bytes, 3)?;
    budget.charge_source_bytes(0, 0, ordering_work(prefixes, folded_bytes)?, 0)?;
    with_temporary(
        checked_add(folded_bytes, checked_mul(prefixes, 384)?)?,
        budget,
        |budget| {
            // Include directory spellings, so Prefabs/a and prefabs/b conflict even when their
            // complete lowercased file paths differ. This is the inventory's Rust lowercase policy,
            // not a claim to emulate every native filesystem's case/normalization equivalence.
            let mut spellings = BTreeMap::new();
            for path in paths() {
                for end in path
                    .match_indices('/')
                    .map(|(end, _)| end)
                    .chain(std::iter::once(path.len()))
                {
                    let spelling = &path[..end];
                    if spellings
                        .insert(spelling.to_lowercase(), spelling)
                        .is_some_and(|old| old != spelling)
                    {
                        return Err(PrefabExtractionRecoveryError::PathConflict);
                    }
                }
            }
            sorted(
                payload
                    .complete_original_sources
                    .iter()
                    .map(|source| source.path.as_str()),
            )?;
            sorted(
                payload
                    .metadata_guards
                    .iter()
                    .map(|guard| guard.path.as_str()),
            )?;
            sorted(payload.roots.iter().map(|root| root.path.as_str()))?;
            sorted(payload.missing_sidecars.iter().map(String::as_str))?;
            let mut files = BTreeSet::new();
            for path in payload
                .file_paths()
                .chain(payload.missing_sidecars.iter().map(String::as_str))
            {
                if !files.insert(path) {
                    return Err(PrefabExtractionRecoveryError::PathConflict);
                }
            }
            for path in &files {
                if path
                    .match_indices('/')
                    .any(|(end, _)| files.contains(&path[..end]))
                {
                    return Err(PrefabExtractionRecoveryError::PathConflict);
                }
            }
            let path_work = checked_add(prefix_bytes, checked_mul(payload.roots.len(), 4)?)?;
            budget.charge_source_bytes(
                0,
                0,
                checked_mul(path_work, checked_add(payload.roots.len(), 1)?)?,
                0,
            )?;
            for root in &payload.roots {
                if files.contains(root.path.as_str())
                    || root
                        .path
                        .match_indices('/')
                        .any(|(end, _)| files.contains(&root.path[..end]))
                {
                    return Err(PrefabExtractionRecoveryError::RootConflict);
                }
                if let RecoveryRootObservation::Missing { ancestor } = &root.observation {
                    let missing = first_missing_component(&root.path, ancestor)?;
                    // Every file role has an existing captured parent, including absent Q
                    // outputs. Its leaf is not evidence of an existing directory. Missing
                    // observations also assert that their exact ancestor already exists.
                    if payload.file_paths().any(|path| below(path, missing))
                        || payload.roots.iter().any(|other| {
                            let existing = match &other.observation {
                                RecoveryRootObservation::Present => other.path.as_str(),
                                RecoveryRootObservation::Missing { ancestor } => ancestor.as_str(),
                            };
                            existing == missing || below(existing, missing)
                        })
                    {
                        return Err(PrefabExtractionRecoveryError::RootConflict);
                    }
                }
            }
            Ok(())
        },
    )
}

fn validate_keys(
    payload: &RecoveryPayload,
    manifest: &ProjectManifest,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), PrefabExtractionRecoveryError> {
    if payload.q.key.kind() != SourceDocumentKind::Prefab {
        return Err(PrefabExtractionRecoveryError::DestinationInvalid);
    }
    let key_bytes = payload
        .complete_original_sources
        .iter()
        .try_fold(payload.q.key.retained_bytes(), |bytes, source| {
            checked_add(bytes, source.key.retained_bytes())
        })?;
    let count = checked_add(payload.complete_original_sources.len(), 1)?;
    let work = ordering_work(
        count,
        checked_add(key_bytes, payload.selection.source.retained_bytes())?,
    )?;
    budget.charge_source_bytes(0, 0, checked_mul(work, 2)?, 0)?;
    with_temporary(
        checked_add(key_bytes, checked_mul(count, 384)?)?,
        budget,
        |_| {
            let mut keys = BTreeSet::new();
            let mut stable_ids = BTreeSet::new();
            for (path, key) in payload
                .complete_original_sources
                .iter()
                .map(|source| (source.path.as_str(), &source.key))
                .chain(std::iter::once((payload.q.path.as_str(), &payload.q.key)))
            {
                if !keys.insert(key) {
                    return Err(PrefabExtractionRecoveryError::KeyConflict);
                }
                match (manifest.project.native_document_identity, key.origin()) {
                    (
                        NativeDocumentIdentity::ProjectLocation,
                        SourceDocumentOrigin::ProjectLocation {
                            universe,
                            path: key_path,
                        },
                    ) if *universe == payload.source_universe && key_path.as_str() == path => {}
                    (
                        NativeDocumentIdentity::StableMetadata,
                        SourceDocumentOrigin::StableAsset { stable_id },
                    ) if !stable_id.as_uuid().is_nil() && stable_ids.insert(*stable_id) => {}
                    _ => return Err(PrefabExtractionRecoveryError::KeyConflict),
                }
            }
            if payload.selection.source.kind() != SourceDocumentKind::Prefab
                || !payload
                    .complete_original_sources
                    .iter()
                    .any(|source| source.key == payload.selection.source)
            {
                return Err(PrefabExtractionRecoveryError::SelectionInvalid);
            }
            Ok(())
        },
    )
}

fn validate_configurations_and_metadata(
    payload: &RecoveryPayload,
    manifest: &ProjectManifest,
    manifest_bytes: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), PrefabExtractionRecoveryError> {
    if payload
        .selected_profile
        .as_ref()
        .is_some_and(|name| !manifest.profiles.contains_key(name))
    {
        return Err(PrefabExtractionRecoveryError::SelectedProfileInvalid);
    }
    let count = checked_add(manifest.profiles.len(), 1)?;
    budget.charge_source_bytes(
        0,
        0,
        checked_mul(manifest_bytes, checked_add(count, 2)?)?,
        0,
    )?;
    let selected = manifest
        .resolve_profile(payload.selected_profile.as_deref())
        .map_err(|_| PrefabExtractionRecoveryError::SelectedProfileInvalid)?;
    if selected.profile_name.as_deref() != payload.selected_profile.as_deref() {
        return Err(PrefabExtractionRecoveryError::SelectedProfileInvalid);
    }
    let selected_prefabs = AssetPathScope::under(
        AssetSourcePath::new(selected.paths.prefabs.as_str())
            .map_err(|_| PrefabExtractionRecoveryError::DestinationInvalid)?,
    );
    drop(selected);
    let mut scopes = Vec::with_capacity(checked_mul(count, 4)?);
    for profile in
        std::iter::once(None).chain(manifest.profiles.keys().map(|name| Some(name.as_str())))
    {
        let settings = manifest
            .resolve_profile(profile)
            .map_err(|_| PrefabExtractionRecoveryError::ConfigurationMismatch)?;
        for path in [
            &settings.paths.assets,
            &settings.paths.scenes,
            &settings.paths.prefabs,
        ] {
            if !payload.roots.iter().any(|root| root.path == path.as_str()) {
                return Err(PrefabExtractionRecoveryError::RootConflict);
            }
        }
        for path in [
            &settings.paths.assets,
            &settings.paths.scenes,
            &settings.paths.prefabs,
            &settings.paths.scripts,
        ] {
            scopes.push(AssetPathScope::under(
                AssetSourcePath::new(path.as_str())
                    .map_err(|_| PrefabExtractionRecoveryError::ConfigurationMismatch)?,
            ));
        }
    }
    let source_path_bytes = payload
        .complete_original_sources
        .iter()
        .try_fold(0, |bytes, source| checked_add(bytes, source.path.len()))?;
    let relation_count = checked_add(
        payload.metadata_guards.len(),
        payload.missing_sidecars.len(),
    )?;
    let relation_work = checked_mul(
        checked_add(source_path_bytes, payload.q.path.len())?,
        checked_add(relation_count, 1)?,
    )?;
    budget.charge_source_bytes(0, 0, checked_mul(relation_work, 4)?, 0)?;
    for source in &payload.complete_original_sources {
        let metadata = payload
            .metadata_guards
            .iter()
            .any(|guard| sidecar_of(&guard.path, &source.path));
        let missing = payload
            .missing_sidecars
            .iter()
            .any(|path| sidecar_of(path, &source.path));
        if metadata == missing
            || (manifest.project.native_document_identity == NativeDocumentIdentity::StableMetadata
                && !metadata)
        {
            return Err(PrefabExtractionRecoveryError::MetadataConflict);
        }
    }
    for missing in &payload.missing_sidecars {
        if !payload
            .complete_original_sources
            .iter()
            .any(|source| sidecar_of(missing, &source.path))
        {
            return Err(PrefabExtractionRecoveryError::MetadataConflict);
        }
    }
    if !sidecar_of(&payload.q_sidecar.path, &payload.q.path) {
        return Err(PrefabExtractionRecoveryError::DestinationInvalid);
    }
    // Stable identities are unique even in project-location mode, where the key itself carries
    // only the physical path. This includes non-source metadata guards and the new Q sidecar.
    let ids = checked_add(payload.metadata_guards.len(), 1)?;
    budget.charge_source_bytes(0, 0, ordering_work(ids, checked_mul(ids, 32)?)?, 0)?;
    with_temporary(checked_mul(ids, 192)?, budget, |budget| {
        let mut stable_ids = BTreeSet::new();
        for (path, blob, new_q) in payload
            .metadata_guards
            .iter()
            .map(|guard| (guard.path.as_str(), &guard.observed, false))
            .chain(std::iter::once((
                payload.q_sidecar.path.as_str(),
                &payload.q_sidecar.output,
                true,
            )))
        {
            let source_path = path
                .strip_suffix(".meta")
                .ok_or(PrefabExtractionRecoveryError::MetadataConflict)?;
            with_small_document(blob.bytes.len(), budget, |budget| {
                let candidate = AssetMetaIdentityCandidate::decode_json_bytes(blob.bytes())
                    .map_err(|_| PrefabExtractionRecoveryError::MetadataConflict)?;
                let local_bytes = candidate.identity().path().as_str().len();
                let path_bytes = scopes.iter().try_fold(0, |bytes, scope| {
                    checked_add(
                        bytes,
                        scope.prefix().map_or(0, |prefix| prefix.as_str().len()),
                    )
                })?;
                let work = checked_add(
                    path_bytes,
                    checked_mul(scopes.len(), checked_add(local_bytes, source_path.len())?)?,
                )?;
                budget.charge_source_bytes(0, 0, checked_mul(work, 4)?, 0)?;
                let identity = candidate.identity();
                if identity.stable_id().as_uuid().is_nil()
                    || !stable_ids.insert(identity.stable_id())
                {
                    return Err(PrefabExtractionRecoveryError::MetadataConflict);
                }
                let record = if new_q {
                    let record = identity.clone().into_record(&selected_prefabs);
                    if record.path().as_str() != payload.q.path
                        || record.source_kind() != &AssetSourceKind::Prefab
                    {
                        return Err(PrefabExtractionRecoveryError::DestinationInvalid);
                    }
                    record
                        .validate_source_document_key(&payload.q.key)
                        .map_err(|_| PrefabExtractionRecoveryError::DestinationInvalid)?;
                    record
                } else {
                    let record = scopes
                        .iter()
                        .find_map(|scope| {
                            let record = identity.clone().into_record(scope);
                            (record.path().as_str() == source_path).then_some(record)
                        })
                        .ok_or(PrefabExtractionRecoveryError::MetadataConflict)?;
                    if let Some(source) = payload
                        .complete_original_sources
                        .iter()
                        .find(|source| source.path == source_path)
                    {
                        record
                            .validate_source_document_key(&source.key)
                            .map_err(|_| PrefabExtractionRecoveryError::MetadataConflict)?;
                    } else if matches!(
                        record.source_kind(),
                        AssetSourceKind::Scene | AssetSourceKind::Prefab
                    ) {
                        return Err(PrefabExtractionRecoveryError::MetadataConflict);
                    }
                    record
                };
                // The record/candidate are data-only temporaries. Native location admission and
                // configuration-local catalogue visibility are still the real Host's duties.
                drop(record);
                Ok(())
            })?;
        }
        Ok(())
    })
}

fn validate_bundle(
    blob: &RecoveryBlob,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), PrefabExtractionRecoveryError> {
    let limits = AuthoringSchemaBundleLimits::default();
    let node = [
        size_of::<ComponentSchema>(),
        size_of::<ComponentFieldSchema>(),
        size_of::<ComponentShapeMember>(),
        size_of::<ComponentValue>(),
        192,
    ]
    .into_iter()
    .try_fold(0, checked_add)?;
    // There is exactly one unchanged data bundle. No successor, Adapter or fresh nominal basis
    // is constructed here; the bundle owner's grammar/catalog checks do not grant Schema authority.
    let (storage, shape) = adaptive_reservation(
        blob.bytes.len(),
        limits.shape(),
        checked_mul(node, 3)?,
        12,
        budget,
    )?;
    budget.charge_source_bytes(0, 0, checked_mul(blob.bytes.len(), 4)?, 0)?;
    with_temporary(storage, budget, |_| {
        let bundle = AuthoringSchemaBundle::from_json_bytes_with_limits(
            blob.bytes(),
            limits.with_shape(shape),
        )
        .map_err(|_| PrefabExtractionRecoveryError::BundleInvalid)?;
        drop(bundle);
        Ok(())
    })
}

/// A source decode returns only a small boolean, so its actual temporary trees and serializer
/// indices are destroyed before the complete reservation is released, including on refusal.
fn validate_source(
    path: &str,
    key: &SourceDocumentKey,
    encoding: RecoverySourceEncoding,
    bytes: &[u8],
    canonical_expected: Option<&[u8]>,
    selected_root: Option<&SceneEntityId>,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<bool, PrefabExtractionRecoveryError> {
    let encoding_matches = match encoding {
        RecoverySourceEncoding::Json => path.ends_with(".json"),
        RecoverySourceEncoding::Ron => path.ends_with(".ron"),
    };
    if !encoding_matches {
        return Err(PrefabExtractionRecoveryError::SourceInvalid);
    }
    let before = budget.retained_candidate_bytes;
    let result = (|| {
        let tree = decode_source(key.kind(), encoding.into(), bytes, limits, budget)
            .map_err(source_decode_error)?;
        if canonical_expected.is_some() {
            tree.admit_canonical_comparison(bytes.len(), budget)
                .map_err(source_decode_error)?;
        }
        match tree {
            SourceTree::Scene(document) => {
                if let Some(expected) = canonical_expected {
                    if !canonical_scene_source_matches(document, encoding.into(), expected, limits)
                        .map_err(|_| PrefabExtractionRecoveryError::SourceInvalid)?
                    {
                        return Err(PrefabExtractionRecoveryError::NonCanonicalOutput);
                    }
                }
                Ok(false)
            }
            SourceTree::Prefab(document) => {
                let found = selected_root
                    .is_some_and(|root| document.entities.iter().any(|entity| &entity.id == root));
                if let Some(expected) = canonical_expected {
                    if !canonical_prefab_source_matches(document, encoding.into(), expected, limits)
                        .map_err(|_| PrefabExtractionRecoveryError::SourceInvalid)?
                    {
                        return Err(PrefabExtractionRecoveryError::NonCanonicalOutput);
                    }
                }
                Ok(found)
            }
        }
    })();
    let temporary = budget
        .retained_candidate_bytes
        .checked_sub(before)
        .ok_or(PrefabExtractionRecoveryError::AccountingOverflow)?;
    budget
        .reconcile_retained_reservation(temporary, 0)
        .map_err(|_| PrefabExtractionRecoveryError::AccountingOverflow)?;
    result
}

fn source_decode_error(error: SourceDecodeError) -> PrefabExtractionRecoveryError {
    use super::super::super::AuthoringProjectSourceAdmissionError;
    match error {
        SourceDecodeError::Admission(error)
        | SourceDecodeError::Project(AuthoringProjectSourceAdmissionError::Budget(error)) => {
            PrefabExtractionRecoveryError::Admission(error)
        }
        SourceDecodeError::Project(AuthoringProjectSourceAdmissionError::AccountingOverflow) => {
            PrefabExtractionRecoveryError::AccountingOverflow
        }
        SourceDecodeError::Project(_)
        | SourceDecodeError::Format(_)
        | SourceDecodeError::Structure(_) => PrefabExtractionRecoveryError::SourceInvalid,
    }
}
