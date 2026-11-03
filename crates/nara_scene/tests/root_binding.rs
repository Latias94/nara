//! Public compiler qualification for explicit root binding and source-owned desired placements.

use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_core::ItemLimit;
use nara_diagnostic::DiagnosticValueRef;
use nara_ecs::{Component, Entity, World};
use nara_hierarchy::{Children, HierarchyRoots, Parent};
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityLookup, EntityReference,
    ReferenceInputId, SpawnedSceneInstance, WorldIdentityDomain,
};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    CompiledSceneDocument, CompiledSceneEntity, InMemoryPrefabSourceResolver, PrefabAddition,
    PrefabComponentIntent, PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance,
    PrefabInstanceBinding, PrefabInstanceHop, PrefabOverlay, PrefabProjectionStep,
    PrefabPropertyClaim, PrefabPropertyIntent, PrefabReferenceArguments, PrefabSourceResolver,
    SceneCompilationFailureKind, SceneCompilationLimits, SceneCompilationOptions,
    SceneCompilationReport, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, SceneEntitySource, SceneOccurrenceKey, ScenePlacement,
    ScenePlacementParent, ScenePlacementSlot, SourceDocumentKey, SourceDocumentKind, spawn_scene,
};

#[derive(Debug, PartialEq, Eq, Component)]
struct Position {
    x: u64,
    y: u64,
}

#[derive(Debug, PartialEq, Eq, Component)]
struct Link(EntityReference);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/root_binding/{name}.prefab.ron")).unwrap()
}

fn document_key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "af06cbaf-c96d-48e3-9f35-3def3f8d537a".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    document_key(name, SourceDocumentKind::Prefab)
}

fn position_type() -> ComponentTypeId {
    ComponentTypeId::new("test.RootBindingPosition")
}

fn link_type() -> ComponentTypeId {
    ComponentTypeId::new("test.RootBindingLink")
}

fn registry() -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_with_codec::<Position, _, _>(
            ComponentSchema::new(
                position_type(),
                "Root binding position",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields(["x", "y"].map(|name| {
                ComponentFieldSchema::required(
                    ComponentFieldId::new(name),
                    name,
                    ComponentFieldPath::from_fields([name]),
                    ComponentValueKind::U64,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            })),
            |value| {
                Ok(Position {
                    x: value.field_u64("x")?,
                    y: value.field_u64("y")?,
                })
            },
            |value| {
                Ok(ComponentValue::map([
                    ("x", ComponentValue::U64(value.x)),
                    ("y", ComponentValue::U64(value.y)),
                ]))
            },
        )
        .unwrap();
    builder
        .register_persistent_component_with_codec::<Link, _, _>(
            ComponentSchema::new(
                link_type(),
                "Root binding link",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
            )
            .with_capabilities([
                ComponentCapability::Scene,
                ComponentCapability::EntityRef,
                ComponentCapability::Inspect,
                ComponentCapability::Edit,
            ])]),
            |value| Ok(Link(value.field_entity_reference("target")?.clone())),
            |value| {
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(value.0.clone()),
                )]))
            },
        )
        .unwrap();
    builder.freeze().unwrap()
}

fn position(x: u64, y: u64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("x", ComponentValue::U64(x)), ("y", ComponentValue::U64(y))]),
    )
}

fn link(reference: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(reference))]),
    )
}

fn local(name: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(name) }
}

fn hops(route: &[(&str, &str)]) -> Vec<PrefabInstanceHop> {
    route
        .iter()
        .map(|(anchor, base)| PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
            expected_base: key(base),
        })
        .collect()
}

fn target(route: &[(&str, &str)], entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: hops(route),
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    }
}

fn routed_reference(route: &[(&str, &str)], entity: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: hops(route),
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(entity))),
        },
    }
}

fn placement(target: PrefabEntityTarget, parent: PrefabEntityTarget) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        target,
        ScenePlacement::last(ScenePlacementParent::Entity(parent)),
    )
}

fn pin_x(target: PrefabEntityTarget, x: u64) -> PrefabComponentIntent {
    property(target, position_type(), "x", ComponentValue::U64(x))
}

fn property(
    target: PrefabEntityTarget,
    component: ComponentTypeId,
    field: &str,
    value: ComponentValue,
) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target,
        component,
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new(field)),
            intent: PrefabPropertyIntent::Set(value),
        }],
    }
}

fn root_instance(base: &str, root: &str) -> PrefabInstance {
    PrefabInstance::root(
        source(base),
        key(base),
        PrefabEntityTarget::source(id(root)),
    )
}

fn q_document() -> PrefabDocument {
    PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("q")),
            placement(target(&[], "d1"), target(&[], "q")),
            placement(target(&[], "d2"), target(&[], "q")),
        ],
        [
            SceneEntityRecord::new(id("q"))
                .with_component(position_type(), position(1, 2))
                .with_component(link_type(), link(local("q"))),
            SceneEntityRecord::new(id("d1"))
                .with_component(position_type(), position(3, 4))
                .with_component(link_type(), link(routed_reference(&[], "q"))),
            SceneEntityRecord::new(id("d2")).with_component(position_type(), position(5, 6)),
        ],
    )
}

