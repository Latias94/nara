//! Public root-selection continuity across branch extraction, with a separate forest control.

use nara_asset::AssetRef;
use nara_diagnostic::DiagnosticValueRef;
use nara_ecs::World;
use nara_hierarchy::{Children, HierarchyRoots, Parent};
use nara_identity::EntityLookup;
use nara_reflect::{
    ComponentRegistryBuilder, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};
use nara_scene::{
    CompiledSceneDocument, InMemoryPrefabSourceResolver, Name, PrefabAddition, PrefabDocument,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceBinding,
    PrefabInstanceHop, PrefabOverlay, PrefabProjectionStep, SceneCompilationFailureKind,
    SceneCompilationReport, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, SceneEntitySource, SceneOccurrenceKey, ScenePlacement,
    ScenePlacementParent, SourceDocumentKey, SourceDocumentKind, Visibility,
    register_scene_components, spawn_scene,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!(
        "prefabs/extraction_root_selection/{name}.prefab.ron"
    ))
    .unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "7dad197a-24bc-4f55-9a6d-a52a4bda8620".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn consumer(mut instance: PrefabInstance, overlay: PrefabOverlay) -> SceneDocument {
    instance.overrides = Some(overlay);
    let mut host = SceneEntityRecord::new(id("host"));
    host.prefab = Some(instance);
    SceneDocument::from_ordered_roots([host])
}

fn compiled(report: SceneCompilationReport) -> CompiledSceneDocument {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    report
        .candidate
        .expect("valid composition must seal a candidate")
}

