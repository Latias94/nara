//! Independent authoring, persistence, compilation and Native topology in one workflow.

use std::{collections::BTreeSet, fs, path::Path};

use nara_asset::AssetRef;
use nara_ecs::{Entity, World};
use nara_hierarchy::{Children, HierarchyRoots, Parent};
use nara_identity::{EntityLookup, SpawnedSceneInstance};
use nara_reflect::{AuthoringSchemaBundle, ComponentRegistryBuilder, ComponentRegistrySnapshot};
use nara_scene::{
    AuthoringDocumentSource, AuthoringSchemaBasis, CompiledSceneDocument,
    InMemoryPrefabSourceResolver, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop,
    PrefabProjectionStep, SceneDocument, SceneDocumentCandidate, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, SceneEntitySource, SceneOccurrenceKey,
    ScenePatchDocument, ScenePlacement, ScenePlacementParent, SourceDocumentKey,
    SourceDocumentKind, spawn_scene,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source(name: &str) -> AssetRef {
    // Named sources have explicit lookup aliases, independent of their file encoding.
    AssetRef::path(format!("prefabs/{name}.prefab")).unwrap()
}

fn key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "c2400bd8-35cb-4d8c-8176-d7386fae77f6".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

fn under(parent: &str) -> ScenePlacementParent {
    ScenePlacementParent::Entity(PrefabEntityTarget::source(id(parent)))
}

fn child(entity: &str, parent: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        PrefabEntityTarget::source(id(entity)),
        ScenePlacement::last(under(parent)),
    )
}

fn q_document() -> PrefabDocument {
    PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("q")),
            child("d1", "q"),
            child("d2", "q"),
        ],
        ["q", "d1", "d2"].map(|name| SceneEntityRecord::new(id(name))),
    )
}

fn p_document() -> PrefabDocument {
    let mut anchor = SceneEntityRecord::new(id("k"));
    anchor.prefab = Some(PrefabInstance::root(
        source("Q"),
        key("Q", SourceDocumentKind::Prefab),
        PrefabEntityTarget::source(id("q")),
    ));
    PrefabDocument::new(
        [SceneEntityPlacement::root(id("k")), child("b", "k")],
        [anchor, SceneEntityRecord::new(id("b"))],
    )
}

fn scene_document() -> SceneDocument {
    SceneDocument::from_ordered_roots(["z_left", "a_right"].map(|name| {
        let mut anchor = SceneEntityRecord::new(id(name));
        anchor.prefab = Some(PrefabInstance::root(
            source("P"),
            key("P", SourceDocumentKind::Prefab),
            PrefabEntityTarget::source(id("k")),
        ));
        anchor
    }))
}

fn admit_prefab(
    candidate: PrefabDocumentCandidate,
    name: &str,
    basis: &AuthoringSchemaBasis,
) -> PrefabAuthoringSession {
    let published = candidate
        .admit_authoring_with_basis(AuthoringDocumentSource::new(source(name)), basis)
        .unwrap()
        .publish();
    PrefabAuthoringSession::from_published_authoring_document(published)
}

fn compile(
    scene: &SceneDocument,
    p: &PrefabDocument,
    q: &PrefabDocument,
    registry: &ComponentRegistrySnapshot,
) -> CompiledSceneDocument {
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(source("P"), key("P", SourceDocumentKind::Prefab), p.clone())
        .with_prefab(source("Q"), key("Q", SourceDocumentKind::Prefab), q.clone());
    let report = scene.compile(registry, &resolver);
    assert!(
        !report.diagnostics.has_errors(),
        "{:#?}",
        report.diagnostics
    );
    assert!(report.failure.is_none(), "{:#?}", report.failure);
    report.candidate.expect("the complete source must compile")
}

fn step(source_name: &str, entity: &str) -> PrefabProjectionStep {
    PrefabProjectionStep::SourceEntity {
        source_document: source(source_name),
        canonical_source: key(source_name, SourceDocumentKind::Prefab),
        entity: id(entity),
    }
}

fn projected_locator(
    document: &CompiledSceneDocument,
    anchor: &SceneEntityId,
    local: &str,
) -> SceneEntityId {
    let expected = match local {
        "b" => vec![step("P", "b")],
        "d1" | "d2" => vec![step("P", "k"), step("Q", local)],
        _ => panic!("unexpected source entity"),
    };
    let mut matches = document
        .canonical_provenance()
        .iter()
        .filter(|(_, provenance)| {
            provenance.local_entity_id() == &id(local)
                && provenance.projection_path().is_some_and(|path| {
                    path.root_anchor() == anchor && path.steps() == expected.as_slice()
                })
        });
    let (locator, _) = matches.next().expect("the canonical occurrence must exist");
    assert!(matches.next().is_none(), "the occurrence must be unique");
    locator.clone()
}