fn p_document() -> PrefabDocument {
    let mut inner = PrefabOverlay::new(key("q"));
    inner.components.push(pin_x(target(&[], "q"), 10));
    let mut instance = root_instance("q", "q");
    instance.overrides = Some(inner);
    let mut k = SceneEntityRecord::new(id("k")).with_component(position_type(), position(20, 30));
    k.prefab = Some(instance);
    PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                target(&[], "b"),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(target(&[], "k")),
                    target(&[("k", "q")], "d2"),
                ),
            ),
        ],
        [
            k,
            SceneEntityRecord::new(id("b"))
                .with_component(position_type(), position(7, 8))
                .with_component(link_type(), link(routed_reference(&[("k", "q")], "q"))),
        ],
    )
}

struct Fixture {
    q: PrefabDocument,
    p: PrefabDocument,
    scene: SceneDocument,
}

impl Fixture {
    fn sources(&self) -> InMemoryPrefabSourceResolver {
        InMemoryPrefabSourceResolver::new()
            .with_prefab(source("q"), key("q"), self.q.clone())
            .with_prefab(source("p"), key("p"), self.p.clone())
    }
}

fn repeated_fixture() -> Fixture {
    let mut records = Vec::new();
    for (name, x) in [("z_left", 40), ("a_right", 41)] {
        let mut outer = PrefabOverlay::new(key("p"));
        outer.components.push(pin_x(target(&[], "k"), x));
        // The parent is Q's root facet; it normalizes to P:k and then the Scene anchor.
        outer.placements.push(SceneEntityPlacement::new(
            target(&[("k", "q")], "d1"),
            ScenePlacement::first(ScenePlacementParent::Entity(target(&[("k", "q")], "q"))),
        ));
        let mut instance = root_instance("p", "k");
        instance.overrides = Some(outer);
        let mut anchor = SceneEntityRecord::new(id(name));
        anchor.prefab = Some(instance);
        records.push(anchor);
    }
    records.extend([
        SceneEntityRecord::new(id("enter_left"))
            .with_component(link_type(), link(routed_reference(&[("z_left", "p")], "k"))),
        SceneEntityRecord::new(id("enter_right")).with_component(
            link_type(),
            link(routed_reference(&[("a_right", "p"), ("k", "q")], "q")),
        ),
        // An equal terminal spelling in the containing Scene must never capture Q's self link.
        SceneEntityRecord::new(id("q")).with_component(position_type(), position(999, 999)),
    ]);
    Fixture {
        q: q_document(),
        p: p_document(),
        scene: SceneDocument::from_ordered_roots(records),
    }
}

fn compiled(report: SceneCompilationReport) -> CompiledSceneDocument {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    report
        .candidate
        .expect("successful compilation must seal a flat document")
}

fn entity<'a>(
    document: &'a CompiledSceneDocument,
    entity_id: &SceneEntityId,
) -> &'a CompiledSceneEntity {
    document
        .entities()
        .iter()
        .find(|entity| entity.id() == entity_id)
        .expect("expected exactly one materialized entity")
}

fn projected_id(document: &CompiledSceneDocument, anchor: &str, local_id: &str) -> SceneEntityId {
    let mut matches = document
        .canonical_provenance()
        .iter()
        .filter(|(_, provenance)| {
            provenance.local_entity_id() == &id(local_id)
                && provenance
                    .projection_path()
                    .is_some_and(|path| path.root_anchor() == &id(anchor))
        });
    let (entity, _) = matches.next().expect("expected a projected source entity");
    assert!(
        matches.next().is_none(),
        "projection must be unique in its occurrence",
    );
    entity.clone()
}

fn assert_position(document: &CompiledSceneDocument, entity_id: &SceneEntityId, x: u64, y: u64) {
    assert_eq!(
        entity(document, entity_id).components()[&position_type()].value(),
        &position(x, y).value,
    );
}

fn assert_link(
    document: &CompiledSceneDocument,
    entity_id: &SceneEntityId,
    target: &SceneEntityId,
) {
    assert_eq!(
        entity(document, entity_id).components()[&link_type()].value(),
        &link(EntityReference::SceneLocal {
            entity: target.clone(),
        })
        .value,
    );
}

fn assert_repeated_document(document: &CompiledSceneDocument) {
    assert_eq!(
        document.roots(),
        &[
            id("z_left"),
            id("a_right"),
            id("enter_left"),
            id("enter_right"),
            id("q"),
        ],
    );
    // Two four-entity occurrences, two entering-reference readers and the local name collision.
    assert_eq!(document.entities().len(), 11);
    assert_eq!(document.canonical_provenance().len(), 11);
    let mut previous_children: Option<[SceneEntityId; 3]> = None;
    for (anchor, x) in [("z_left", 40), ("a_right", 41)] {
        let anchor_id = id(anchor);
        let children = ["d1", "b", "d2"].map(|local| projected_id(document, anchor, local));
        assert_eq!(entity(document, &anchor_id).children(), &children);
        assert_position(document, &anchor_id, x, 30);
        assert_link(document, &anchor_id, &anchor_id);
        let provenance = &document.canonical_provenance()[&anchor_id];
        assert_eq!(provenance.local_entity_id(), &anchor_id);
        assert!(
            provenance.projection_path().is_none(),
            "the containing anchor owns identity",
        );
        for (child, local_id, base, position) in [
            (&children[0], "d1", "q", (3, 4)),
            (&children[1], "b", "p", (7, 8)),
            (&children[2], "d2", "q", (5, 6)),
        ] {
            assert_position(document, child, position.0, position.1);
            assert!(entity(document, child).children().is_empty());
            let path = document.canonical_provenance()[child]
                .projection_path()
                .unwrap();
            assert!(matches!(
                path.steps().last(),
                Some(PrefabProjectionStep::SourceEntity { canonical_source, entity, .. })
                    if canonical_source == &key(base) && entity == &id(local_id)
            ));
        }
        assert_link(document, &children[0], &anchor_id);
        assert_link(document, &children[1], &anchor_id);
        if let Some(previous) = previous_children {
            for child in &children {
                assert!(
                    !previous.contains(child),
                    "repeated instances must have distinct identities",
                );
            }
        }
        previous_children = Some(children);
    }
    assert_link(document, &id("enter_left"), &id("z_left"));
    assert_link(document, &id("enter_right"), &id("a_right"));
    assert_position(document, &id("q"), 999, 999);
    assert!(
        !document
            .entities()
            .iter()
            .any(|entity| entity.id() == &id("k")),
    );
}

