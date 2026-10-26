//! Component presence is checked by layer; only surviving value programs are interpreted.

use std::convert::Infallible;

use nara_asset::AssetRef;
use nara_ecs::{Component, World};
use nara_identity::EntityReference;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentRegistry, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabComponentIntent, PrefabComponentSetMode, PrefabDocument,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabExpansionReport, PrefabInstance,
    PrefabInstanceHop, PrefabOverlay, PrefabProjectionStep, PrefabPropertyClaim,
    PrefabPropertyIntent, PrefabReferenceBinding, PrefabSourceResolver, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord, SceneOccurrenceKey, SourceDocumentKey,
    SourceDocumentKind, spawn_scene,
};

const UNIVERSE: &str = "ec3de96a-21d9-4ab1-a6e2-f07391d6ce48";

#[derive(Debug, PartialEq, Component)]
struct Amount(i64);

#[derive(Debug, PartialEq, Component)]
struct Link(EntityReference);

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/deferred/{name}.prefab.ron")).unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        UNIVERSE.parse().unwrap(),
        format!("canonical.deferred.{name}"),
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn amount_type() -> ComponentTypeId {
    ComponentTypeId::new("test.DeferredAmount")
}

fn link_type() -> ComponentTypeId {
    ComponentTypeId::new("test.DeferredLink")
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<Amount, _, _>(
            ComponentSchema::new(amount_type(), "Amount", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("amount"),
                    "Amount",
                    ComponentFieldPath::from_fields(["amount"]),
                    ComponentValueKind::I64,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            |value| Ok(Amount(value.field_i64("amount")?)),
            |value| {
                Ok(ComponentValue::map([(
                    "amount",
                    ComponentValue::I64(value.0),
                )]))
            },
        )
        .unwrap();
    registry
        .register_persistent_component_with_codec::<Link, _, _>(
            ComponentSchema::new(link_type(), "Link", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("target"),
                    "Target",
                    ComponentFieldPath::from_fields(["target"]),
                    ComponentValueKind::EntityRef,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_capability(ComponentCapability::EntityRef)]),
            |value| Ok(Link(value.field_entity_reference("target")?.clone())),
            |value| {
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(value.0.clone()),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn amount(value: i64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("amount", ComponentValue::I64(value))]),
    )
}

fn invalid_amount() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("amount", ComponentValue::String("not an integer".into()))]),
    )
}

fn local_reference(entity: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(entity) })
}

fn link(entity: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", local_reference(entity))]),
    )
}

fn instance(entity: &str, base: &str, overrides: Option<PrefabOverlay>) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(entity));
    record.prefab = Some(PrefabInstance {
        source: source(base),
        overrides,
    });
    record
}

fn scene(base: &str, overrides: Option<PrefabOverlay>) -> SceneDocument {
    SceneDocument::from_ordered_roots([instance("instance", base, overrides)])
}

fn sources(
    entries: impl IntoIterator<Item = (&'static str, PrefabDocument)>,
) -> InMemoryPrefabSourceResolver {
    entries.into_iter().fold(
        InMemoryPrefabSourceResolver::new(),
        |resolver, (name, document)| resolver.with_prefab(source(name), key(name), document),
    )
}

fn target(through: &[(&str, &str)], entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: through
            .iter()
            .map(|(anchor, base)| PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: key(base),
            })
            .collect(),
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    }
}

fn set_amount(
    target: PrefabEntityTarget,
    mode: PrefabComponentSetMode,
    value: i64,
) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode,
        target,
        component: amount_type(),
        record: amount(value),
        bindings: Vec::new(),
    }
}

fn pin_amount(target: PrefabEntityTarget, field: &str, value: i64) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target,
        component: amount_type(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new(field)),
            intent: PrefabPropertyIntent::Set(ComponentValue::I64(value)),
            bindings: Vec::new(),
        }],
    }
}