fn assert_order(document: &CompiledSceneDocument, order: [&str; 3]) -> [[SceneEntityId; 4]; 2] {
    let roots = ["z_left", "a_right"].map(id);
    assert_eq!(document.roots(), &roots);
    assert_eq!(document.entities().len(), 8);
    assert_eq!(document.canonical_provenance().len(), 8);
    let occurrences = roots.map(|root| {
        let provenance = &document.canonical_provenance()[&root];
        assert_eq!(provenance.local_entity_id(), &root);
        assert!(provenance.projection_path().is_none());
        let children = order.map(|local| projected_locator(document, &root, local));
        let record = document
            .entities()
            .iter()
            .find(|entity| entity.id() == &root)
            .unwrap();
        assert_eq!(record.children(), &children);
        for child in &children {
            assert!(
                document
                    .entities()
                    .iter()
                    .find(|entity| entity.id() == child)
                    .unwrap()
                    .children()
                    .is_empty()
            );
        }
        let [first, second, third] = children;
        [root, first, second, third]
    });
    assert_eq!(
        occurrences.iter().flatten().collect::<BTreeSet<_>>().len(),
        8,
        "repeated root-bound occurrences must have distinct final identities",
    );
    occurrences
}

fn resolve(world: &World, instance: &SpawnedSceneInstance, locator: &SceneEntityId) -> Entity {
    match instance.resolve(world, locator) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live occurrence entity, got {other:?}"),
    }
}

fn assert_native_spawn(document: CompiledSceneDocument, registry: &ComponentRegistrySnapshot) {
    let locators = assert_order(&document, ["d1", "b", "d2"]);
    let candidate = document.bind_source(key("consumer.scene", SourceDocumentKind::Scene));
    let occurrence = SceneOccurrenceKey::new("consumer.root-binding", "main").unwrap();
    let mut world = World::new();
    let report = spawn_scene(&mut world, registry, &candidate, &occurrence, None);
    assert!(
        !report.diagnostics.has_errors(),
        "{:#?}",
        report.diagnostics
    );
    let instance = report.instance.expect("ordinary Native spawn must succeed");
    assert_eq!(instance.len(), 8);
    assert_eq!(world.query::<&SceneEntitySource>().iter(&world).count(), 8);
    let runtime =
        locators.map(|entities| entities.map(|entity| resolve(&world, &instance, &entity)));
    assert_eq!(runtime.iter().flatten().collect::<BTreeSet<_>>().len(), 8);
    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        &[runtime[0][0], runtime[1][0]],
    );
    for [root, d1, b, d2] in runtime {
        assert!(world.get::<Parent>(root).is_none());
        assert_eq!(
            world.get::<Children>(root).unwrap().as_slice(),
            &[d1, b, d2]
        );
        for child in [d1, b, d2] {
            assert_eq!(world.get::<Parent>(child).unwrap().parent(), root);
            assert!(
                world
                    .get::<Children>(child)
                    .is_none_or(|children| children.as_slice().is_empty())
            );
        }
    }
    for entity in candidate.document().entities() {
        let runtime = resolve(&world, &instance, entity.id());
        let source = world.get::<SceneEntitySource>(runtime).unwrap();
        assert_eq!(&source.entity_id, entity.id());
        assert_eq!(source.instance_id, instance.instance_id());
        assert_eq!(source.containing_source(), candidate.source_key());
        assert_eq!(source.occurrence(), &occurrence);
        assert_eq!(
            source.provenance(),
            &candidate.document().canonical_provenance()[entity.id()],
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum Encoding {
    Json,
    Ron,
}

impl Encoding {
    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Ron => "ron",
        }
    }

    fn reopen_prefab(
        self,
        directory: &Path,
        name: &str,
        document: &PrefabDocument,
        basis: &AuthoringSchemaBasis,
    ) -> PrefabAuthoringSession {
        let encoded = match self {
            Self::Json => document.to_json_string().unwrap(),
            Self::Ron => document.to_ron_string().unwrap(),
        };
        let path = directory.join(format!("{name}.prefab.{}", self.extension()));
        fs::write(&path, encoded).unwrap();
        let saved = fs::read(&path).unwrap();
        let candidate = match self {
            Self::Json => PrefabDocumentCandidate::decode_json_bytes(&saved),
            Self::Ron => PrefabDocumentCandidate::decode_ron_bytes(&saved),
        }
        .unwrap();
        admit_prefab(candidate, name, basis)
    }

    fn reopen_scene(
        self,
        directory: &Path,
        document: &SceneDocument,
        basis: &AuthoringSchemaBasis,
    ) -> SceneDocument {
        let encoded = match self {
            Self::Json => document.to_json_string().unwrap(),
            Self::Ron => document.to_ron_string().unwrap(),
        };
        let path = directory.join(format!("repeated.scene.{}", self.extension()));
        fs::write(&path, encoded).unwrap();
        let saved = fs::read(&path).unwrap();
        let candidate = match self {
            Self::Json => SceneDocumentCandidate::decode_json_bytes(&saved),
            Self::Ron => SceneDocumentCandidate::decode_ron_bytes(&saved),
        }
        .unwrap();
        candidate
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/repeated.scene").unwrap()),
                basis,
            )
            .unwrap()
            .publish()
            .document()
            .clone()
    }
}