fn resolve(world: &World, instance: &SpawnedSceneInstance, entity: &SceneEntityId) -> Entity {
    match instance.resolve(world, entity) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live materialized entity, got {other:?}"),
    }
}

fn assert_native_spawn(document: CompiledSceneDocument, registry: &ComponentRegistrySnapshot) {
    let candidate = document.bind_source(document_key(
        "root-binding-scene",
        SourceDocumentKind::Scene,
    ));
    let occurrence = SceneOccurrenceKey::new("nara.scene.tests", "root_binding").unwrap();
    let mut world = World::new();
    let report = spawn_scene(&mut world, registry, &candidate, &occurrence, None);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let instance = report.instance.expect("ordinary native spawn must succeed");
    let document = candidate.document();
    assert_eq!(instance.len(), document.entities().len());
    // ECS component-registration entities are independent of this Scene occurrence.
    assert_eq!(
        world
            .iter_entities()
            .filter(|entity| entity.contains::<SceneEntitySource>())
            .count(),
        document.entities().len(),
    );
    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        document
            .roots()
            .iter()
            .map(|id| resolve(&world, &instance, id))
            .collect::<Vec<_>>(),
    );
    for compiled in document.entities() {
        let runtime = resolve(&world, &instance, compiled.id());
        let source = world.get::<SceneEntitySource>(runtime).unwrap();
        assert_eq!(&source.entity_id, compiled.id());
        assert_eq!(source.containing_source(), candidate.source_key());
        assert_eq!(source.occurrence(), &occurrence);
        assert_eq!(
            source.provenance(),
            &document.canonical_provenance()[compiled.id()],
        );
        let children = compiled
            .children()
            .iter()
            .map(|id| resolve(&world, &instance, id))
            .collect::<Vec<_>>();
        assert_eq!(
            world
                .get::<Children>(runtime)
                .map_or(&[][..], Children::as_slice),
            children.as_slice(),
        );
        for child in children {
            assert_eq!(world.get::<Parent>(child).unwrap().parent(), runtime);
        }
        if document.roots().contains(compiled.id()) {
            assert!(world.get::<Parent>(runtime).is_none());
        }
        if let Some(position) = compiled.components().get(&position_type()) {
            assert_eq!(
                world.get::<Position>(runtime),
                Some(&Position {
                    x: position.value().field_u64("x").unwrap(),
                    y: position.value().field_u64("y").unwrap(),
                }),
            );
        }
        if let Some(link) = compiled.components().get(&link_type()) {
            let expected = link.value().field_entity_reference("target").unwrap();
            let EntityReference::SceneLocal { entity: target } = expected else {
                panic!("compiled authored references must be lowered before the native codec");
            };
            let actual = world.get::<Link>(runtime).unwrap();
            assert_eq!(&actual.0, expected);
            assert_eq!(
                world
                    .resource::<WorldIdentityDomain>()
                    .resolve_entity_reference(&world, &actual.0, Some(instance.instance_id())),
                EntityLookup::Resolved(resolve(&world, &instance, target)),
            );
        }
    }
}

fn assert_refused(
    scene: &SceneDocument,
    sources: &InMemoryPrefabSourceResolver,
    registry: &ComponentRegistrySnapshot,
    reason: SceneCompilationFailureKind,
    context: &str,
) {
    let before = scene.clone();
    let report = scene.compile(registry, sources);
    assert!(
        report.candidate.is_none(),
        "{context}: invalid composition must not publish a flat candidate",
    );
    assert!(
        report.diagnostics.has_errors(),
        "{context}: {:?}",
        report.diagnostics
    );
    assert_eq!(
        report.failure.as_ref().map(|failure| *failure.reason()),
        Some(reason),
        "{context}: {:?}",
        report.diagnostics,
    );
    assert_eq!(scene, &before);
}

#[test]
fn ordered_root_constructors_preserve_declaration_order_independently_of_record_sorting() {
    let registry = registry();
    let records = [
        SceneEntityRecord::new(id("z")),
        SceneEntityRecord::new(id("a")),
    ];
    let scene = SceneDocument::from_ordered_roots(records.clone());
    let prefab = PrefabDocument::from_ordered_roots(records);
    assert_eq!(
        scene.placements,
        [
            SceneEntityPlacement::root(id("z")),
            SceneEntityPlacement::root(id("a"))
        ],
    );
    assert_eq!(prefab.placements, scene.placements);
    assert_eq!(
        scene
            .entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>(),
        [id("a"), id("z")],
    );
    let sources = InMemoryPrefabSourceResolver::new();
    assert_eq!(
        compiled(scene.compile(&registry, &sources)).roots(),
        &[id("z"), id("a")],
    );
    assert_eq!(
        compiled(prefab.compile(&registry, &sources)).roots(),
        &[id("z"), id("a")],
    );
}

