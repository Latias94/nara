//! Actual unpublished source construction consumed by the shared authoring compiler.
//! Source-local admission below grants no complete-project or publication authority.

use std::{collections::BTreeMap, convert::Infallible, sync::Arc};

use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::SceneComponentRecord;
use crate::{
    __private::AuthoringAdapterReceipt,
    AuthoringDocumentSource, ExpandedSceneDocument, PrefabComponentIntent, PrefabDocumentCandidate,
    PrefabEntityOrigin, PrefabInstanceHop, PrefabOverlay, PrefabProjectionStep,
    PrefabPropertyClaim, PrefabPropertyIntent, ResolvedPrefabSource, SceneCompilationLimits,
    SceneEntityId, SceneEntityPlacement, ScenePlacement, ScenePlacementParent, SourceDocumentKind,
    prefab::{
        extraction::try_capture_source_cut_with_loader, try_compile_authoring_scene_with_loader,
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "7dcc5a96-240a-48c9-99b8-3f6bbc372758".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/extraction-construction/{value}.ron")).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.ExtractionConstructionLink")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.extraction-construction").unwrap()
}

fn local(entity: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(entity) }
}

fn input(name: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference::input(ReferenceInputId::new(name).unwrap()),
    }
}

fn target(route: &[(&str, &str)], entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: route
            .iter()
            .map(|(anchor, base)| PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: key(base),
            })
            .collect(),
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    }
}

fn reference(route: &[(&str, &str)], entity: &str) -> EntityReference {
    let target = target(route, entity);
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: target.through,
            target: AuthoredReferenceTarget::Entity(target.entity),
        },
    }
}

fn basis() -> AuthoringSchemaBasis {
    let target = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueShape::EntityRef,
        ComponentValue::EntityReference(local("schema-default")),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let external = ComponentFieldSchema::required(
        ComponentFieldId::new("external"),
        "External",
        ComponentFieldPath::from_fields(["external"]),
        ComponentValueShape::EntityRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let score = ComponentFieldSchema::required(
        ComponentFieldId::new("score"),
        "Score",
        ComponentFieldPath::from_fields(["score"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    let schema = ComponentSchema::new(
        component(),
        "Construction link",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([target, external, score]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    assert!(basis.has_authoring_adapter(&component()));
    assert!(!basis.has_native_preview_adapter(&component()));
    basis
}

fn record(target: EntityReference, external: EntityReference, score: u64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            ("target", ComponentValue::EntityReference(target)),
            ("external", ComponentValue::EntityReference(external)),
            ("score", ComponentValue::U64(score)),
        ]),
    )
    .with_owner(owner())
    .with_contextual_default(
        ComponentFieldId::new("target"),
        ComponentValue::EntityReference(local("x")),
    )
}

fn under(child: PrefabEntityTarget, parent: PrefabEntityTarget) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        child,
        ScenePlacement::last(ScenePlacementParent::Entity(parent)),
    )
}

fn original() -> PrefabDocument {
    // Record order deliberately differs from both canonical ID order and hierarchy order.
    PrefabDocument {
        inputs: [ReferenceInputId::new("u").unwrap()].into(),
        placements: vec![
            SceneEntityPlacement::root(id("outer")),
            under(target(&[], "x"), target(&[], "outer")),
            SceneEntityPlacement::new(
                target(&[], "k"),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(target(&[], "outer")),
                    target(&[], "x"),
                ),
            ),
            under(target(&[], "b"), target(&[], "k")),
        ],
        entities: vec![
            SceneEntityRecord::new(id("x"))
                .with_component(component(), record(local("b"), input("u"), 3)),
            SceneEntityRecord::new(id("b"))
                .with_component(component(), record(local("k"), local("x"), 2)),
            SceneEntityRecord::new(id("outer")),
            SceneEntityRecord::new(id("k"))
                .with_component(component(), record(local("b"), input("u"), 1)),
        ],
    }
}

fn consumer() -> SceneDocument {
    let mut instance = PrefabInstance::forest(request("p"));
    instance.arguments = Some(PrefabReferenceArguments {
        base: key("p"),
        values: [(ReferenceInputId::new("u").unwrap(), local("socket"))].into(),
    });
    let mut host = SceneEntityRecord::new(id("host"));
    host.prefab = Some(instance);
    SceneDocument::from_ordered_roots([host, SceneEntityRecord::new(id("socket"))])
}