#[test]
fn independent_mixed_owner_edit_survives_exact_history_disk_admission_compile_and_native_spawn() {
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[]).unwrap();
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let q = q_document();
    let original = p_document();
    let scene = scene_document();
    assert_eq!(
        q.entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>(),
        ["d1", "d2", "q"].map(id),
    );
    assert_eq!(
        original
            .entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>(),
        ["b", "k"].map(id),
    );
    assert_eq!(
        scene
            .entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>(),
        ["a_right", "z_left"].map(id),
    );
    assert_eq!(q.placements[0], SceneEntityPlacement::root(id("q")));
    assert_eq!(original.placements[0], SceneEntityPlacement::root(id("k")));
    assert_eq!(
        scene.placements[0],
        SceneEntityPlacement::root(id("z_left"))
    );
    let before = compile(&scene, &original, &q, &registry);
    assert_order(&before, ["d1", "d2", "b"]);

    let mut session = admit_prefab(
        PrefabDocumentCandidate::decode_json_str(&original.to_json_string().unwrap()).unwrap(),
        "P",
        &basis,
    );
    let initial_content = session.content_state();
    assert_eq!(session.document(), &original);
    assert_eq!(session.history_status().undo_depth, 0);
    let desired = ScenePlacement::before(
        under("k"),
        PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("k")),
                expected_base: key("Q", SourceDocumentKind::Prefab),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("d2")),
        },
    );
    let mut expected = original.clone();
    expected.placements[1].placement = desired.clone();
    let patch = ScenePatchDocument::set_placement(
        &session.document().placements,
        PrefabEntityTarget::source(id("b")),
        desired,
    );
    let applied = session.apply_authoring_patch(&patch);
    assert!(applied.applied, "{:#?}", applied.diagnostics);
    assert_eq!(session.document(), &expected);
    assert_eq!(session.document().entities, original.entities);
    assert_eq!(session.history_status().undo_depth, 1);
    let edited_content = session.content_state();
    assert_ne!(edited_content, initial_content);
    let edited = compile(&scene, session.document(), &q, &registry);
    assert_order(&edited, ["d1", "b", "d2"]);

    let undone = session.undo_authoring();
    assert!(undone.applied, "{:#?}", undone.diagnostics);
    assert_eq!(session.document(), &original);
    assert_eq!(session.content_state(), initial_content);
    assert_eq!(session.history_status().undo_depth, 0);
    assert_eq!(session.history_status().redo_depth, 1);
    let undo_compiled = compile(&scene, session.document(), &q, &registry);
    assert_eq!(undo_compiled, before);
    assert_order(&undo_compiled, ["d1", "d2", "b"]);

    let redone = session.redo_authoring();
    assert!(redone.applied, "{:#?}", redone.diagnostics);
    assert_eq!(session.document(), &expected);
    assert_eq!(session.content_state(), edited_content);
    assert_eq!(session.history_status().undo_depth, 1);
    assert_eq!(session.history_status().redo_depth, 0);
    assert_eq!(compile(&scene, session.document(), &q, &registry), edited);
    assert_eq!(
        q,
        q_document(),
        "P's edit must leave the complete reusable Q unchanged",
    );

    let directory = tempfile::tempdir().unwrap();
    for encoding in [Encoding::Json, Encoding::Ron] {
        let reopened_p = encoding.reopen_prefab(directory.path(), "P", session.document(), &basis);
        let reopened_q = encoding.reopen_prefab(directory.path(), "Q", &q, &basis);
        let reopened_scene = encoding.reopen_scene(directory.path(), &scene, &basis);
        assert_eq!(reopened_p.document(), &expected, "{encoding:?}");
        assert_eq!(reopened_q.document(), &q, "{encoding:?}");
        assert_eq!(reopened_scene, scene, "{encoding:?}");
        assert_eq!(reopened_p.history_status().undo_depth, 0);
        assert_eq!(reopened_p.history_status().redo_depth, 0);
        let compiled = compile(
            &reopened_scene,
            reopened_p.document(),
            reopened_q.document(),
            &registry,
        );
        assert_eq!(compiled, edited, "{encoding:?}");
        assert_native_spawn(compiled, &registry);
    }
    assert_eq!(session.document(), &expected);
    assert_eq!(q, q_document());
    assert_eq!(scene, scene_document());
}