#[test]
fn repeated_root_bindings_merge_identity_components_children_and_references_without_wrappers() {
    let registry = registry();
    let fixture = repeated_fixture();
    let sources = fixture.sources();
    let before = fixture.scene.clone();
    let document = compiled(fixture.scene.compile(&registry, &sources));
    assert_repeated_document(&document);
    assert_native_spawn(document, &registry);
    assert_eq!(fixture.scene, before);
    assert_eq!(
        sources.resolve_prefab(&source("q")).unwrap().document(),
        &fixture.q,
    );
    assert_eq!(
        sources.resolve_prefab(&source("p")).unwrap().document(),
        &fixture.p,
    );
}

#[test]
fn materialized_entity_limit_counts_final_entities_instead_of_hidden_bound_root_facets() {
    let registry = registry();
    let fixture = repeated_fixture();
    let sources = fixture.sources();
    // The source skeletons have 5 + 2 * (2 + 3) = 15 declaration nodes. Each P:k
    // and Q:q pair aliases one containing anchor, leaving exactly eleven Runtime entities.
    let options = |maximum| {
        SceneCompilationOptions::default().with_limits(
            SceneCompilationLimits::default()
                .with_materialized_entities(ItemLimit::new(maximum).unwrap()),
        )
    };
    let exact = compiled(
        fixture
            .scene
            .compile_with_options(&registry, &sources, options(11)),
    );
    assert_repeated_document(&exact);
    assert_native_spawn(exact, &registry);

    let short = fixture
        .scene
        .compile_with_options(&registry, &sources, options(10));
    assert!(short.candidate.is_none());
    assert_eq!(
        short.failure.as_ref().map(|failure| *failure.reason()),
        Some(SceneCompilationFailureKind::WorkLimit),
    );
    let diagnostic = short
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded")
        .unwrap_or_else(|| {
            panic!(
                "missing final-entity budget diagnostic: {:?}",
                short.diagnostics
            )
        });
    let field = |key: &str| {
        diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == key)
            .unwrap()
            .value()
    };
    assert_eq!(
        field("budget-kind"),
        DiagnosticValueRef::Identifier("materialized-entities"),
    );
    assert_eq!(field("observed"), DiagnosticValueRef::Unsigned(11));
    assert_eq!(field("maximum"), DiagnosticValueRef::Unsigned(10));
}

#[test]
fn root_components_compose_source_then_inner_intent_then_complete_anchor_then_outer_intent() {
    let registry = registry();
    let fixture = repeated_fixture();
    let sources = fixture.sources();
    let q = compiled(fixture.q.compile(&registry, &sources));
    assert_position(&q, &id("q"), 1, 2);

    let mut anchor = SceneEntityRecord::new(id("k"));
    anchor.prefab = fixture
        .p
        .entities
        .iter()
        .find(|entity| entity.id == id("k"))
        .unwrap()
        .prefab
        .clone();
    let inner = compiled(SceneDocument::from_ordered_roots([anchor]).compile(&registry, &sources));
    assert_eq!(inner.entities().len(), 3);
    assert_position(&inner, &id("k"), 10, 2);
    assert_link(&inner, &id("k"), &id("k"));
    assert_native_spawn(inner, &registry);

    let p = compiled(fixture.p.compile(&registry, &sources));
    assert_eq!(p.entities().len(), 4);
    assert_position(&p, &id("k"), 20, 30);
    assert_link(&p, &id("k"), &id("k"));
    assert_native_spawn(p, &registry);

    let outer = compiled(fixture.scene.compile(&registry, &sources));
    assert_position(&outer, &id("z_left"), 40, 30);
    assert_position(&outer, &id("a_right"), 41, 30);
}

#[test]
fn containing_source_and_outer_overlay_can_both_interleave_local_and_inherited_children() {
    let registry = registry();
    let mut fixture = repeated_fixture();
    // P owns b and places it between Q's d1/d2 without modifying Q or declaring an addition.
    let p = compiled(fixture.p.compile(&registry, &fixture.sources()));
    assert_eq!(
        entity(&p, &id("k")).children(),
        &[
            projected_id(&p, "k", "d1"),
            id("b"),
            projected_id(&p, "k", "d2")
        ],
    );
    for anchor in &mut fixture.scene.entities {
        let Some(instance) = &mut anchor.prefab else {
            continue;
        };
        instance.overrides.as_mut().unwrap().placements = vec![SceneEntityPlacement::new(
            target(&[], "b"),
            ScenePlacement::after(
                ScenePlacementParent::Entity(target(&[], "k")),
                target(&[("k", "q")], "d2"),
            ),
        )];
    }
    let document = compiled(fixture.scene.compile(&registry, &fixture.sources()));
    for anchor in ["z_left", "a_right"] {
        assert_eq!(
            entity(&document, &id(anchor)).children(),
            &[
                projected_id(&document, anchor, "d1"),
                projected_id(&document, anchor, "d2"),
                projected_id(&document, anchor, "b"),
            ],
        );
    }
    assert_native_spawn(document, &registry);
}