#[test]
fn extraction_preserves_the_selected_projected_root_without_a_wrapper() {
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let p_before = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("b")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("k"),
                ))),
            ),
        ],
        [
            SceneEntityRecord::new(id("k")),
            SceneEntityRecord::new(id("b")),
        ],
    );
    let mut overlay_before = PrefabOverlay::new(key("p"));
    overlay_before
        .placements
        .push(SceneEntityPlacement::root(id("b")));
    overlay_before
        .suppressions
        .push(PrefabEntityTarget::source(id("k")));
    let scene_before = consumer(
        PrefabInstance::root(source("p"), key("p"), PrefabEntityTarget::source(id("b"))),
        overlay_before.clone(),
    );
    let sources_before =
        InMemoryPrefabSourceResolver::new().with_prefab(source("p"), key("p"), p_before.clone());
    let scene_before_snapshot = scene_before.clone();
    let sources_before_snapshot = sources_before.clone();

    // The overlay moves b out of k before suppressing k. Root binding contributes b
    // directly to host; no mounted source entity remains below it.
    let before = compiled(scene_before.compile(&registry, &sources_before));
    assert_eq!(before.roots(), &[id("host")]);
    assert_eq!(before.entities().len(), 1);
    assert_eq!(before.entities()[0].id(), &id("host"));
    assert!(before.entities()[0].children().is_empty());
    assert!(before.entities()[0].components().is_empty());
    assert_eq!(before.canonical_provenance().len(), 1);
    let host_origin = &before.canonical_provenance()[&id("host")];
    assert_eq!(host_origin.local_entity_id(), &id("host"));
    assert!(host_origin.projection_path().is_none());

    // Hand-construct extraction: Q owns the original declarations and placements,
    // while P keeps k as Q's root-bound anchor under the same P key and asset.
    let q = p_before.clone();
    assert_ne!(key("q"), key("p"));
    assert_ne!(source("q"), source("p"));
    let mut k = SceneEntityRecord::new(id("k"));
    k.prefab = Some(PrefabInstance::root(
        source("q"),
        key("q"),
        PrefabEntityTarget::source(id("k")),
    ));
    let p_after = PrefabDocument::from_ordered_roots([k]);
    let sources_after = InMemoryPrefabSourceResolver::new()
        .with_prefab(source("p"), key("p"), p_after)
        .with_prefab(source("q"), key("q"), q);

    // Both b's placement and the outer root selection follow its new declaration.
    // Suppression still names P:k; the containing instance keeps P's key and asset.
    let projected_b = PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("k")),
            expected_base: key("q"),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id("b")),
    };
    let mut overlay_after = overlay_before.clone();
    overlay_after.placements[0].target = projected_b.clone();
    assert_eq!(overlay_after.base, key("p"));
    assert_eq!(overlay_after.suppressions, overlay_before.suppressions);
    let scene_after = consumer(
        PrefabInstance::root(source("p"), key("p"), projected_b.clone()),
        overlay_after.clone(),
    );
    assert_eq!(
        scene_after.entities[0].prefab.as_ref().unwrap().binding,
        PrefabInstanceBinding::Root {
            base: key("p"),
            root: projected_b.clone(),
        },
    );
    assert_eq!(
        scene_after.entities[0].prefab.as_ref().unwrap().source,
        scene_before.entities[0].prefab.as_ref().unwrap().source,
    );
    let scene_after_snapshot = scene_after.clone();
    let sources_after_snapshot = sources_after.clone();

    let after = compiled(scene_after.compile(&registry, &sources_after));
    assert_eq!(after.roots(), &[id("host")]);
    assert_eq!(after.entities().len(), 1);
    assert_eq!(after.entities()[0].id(), &id("host"));
    assert!(after.entities()[0].children().is_empty());
    assert!(after.entities()[0].components().is_empty());
    assert_eq!(after.canonical_provenance().len(), 1);
    assert_eq!(after.canonical_provenance()[&id("host")], *host_origin);
    assert_eq!(after, before);

    // Forest is a separate control consumer of exactly the same after-source and
    // overlay. Its extra entity is intentional: it proves the projected b root
    // exists independently of the single-entity root-bound result.
    let forest_scene = consumer(PrefabInstance::forest(source("p")), overlay_after.clone());
    assert_eq!(
        forest_scene.entities[0].prefab.as_ref().unwrap().binding,
        PrefabInstanceBinding::Forest,
    );
    let forest_scene_snapshot = forest_scene.clone();
    let forest = compiled(forest_scene.compile(&registry, &sources_after));
    assert_eq!(forest.roots(), &[id("host")]);
    assert_eq!(forest.entities().len(), 2);
    assert_eq!(forest.canonical_provenance().len(), 2);
    let (b_id, b_origin) = forest
        .canonical_provenance()
        .iter()
        .find(|(_, origin)| origin.local_entity_id() == &id("b"))
        .expect("rewritten structural intent must preserve Q:b");
    assert_ne!(b_id, &id("host"));
    let path = b_origin.projection_path().expect("b is inherited from Q");
    assert_eq!(path.root_anchor(), &id("host"));
    assert_eq!(
        path.steps(),
        &[
            PrefabProjectionStep::SourceEntity {
                source_document: source("p"),
                canonical_source: key("p"),
                entity: id("k"),
            },
            PrefabProjectionStep::SourceEntity {
                source_document: source("q"),
                canonical_source: key("q"),
                entity: id("b"),
            },
        ],
    );
    let host = forest
        .entities()
        .iter()
        .find(|entity| entity.id() == &id("host"))
        .unwrap();
    assert_eq!(forest.canonical_provenance()[&id("host")], *host_origin);
    assert_eq!(host.children(), std::slice::from_ref(b_id));
    let b = forest
        .entities()
        .iter()
        .find(|entity| entity.id() == b_id)
        .unwrap();
    assert!(b.children().is_empty());
    assert!(
        forest
            .entities()
            .iter()
            .all(|entity| entity.components().is_empty())
    );

    // Invalid root intent cannot fall back to another base, origin or effective root.
    let mut wrong_base = projected_b.clone();
    wrong_base.through[0].expected_base = key("different-q");
    let mut wrong_anchor_namespace = projected_b.clone();
    wrong_anchor_namespace.through[0].anchor = PrefabEntityOrigin::InstanceAddition(id("k"));
    let mut wrong_terminal_namespace = projected_b.clone();
    wrong_terminal_namespace.entity = PrefabEntityOrigin::InstanceAddition(id("b"));
    let mut missing_terminal = projected_b.clone();
    missing_terminal.entity = PrefabEntityOrigin::SourceEntity(id("missing"));
    let mut suppressed_root = projected_b.clone();
    suppressed_root.entity = PrefabEntityOrigin::SourceEntity(id("k"));
    for (case, root, expected) in [
        (
            "wrong_base",
            wrong_base,
            SceneCompilationFailureKind::SourceChanged,
        ),
        (
            "wrong_anchor_namespace",
            wrong_anchor_namespace,
            SceneCompilationFailureKind::RootBinding,
        ),
        (
            "wrong_terminal_namespace",
            wrong_terminal_namespace,
            SceneCompilationFailureKind::RootBinding,
        ),
        (
            "missing_terminal",
            missing_terminal,
            SceneCompilationFailureKind::RootBinding,
        ),
        (
            "old_source_namespace",
            PrefabEntityTarget::source(id("b")),
            SceneCompilationFailureKind::RootBinding,
        ),
        (
            "suppressed_root",
            suppressed_root,
            SceneCompilationFailureKind::RootBinding,
        ),
        (
            "extra_root",
            projected_b,
            SceneCompilationFailureKind::RootBinding,
        ),
    ] {
        let mut overlay = overlay_after.clone();
        if case == "extra_root" {
            overlay.additions.push(PrefabAddition::new(id("extra")));
            overlay.placements.push(SceneEntityPlacement::new(
                PrefabEntityTarget::addition(id("extra")),
                ScenePlacement::last(ScenePlacementParent::Root),
            ));
        }
        let scene = consumer(PrefabInstance::root(source("p"), key("p"), root), overlay);
        let snapshot = scene.clone();
        let refused = scene.compile(&registry, &sources_after);
        assert!(refused.candidate.is_none(), "{case}");
        assert!(
            refused.diagnostics.has_errors(),
            "{case}: {:?}",
            refused.diagnostics,
        );
        assert_eq!(
            refused.failure.as_ref().map(|failure| *failure.reason()),
            Some(expected),
            "{case}: {refused:?}",
        );
        let diagnostic = refused
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-compilation-failed")
            .expect("root selection must expose its structured compilation diagnostic");
        let reason = diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == "failure-kind")
            .expect("compilation diagnostic must carry its failure kind");
        let expected_reason = if expected == SceneCompilationFailureKind::SourceChanged {
            "source-changed"
        } else {
            "root-binding"
        };
        assert_eq!(
            reason.value(),
            DiagnosticValueRef::Identifier(expected_reason),
            "{case}"
        );
        assert_eq!(scene, snapshot, "{case}");
        assert_eq!(sources_after, sources_after_snapshot, "{case}");
    }

    // Successful and refused compilation preserve the caller's exact source
    // documents, canonical bindings, overlay declarations and lookup assets.
    assert_eq!(scene_before, scene_before_snapshot);
    assert_eq!(sources_before, sources_before_snapshot);
    assert_eq!(scene_after, scene_after_snapshot);
    assert_eq!(sources_after, sources_after_snapshot);
    assert_eq!(forest_scene, forest_scene_snapshot);
}

