use nara_asset::AssetRef;
use nara_reflect::ComponentRegistry;
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, PrefabSourceResolver,
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument, SourceDocumentKey,
    SourceDocumentKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn anchor(name: &str, source: AssetRef) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(name));
    record.prefab = Some(PrefabInstance {
        source,
        overrides: ScenePatchDocument::default(),
    });
    record
}

#[test]
fn canonical_aliases_share_one_document_and_recover_after_cycle_refusal() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let path = AssetRef::path("prefabs/character.ron").unwrap();
    let stable = AssetRef::StableId("602e48c3-50ce-467b-8e0d-80ec1b4f1d2a".parse().unwrap());
    let key = SourceDocumentKey::named(
        "46806b15-ef25-4da2-9983-32078fd5b5a7".parse().unwrap(),
        "consumer.character",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let recursive = PrefabDocument::from_ordered_roots([anchor("cycle", stable.clone())]);
    let mut resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(path.clone(), key.clone(), recursive.clone())
        .with_prefab(stable.clone(), key.clone(), recursive.clone());
    let scene = SceneDocument::from_ordered_roots([
        anchor("left", path.clone()),
        anchor("right", stable.clone()),
    ]);
    let before = resolver.clone();
    let rejected = scene.expand_prefabs(&registry, &resolver);
    assert!(rejected.candidate.is_none());
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-cycle" })
    );
    assert!(
        rejected
            .diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.code().as_str() != "scene.prefab-depth-exceeded" })
    );
    assert_eq!(resolver, before);

    let successor = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]);
    assert_eq!(
        resolver.insert(stable.clone(), key.clone(), successor.clone()),
        Some(recursive)
    );
    let path_source = resolver.resolve_prefab(&path).unwrap();
    let stable_source = resolver.resolve_prefab(&stable).unwrap();
    assert_eq!(path_source.key(), &key);
    assert_eq!(stable_source.key(), &key);
    assert_eq!(path_source.document(), &successor);
    assert!(std::ptr::eq(
        path_source.document(),
        stable_source.document()
    ));

    let expansion = scene.expand_prefabs(&registry, &resolver);
    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics
    );
    let candidate = expansion.candidate.unwrap();
    assert_eq!(candidate.document().entities.len(), 4);
    let projected = candidate
        .provenance()
        .values()
        .filter_map(|source| source.projection_path())
        .collect::<Vec<_>>();
    assert_eq!(projected.len(), 2);
    assert_ne!(projected[0].root_anchor(), projected[1].root_anchor());
    for projection in projected {
        assert_eq!(projection.source_chain().len(), 1);
        let step = &projection.source_chain()[0];
        assert_eq!(step.canonical_source(), &key);
        assert_eq!(step.source_entity(), &id("leaf"));
    }
}