#[test]
fn inherited_source_placements_are_optional_pins_and_unpinned_source_values_keep_propagating() {
    let registry = registry();
    for pin in [false, true] {
        for reverse_source in [false, true] {
            let mut q = q_document();
            if reverse_source {
                q.placements.swap(1, 2);
            }
            q.entities
                .iter_mut()
                .find(|entity| entity.id == id("d1"))
                .unwrap()
                .components
                .insert(position_type(), position(103, 104));
            let mut p = p_document();
            if pin {
                p.placements.push(SceneEntityPlacement::new(
                    target(&[("k", "q")], "d1"),
                    ScenePlacement::after(
                        ScenePlacementParent::Entity(target(&[("k", "q")], "q")),
                        target(&[("k", "q")], "d2"),
                    ),
                ));
            }
            let sources = InMemoryPrefabSourceResolver::new().with_prefab(source("q"), key("q"), q);
            let document = compiled(p.compile(&registry, &sources));
            let d1 = projected_id(&document, "k", "d1");
            let d2 = projected_id(&document, "k", "d2");
            let expected = if pin || reverse_source {
                [id("b"), d2, d1.clone()]
            } else {
                [d1.clone(), id("b"), d2]
            };
            assert_eq!(
                entity(&document, &id("k")).children(),
                &expected,
                "pin={pin}, reverse_source={reverse_source}",
            );
            assert_position(&document, &d1, 103, 104);
            assert_native_spawn(document, &registry);
        }
    }
}

#[test]
fn source_placement_cohorts_follow_declaration_order_under_a_normalized_root_parent() {
    let registry = registry();
    for slot in ["first", "last", "before", "after"] {
        let mut k = SceneEntityRecord::new(id("k"));
        k.prefab = Some(root_instance("q", "q"));
        let mut p = PrefabDocument::from_ordered_roots([k]);
        // These two local IDs intentionally sort opposite to their declaration order.
        for name in ["z_local", "a_local"] {
            p.entities.push(SceneEntityRecord::new(id(name)));
            let parent = ScenePlacementParent::Entity(target(&[("k", "q")], "q"));
            let placement = match slot {
                "first" => ScenePlacement::first(parent),
                "last" => ScenePlacement::last(parent),
                "before" => ScenePlacement::before(parent, target(&[("k", "q")], "d2")),
                "after" => ScenePlacement::after(parent, target(&[("k", "q")], "d1")),
                _ => unreachable!(),
            };
            p.placements
                .push(SceneEntityPlacement::new(target(&[], name), placement));
        }
        let sources =
            InMemoryPrefabSourceResolver::new().with_prefab(source("q"), key("q"), q_document());
        let document = compiled(p.compile(&registry, &sources));
        let d1 = projected_id(&document, "k", "d1");
        let d2 = projected_id(&document, "k", "d2");
        let expected = match slot {
            "first" => [id("z_local"), id("a_local"), d1, d2],
            "last" => [d1, d2, id("z_local"), id("a_local")],
            _ => [d1, id("z_local"), id("a_local"), d2],
        };
        assert_eq!(entity(&document, &id("k")).children(), &expected, "{slot}");
        assert_native_spawn(document, &registry);
    }
}

fn input() -> ReferenceInputId {
    ReferenceInputId::new("u").unwrap()
}

fn input_reference(route: &[(&str, &str)]) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: hops(route),
            target: AuthoredReferenceTarget::Input(input()),
        },
    }
}

fn with_argument(
    mut instance: PrefabInstance,
    base: &str,
    reference: EntityReference,
) -> PrefabInstance {
    instance.arguments = Some(PrefabReferenceArguments {
        base: key(base),
        values: BTreeMap::from([(input(), reference)]),
    });
    instance
}

fn lexical_fixture(choice: usize) -> Fixture {
    let mut q = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("q")).with_component(link_type(), link(input_reference(&[])))
    ]);
    q.inputs.insert(input());
    let mut instance = with_argument(root_instance("q", "q"), "q", local("b"));
    if choice == 1 || choice == 2 {
        let value = input_reference(if choice == 1 { &[] } else { &[("k", "q")] });
        let mut overlay = PrefabOverlay::new(key("q"));
        overlay.components.push(property(
            target(&[], "q"),
            link_type(),
            "target",
            ComponentValue::EntityReference(value),
        ));
        instance.overrides = Some(overlay);
    }
    let mut k = SceneEntityRecord::new(id("k"));
    k.prefab = Some(instance);
    if choice == 3 {
        k.components.insert(link_type(), link(input_reference(&[])));
    }
    let mut p = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            placement(target(&[], "b"), target(&[], "k")),
        ],
        [k, SceneEntityRecord::new(id("b"))],
    );
    p.inputs.insert(input());
    let mut records = vec![SceneEntityRecord::new(id("c"))];
    for name in ["left", "right"] {
        let mut anchor = SceneEntityRecord::new(id(name));
        anchor.prefab = Some(with_argument(root_instance("p", "k"), "p", local("c")));
        records.push(anchor);
    }
    Fixture {
        q,
        p,
        scene: SceneDocument::from_ordered_roots(records),
    }
}

