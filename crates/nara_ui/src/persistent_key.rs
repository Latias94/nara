use std::collections::{BTreeMap, btree_map::Entry};

use nara_scene::{
    PrefabProjectionPath, SceneEntityId, SceneEntitySource, SceneOccurrenceKey, SourceDocumentKey,
    SourceDocumentKind, SourceDocumentOrigin,
};
use nara_ui_kernel::{UiKeyError, UiOpaqueId, UiPersistentIdentity};

const MAX_BYTES: usize = nara_ui_kernel::MAX_UI_KEY_BYTES;
const MAX_PROJECTION_STEPS: usize = 32;

#[derive(Default)]
pub(crate) struct PersistentKeyProjection<'a> {
    contexts: BTreeMap<(&'a SourceDocumentKey, &'a SceneOccurrenceKey), UiOpaqueId>,
}

impl<'a> PersistentKeyProjection<'a> {
    pub(crate) fn identity(
        &mut self,
        source: &'a SceneEntitySource,
    ) -> Result<UiPersistentIdentity, UiKeyError> {
        let key = (source.containing_source(), source.occurrence());
        let context = match self.contexts.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(source_context(key.0, key.1)?),
        };
        persistent_identity(
            context,
            &source.source_entity_id,
            source.projection_path.as_ref(),
        )
    }
}

pub(crate) fn source_context(
    source: &SourceDocumentKey,
    occurrence: &SceneOccurrenceKey,
) -> Result<UiOpaqueId, UiKeyError> {
    let source = canonical_source(source)?;
    let mut bytes = vec![1];
    field(&mut bytes, &source)?;
    field(&mut bytes, occurrence.namespace().as_bytes())?;
    field(&mut bytes, occurrence.slot().as_bytes())?;
    UiOpaqueId::new(bytes)
}

pub(crate) fn persistent_identity(
    context: &UiOpaqueId,
    entity: &SceneEntityId,
    projection: Option<&PrefabProjectionPath>,
) -> Result<UiPersistentIdentity, UiKeyError> {
    let mut bytes = vec![u8::from(projection.is_some())];
    if let Some(path) = projection {
        if path.source_chain().len() > MAX_PROJECTION_STEPS {
            return Err(UiKeyError::ProjectionDepthExceeded {
                maximum: MAX_PROJECTION_STEPS,
            });
        }
        bytes.push(path.source_chain().len() as u8);
        field(&mut bytes, path.root_anchor().as_str().as_bytes())?;
        for step in path.source_chain() {
            field(&mut bytes, &canonical_source(step.canonical_source())?)?;
            field(&mut bytes, step.source_entity().as_str().as_bytes())?;
        }
    }
    field(&mut bytes, entity.as_str().as_bytes())?;
    Ok(UiPersistentIdentity::new(
        context.clone(),
        UiOpaqueId::new(bytes)?,
    ))
}

fn canonical_source(source: &SourceDocumentKey) -> Result<Vec<u8>, UiKeyError> {
    let kind = match source.kind() {
        SourceDocumentKind::Scene => 0,
        SourceDocumentKind::Prefab => 1,
    };
    let mut bytes = vec![1, kind];
    match source.origin() {
        SourceDocumentOrigin::Named { universe, name } => {
            bytes.push(0);
            bytes.extend_from_slice(universe.as_uuid().as_bytes());
            field(&mut bytes, name.as_bytes())?;
        }
        SourceDocumentOrigin::ProjectLocation { universe, path } => {
            bytes.push(1);
            bytes.extend_from_slice(universe.as_uuid().as_bytes());
            field(&mut bytes, path.as_str().as_bytes())?;
        }
        SourceDocumentOrigin::StableAsset { stable_id } => {
            bytes.push(2);
            bytes.extend_from_slice(stable_id.as_uuid().as_bytes());
        }
    }
    Ok(bytes)
}

fn field(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(), UiKeyError> {
    let actual = bytes.len().saturating_add(2).saturating_add(value.len());
    if actual > MAX_BYTES {
        return Err(UiKeyError::KeyTooLong {
            actual,
            maximum: MAX_BYTES,
        });
    }
    bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_context_preserves_binary_uuid_text_and_field_boundaries() {
        let source = |name| {
            SourceDocumentKey::named(
                "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
                name,
                SourceDocumentKind::Scene,
            )
            .unwrap()
        };
        let occurrence = SceneOccurrenceKey::new("tools\nseat", "main").unwrap();
        let upper_uuid = SourceDocumentKey::named(
            "72011426-37E2-48BA-BC06-78841468889A".parse().unwrap(),
            "hud\nname",
            SourceDocumentKind::Scene,
        )
        .unwrap();
        let context = source_context(&source("hud\nname"), &occurrence).unwrap();
        assert_eq!(context, source_context(&upper_uuid, &occurrence).unwrap());
        assert!(context.as_bytes().windows(16).any(|bytes| bytes
            == [
                0x72, 0x01, 0x14, 0x26, 0x37, 0xe2, 0x48, 0xba, 0xbc, 0x06, 0x78, 0x84, 0x14, 0x68,
                0x88, 0x9a,
            ]));
        assert_ne!(
            source_context(&source("hud\nname"), &occurrence),
            source_context(&source("hudname"), &occurrence)
        );
        assert_ne!(
            source_context(&source("hud"), &SceneOccurrenceKey::new("a", "bc").unwrap()),
            source_context(&source("hud"), &SceneOccurrenceKey::new("ab", "c").unwrap())
        );
    }

    #[test]
    fn oversized_source_or_complete_context_rejects_without_omitting_coordinates() {
        let occurrence = SceneOccurrenceKey::new("tests", "canvas").unwrap();
        for length in [230, 257] {
            let source = SourceDocumentKey::named(
                "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
                "x".repeat(length),
                SourceDocumentKind::Scene,
            )
            .unwrap();
            assert!(matches!(
                source_context(&source, &occurrence),
                Err(UiKeyError::KeyTooLong {
                    maximum: MAX_BYTES,
                    ..
                })
            ));
        }
    }
}
