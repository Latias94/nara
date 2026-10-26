use nara_asset::AssetRef;
use nara_ecs::World;
use nara_reflect::ComponentRegistry;
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabAddition, PrefabDocument, PrefabEntityTarget,
    PrefabInstance, PrefabOverlay, PrefabPlacement, PrefabPlacementParent, PrefabPlacementSlot,
    PrefabProjectionStep, PrefabSourceResolver, SceneDocument, SceneEntityId, SceneEntityRecord,
    SceneEntitySource, SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn anchor(name: &str, source: AssetRef) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(name));
    record.prefab = Some(PrefabInstance {
        source,
        overrides: None,
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

    let retained = resolver.resolve_prefab(&path).unwrap();
    let successor = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]);
    assert_eq!(
        resolver.insert(stable.clone(), key.clone(), successor.clone()),
        Some(recursive.clone())
    );
    assert_eq!(retained.key(), &key);
    assert_eq!(retained.document(), &recursive);
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
        let [step] = projection.steps() else {
            panic!("each projection must contain one source step");
        };
        let PrefabProjectionStep::SourceEntity {
            source_document,
            canonical_source,
            entity,
        } = step
        else {
            panic!("the leaf must originate in the prefab source");
        };
        let expected_request = match projection.root_anchor().as_str() {
            "left" => &path,
            "right" => &stable,
            _ => panic!("unexpected prefab anchor"),
        };
        assert_eq!(source_document, expected_request);
        assert_eq!(canonical_source, &key);
        assert_eq!(entity, &id("leaf"));
        assert_eq!(step.entity_id(), entity);
    }
}

#[test]
fn nested_instance_additions_preserve_tagged_provenance_through_public_spawn() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let universe = "46806b15-ef25-4da2-9983-32078fd5b5a7".parse().unwrap();
    let outer_request = AssetRef::path("prefabs/outer.ron").unwrap();
    let inner_request = AssetRef::path("prefabs/inner.ron").unwrap();
    let outer_key =
        SourceDocumentKey::named(universe, "consumer.outer", SourceDocumentKind::Prefab).unwrap();
    let inner_key =
        SourceDocumentKey::named(universe, "consumer.inner", SourceDocumentKind::Prefab).unwrap();
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            outer_request.clone(),
            outer_key.clone(),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("outer-leaf"))]),
        )
        .with_prefab(
            inner_request.clone(),
            inner_key.clone(),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("inner-leaf"))]),
        );
    let mut inner_overlay =
        PrefabOverlay::new(resolver.resolve_prefab(&inner_request).unwrap().key().clone());
    inner_overlay
        .additions
        .push(PrefabAddition::new(id("inner-added")));
    inner_overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("inner-added")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    let mut nested = PrefabAddition::new(id("nested-added"));
    nested.prefab = Some(PrefabInstance {
        source: inner_request.clone(),
        overrides: Some(inner_overlay),
    });
    let mut outer_overlay =
        PrefabOverlay::new(resolver.resolve_prefab(&outer_request).unwrap().key().clone());
    outer_overlay.additions.push(nested);
    outer_overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("nested-added")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    let anchors = ["outer", "other-outer"].map(|name| {
        let mut record = anchor(name, outer_request.clone());
        record.prefab.as_mut().unwrap().overrides = Some(outer_overlay.clone());
        record
    });
    let scene = SceneDocument::from_ordered_roots(anchors);
    let original = scene.clone();
    let expansion = scene.expand_prefabs(&registry, &resolver);
    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics
    );
    assert_eq!(scene, original);
    let candidate = expansion.candidate.unwrap();
    let addition = PrefabProjectionStep::InstanceAddition {
        entity: id("nested-added"),
    };
    for (local, expected) in [
        (
            "outer-leaf",
            vec![PrefabProjectionStep::SourceEntity {
                source_document: outer_request,
                canonical_source: outer_key,
                entity: id("outer-leaf"),
            }],
        ),
        ("nested-added", vec![addition.clone()]),
        (
            "inner-leaf",
            vec![
                addition.clone(),
                PrefabProjectionStep::SourceEntity {
                    source_document: inner_request,
                    canonical_source: inner_key,
                    entity: id("inner-leaf"),
                },
            ],
        ),
        (
            "inner-added",
            vec![
                addition,
                PrefabProjectionStep::InstanceAddition {
                    entity: id("inner-added"),
                },
            ],
        ),
    ] {
        let provenances = candidate
            .provenance()
            .values()
            .filter(|source| source.local_entity_id().as_str() == local)
            .collect::<Vec<_>>();
        assert_eq!(provenances.len(), 2);
        assert_eq!(
            provenances
                .iter()
                .map(|source| source.projection_path().unwrap().root_anchor().clone())
                .collect::<std::collections::BTreeSet<_>>(),
            [id("outer"), id("other-outer")].into_iter().collect(),
        );
        for provenance in provenances {
            let path = provenance.projection_path().unwrap();
            assert_eq!(path.steps(), expected.as_slice());
            assert_eq!(
                path.steps().last().unwrap().entity_id(),
                provenance.local_entity_id(),
            );
        }
    }

    let expected_provenance = candidate.provenance().clone();
    let source =
        SourceDocumentKey::named(universe, "consumer.scene", SourceDocumentKind::Scene).unwrap();
    let occurrence = SceneOccurrenceKey::new("consumer", "nested-additions").unwrap();
    let candidate = candidate.bind_source(source.clone());
    let mut world = World::new();
    let spawned = nara_scene::spawn_scene(&mut world, &registry, &candidate, &occurrence, None);
    assert!(
        !spawned.diagnostics.has_errors(),
        "{:?}",
        spawned.diagnostics
    );
    let instance = spawned.instance.unwrap();
    assert_eq!(instance.len(), 10);
    let mut query = world.query::<&SceneEntitySource>();
    assert_eq!(query.iter(&world).count(), 10);
    for materialized in query.iter(&world) {
        assert_eq!(materialized.instance_id, instance.instance_id());
        assert_eq!(materialized.containing_source(), &source);
        assert_eq!(materialized.occurrence(), &occurrence);
        assert_eq!(
            materialized.provenance(),
            &expected_provenance[&materialized.entity_id],
        );
    }
}