#[test]
fn merged_root_facets_retain_distinct_lexical_payload_and_input_environments() {
    let registry = registry();
    for choice in 0..4 {
        let fixture = lexical_fixture(choice);
        let document = compiled(fixture.scene.compile(&registry, &fixture.sources()));
        assert_eq!(document.entities().len(), 5);
        assert_eq!(document.roots(), &[id("c"), id("left"), id("right")]);
        let mut previous_target = None;
        for anchor in ["left", "right"] {
            // Q's own u and an explicit Q.u route resolve to that occurrence's b. The
            // P-authored overlay's plain u and P's complete anchor record resolve to c.
            let expected = if choice == 0 || choice == 2 {
                projected_id(&document, anchor, "b")
            } else {
                id("c")
            };
            assert_link(&document, &id(anchor), &expected);
            assert_eq!(
                entity(&document, &id(anchor)).children(),
                &[projected_id(&document, anchor, "b")],
            );
            if choice == 0 || choice == 2 {
                assert_ne!(previous_target.as_ref(), Some(&expected));
            }
            previous_target = Some(expected);
        }
        assert_native_spawn(document, &registry);
    }
}

#[test]
fn nested_root_binding_chains_normalize_deep_self_and_entering_references_to_the_outer_anchor() {
    let registry = registry();
    let fixture = repeated_fixture();
    let mut r = SceneEntityRecord::new(id("r")).with_component(position_type(), position(70, 80));
    r.prefab = Some(root_instance("p", "k"));
    let r = PrefabDocument::from_ordered_roots([r]);
    let sources = fixture.sources().with_prefab(source("r"), key("r"), r);
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(root_instance("r", "r"));
    let reader = SceneEntityRecord::new(id("reader")).with_component(
        link_type(),
        link(routed_reference(
            &[("anchor", "r"), ("r", "p"), ("k", "q")],
            "q",
        )),
    );
    let document =
        compiled(SceneDocument::from_ordered_roots([anchor, reader]).compile(&registry, &sources));
    assert_eq!(document.entities().len(), 5);
    assert_eq!(document.roots(), &[id("anchor"), id("reader")]);
    assert_position(&document, &id("anchor"), 70, 80);
    assert_link(&document, &id("anchor"), &id("anchor"));
    assert_link(&document, &id("reader"), &id("anchor"));
    assert_eq!(
        entity(&document, &id("anchor")).children(),
        &[
            projected_id(&document, "anchor", "d1"),
            projected_id(&document, "anchor", "b"),
            projected_id(&document, "anchor", "d2"),
        ],
    );
    let provenance = &document.canonical_provenance()[&id("anchor")];
    assert!(provenance.projection_path().is_none());
    assert_eq!(provenance.local_entity_id(), &id("anchor"));
    assert_native_spawn(document, &registry);
}

#[test]
fn explicit_forest_keeps_one_or_multiple_roots_and_allows_source_owned_sibling_interleaving() {
    let registry = registry();
    for roots in [1, 2] {
        for interleave in [false, true] {
            let records = ["z_root", "a_root"].into_iter().take(roots).map(|name| {
                SceneEntityRecord::new(id(name)).with_component(link_type(), link(local(name)))
            });
            let prefab = PrefabDocument::from_ordered_roots(records);
            let sources = InMemoryPrefabSourceResolver::new().with_prefab(
                source("forest"),
                key("forest"),
                prefab,
            );
            let mut anchor = SceneEntityRecord::new(id("anchor"));
            anchor.prefab = Some(PrefabInstance::forest(source("forest")));
            let local_placement = if interleave {
                SceneEntityPlacement::new(
                    target(&[], "b"),
                    ScenePlacement::before(
                        ScenePlacementParent::Entity(target(&[], "anchor")),
                        target(
                            &[("anchor", "forest")],
                            if roots == 1 { "z_root" } else { "a_root" },
                        ),
                    ),
                )
            } else {
                placement(target(&[], "b"), target(&[], "anchor"))
            };
            let scene = SceneDocument::new(
                [SceneEntityPlacement::root(id("anchor")), local_placement],
                [anchor, SceneEntityRecord::new(id("b"))],
            );
            let document = compiled(scene.compile(&registry, &sources));
            assert_eq!(document.entities().len(), roots + 2);
            assert_eq!(document.roots(), &[id("anchor")]);
            let first = projected_id(&document, "anchor", "z_root");
            assert_ne!(
                first,
                id("anchor"),
                "Forest must not infer root binding from a single root",
            );
            assert_link(&document, &first, &first);
            let mut children = vec![first];
            if roots == 2 {
                let second = projected_id(&document, "anchor", "a_root");
                assert_link(&document, &second, &second);
                children.push(second);
            }
            children.insert(if interleave { roots - 1 } else { roots }, id("b"));
            assert_eq!(entity(&document, &id("anchor")).children(), &children);
            assert_native_spawn(document, &registry);
        }
    }
}

#[test]
fn root_binding_requires_the_specified_local_root_as_the_only_effective_root() {
    let registry = registry();
    for case in [
        "missing",
        "non_root",
        "extra_source",
        "suppressed",
        "replacement_addition",
        "extra_overlay",
    ] {
        let mut q = q_document();
        let selected = match case {
            "missing" => "missing",
            "non_root" => "d1",
            _ => "q",
        };
        let mut instance = root_instance("q", selected);
        let mut overlay = PrefabOverlay::new(key("q"));
        match case {
            "extra_source" => {
                q.entities.push(SceneEntityRecord::new(id("extra")));
                q.placements.push(SceneEntityPlacement::root(id("extra")));
            }
            "suppressed" | "replacement_addition" => {
                overlay.suppressions.push(target(&[], "q"));
                if case == "replacement_addition" {
                    overlay.additions.push(PrefabAddition::new(id("q")));
                    overlay.placements.push(SceneEntityPlacement::new(
                        PrefabEntityTarget::addition(id("q")),
                        ScenePlacement::last(ScenePlacementParent::Root),
                    ));
                }
            }
            "extra_overlay" => {
                overlay.additions.push(PrefabAddition::new(id("extra")));
                overlay.placements.push(SceneEntityPlacement::new(
                    PrefabEntityTarget::addition(id("extra")),
                    ScenePlacement::last(ScenePlacementParent::Root),
                ));
            }
            _ => {}
        }
        instance.overrides = Some(overlay);
        let sources =
            InMemoryPrefabSourceResolver::new().with_prefab(source("q"), key("q"), q.clone());
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(instance);
        let scene = SceneDocument::from_ordered_roots([anchor]);
        assert_refused(
            &scene,
            &sources,
            &registry,
            SceneCompilationFailureKind::RootBinding,
            case,
        );
        assert_eq!(
            sources.resolve_prefab(&source("q")).unwrap().document(),
            &q,
            "{case}",
        );
    }
}