fn expand(
    registry: &ComponentRegistry,
    document: &SceneDocument,
    resolver: &InMemoryPrefabSourceResolver,
    expected_sources: &[&str],
) -> PrefabExpansionReport {
    let before_document = document.clone();
    let before_sources = resolver.clone();
    let mut requested = Vec::new();
    let report = document
        .try_expand_prefabs_with_loader(registry, Default::default(), None, |request, _| {
            requested.push(request.clone());
            Ok::<_, Infallible>(resolver.resolve_prefab(request))
        })
        .unwrap();
    assert_eq!(
        requested,
        expected_sources
            .iter()
            .map(|name| source(name))
            .collect::<Vec<_>>(),
        "{:?}",
        report.diagnostics
    );
    assert_eq!(
        document, &before_document,
        "expansion must preserve source intent"
    );
    assert_eq!(
        resolver, &before_sources,
        "expansion must preserve captured bases"
    );
    report
}

fn expanded(report: &PrefabExpansionReport) -> &SceneDocument {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.document().expect("effective values must expand")
}

fn assert_rejected(report: &PrefabExpansionReport) {
    assert!(report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.candidate.is_none());
    assert!(report.document().is_none());
    assert!(report.provenance().is_none());
}

fn projected_id(report: &PrefabExpansionReport, path: &[(&str, &str)]) -> SceneEntityId {
    expanded(report);
    let steps = path
        .iter()
        .map(|(base, entity)| PrefabProjectionStep::SourceEntity {
            source_document: source(base),
            canonical_source: key(base),
            entity: id(entity),
        })
        .collect::<Vec<_>>();
    let mut matches = report
        .provenance()
        .unwrap()
        .iter()
        .filter(|(_, provenance)| {
            provenance.projection_path().is_some_and(|projection| {
                projection.root_anchor() == &id("instance")
                    && projection.steps() == steps.as_slice()
            })
        });
    let (entity, provenance) = matches.next().expect("expected typed source projection");
    assert!(matches.next().is_none(), "source projection must be unique");
    assert_eq!(
        provenance.local_entity_id(),
        steps.last().unwrap().entity_id()
    );
    entity.clone()
}

fn entity<'a>(report: &'a PrefabExpansionReport, entity: &SceneEntityId) -> &'a SceneEntityRecord {
    expanded(report)
        .entities
        .iter()
        .find(|record| &record.id == entity)
        .expect("expected materialized entity")
}

fn materialize(registry: &ComponentRegistry, report: PrefabExpansionReport) -> World {
    expanded(&report);
    let containing_source = SourceDocumentKey::named(
        UNIVERSE.parse().unwrap(),
        "canonical.deferred.scene",
        SourceDocumentKind::Scene,
    )
    .unwrap();
    let candidate = report.candidate.unwrap().bind_source(containing_source);
    let mut world = World::new();
    let report = spawn_scene(
        &mut world,
        registry,
        &candidate,
        &SceneOccurrenceKey::new("nara.scene.tests", "deferred-values").unwrap(),
        None,
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.instance.is_some(), "{:?}", report.diagnostics);
    world
}

fn assert_native_amount(registry: &ComponentRegistry, report: PrefabExpansionReport, value: i64) {
    let world = materialize(registry, report);
    let amounts = world
        .iter_entities()
        .filter_map(|entity| entity.get::<Amount>().map(|amount| amount.0))
        .collect::<Vec<_>>();
    assert_eq!(amounts, [value]);
}

#[test]
fn complete_replace_discards_invalid_or_unsupported_inherited_payload() {
    let registry = registry();
    let mut unsupported = amount(3);
    unsupported.version = ComponentSchemaVersion::new(2).unwrap();
    for inherited in [invalid_amount(), unsupported] {
        let resolver = sources([(
            "base",
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("root")).with_component(amount_type(), inherited)
            ]),
        )]);
        let mut overlay = PrefabOverlay::new(key("base"));
        overlay.components.push(set_amount(
            target(&[], "root"),
            PrefabComponentSetMode::Replace,
            42,
        ));
        let document = scene("base", Some(overlay));

        let report = expand(&registry, &document, &resolver, &["base"]);

        let root = projected_id(&report, &[("base", "root")]);
        assert_eq!(
            entity(&report, &root).components[&amount_type()],
            amount(42)
        );
        assert_native_amount(&registry, report, 42);
    }
}