fn load(
    reference: &AssetRef,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> Option<ResolvedPrefabSource> {
    let (name, document) = sources
        .iter()
        .find(|(name, _)| request(name) == *reference)?;
    Some(ResolvedPrefabSource::bound_authoring_source(
        Arc::clone(document),
        basis.clone(),
        key(name),
    ))
}

fn capture<'a>(
    document: &'a PrefabDocument,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> (SourceCut<'a>, Option<usize>) {
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(document),
        &id("k"),
        basis,
        Default::default(),
        None,
        |reference, _| Ok::<_, Infallible>(load(reference, basis, sources)),
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.budget_failed);
    assert!(report.work_bytes.is_some_and(|bytes| bytes > 0));
    (report.cut.unwrap(), report.work_bytes)
}

fn compile(
    scene: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> ExpandedSceneDocument {
    let report = try_compile_authoring_scene_with_loader(
        scene,
        basis,
        Default::default(),
        None,
        |reference, _| Ok::<_, Infallible>(load(reference, basis, sources)),
    )
    .unwrap();
    assert!(!report.diagnostics().has_errors(), "{report:?}");
    assert!(report.failure().is_none(), "{report:?}");
    assert!(!report.budget_exceeded());
    report.into_parts().candidate.unwrap()
}

type Observation = BTreeMap<&'static str, (Vec<&'static str>, Option<ComponentValue>)>;

fn observe(
    document: &ExpandedSceneDocument,
    routes: &[(&'static str, &[(&str, &str)])],
) -> Observation {
    let mut names = BTreeMap::from([(id("host"), "host"), (id("socket"), "socket")]);
    for (name, route) in routes {
        let steps = route
            .iter()
            .map(|(base, entity)| PrefabProjectionStep::SourceEntity {
                source_document: request(base),
                canonical_source: key(base),
                entity: id(entity),
            })
            .collect::<Vec<_>>();
        let mut matches = document.provenance().iter().filter(|(_, origin)| {
            origin.projection_path().is_some_and(|path| {
                path.root_anchor() == &id("host") && path.steps() == steps.as_slice()
            })
        });
        let (entity, _) = matches.next().expect("exact typed occurrence must exist");
        assert!(matches.next().is_none());
        assert!(names.insert(entity.clone(), *name).is_none());
    }
    assert_eq!(document.entities().len(), names.len());
    assert_eq!(document.provenance().len(), names.len());
    assert_eq!(document.roots(), &[id("host"), id("socket")]);
    for name in ["host", "socket"] {
        let origin = &document.provenance()[&id(name)];
        assert_eq!(origin.local_entity_id(), &id(name));
        assert!(origin.projection_path().is_none());
    }
    document
        .entities()
        .iter()
        .map(|entity| {
            assert!(entity.components().len() <= 1);
            let value = entity.components().get(&component()).map(|record| {
                assert_eq!(record.owner(), Some(&owner()));
                assert_eq!(record.version, ComponentSchemaVersion::ONE);
                let ComponentValue::Map(mut fields) = record.value.clone() else {
                    panic!("schema map")
                };
                for field in ["target", "external"] {
                    let ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) =
                        &fields[field]
                    else {
                        panic!("compiler must lower the actual reference")
                    };
                    let value = ComponentValue::EntityReference(local(names[entity]));
                    fields.insert(field.into(), value);
                }
                ComponentValue::Map(fields)
            });
            (
                names[entity.id()],
                (
                    entity.children().iter().map(|child| names[child]).collect(),
                    value,
                ),
            )
        })
        .collect()
}

fn reopen(
    document: &PrefabDocument,
    name: &str,
    basis: &AuthoringSchemaBasis,
    json: bool,
) -> PrefabDocument {
    let candidate = if json {
        PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap()
    } else {
        PrefabDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap()).unwrap()
    };
    let published = candidate
        .admit_authoring_with_basis(AuthoringDocumentSource::new(request(name)), basis)
        .unwrap()
        .publish();
    assert_eq!(published.schema_basis(), *basis);
    assert_eq!(published.document(), document);
    published.document().clone()
}

