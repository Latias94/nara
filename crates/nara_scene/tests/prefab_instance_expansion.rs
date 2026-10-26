//! Production expansion applies exact instance overrides before resolving nested declarations.

use std::cell::RefCell;

use nara_asset::AssetRef;
use nara_reflect::{ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabExpansionReport, PrefabInstance,
    PrefabProjectionStep, PrefabSourceResolver, ResolvedPrefabSource, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePlacement, ScenePlacementOwner, SourceDocumentKey, SourceDocumentKind,
    register_scene_components,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/{name}.prefab.ron")).unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "92387b8a-a667-47e5-bda4-6c0c77cc84df".parse().unwrap(),
        format!("canonical.{name}"),
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    registry
}

fn old_instance() -> PrefabInstance {
    PrefabInstance {
        source: source("q"),
        overrides: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id("q_overlay")),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }]),
    }
}

fn replacement() -> PrefabInstance {
    PrefabInstance {
        source: source("r"),
        overrides: ScenePatchDocument::default(),
    }
}

fn outer_prefab() -> PrefabDocument {
    let mut nested = SceneEntityRecord::new(id("n"))
        .with_children([id("p_local_first"), id("p_local_last")])
        .with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("P-owned nested anchor".into()),
            ),
        );
    nested.prefab = Some(old_instance());
    PrefabDocument::new(
        [id("p_root")],
        [
            SceneEntityRecord::new(id("p_root")).with_children([id("n"), id("p_sibling")]),
            nested,
            SceneEntityRecord::new(id("p_local_first")),
            SceneEntityRecord::new(id("p_local_last")),
            SceneEntityRecord::new(id("p_sibling")),
        ],
    )
}

fn scene(overrides: ScenePatchDocument) -> SceneDocument {
    let mut outer = SceneEntityRecord::new(id("outer")).with_children([id("scene_local")]);
    outer.prefab = Some(PrefabInstance {
        source: source("p"),
        overrides,
    });
    SceneDocument::new(
        [id("outer")],
        [outer, SceneEntityRecord::new(id("scene_local"))],
    )
}

#[derive(Default)]
struct RecordingResolver {
    sources: InMemoryPrefabSourceResolver,
    requests: RefCell<Vec<AssetRef>>,
}

impl PrefabSourceResolver for RecordingResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<ResolvedPrefabSource<'_>> {
        self.requests.borrow_mut().push(source.clone());
        self.sources.resolve_prefab(source)
    }
}

fn resolver() -> RecordingResolver {
    RecordingResolver {
        sources: InMemoryPrefabSourceResolver::new()
            .with_prefab(source("p"), key("p"), outer_prefab())
            .with_prefab(
                source("r"),
                key("r"),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("r_root"))]),
            ),
        ..RecordingResolver::default()
    }
}

fn inherited(source_name: &str, entity: &str) -> PrefabProjectionStep {
    PrefabProjectionStep::SourceEntity {
        source_document: source(source_name),
        canonical_source: key(source_name),
        entity: id(entity),
    }
}

fn projected_id(report: &PrefabExpansionReport, steps: &[PrefabProjectionStep]) -> SceneEntityId {
    let mut matches = report
        .provenance()
        .unwrap()
        .iter()
        .filter(|(_, provenance)| {
            provenance
                .projection_path()
                .is_some_and(|path| path.root_anchor() == &id("outer") && path.steps() == steps)
        });
    let (materialized, provenance) = matches.next().expect("expected source projection");
    assert!(matches.next().is_none(), "source projection must be unique");
    assert_eq!(
        provenance.local_entity_id(),
        steps.last().unwrap().entity_id()
    );
    materialized.clone()
}

fn entity<'a>(document: &'a SceneDocument, entity_id: &SceneEntityId) -> &'a SceneEntityRecord {
    document
        .entities
        .iter()
        .find(|entity| &entity.id == entity_id)
        .expect("expected materialized entity")
}

fn assert_replaced(report: &PrefabExpansionReport) {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let expanded = report.document().expect("replacement must expand");
    let p_root = projected_id(report, &[inherited("p", "p_root")]);
    let nested = projected_id(report, &[inherited("p", "n")]);
    let first = projected_id(report, &[inherited("p", "p_local_first")]);
    let last = projected_id(report, &[inherited("p", "p_local_last")]);
    let sibling = projected_id(report, &[inherited("p", "p_sibling")]);
    let r_root = projected_id(report, &[inherited("p", "n"), inherited("r", "r_root")]);

    assert_eq!(expanded.roots, [id("outer")]);
    assert_eq!(expanded.entities.len(), 8);
    assert_eq!(report.provenance().unwrap().len(), 8);
    assert_eq!(
        entity(expanded, &id("outer")).children,
        [p_root.clone(), id("scene_local")]
    );
    assert_eq!(
        entity(expanded, &p_root).children,
        [nested.clone(), sibling]
    );
    assert_eq!(entity(expanded, &nested).children, [r_root, first, last]);
    let original = outer_prefab();
    assert_eq!(
        entity(expanded, &nested).components,
        original
            .entities
            .iter()
            .find(|record| record.id == id("n"))
            .unwrap()
            .components
    );
    for local in ["outer", "scene_local"] {
        let provenance = &report.provenance().unwrap()[&id(local)];
        assert_eq!(provenance.local_entity_id(), &id(local));
        assert!(provenance.projection_path().is_none());
    }
    assert!(
        expanded
            .entities
            .iter()
            .all(|entity| entity.prefab.is_none())
    );
}

