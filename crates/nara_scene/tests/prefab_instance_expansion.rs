//! Production expansion selects durable instance intent before resolving nested declarations.

use std::cell::RefCell;

use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentPropertyStep, ComponentRegistry, ComponentSchema,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode,
    PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabExpansionReport, PrefabInstance,
    PrefabInstanceClaim, PrefabInstanceHop, PrefabOverlay, PrefabPlacement, PrefabPlacementParent,
    PrefabPlacementSlot, PrefabProjectionStep, PrefabPropertyClaim, PrefabPropertyIntent,
    PrefabSourceResolver, ResolvedPrefabSource, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityRecord, SourceDocumentKey, SourceDocumentKind, register_scene_components,
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
    registry
        .register_persistent_component_with_codec::<Position, _, _>(
            ComponentSchema::new(
                position_component(),
                "Position",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("position"),
                "Position",
                ComponentFieldPath::from_fields(["position"]),
                ComponentValueShape::record_members(["x", "y"].map(|name| {
                    ComponentShapeMember::new(name, ComponentValueKind::U64)
                        .with_id(ComponentFieldId::new(name))
                })),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            |value| Ok(Position(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

#[derive(Component)]
struct Position(ComponentValue);

fn position_component() -> ComponentTypeId {
    ComponentTypeId::new("test.OverlayPosition")
}

fn position(x: u64, y: u64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "position",
            ComponentValue::map([("x", ComponentValue::U64(x)), ("y", ComponentValue::U64(y))]),
        )]),
    )
}

fn pin_x(target: PrefabEntityTarget, x: u64) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target,
        component: position_component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("position"))
                .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new("x"))]),
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(x)),
            bindings: Vec::new(),
        }],
    }
}

fn old_instance() -> PrefabInstance {
    let mut overlay = PrefabOverlay::new(key("q"));
    overlay.additions.push(PrefabAddition::new(id("q_overlay")));
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("q_overlay")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    PrefabInstance {
        source: source("q"),
        overrides: Some(overlay),
    }
}

fn replacement() -> PrefabInstance {
    PrefabInstance {
        source: source("r"),
        overrides: None,
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

fn scene(overrides: PrefabOverlay) -> SceneDocument {
    let mut outer = SceneEntityRecord::new(id("outer")).with_children([id("scene_local")]);
    outer.prefab = Some(PrefabInstance {
        source: source("p"),
        overrides: Some(overrides),
    });
    SceneDocument::new(
        [id("outer")],
        [outer, SceneEntityRecord::new(id("scene_local"))],
    )
}

fn nested_instance(value: Option<PrefabInstance>) -> PrefabOverlay {
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("n")),
        value,
    });
    overlay
}

#[derive(Default)]
struct RecordingResolver {
    sources: InMemoryPrefabSourceResolver,
    requests: RefCell<Vec<AssetRef>>,
}