#[test]
fn affected_unknown_or_unbound_default_meaning_refuses_before_any_source_change() {
    let basis = basis();
    for case in [
        "missing_context",
        "unknown_schema",
        "wrong_owner",
        "wrong_version",
    ] {
        let mut p = original();
        let record = p
            .entities
            .iter_mut()
            .find(|entity| entity.id == id("b"))
            .unwrap()
            .components
            .get_mut(&component())
            .unwrap();
        match case {
            "missing_context" => record.contextual_defaults.clear(),
            "wrong_owner" => {
                record.owner =
                    Some(PersistentComponentSchemaOwnerId::try_new("game.other-owner").unwrap())
            }
            "wrong_version" => record.version = ComponentSchemaVersion(2),
            "unknown_schema" => {
                let raw = record.clone();
                p.entities
                    .iter_mut()
                    .find(|entity| entity.id == id("b"))
                    .unwrap()
                    .components
                    .insert(ComponentTypeId::new("game.UnknownMovedMeaning"), raw);
            }
            _ => unreachable!(),
        }
        let before = p.clone();
        let (cut, captured) = capture(&p, &basis, &[]);
        let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
        let result =
            construct_source_split(&cut, &key("p"), &key("q"), &request("q"), &basis, &mut work);
        match case {
            "missing_context" => assert!(matches!(result, Err(ExtractionError::Value(_)))),
            "wrong_owner" => assert!(matches!(result, Err(ExtractionError::OwnerMismatch))),
            "wrong_version" => assert!(matches!(result, Err(ExtractionError::VersionMismatch))),
            "unknown_schema" => assert!(matches!(result, Err(ExtractionError::UnknownSchema))),
            _ => unreachable!(),
        }
        assert_eq!(p, before);
    }
}