#[test]
fn complete_suppress_discards_invalid_and_unknown_inherited_payloads() {
    let registry = registry();
    let unknown = ComponentTypeId::new("test.UnavailableDeferredComponent");
    let resolver = sources([(
        "base",
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("root"))
            .with_component(amount_type(), invalid_amount())
            .with_component(
                unknown.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("preserved opaque payload".into()),
                ),
            )]),
    )]);
    let mut overlay = PrefabOverlay::new(key("base"));
    for component in [amount_type(), unknown] {
        overlay.components.push(PrefabComponentIntent::Suppress {
            target: target(&[], "root"),
            component,
        });
    }
    let document = scene("base", Some(overlay));

    let report = expand(&registry, &document, &resolver, &["base"]);

    let root = projected_id(&report, &[("base", "root")]);
    assert!(entity(&report, &root).components.is_empty());
    let world = materialize(&registry, report);
    assert_eq!(
        world
            .iter_entities()
            .filter(|entity| entity.contains::<nara_scene::SceneEntitySource>())
            .count(),
        2,
    );
    assert!(
        world
            .iter_entities()
            .all(|entity| entity.get::<Amount>().is_none())
    );
}

#[test]
fn outer_replace_discards_a_stale_inner_property_program() {
    let registry = registry();
    let mut inner = PrefabOverlay::new(key("leaf"));
    inner
        .components
        .push(pin_amount(target(&[], "root"), "retired_field", 13));
    let resolver = sources([
        (
            "leaf",
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("root")).with_component(amount_type(), amount(3))
            ]),
        ),
        (
            "base",
            PrefabDocument::from_ordered_roots([instance("nested", "leaf", Some(inner))]),
        ),
    ]);
    let inherited = scene("base", None);
    let retained = expand(&registry, &inherited, &resolver, &["base", "leaf"]);
    assert_rejected(&retained);

    let mut outer = PrefabOverlay::new(key("base"));
    outer.components.push(set_amount(
        target(&[("nested", "leaf")], "root"),
        PrefabComponentSetMode::Replace,
        42,
    ));
    let document = scene("base", Some(outer));

    let report = expand(&registry, &document, &resolver, &["base", "leaf"]);

    let root = projected_id(&report, &[("base", "nested"), ("leaf", "root")]);
    assert_eq!(
        entity(&report, &root).components[&amount_type()],
        amount(42)
    );
    assert_native_amount(&registry, report, 42);
}

#[test]
fn outer_replace_cannot_legalize_an_earlier_invalid_add_or_replace_presence() {
    let registry = registry();
    for mode in [PrefabComponentSetMode::Add, PrefabComponentSetMode::Replace] {
        let mut root = SceneEntityRecord::new(id("root"));
        if mode == PrefabComponentSetMode::Add {
            root = root.with_component(amount_type(), amount(3));
        }
        let mut invalid_inner = PrefabOverlay::new(key("leaf"));
        invalid_inner
            .components
            .push(set_amount(target(&[], "root"), mode, 4));

        // Without the invalid earlier claim, this layer establishes valid outer presence.
        // In particular, the absent-base Replace case must not fail only at the final Replace.
        let mut middle = PrefabOverlay::new(key("inner"));
        middle.components.push(set_amount(
            target(&[("inner_anchor", "leaf")], "root"),
            if mode == PrefabComponentSetMode::Add {
                PrefabComponentSetMode::Replace
            } else {
                PrefabComponentSetMode::Add
            },
            6,
        ));
        let mut outer = PrefabOverlay::new(key("middle"));
        outer.components.push(set_amount(
            target(
                &[("middle_anchor", "inner"), ("inner_anchor", "leaf")],
                "root",
            ),
            PrefabComponentSetMode::Replace,
            42,
        ));
        let document = scene("middle", Some(outer));
        let mut resolver = sources([
            ("leaf", PrefabDocument::from_ordered_roots([root])),
            (
                "inner",
                PrefabDocument::from_ordered_roots([instance("inner_anchor", "leaf", None)]),
            ),
            (
                "middle",
                PrefabDocument::from_ordered_roots([instance(
                    "middle_anchor",
                    "inner",
                    Some(middle),
                )]),
            ),
        ]);
        let control = expand(
            &registry,
            &document,
            &resolver,
            &["middle", "inner", "leaf"],
        );
        let root = projected_id(
            &control,
            &[
                ("middle", "middle_anchor"),
                ("inner", "inner_anchor"),
                ("leaf", "root"),
            ],
        );
        assert_eq!(
            entity(&control, &root).components[&amount_type()],
            amount(42)
        );
        assert_native_amount(&registry, control, 42);
        resolver.insert(
            source("inner"),
            key("inner"),
            PrefabDocument::from_ordered_roots([instance(
                "inner_anchor",
                "leaf",
                Some(invalid_inner),
            )]),
        );

        let report = expand(
            &registry,
            &document,
            &resolver,
            &["middle", "inner", "leaf"],
        );

        assert_rejected(&report);
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.prefab-overlay-conflict"
            }),
            "invalid {mode:?} presence must remain a conflict: {:?}",
            report.diagnostics
        );
    }
}

