use std::{convert::Infallible, sync::Arc};

use nara_asset::{
    AssetProductRef, AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath, ImportedProductId,
    StableAssetId,
};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    PrefabDocument, PrefabInstance, ResolvedPrefabSource, SceneDocument, SceneEntityId,
    SceneEntityRecord, SourceDocumentBindingError, SourceDocumentKey, SourceDocumentKind,
};

fn source_id() -> StableAssetId {
    "d0d84c6b-2054-4fc9-8918-292dfe130b88".parse().unwrap()
}

fn location() -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "39f9d09d-63e3-5fc4-8e3f-71e84893cb8d".parse().unwrap(),
        AssetSourcePath::new("content/templates/leaf.prefab.json").unwrap(),
        SourceDocumentKind::Prefab,
    )
}

fn document() -> Arc<PrefabDocument> {
    Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("leaf").unwrap()),
    ]))
}

fn record() -> AssetRecord {
    AssetRecord::new(
        source_id(),
        AssetSourcePath::new("content/templates/leaf.prefab.json").unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn containing_scene(request: AssetRef) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        source: request,
        overrides: None,
    });
    SceneDocument::from_ordered_roots([anchor])
}

#[test]
fn explicit_source_association_keeps_location_identity_for_all_primary_lookup_forms() {
    let key = location();
    let source = ResolvedPrefabSource::bound(document(), key.clone())
        .with_source_asset(&record())
        .unwrap();
    assert_eq!(source.key(), &key);
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let primary = AssetProductRef::new(source_id(), ImportedProductId::PRIMARY).unwrap();
    let mut expected_ids = None;
    for request in [
        AssetRef::path("leaf.prefab.json").unwrap(),
        AssetRef::StableId(source_id()),
        AssetRef::Product(primary),
    ] {
        let scene = containing_scene(request);
        let report = scene
            .try_expand_prefabs_with_loader(&registry, Default::default(), None, |_, _| {
                Ok::<_, Infallible>(Some(source.clone()))
            })
            .unwrap();
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let ids = report
            .document()
            .unwrap()
            .entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 2);
        if let Some(expected) = &expected_ids {
            assert_eq!(&ids, expected);
        }
        expected_ids = Some(ids);
    }
    assert_eq!(source.key(), &key);
}

#[test]
fn product_lookup_needs_the_actual_association_and_rejects_another_source() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let other: StableAssetId = "28d89092-497d-4ee1-92ed-42b669fbef78".parse().unwrap();
    let bound = ResolvedPrefabSource::bound(document(), location())
        .with_source_asset(&record())
        .unwrap();
    for (resolved, selected) in [
        (
            ResolvedPrefabSource::bound(document(), location()),
            source_id(),
        ),
        (bound, other),
    ] {
        let scene = containing_scene(AssetRef::Product(
            AssetProductRef::new(selected, ImportedProductId::PRIMARY).unwrap(),
        ));
        let report = scene
            .try_expand_prefabs_with_loader(&registry, Default::default(), None, |_, _| {
                Ok::<_, Infallible>(Some(resolved.clone()))
            })
            .unwrap();
        assert!(report.candidate.is_none());
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.prefab-source-identity-invalid"
        }));
    }
}

#[test]
fn metadata_association_refuses_conflicting_location_kind_and_stable_identity() {
    let other: StableAssetId = "28d89092-497d-4ee1-92ed-42b669fbef78".parse().unwrap();
    let path = record().path().clone();
    for (key, metadata, expected) in [
        (
            location(),
            AssetRecord::new(source_id(), path.clone(), AssetSourceKind::Image),
            SourceDocumentBindingError::SourceKind,
        ),
        (
            location(),
            AssetRecord::new(
                source_id(),
                AssetSourcePath::new("elsewhere/leaf.prefab.json").unwrap(),
                AssetSourceKind::Prefab,
            ),
            SourceDocumentBindingError::SourceKeyMismatch,
        ),
        (
            SourceDocumentKey::stable_asset(other, SourceDocumentKind::Prefab),
            record(),
            SourceDocumentBindingError::SourceKeyMismatch,
        ),
        (
            SourceDocumentKey::stable_asset(source_id(), SourceDocumentKind::Scene),
            record(),
            SourceDocumentBindingError::DocumentKind,
        ),
    ] {
        assert_eq!(
            ResolvedPrefabSource::bound(document(), key)
                .with_source_asset(&metadata)
                .unwrap_err(),
            expected
        );
    }
}