#[test]
fn root_binding_checks_effective_roots_after_selected_suppressions() {
    let registry = registry();
    let mut q = q_document();
    q.entities.push(SceneEntityRecord::new(id("extra")));
    q.placements.push(SceneEntityPlacement::root(id("extra")));
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(source("q"), key("q"), q);
    let mut overlay = PrefabOverlay::new(key("q"));
    overlay.suppressions.push(target(&[], "extra"));
    let mut instance = root_instance("q", "q");
    instance.overrides = Some(overlay);
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(instance);
    let document =
        compiled(SceneDocument::from_ordered_roots([anchor]).compile(&registry, &sources));
    assert_eq!(document.entities().len(), 3);
    assert_eq!(document.roots(), &[id("anchor")]);
    assert_link(&document, &id("anchor"), &id("anchor"));
    assert_native_spawn(document, &registry);
}

#[test]
fn root_bindings_and_source_placement_routes_refuse_a_different_canonical_base() {
    let registry = registry();
    for wrong in ["outer_binding", "inner_binding", "source_route"] {
        let mut fixture = repeated_fixture();
        if wrong == "source_route" {
            let ScenePlacementSlot::Before(sibling) = &mut fixture.p.placements[1].placement.slot
            else {
                unreachable!()
            };
            sibling.through[0].expected_base = key("different-q");
        } else {
            let instance = if wrong == "outer_binding" {
                fixture
                    .scene
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == id("z_left"))
                    .unwrap()
                    .prefab
                    .as_mut()
                    .unwrap()
            } else {
                fixture
                    .p
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == id("k"))
                    .unwrap()
                    .prefab
                    .as_mut()
                    .unwrap()
            };
            let PrefabInstanceBinding::Root { base, .. } = &mut instance.binding else {
                unreachable!()
            };
            *base = key("different-base");
        }
        assert_refused(
            &fixture.scene,
            &fixture.sources(),
            &registry,
            SceneCompilationFailureKind::SourceChanged,
            wrong,
        );
    }
}

#[test]
fn source_local_placements_are_required_even_when_instance_hierarchy_could_supply_an_order() {
    let registry = registry();
    for owner in ["scene", "prefab"] {
        let mut fixture = repeated_fixture();
        if owner == "scene" {
            fixture
                .scene
                .placements
                .retain(|placement| placement.target != target(&[], "q"));
        } else {
            fixture
                .p
                .placements
                .retain(|placement| placement.target != target(&[], "b"));
        }
        let report = fixture.scene.compile(&registry, &fixture.sources());
        assert!(report.candidate.is_none());
        assert!(report.diagnostics.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code().as_str() == "scene.missing-placement" }),
            "{owner}: {:?}",
            report.diagnostics,
        );
    }
}

#[test]
fn source_and_overlay_placements_normalize_root_aliases_before_duplicate_self_and_cycle_checks() {
    let registry = registry();
    for owner in ["source", "overlay"] {
        for conflict in [
            "duplicate",
            "before_self",
            "after_self",
            "self_parent",
            "parent_cycle",
            "order_cycle",
        ] {
            let mut fixture = repeated_fixture();
            let root = target(&[], "k");
            let alias = target(&[("k", "q")], "q");
            let mut declarations = vec![SceneEntityPlacement::root(id("k"))];
            match conflict {
                "duplicate" => declarations.push(SceneEntityPlacement::new(
                    alias.clone(),
                    ScenePlacement::last(ScenePlacementParent::Root),
                )),
                "before_self" | "after_self" => {
                    declarations[0].placement.slot = if conflict == "before_self" {
                        ScenePlacementSlot::Before(alias.clone())
                    } else {
                        ScenePlacementSlot::After(alias.clone())
                    };
                }
                "self_parent" => {
                    declarations[0].placement.parent = ScenePlacementParent::Entity(alias.clone());
                }
                "parent_cycle" => {
                    declarations[0].placement.parent =
                        ScenePlacementParent::Entity(target(&[("k", "q")], "d1"));
                }
                "order_cycle" => {
                    declarations[0].placement.slot = ScenePlacementSlot::Before(target(&[], "b"));
                    declarations.push(SceneEntityPlacement::new(
                        target(&[], "b"),
                        ScenePlacement::before(ScenePlacementParent::Root, alias),
                    ));
                }
                _ => unreachable!(),
            }
            if conflict != "order_cycle" {
                declarations.push(placement(target(&[], "b"), root));
            }
            if owner == "source" {
                fixture.p.placements = declarations;
            } else {
                let mut overlay = PrefabOverlay::new(key("p"));
                overlay.placements = declarations;
                for entity in &mut fixture.scene.entities {
                    if let Some(instance) = &mut entity.prefab {
                        instance.overrides = Some(overlay.clone());
                    }
                }
            }
            assert_refused(
                &fixture.scene,
                &fixture.sources(),
                &registry,
                if conflict == "duplicate" {
                    SceneCompilationFailureKind::DuplicateIntent
                } else {
                    SceneCompilationFailureKind::Placement
                },
                &format!("{owner}: {conflict}"),
            );
        }
    }
}