impl PrefabSourceResolver for RecordingResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<ResolvedPrefabSource> {
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

#[derive(Debug, PartialEq, Eq)]
enum LoaderError {
    Refused(AssetRef),
}

#[test]
fn actual_loader_requests_only_selected_sources_and_preserves_typed_errors() {
    let registry = registry();
    let document = scene(nested_instance(Some(replacement())));
    let resolver = resolver();
    let requests = RefCell::new(Vec::new());
    let report = document
        .try_expand_prefabs_with_loader(&registry, Default::default(), None, |requested, _| {
            requests.borrow_mut().push(requested.clone());
            Ok::<_, LoaderError>(resolver.sources.resolve_prefab(requested))
        })
        .unwrap();
    assert_replaced(&report);
    assert_eq!(*requests.borrow(), [source("p"), source("r")]);

    let requests = RefCell::new(Vec::new());
    let error = document
        .try_expand_prefabs_with_loader(&registry, Default::default(), None, |requested, _| {
            requests.borrow_mut().push(requested.clone());
            if requested == &source("r") {
                Err(LoaderError::Refused(requested.clone()))
            } else {
                Ok(resolver.sources.resolve_prefab(requested))
            }
        })
        .unwrap_err();
    assert_eq!(error, LoaderError::Refused(source("r")));
    assert_eq!(*requests.borrow(), [source("p"), source("r")]);
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
    let document = scene(nested_instance(Some(replacement())));
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
    let document = scene(nested_instance(Some(replacement())));
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
    let document = scene(nested_instance(None));
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
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("n")));
    let document = scene(overlay);
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
fn durable_nested_replacement_survives_changes_to_the_discarded_declaration() {
    let registry = registry();
    let resolver = resolver();
    let mut resolver = resolver;
    let mut changed = outer_prefab();
    let old = changed
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("n"))
        .unwrap();
    old.prefab.as_mut().unwrap().overrides = None;
    resolver.sources.insert(source("p"), key("p"), changed);
    let document = scene(nested_instance(Some(replacement())));
    let before_document = document.clone();
    let before_sources = resolver.sources.clone();

    let report = document.expand_prefabs(&registry, &resolver);

    assert_replaced(&report);
    assert_eq!(*resolver.requests.borrow(), [source("p"), source("r")]);
    assert_eq!(document, before_document);
    assert_eq!(resolver.sources, before_sources);
}

#[test]
fn descendant_claims_bind_the_selected_replacement_base_instead_of_the_old_source() {
    let registry = registry();
    for expected in ["r", "q"] {
        let resolver = resolver();
        let mut overlay = nested_instance(Some(replacement()));
        overlay.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Add,
            target: PrefabEntityTarget {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("n")),
                    expected_base: key(expected),
                }],
                entity: PrefabEntityOrigin::SourceEntity(id("r_root")),
            },
            component: ComponentTypeId::new("nara.scene.Name"),
            record: SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Pinned replacement".into()),
            ),
            bindings: Vec::new(),
        });
        let document = scene(overlay);
        let before = document.clone();
        let report = document.expand_prefabs(&registry, &resolver);
        if expected == "r" {
            let materialized =
                projected_id(&report, &[inherited("p", "n"), inherited("r", "r_root")]);
            assert_eq!(
                entity(report.document().unwrap(), &materialized).components
                    [&ComponentTypeId::new("nara.scene.Name")]
                    .value,
                ComponentValue::String("Pinned replacement".into())
            );
        } else {
            assert!(report.candidate.is_none());
            assert!(report.diagnostics.iter().any(|diagnostic|
                diagnostic.code().as_str() == "scene.prefab-overlay-conflict"));
        }
        assert_eq!(*resolver.requests.borrow(), [source("p"), source("r")]);
        assert_eq!(document, before);
    }
}

#[test]
fn explicit_equal_leaf_pin_through_an_added_instance_inherits_rebased_siblings() {
    let registry = registry();
    let mut overlay = PrefabOverlay::new(key("p"));
    let mut addition = PrefabAddition::new(id("added_instance"));
    addition.prefab = Some(replacement());
    overlay.additions.push(addition);
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("added_instance")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    overlay.components.push(pin_x(
        PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::InstanceAddition(id("added_instance")),
                expected_base: key("r"),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("r_root")),
        },
        1,
    ));
    let document = scene(overlay);
    let before = document.clone();
    for (x, y) in [(1, 2), (9, 20)] {
        let resolver = InMemoryPrefabSourceResolver::new()
            .with_prefab(source("p"), key("p"), PrefabDocument::default())
            .with_prefab(
                source("r"),
                key("r"),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("r_root"))
                    .with_component(position_component(), position(x, y))]),
            );
        let report = document.expand_prefabs(&registry, &resolver);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let materialized = projected_id(
            &report,
            &[
                PrefabProjectionStep::InstanceAddition {
                    entity: id("added_instance"),
                },
                inherited("r", "r_root"),
            ],
        );
        assert_eq!(
            entity(report.document().unwrap(), &materialized).components[&position_component()],
            position(1, y)
        );
        assert_eq!(document, before);
    }
}