#[test]
fn superseded_complete_and_property_bindings_do_not_request_a_nested_source() {
    let registry = registry();
    for property_binding in [false, true] {
        let binding = PrefabReferenceBinding {
            slot: if property_binding {
                ComponentFieldPath::empty()
            } else {
                ComponentFieldPath::from_fields(["target"])
            },
            expected: id("inner_spelling"),
            target: target(&[("unused", "unavailable")], "target"),
        };
        let mut inner = PrefabOverlay::new(key("leaf"));
        inner.components.push(if property_binding {
            PrefabComponentIntent::Properties {
                target: target(&[], "root"),
                component: link_type(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    intent: PrefabPropertyIntent::Set(local_reference("inner_spelling")),
                    bindings: vec![binding],
                }],
            }
        } else {
            PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Replace,
                target: target(&[], "root"),
                component: link_type(),
                record: link("inner_spelling"),
                bindings: vec![binding],
            }
        });
        let resolver = sources([
            (
                "leaf",
                PrefabDocument::from_ordered_roots([
                    SceneEntityRecord::new(id("root")).with_component(link_type(), link("root")),
                    instance("unused", "unavailable", None),
                ]),
            ),
            (
                "base",
                PrefabDocument::from_ordered_roots([instance("nested", "leaf", Some(inner))]),
            ),
        ]);
        let mut outer = PrefabOverlay::new(key("base"));
        // The slot has no live structural demand; only a retained binding could request it.
        outer
            .suppressions
            .push(target(&[("nested", "leaf")], "unused"));
        outer.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: target(&[("nested", "leaf")], "root"),
            component: link_type(),
            record: link("outer_spelling"),
            bindings: vec![PrefabReferenceBinding {
                slot: ComponentFieldPath::from_fields(["target"]),
                expected: id("outer_spelling"),
                target: target(&[("nested", "leaf")], "root"),
            }],
        });
        let document = scene("base", Some(outer));

        let report = expand(&registry, &document, &resolver, &["base", "leaf"]);

        let root = projected_id(&report, &[("base", "nested"), ("leaf", "root")]);
        let expected = EntityReference::SceneLocal {
            entity: root.clone(),
        };
        assert_eq!(
            entity(&report, &root).components[&link_type()]
                .value
                .field_entity_reference("target")
                .unwrap(),
            &expected
        );
        assert_eq!(expanded(&report).entities.len(), 3);
        let world = materialize(&registry, report);
        let links = world
            .iter_entities()
            .filter_map(|entity| entity.get::<Link>().map(|link| link.0.clone()))
            .collect::<Vec<_>>();
        assert_eq!(links, [expected]);
    }
}

#[test]
fn retained_properties_require_an_interpretable_base_even_when_pinning_the_bad_field() {
    let registry = registry();
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay
        .components
        .push(pin_amount(target(&[], "root"), "amount", 42));
    let document = scene("base", Some(overlay));
    let mut resolver = sources([(
        "base",
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("root")).with_component(amount_type(), amount(3))
        ]),
    )]);
    let control = expand(&registry, &document, &resolver, &["base"]);
    let root = projected_id(&control, &[("base", "root")]);
    assert_eq!(
        entity(&control, &root).components[&amount_type()],
        amount(42)
    );
    assert_native_amount(&registry, control, 42);
    resolver.insert(
        source("base"),
        key("base"),
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("root")).with_component(amount_type(), invalid_amount())
        ]),
    );

    let report = expand(&registry, &document, &resolver, &["base"]);

    assert_rejected(&report);
}