#[test]
fn component_claims_through_different_root_facets_still_collide_on_one_actual_entity() {
    let registry = registry();
    let mut fixture = repeated_fixture();
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay.components = vec![
        pin_x(target(&[], "k"), 100),
        pin_x(target(&[("k", "q")], "q"), 200),
    ];
    for entity in &mut fixture.scene.entities {
        if let Some(instance) = &mut entity.prefab {
            instance.overrides = Some(overlay.clone());
        }
    }
    assert_refused(
        &fixture.scene,
        &fixture.sources(),
        &registry,
        SceneCompilationFailureKind::DuplicateIntent,
        "component root aliases",
    );
}

#[cfg(feature = "serde")]
mod serialization {
    use super::*;
    use nara_scene::{PrefabDocumentCandidate, SceneDocumentCandidate};

    #[derive(Debug, Clone, Copy)]
    enum Encoding {
        Json,
        Ron,
    }

    impl Encoding {
        fn scene(self, document: &SceneDocument) -> SceneDocument {
            let candidate = match self {
                Self::Json => {
                    SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
                }
                Self::Ron => {
                    SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap())
                }
            }
            .unwrap();
            candidate.into_composition_source().unwrap()
        }

        fn prefab(self, document: &PrefabDocument) -> PrefabDocument {
            let candidate = match self {
                Self::Json => {
                    PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
                }
                Self::Ron => {
                    PrefabDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap())
                }
            }
            .unwrap();
            candidate.into_composition_source().unwrap()
        }

        fn fixture(self, fixture: &Fixture) -> Fixture {
            Fixture {
                q: self.prefab(&fixture.q),
                p: self.prefab(&fixture.p),
                scene: self.scene(&fixture.scene),
            }
        }
    }

    #[test]
    fn json_and_ron_roundtrip_source_placements_bindings_overlays_and_actual_spawn() {
        let registry = registry();
        let fixture = repeated_fixture();
        let original = compiled(fixture.scene.compile(&registry, &fixture.sources()));
        for encoding in [Encoding::Json, Encoding::Ron] {
            let decoded = encoding.fixture(&fixture);
            assert_eq!(decoded.q, fixture.q, "{encoding:?}");
            assert_eq!(decoded.p, fixture.p, "{encoding:?}");
            assert_eq!(decoded.scene, fixture.scene, "{encoding:?}");
            let document = compiled(decoded.scene.compile(&registry, &decoded.sources()));
            assert_eq!(document, original, "{encoding:?}");
            assert_repeated_document(&document);
            assert_native_spawn(document, &registry);
        }
    }

    #[test]
    fn json_and_ron_preserve_distinct_root_facet_inputs_and_lexical_arguments() {
        let registry = registry();
        for choice in 0..4 {
            let fixture = lexical_fixture(choice);
            let original = compiled(fixture.scene.compile(&registry, &fixture.sources()));
            for encoding in [Encoding::Json, Encoding::Ron] {
                let decoded = encoding.fixture(&fixture);
                assert_eq!(decoded.q, fixture.q, "{encoding:?}");
                assert_eq!(decoded.p, fixture.p, "{encoding:?}");
                assert_eq!(decoded.scene, fixture.scene, "{encoding:?}");
                let document = compiled(decoded.scene.compile(&registry, &decoded.sources()));
                assert_eq!(document, original, "choice={choice}, {encoding:?}");
                assert_native_spawn(document, &registry);
            }
        }
    }

    #[test]
    fn source_wire_shape_owns_placements_and_requires_an_explicit_instance_binding() {
        let fixture = repeated_fixture();
        for source in [
            fixture.scene.to_json_string().unwrap(),
            fixture.p.to_json_string().unwrap(),
            fixture.q.to_json_string().unwrap(),
        ] {
            let wire: serde_json::Value = serde_json::from_str(&source).unwrap();
            let payload = &wire["payload"];
            assert!(payload["placements"].is_array());
            assert!(payload.get("roots").is_none());
            for entity in payload["entities"].as_array().unwrap() {
                assert!(entity.get("children").is_none());
                assert!(entity.get("parent").is_none());
            }
            for placement in payload["placements"].as_array().unwrap() {
                assert!(placement.get("target").is_some());
                assert!(placement.get("placement").is_some());
            }
        }
        for instance in [PrefabInstance::forest(source("q")), root_instance("q", "q")] {
            let mut json = serde_json::to_value(&instance).unwrap();
            assert_eq!(
                serde_json::from_value::<PrefabInstance>(json.clone()).unwrap(),
                instance,
            );
            let ron = ron::ser::to_string(&instance).unwrap();
            assert_eq!(ron::from_str::<PrefabInstance>(&ron).unwrap(), instance);
            json.as_object_mut().unwrap().remove("binding");
            assert!(
                serde_json::from_value::<PrefabInstance>(json).is_err(),
                "missing mode must not infer Forest from root count",
            );
        }
    }
}