#[test]
fn outer_instance_replaces_nested_missing_source_before_resolving_it() {
    let registry = registry();
    let resolver = resolver();
    let document = scene(ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: id("n"),
            expected: Some(old_instance()),
            value: Some(replacement()),
        },
    ]));
    let before_document = document.clone();
    let before_sources = resolver.sources.clone();

    let report = document.expand_prefabs(&registry, &resolver);

    assert_replaced(&report);
    assert_eq!(*resolver.requests.borrow(), [source("p"), source("r")]);
    assert_eq!(document, before_document);
    assert_eq!(resolver.sources, before_sources);
}

#[test]
fn nested_replacement_never_requests_registered_source_with_invalid_topology() {
    let registry = registry();
    let mut resolver = resolver();
    let invalid = PrefabDocument::new([id("missing_root")], []);
    let invalid_diagnostics = invalid.validate(&registry);
    assert!(
        invalid_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.dangling-root"),
        "{invalid_diagnostics:?}"
    );
    resolver.sources.insert(source("q"), key("q"), invalid);
    let document = scene(ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: id("n"),
            expected: Some(old_instance()),
            value: Some(replacement()),
        },
    ]));
    let before_document = document.clone();
    let before_sources = resolver.sources.clone();

    let report = document.expand_prefabs(&registry, &resolver);

    assert_replaced(&report);
    assert_eq!(*resolver.requests.borrow(), [source("p"), source("r")]);
    assert_eq!(document, before_document);
    assert_eq!(resolver.sources, before_sources);
}

#[test]
fn outer_instance_detaches_nested_missing_source_and_retains_its_local_children() {
    let registry = registry();
    let resolver = resolver();
    let document = scene(ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: id("n"),
            expected: Some(old_instance()),
            value: None,
        },
    ]));
    let before_document = document.clone();
    let before_sources = resolver.sources.clone();

    let report = document.expand_prefabs(&registry, &resolver);

    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let expanded = report.document().expect("explicit detach must expand");
    let p_root = projected_id(&report, &[inherited("p", "p_root")]);
    let nested = projected_id(&report, &[inherited("p", "n")]);
    let first = projected_id(&report, &[inherited("p", "p_local_first")]);
    let last = projected_id(&report, &[inherited("p", "p_local_last")]);
    let sibling = projected_id(&report, &[inherited("p", "p_sibling")]);
    assert_eq!(expanded.roots, [id("outer")]);
    assert_eq!(expanded.entities.len(), 7);
    assert_eq!(report.provenance().unwrap().len(), 7);
    assert_eq!(
        entity(expanded, &id("outer")).children,
        [p_root.clone(), id("scene_local")]
    );
    assert_eq!(
        entity(expanded, &p_root).children,
        [nested.clone(), sibling]
    );
    assert_eq!(entity(expanded, &nested).children, [first, last]);
    assert!(entity(expanded, &nested).prefab.is_none());
    let original = outer_prefab();
    assert_eq!(
        entity(expanded, &nested).components,
        original
            .entities
            .iter()
            .find(|record| record.id == id("n"))
            .unwrap()
            .components
    );
    assert_eq!(*resolver.requests.borrow(), [source("p")]);
    assert_eq!(document, before_document);
    assert_eq!(resolver.sources, before_sources);
}

#[test]
fn outer_instance_suppresses_complete_nested_subtree_without_resolving_missing_source() {
    let registry = registry();
    let resolver = resolver();
    let document = scene(ScenePatchDocument::new([
        ScenePatchOperation::RemoveEntity {
            entity: id("n"),
            from: ScenePlacement::first(ScenePlacementOwner::Parent {
                parent: id("p_root"),
            }),
        },
    ]));
    let before_document = document.clone();
    let before_sources = resolver.sources.clone();

    let report = document.expand_prefabs(&registry, &resolver);

    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let expanded = report.document().expect("complete suppression must expand");
    let p_root = projected_id(&report, &[inherited("p", "p_root")]);
    let sibling = projected_id(&report, &[inherited("p", "p_sibling")]);
    assert_eq!(expanded.roots, [id("outer")]);
    assert_eq!(expanded.entities.len(), 4);
    assert_eq!(report.provenance().unwrap().len(), 4);
    assert_eq!(
        entity(expanded, &id("outer")).children,
        [p_root.clone(), id("scene_local")]
    );
    assert_eq!(entity(expanded, &p_root).children, [sibling]);
    assert!(report.provenance().unwrap().values().all(|provenance| {
        !["n", "p_local_first", "p_local_last", "q_overlay", "r_root"]
            .contains(&provenance.local_entity_id().as_str())
    }));
    assert_eq!(*resolver.requests.borrow(), [source("p")]);
    assert_eq!(document, before_document);
    assert_eq!(resolver.sources, before_sources);
}

#[test]
fn stale_nested_declaration_rejects_before_resolving_old_or_replacement_source() {
    let registry = registry();
    let resolver = resolver();
    let actual = old_instance();
    let stale = PrefabInstance {
        source: actual.source.clone(),
        overrides: ScenePatchDocument::default(),
    };
    assert_ne!(actual.overrides, stale.overrides);
    let document = scene(ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: id("n"),
            expected: Some(stale),
            value: Some(replacement()),
        },
    ]));
    let before_document = document.clone();
    let before_sources = resolver.sources.clone();

    let report = document.expand_prefabs(&registry, &resolver);

    assert!(report.candidate.is_none());
    assert!(report.document().is_none());
    assert!(report.provenance().is_none());
    assert!(report.diagnostics.has_errors());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str()
                == "scene.patch-prefab-instance-context-mismatch"),
        "{:?}",
        report.diagnostics
    );
    assert_eq!(*resolver.requests.borrow(), [source("p")]);
    assert_eq!(document, before_document);
    assert_eq!(resolver.sources, before_sources);
}