#[test]
fn actual_non_root_split_preserves_values_inputs_hidden_defaults_and_reopened_absent() {
    let basis = basis();
    let basis_before = basis.clone();
    let p = original();
    let before = p.clone();
    let (cut, captured) = capture(&p, &basis, &[]);
    assert!(std::ptr::eq(cut.root(), &p.entities[3]));
    assert_eq!(
        cut.entities()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "k"]
    );
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let split =
        construct_source_split(&cut, &key("p"), &key("q"), &request("q"), &basis, &mut work)
            .unwrap();
    assert!(
        matches!(split.original, SourceDocumentInput::Prefab(source) if std::ptr::eq(source, &p))
    );
    assert_eq!(split.basis, basis_before);
    let SplitSourceDocument::Prefab(after) = &split.containing else {
        panic!("Prefab kind must stay intact")
    };
    assert_eq!(after.inputs, p.inputs);
    assert_eq!(
        after
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        ["k", "outer", "x"]
    );
    assert!(after.entities[0].components.is_empty());
    assert_eq!(
        split
            .extracted
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "k"]
    );
    assert_eq!(
        split.records,
        [
            RecordDestination::Retained,
            RecordDestination::Extracted,
            RecordDestination::Retained,
            RecordDestination::Extracted
        ]
    );
    assert_eq!(
        split.placements,
        [
            PlacementDestination::Retained(0),
            PlacementDestination::Retained(1),
            PlacementDestination::Retained(2),
            PlacementDestination::Extracted(1)
        ]
    );
    assert_eq!(after.placements, p.placements[..3]);
    assert_eq!(
        split.extracted.placements,
        [SceneEntityPlacement::root(id("k")), p.placements[3].clone()]
    );
    // The new P slot and Q root baseline have no fabricated original record/placement.
    assert!(p.entities[3].prefab.is_none());
    assert_eq!(split.records.len(), p.entities.len());
    let instance = after.entities[0].prefab.as_ref().unwrap();
    assert_eq!(
        instance.binding,
        PrefabInstance::root(request("q"), key("q"), target(&[], "k")).binding
    );
    assert_eq!(instance.source, request("q"));
    let arguments = instance.arguments.as_ref().unwrap();
    assert_eq!(arguments.base, key("q"));
    assert_eq!(
        split.extracted.inputs,
        arguments.values.keys().cloned().collect()
    );
    assert_eq!(arguments.values.len(), 2);
    let external = arguments
        .values
        .iter()
        .find(|(_, value)| **value == local("x"))
        .unwrap()
        .0;
    assert!(arguments.values.values().any(|value| *value == input("u")));
    let b = &split.extracted.entities[0].components[&component()];
    assert_eq!(
        b.value,
        record(local("k"), input(external.as_str()), 2).value
    );
    assert_eq!(
        b.contextual_defaults[&ComponentFieldId::new("target")],
        ComponentValue::EntityReference(input(external.as_str()))
    );

    let before_routes: &[(&str, &[(&str, &str)])] = &[
        ("outer", &[("p", "outer")]),
        ("x", &[("p", "x")]),
        ("k", &[("p", "k")]),
        ("b", &[("p", "b")]),
    ];
    let after_routes: &[(&str, &[(&str, &str)])] = &[
        ("outer", &[("p", "outer")]),
        ("x", &[("p", "x")]),
        ("k", &[("p", "k")]),
        ("b", &[("p", "k"), ("q", "b")]),
    ];
    let scene = consumer();
    let sources_before = [("p", Arc::new(p.clone()))];
    let expected = observe(&compile(&scene, &basis, &sources_before), before_routes);
    assert_eq!(expected["outer"], (vec!["k", "x"], None));
    assert_eq!(
        expected["k"],
        (
            vec!["b"],
            Some(record(local("b"), local("socket"), 1).value)
        )
    );
    assert_eq!(
        expected["b"],
        (vec![], Some(record(local("k"), local("x"), 2).value))
    );
    assert_eq!(
        expected["x"],
        (vec![], Some(record(local("b"), local("socket"), 3).value))
    );
    for json in [true, false] {
        let sources = [
            ("p", Arc::new(reopen(after, "p", &basis, json))),
            ("q", Arc::new(reopen(&split.extracted, "q", &basis, json))),
        ];
        assert_eq!(
            observe(&compile(&scene, &basis, &sources), after_routes),
            expected
        );
        let mut reset = scene.clone();
        let mut overlay = PrefabOverlay::new(key("p"));
        overlay.components.push(PrefabComponentIntent::Properties {
            target: target(&[("k", "q")], "b"),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                intent: PrefabPropertyIntent::Absent,
            }],
        });
        reset
            .entities
            .iter_mut()
            .find(|entity| entity.id == id("host"))
            .unwrap()
            .prefab
            .as_mut()
            .unwrap()
            .overrides = Some(overlay);
        let actual_reset = observe(&compile(&reset, &basis, &sources), after_routes);
        reset
            .entities
            .iter_mut()
            .find(|entity| entity.id == id("host"))
            .unwrap()
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .as_mut()
            .unwrap()
            .components[0] = PrefabComponentIntent::Properties {
            target: target(&[], "b"),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                intent: PrefabPropertyIntent::Absent,
            }],
        };
        assert_eq!(
            actual_reset,
            observe(&compile(&reset, &basis, &sources_before), before_routes)
        );
        assert_eq!(
            actual_reset["b"],
            (vec![], Some(record(local("x"), local("x"), 2).value))
        );
    }
    let (nodes, bytes) = work.usage();
    assert!(bytes > captured.unwrap());
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(bytes).unwrap())
        .with_reference_work_nodes(ItemLimit::new(nodes).unwrap());
    let mut exact_work = ExtractionWork::from_cut(exact, &cut, captured).unwrap();
    let repeated = construct_source_split(
        &cut,
        &key("p"),
        &key("q"),
        &request("q"),
        &basis,
        &mut exact_work,
    )
    .unwrap();
    assert_eq!(exact_work.usage(), (nodes, bytes));
    assert_eq!(repeated.extracted, split.extracted);
    for short in [
        exact.with_working_bytes(ByteLimit::new(bytes - 1).unwrap()),
        exact.with_reference_work_nodes(ItemLimit::new(nodes - 1).unwrap()),
    ] {
        let mut work = ExtractionWork::from_cut(short, &cut, captured).unwrap();
        assert!(matches!(
            construct_source_split(&cut, &key("p"), &key("q"), &request("q"), &basis, &mut work),
            Err(ExtractionError::Capacity)
        ));
    }
    assert_eq!(p, before);
    assert_eq!(basis, basis_before);
}

