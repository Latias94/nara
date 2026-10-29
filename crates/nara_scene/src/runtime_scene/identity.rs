use std::{collections::BTreeMap, mem::size_of};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use super::{
    RuntimeSceneError,
    budget::{WorkBudget, check},
};
use crate::{
    PrefabProjectionStep, SceneDocument, SceneEntityId, SceneEntityProvenance,
    SceneEntityProvenanceMap, SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin,
};

const PREFIX: &str = "runtime_v1_";
const DOMAIN: &[u8] = b"nara.runtime_scene.entity.v1";

/// Maps all old IDs exactly once, retaining local spelling and rejecting aliases of logical
/// addresses. Lookup evidence is deliberately absent from the canonical byte stream.
pub(super) fn canonical_ids<'a>(
    source: &SourceDocumentKey,
    document: &'a SceneDocument,
    provenance: &SceneEntityProvenanceMap,
    require_canonical: bool,
    work: &mut WorkBudget,
) -> Result<BTreeMap<&'a SceneEntityId, SceneEntityId>, RuntimeSceneError> {
    validate_key(source, SourceDocumentKind::Scene)?;
    if document.entities.len() != provenance.len() {
        return Err(invalid_provenance());
    }
    // Two tree indexes, including conservative node links, are admitted before allocation.
    work.reserve(document.entities.len().checked_mul(
        2 * (size_of::<SceneEntityId>() + size_of::<&SceneEntityId>() + 6 * size_of::<usize>()),
    ))?;
    let mut targets = BTreeMap::<SceneEntityId, &SceneEntityId>::new();
    // Register preserved locals first so no generated namespace can silently replace one.
    for projected in [false, true] {
        for entity in &document.entities {
            let origin = provenance.get(&entity.id).ok_or_else(invalid_provenance)?;
            if origin.projection_path().is_some() != projected {
                continue;
            }
            let id = if let Some(path) = origin.projection_path() {
                if path.steps().is_empty()
                    || path.steps().last().map(|step| step.entity_id())
                        != Some(origin.local_entity_id())
                    || !provenance.get(path.root_anchor()).is_some_and(|anchor| {
                        anchor.projection_path().is_none()
                            && anchor.local_entity_id() == path.root_anchor()
                    })
                {
                    return Err(invalid_provenance());
                }
                check(
                    "provenance-depth",
                    path.steps().len(),
                    work.limits().provenance_depth().get(),
                )?;
                for step in path.steps() {
                    if let PrefabProjectionStep::SourceEntity {
                        source_document,
                        canonical_source,
                        ..
                    } = step
                    {
                        validate_key(canonical_source, SourceDocumentKind::Prefab)?;
                        // An auxiliary domain product is never an entity-bearing Prefab source.
                        // These checks inspect evidence; they do not resolve it at Runtime.
                        if !source_document.is_primary() {
                            return Err(invalid_provenance());
                        }
                        if let Some(reference) = source_document.as_product() {
                            let stable_id = reference.source();
                            if stable_id.as_uuid().is_nil()
                                || !matches!(canonical_source.origin(), SourceDocumentOrigin::StableAsset { stable_id: key } if *key == stable_id)
                            {
                                return Err(invalid_provenance());
                            }
                        }
                    }
                }
                projected_id(source, origin, work)?
            } else {
                if origin.local_entity_id() != &entity.id {
                    return Err(invalid_provenance());
                }
                check(
                    "identity-string-bytes",
                    entity.id.as_str().len(),
                    work.limits().base().shape().string_bytes().get(),
                )?;
                work.reserve(Some(entity.id.as_str().len()))?;
                entity.id.clone()
            };
            if require_canonical && id != entity.id {
                return Err(RuntimeSceneError::refused(
                    "scene.runtime-noncanonical-entity-id",
                    "Runtime entity ID does not match its complete logical provenance",
                ));
            }
            if let Some(previous) = targets.get(&id) {
                let local = provenance
                    .get(*previous)
                    .is_some_and(|origin| origin.projection_path().is_none());
                return Err(if local && projected {
                    RuntimeSceneError::refused(
                        "scene.runtime-entity-id-collision",
                        "Projected Runtime entity ID collides with a preserved local ID",
                    )
                } else {
                    RuntimeSceneError::refused(
                        "scene.runtime-duplicate-logical-address",
                        "Runtime Scene repeats an entity ID or a complete logical address",
                    )
                });
            }
            targets.insert(id, &entity.id);
        }
    }
    if targets.len() != document.entities.len() {
        return Err(invalid_provenance());
    }
    Ok(targets
        .into_iter()
        .map(|(target, original)| (original, target))
        .collect())
}