#[test]
fn addition_root_with_a_suppressed_source_namesake_normalizes_through_nested_root_bindings() {
    let mut registry = ComponentRegistryBuilder::new();
    register_scene_components(&mut registry).unwrap();
    let registry = registry.freeze().unwrap();
    let name = ComponentTypeId::new("nara.scene.Name");
    let visibility = ComponentTypeId::new("nara.scene.Visibility");
    let record = |value: &str| {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(value.to_owned()),
        )
    };
    let q =
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("b"))
            .with_component(name.clone(), record("suppressed source"))]);
    let mut addition = PrefabAddition::new(id("b"));
    addition
        .components
        .insert(name.clone(), record("addition root"));
    addition
        .components
        .insert(visibility.clone(), record("hidden"));
    let mut overlay = PrefabOverlay::new(key("q"));
    overlay.additions.push(addition);
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("b")));
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("b")),
        ScenePlacement::last(ScenePlacementParent::Root),
    ));
    let mut inner =
        PrefabInstance::root(source("q"), key("q"), PrefabEntityTarget::addition(id("b")));
    inner.overrides = Some(overlay);
    let mut k = SceneEntityRecord::new(id("k"));
    k.prefab = Some(inner);
    let p = PrefabDocument::from_ordered_roots([k]);
    let q_facet = PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("k")),
            expected_base: key("q"),
        }],
        entity: PrefabEntityOrigin::InstanceAddition(id("b")),
    };
    let mut r = SceneEntityRecord::new(id("r"));
    r.prefab = Some(PrefabInstance::root(source("p"), key("p"), q_facet.clone()));
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(source("q"), key("q"), q)
        .with_prefab(source("p"), key("p"), p)
        .with_prefab(
            source("r"),
            key("r"),
            PrefabDocument::from_ordered_roots([r]),
        );
    let sources_snapshot = sources.clone();
    let mut root = q_facet;
    root.through.insert(
        0,
        PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("r")),
            expected_base: key("p"),
        },
    );
    let direct_scene = consumer(
        PrefabInstance::root(source("r"), key("r"), PrefabEntityTarget::source(id("r"))),
        PrefabOverlay::new(key("r")),
    );
    let scene = consumer(
        PrefabInstance::root(source("r"), key("r"), root.clone()),
        PrefabOverlay::new(key("r")),
    );
    let direct_snapshot = direct_scene.clone();
    let scene_snapshot = scene.clone();

    // Q's addition aliases P:k, then R:r, then host. The routed selector must keep
    // Q's addition facet for lookup and normalize it to the same final canonical root.
    let direct = compiled(direct_scene.compile(&registry, &sources));
    let routed = compiled(scene.compile(&registry, &sources));
    assert_eq!(routed, direct);
    assert_eq!(routed.roots(), &[id("host")]);
    assert_eq!(routed.entities().len(), 1);
    let host = &routed.entities()[0];
    assert_eq!(host.id(), &id("host"));
    assert!(host.children().is_empty());
    assert_eq!(host.components().len(), 2);
    assert_eq!(
        host.components()[&name].value(),
        &ComponentValue::String("addition root".into())
    );
    assert_eq!(
        host.components()[&visibility].value(),
        &ComponentValue::String("hidden".into())
    );
    assert_eq!(routed.canonical_provenance().len(), 1);
    let origin = &routed.canonical_provenance()[&id("host")];
    assert_eq!(origin.local_entity_id(), &id("host"));
    assert!(origin.projection_path().is_none());

    // The source facet with the same spelling was suppressed; it cannot select the addition.
    root.entity = PrefabEntityOrigin::SourceEntity(id("b"));
    let wrong_namespace = consumer(
        PrefabInstance::root(source("r"), key("r"), root),
        PrefabOverlay::new(key("r")),
    );
    let wrong_snapshot = wrong_namespace.clone();
    let refused = wrong_namespace.compile(&registry, &sources);
    assert!(refused.candidate.is_none());
    assert!(
        refused.diagnostics.has_errors(),
        "{:?}",
        refused.diagnostics
    );
    assert_eq!(
        refused.failure.as_ref().map(|failure| *failure.reason()),
        Some(SceneCompilationFailureKind::RootBinding),
    );

    let scene_key = SourceDocumentKey::named(
        "7dad197a-24bc-4f55-9a6d-a52a4bda8620".parse().unwrap(),
        "addition-root-chain",
        SourceDocumentKind::Scene,
    )
    .unwrap();
    let candidate = routed.bind_source(scene_key.clone());
    let occurrence = SceneOccurrenceKey::new("nara.scene.tests", "addition-root-chain").unwrap();
    let mut world = World::new();
    let report = spawn_scene(&mut world, &registry, &candidate, &occurrence, None);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let instance = report
        .instance
        .expect("typed addition root must spawn through the ordinary Native path");
    assert_eq!(instance.len(), 1);
    let EntityLookup::Resolved(runtime) = instance.resolve(&world, &id("host")) else {
        panic!("the normalized root must resolve to the live host");
    };
    assert_eq!(
        world.get::<Name>(runtime),
        Some(&Name::new("addition root"))
    );
    assert_eq!(world.get::<Visibility>(runtime), Some(&Visibility::Hidden));
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), &[runtime]);
    assert!(world.get::<Parent>(runtime).is_none());
    assert!(
        world
            .get::<Children>(runtime)
            .map_or(&[][..], Children::as_slice)
            .is_empty()
    );
    let runtime_source = world.get::<SceneEntitySource>(runtime).unwrap();
    assert_eq!(runtime_source.entity_id, id("host"));
    assert_eq!(runtime_source.containing_source(), &scene_key);
    assert_eq!(runtime_source.occurrence(), &occurrence);
    assert_eq!(
        runtime_source.provenance(),
        &candidate.document().canonical_provenance()[&id("host")]
    );
    assert_eq!(
        world
            .iter_entities()
            .filter(|entity| entity.contains::<SceneEntitySource>())
            .count(),
        1,
    );
    assert_eq!(scene, scene_snapshot);
    assert_eq!(direct_scene, direct_snapshot);
    assert_eq!(wrong_namespace, wrong_snapshot);
    assert_eq!(sources, sources_snapshot);
}