#[test]
fn actual_split_preserves_nested_root_program_and_local_child_between_inherited_siblings() {
    let basis = basis();
    let mut p = original();
    let mut r = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("r-root")),
            under(target(&[], "d1"), target(&[], "r-root")),
            under(target(&[], "d0"), target(&[], "d1")),
            under(target(&[], "d2"), target(&[], "d1")),
        ],
        [
            SceneEntityRecord::new(id("r-root")).with_component(
                component(),
                record(local("d1"), input("v"), 11).with_contextual_default(
                    ComponentFieldId::new("target"),
                    ComponentValue::EntityReference(local("d1")),
                ),
            ),
            SceneEntityRecord::new(id("d1")),
            SceneEntityRecord::new(id("d0")),
            SceneEntityRecord::new(id("d2")),
        ],
    );
    r.inputs.insert(ReferenceInputId::new("v").unwrap());
    let r_before = r.clone();
    let r = Arc::new(r);
    let mut instance = PrefabInstance::root(request("r"), key("r"), target(&[], "r-root"));
    instance.arguments = Some(PrefabReferenceArguments {
        base: key("r"),
        values: [(ReferenceInputId::new("v").unwrap(), input("u"))].into(),
    });
    let mut overlay = PrefabOverlay::new(key("r"));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: target(&[], "r-root"),
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("score")),
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(77)),
        }],
    });
    instance.overrides = Some(overlay);
    p.entities[3].components.clear();
    p.entities[3].prefab = Some(instance.clone());
    p.entities[1].components.insert(
        component(),
        record(reference(&[("k", "r")], "d0"), local("x"), 2),
    );
    p.placements[3] = SceneEntityPlacement::new(
        target(&[], "b"),
        ScenePlacement::before(
            ScenePlacementParent::Entity(target(&[("k", "r")], "d1")),
            target(&[("k", "r")], "d2"),
        ),
    );
    let before = p.clone();
    let (cut, captured) = capture(&p, &basis, &[("r", Arc::clone(&r))]);
    assert_eq!(
        cut.entities()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "k"]
    );
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let split =
        construct_source_split(&cut, &key("p"), &key("q"), &request("q"), &basis, &mut work)
            .unwrap();
    let SplitSourceDocument::Prefab(after) = &split.containing else {
        panic!("original is a Prefab")
    };
    let q = &split.extracted;
    let moved = q
        .entities
        .iter()
        .find(|entity| entity.id == id("k"))
        .unwrap()
        .prefab
        .as_ref()
        .unwrap();
    assert_eq!(moved.source, instance.source);
    assert_eq!(moved.binding, instance.binding);
    assert_eq!(moved.overrides, instance.overrides);
    assert_eq!(moved.arguments.as_ref().unwrap().base, key("r"));
    let arguments = after.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .arguments
        .as_ref()
        .unwrap();
    let forwarded = arguments
        .values
        .iter()
        .find(|(_, value)| **value == input("u"))
        .unwrap()
        .0;
    assert_eq!(
        moved.arguments.as_ref().unwrap().values[&ReferenceInputId::new("v").unwrap()],
        input(forwarded.as_str())
    );
    assert_eq!(
        q.placements,
        [SceneEntityPlacement::root(id("k")), p.placements[3].clone()]
    );
    assert_eq!(
        split.placements,
        [
            PlacementDestination::Retained(0),
            PlacementDestination::Retained(1),
            PlacementDestination::Retained(2),
            PlacementDestination::Extracted(1)
        ]
    );
    let before_routes: &[(&str, &[(&str, &str)])] = &[
        ("outer", &[("p", "outer")]),
        ("x", &[("p", "x")]),
        ("k", &[("p", "k")]),
        ("b", &[("p", "b")]),
        ("d1", &[("p", "k"), ("r", "d1")]),
        ("d0", &[("p", "k"), ("r", "d0")]),
        ("d2", &[("p", "k"), ("r", "d2")]),
    ];
    let after_routes: &[(&str, &[(&str, &str)])] = &[
        ("outer", &[("p", "outer")]),
        ("x", &[("p", "x")]),
        ("k", &[("p", "k")]),
        ("b", &[("p", "k"), ("q", "b")]),
        ("d1", &[("p", "k"), ("q", "k"), ("r", "d1")]),
        ("d0", &[("p", "k"), ("q", "k"), ("r", "d0")]),
        ("d2", &[("p", "k"), ("q", "k"), ("r", "d2")]),
    ];
    let scene = consumer();
    let expected = observe(
        &compile(
            &scene,
            &basis,
            &[("p", Arc::new(p.clone())), ("r", Arc::clone(&r))],
        ),
        before_routes,
    );
    let actual = observe(
        &compile(
            &scene,
            &basis,
            &[
                ("p", Arc::new(after.clone())),
                ("q", Arc::new(q.clone())),
                ("r", Arc::clone(&r)),
            ],
        ),
        after_routes,
    );
    assert_eq!(actual, expected);
    assert_eq!(
        actual["k"],
        (
            vec!["d1"],
            Some(record(local("d1"), local("socket"), 77).value)
        )
    );
    assert_eq!(actual["d1"], (vec!["d0", "b", "d2"], None));
    assert_eq!(
        actual["b"],
        (vec![], Some(record(local("d0"), local("x"), 2).value))
    );
    assert_eq!(p, before);
    assert_eq!(r.as_ref(), &r_before);
    assert_eq!(split.basis, basis);
}