fn validate_key(
    key: &SourceDocumentKey,
    kind: SourceDocumentKind,
) -> Result<(), RuntimeSceneError> {
    if key.kind() != kind
        || matches!(key.origin(), SourceDocumentOrigin::StableAsset { stable_id } if stable_id.as_uuid().is_nil())
    {
        return Err(invalid_provenance());
    }
    Ok(())
}

fn invalid_provenance() -> RuntimeSceneError {
    RuntimeSceneError::refused(
        "scene.runtime-invalid-provenance",
        "Runtime Scene provenance must cover every entity with valid typed source ownership",
    )
}

/// Count and write through the same framing routine. Fixed-width length prefixes and explicit
/// tags make this injective; no digest, source alias or generated expanded ID supplies identity.
fn projected_id(
    source: &SourceDocumentKey,
    origin: &SceneEntityProvenance,
    work: &mut WorkBudget,
) -> Result<SceneEntityId, RuntimeSceneError> {
    let mut length = 0usize;
    canonical_address(source, origin, &mut |bytes| {
        length = length
            .checked_add(bytes.len())
            .ok_or_else(identity_overflow)?;
        Ok(())
    })?;
    let encoded_length = base64::encoded_len(length, false)
        .and_then(|length| length.checked_add(PREFIX.len()))
        .ok_or_else(identity_overflow)?;
    check(
        "identity-string-bytes",
        encoded_length,
        work.limits().base().shape().string_bytes().get(),
    )?;
    work.reserve(Some(length))?;
    work.reserve(Some(encoded_length))?;
    let mut address = Vec::with_capacity(length);
    canonical_address(source, origin, &mut |bytes| {
        address.extend_from_slice(bytes);
        Ok(())
    })?;
    let mut id = String::with_capacity(encoded_length);
    id.push_str(PREFIX);
    URL_SAFE_NO_PAD.encode_string(&address, &mut id);
    SceneEntityId::new(id).map_err(|_| identity_overflow())
}

fn canonical_address(
    source: &SourceDocumentKey,
    origin: &SceneEntityProvenance,
    write: &mut impl FnMut(&[u8]) -> Result<(), RuntimeSceneError>,
) -> Result<(), RuntimeSceneError> {
    let path = origin.projection_path().ok_or_else(invalid_provenance)?;
    frame(DOMAIN, write)?;
    source_key(source, write)?;
    frame(path.root_anchor().as_str().as_bytes(), write)?;
    write(
        &u64::try_from(path.steps().len())
            .map_err(|_| identity_overflow())?
            .to_be_bytes(),
    )?;
    for step in path.steps() {
        match step {
            PrefabProjectionStep::SourceEntity {
                canonical_source,
                entity,
                ..
            } => {
                write(&[0])?;
                source_key(canonical_source, write)?;
                frame(entity.as_str().as_bytes(), write)?;
            }
            PrefabProjectionStep::InstanceAddition { entity } => {
                write(&[1])?;
                frame(entity.as_str().as_bytes(), write)?;
            }
        }
    }
    frame(origin.local_entity_id().as_str().as_bytes(), write)
}

fn source_key(
    key: &SourceDocumentKey,
    write: &mut impl FnMut(&[u8]) -> Result<(), RuntimeSceneError>,
) -> Result<(), RuntimeSceneError> {
    write(&[match key.kind() {
        SourceDocumentKind::Scene => 0,
        SourceDocumentKind::Prefab => 1,
    }])?;
    match key.origin() {
        SourceDocumentOrigin::Named { universe, name } => {
            write(&[0])?;
            write(universe.as_uuid().as_bytes())?;
            frame(name.as_bytes(), write)
        }
        SourceDocumentOrigin::ProjectLocation { universe, path } => {
            write(&[1])?;
            write(universe.as_uuid().as_bytes())?;
            frame(path.as_str().as_bytes(), write)
        }
        SourceDocumentOrigin::StableAsset { stable_id } => {
            write(&[2])?;
            write(stable_id.as_uuid().as_bytes())
        }
    }
}

fn frame(
    bytes: &[u8],
    write: &mut impl FnMut(&[u8]) -> Result<(), RuntimeSceneError>,
) -> Result<(), RuntimeSceneError> {
    write(
        &u64::try_from(bytes.len())
            .map_err(|_| identity_overflow())?
            .to_be_bytes(),
    )?;
    write(bytes)
}

fn identity_overflow() -> RuntimeSceneError {
    RuntimeSceneError::refused(
        "scene.runtime-identity-overflow",
        "Runtime Scene canonical identity exceeds representable framed storage",
    )
}
