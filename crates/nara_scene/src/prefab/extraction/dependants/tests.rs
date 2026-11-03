//! Real P/Q construction and original-compiler consumers, plus complete inactive source intent.

use std::{convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueShape, PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::AuthoringAdapterReceipt,
    ExpandedSceneDocument, PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode,
    PrefabDocument, PrefabEntityOrigin, PrefabInstance, PrefabInstanceClaim, PrefabInstanceHop,
    PrefabOverlay, PrefabProjectionStep, PrefabPropertyClaim, PrefabPropertyIntent,
    PrefabReferenceArguments, ResolvedPrefabSource, SceneCompilationLimits, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePlacement,
    prefab::{
        extraction::{
            SourceCut, construction::construct_source_split, try_capture_source_cut_with_loader,
        },
        try_compile_authoring_scene_with_loader,
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.DependantLink")
}
fn field() -> ComponentFieldId {
    ComponentFieldId::new("target")
}
fn local(value: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(value) }
}
fn key(value: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "7ec7e86f-e450-4d7a-8d9c-bdcb25a7ef57".parse().unwrap(),
        value,
        kind,
    )
    .unwrap()
}
fn prefab_key(value: &str) -> SourceDocumentKey {
    key(value, SourceDocumentKind::Prefab)
}
fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/extraction-dependants/{value}.ron")).unwrap()
}
fn target(route: &[(&str, &str)], entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: route
            .iter()
            .map(|(anchor, base)| PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: prefab_key(base),
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
    let field = ComponentFieldSchema::optional_with_default(
        field(),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueShape::EntityRef,
        ComponentValue::EntityReference(local("b")),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let schema = ComponentSchema::new(component(), "Link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.dependants").unwrap(),
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
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}
fn record(value: EntityReference, default: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(value))]),
    )
    .with_contextual_default(field(), ComponentValue::EntityReference(default))
}
fn original() -> PrefabDocument {
    PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                target(&[], "b"),
                ScenePlacement::last(ScenePlacementParent::Entity(target(&[], "k"))),
            ),
        ],
        [
            SceneEntityRecord::new(id("k")),
            SceneEntityRecord::new(id("b"))
                .with_component(component(), record(local("b"), local("b"))),
        ],
    )
}
fn capture<'a>(
    source: &'a PrefabDocument,
    basis: &AuthoringSchemaBasis,
) -> (SourceCut<'a>, ExtractionWork) {
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(source),
        &id("k"),
        basis,
        Default::default(),
        None,
        |_, _| -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("direct-only cut must not load a source")
        },
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors() && report.failure.is_none());
    let cut = report.cut.unwrap();
    let work = ExtractionWork::from_cut(Default::default(), &cut, report.work_bytes).unwrap();
    (cut, work)
}
fn compile(
    scene: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, &Arc<PrefabDocument>)],
) -> ExpandedSceneDocument {
    let report = try_compile_authoring_scene_with_loader(
        scene,
        basis,
        Default::default(),
        None,
        |reference, _| {
            Ok::<_, Infallible>(
                sources
                    .iter()
                    .find(|(name, _)| request(name) == *reference)
                    .map(|(name, source)| {
                        ResolvedPrefabSource::bound_authoring_source(
                            Arc::clone(source),
                            basis.clone(),
                            prefab_key(name),
                        )
                    }),
            )
        },
    )
    .unwrap();
    assert!(
        !report.diagnostics().has_errors() && report.failure().is_none(),
        "{report:?}"
    );
    assert!(!report.budget_exceeded());
    report.into_parts().candidate.unwrap()
}
fn host(name: &str) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(name));
    let mut instance = PrefabInstance::root(request("p"), prefab_key("p"), target(&[], "k"));
    let mut overlay = PrefabOverlay::new(prefab_key("p"));
    overlay
        .components
        .push(properties(target(&[], "b"), local("b")));
    instance.overrides = Some(overlay);
    entity.prefab = Some(instance);
    entity
}
fn properties(target: PrefabEntityTarget, value: EntityReference) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target,
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(field()),
            intent: PrefabPropertyIntent::Set(ComponentValue::EntityReference(value)),
        }],
    }
}
fn occurrence(document: &ExpandedSceneDocument, anchor: &str, after: bool) -> SceneEntityId {
    let route = if after {
        vec![("p", "k"), ("q", "b")]
    } else {
        vec![("p", "b")]
    };
    let steps = route
        .into_iter()
        .map(|(base, entity)| PrefabProjectionStep::SourceEntity {
            source_document: request(base),
            canonical_source: prefab_key(base),
            entity: id(entity),
        })
        .collect::<Vec<_>>();
    let mut matches = document.provenance().iter().filter(|(_, origin)| {
        origin.projection_path().is_some_and(|path| {
            path.root_anchor() == &id(anchor) && path.steps() == steps.as_slice()
        })
    });
    let entity = matches.next().expect("actual typed occurrence").0.clone();
    assert!(matches.next().is_none());
    entity
}

#[test]
fn two_scenes_and_repeated_p_keep_actual_reference_targets_and_sibling_order() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let split = construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut work).unwrap();
    let SplitSourceDocument::Prefab(p_after) = split.containing else {
        panic!("Prefab source")
    };
    let (before_p, after_p, q) = (
        Arc::new(p.clone()),
        Arc::new(p_after),
        Arc::new(split.extracted),
    );
    for (name, use_default) in [("first", false), ("second", true)] {
        let mut observer = record(
            reference(&[("h1", "p")], "b"),
            reference(&[("h2", "p")], "b"),
        );
        if use_default {
            observer.value = ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>());
        }
        let mut scene = SceneDocument::from_ordered_roots([
            host("h1"),
            host("h2"),
            SceneEntityRecord::new(id("b"))
                .with_component(component(), record(local("b"), local("b"))),
            SceneEntityRecord::new(id("observer")).with_component(component(), observer),
        ]);
        scene.entities.extend([
            SceneEntityRecord::new(id("x")),
            SceneEntityRecord::new(id("y")),
        ]);
        for (entity, slot) in [
            ("x", ScenePlacementSlot::Before(target(&[("h1", "p")], "b"))),
            ("y", ScenePlacementSlot::After(target(&[("h1", "p")], "b"))),
        ] {
            scene.placements.push(SceneEntityPlacement::new(
                target(&[], entity),
                ScenePlacement {
                    parent: ScenePlacementParent::Entity(target(&[], "h1")),
                    slot,
                },
            ));
        }
        let untouched = scene.clone();
        let rewritten = rewrite_dependant(
            SourceDocumentInput::Scene(&scene),
            &key(name, SourceDocumentKind::Scene),
            &relocation,
            &basis,
            false,
            &mut work,
        )
        .unwrap();
        assert!(rewritten.source_changed);
        let SplitSourceDocument::Scene(rewritten) = rewritten.document else {
            panic!("Scene source")
        };
        assert!(
            verify_dependant(
                SourceDocumentInput::Scene(&scene),
                &key(name, SourceDocumentKind::Scene),
                SourceDocumentInput::Scene(&rewritten),
                &relocation,
                &basis,
                false,
                &mut work,
            )
            .unwrap()
        );
        assert_eq!(scene, untouched);
        assert_eq!(
            rewritten.entities.iter().map(|e| &e.id).collect::<Vec<_>>(),
            scene.entities.iter().map(|e| &e.id).collect::<Vec<_>>()
        );
        assert_eq!(rewritten.entities[0], scene.entities[0]); // The unrelated local b cannot move.
        for (document, after) in [
            (compile(&scene, &basis, &[("p", &before_p)]), false),
            (
                compile(&rewritten, &basis, &[("p", &after_p), ("q", &q)]),
                true,
            ),
        ] {
            assert_eq!(document.entities().len(), 8);
            assert_eq!(
                document.roots(),
                &[id("h1"), id("h2"), id("b"), id("observer")]
            );
            let (first, second) = (
                occurrence(&document, "h1", after),
                occurrence(&document, "h2", after),
            );
            assert_ne!(first, second);
            let h1 = document
                .entities()
                .iter()
                .find(|e| e.id() == &id("h1"))
                .unwrap();
            assert_eq!(h1.children(), &[id("x"), first.clone(), id("y")]);
            for (entity, expected) in [
                (&first, local("b")),
                (&second, local("b")),
                (
                    &id("observer"),
                    EntityReference::SceneLocal {
                        entity: if use_default {
                            second.clone()
                        } else {
                            first.clone()
                        },
                    },
                ),
            ] {
                let entity = document
                    .entities()
                    .iter()
                    .find(|e| e.id() == entity)
                    .unwrap();
                assert_eq!(
                    entity.components()[&component()].value,
                    ComponentValue::map([("target", ComponentValue::EntityReference(expected))])
                );
            }
        }
    }
}

fn inactive(after: bool) -> PrefabDocument {
    let external = if after {
        target(&[("host", "p"), ("k", "q")], "b")
    } else {
        target(&[("host", "p")], "b")
    };
    let internal = if after {
        target(&[("k", "q")], "b")
    } else {
        target(&[], "b")
    };
    let external_ref = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: external.through.clone(),
            target: AuthoredReferenceTarget::Entity(external.entity.clone()),
        },
    };
    let input = ReferenceInputId::new("u").unwrap();
    let mut nested = PrefabInstance::root(request("p"), prefab_key("p"), internal.clone());
    nested.arguments = Some(PrefabReferenceArguments {
        base: prefab_key("p"),
        values: [
            (input.clone(), local("b")),
            (
                ReferenceInputId::new("crossing").unwrap(),
                external_ref.clone(),
            ),
            (
                ReferenceInputId::new("forwarded").unwrap(),
                EntityReference::Authored {
                    reference: AuthoredEntityReference::input(input.clone()),
                },
            ),
        ]
        .into(),
    });
    let mut overlay = PrefabOverlay::new(prefab_key("p"));
    let mut addition = PrefabAddition::new(id("a"));
    addition.prefab = Some(nested.clone());
    addition
        .components
        .insert(component(), record(local("b"), external_ref.clone()));
    overlay.additions.push(addition);
    overlay.instances.push(PrefabInstanceClaim {
        target: if after {
            target(&[("k", "q")], "k")
        } else {
            target(&[], "k")
        },
        value: Some(nested),
    });
    overlay
        .components
        .push(properties(internal.clone(), local("b")));
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("a")),
        ScenePlacement::last(ScenePlacementParent::Entity(internal.clone())),
    ));
    overlay.suppressions.push(internal);
    let mut host = host("host");
    host.prefab.as_mut().unwrap().overrides = Some(overlay);
    let mut document = PrefabDocument::from_ordered_roots([
        host,
        SceneEntityRecord::new(id("b"))
            .with_component(component(), record(local("b"), external_ref)),
        SceneEntityRecord::new(id("z")),
        SceneEntityRecord::new(id("w")),
        SceneEntityRecord::new(id("child")),
    ]);
    document.inputs.insert(input);
    document.placements.push(SceneEntityPlacement::new(
        external.clone(),
        ScenePlacement::last(ScenePlacementParent::Entity(target(&[], "b"))),
    ));
    document.placements[2] = SceneEntityPlacement::new(
        target(&[], "z"),
        ScenePlacement::before(
            ScenePlacementParent::Entity(target(&[], "b")),
            external.clone(),
        ),
    );
    document.placements[3] = SceneEntityPlacement::new(
        target(&[], "w"),
        ScenePlacement::after(
            ScenePlacementParent::Entity(target(&[], "b")),
            external.clone(),
        ),
    );
    document.placements[4] = SceneEntityPlacement::new(
        target(&[], "child"),
        ScenePlacement::last(ScenePlacementParent::Entity(external)),
    );
    document
}

#[test]
fn inactive_prefab_rewrites_all_physical_claims_defaults_and_arguments_without_loading() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let original = inactive(false);
    let untouched = original.clone();
    let rewritten = rewrite_dependant(
        SourceDocumentInput::Prefab(&original),
        &prefab_key("inactive"),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap();
    assert!(rewritten.source_changed);
    let SplitSourceDocument::Prefab(candidate) = rewritten.document else {
        panic!("Prefab source")
    };
    assert!(
        verify_dependant(
            SourceDocumentInput::Prefab(&original),
            &prefab_key("inactive"),
            SourceDocumentInput::Prefab(&candidate),
            &relocation,
            &basis,
            false,
            &mut work,
        )
        .unwrap()
    );
    assert_eq!(candidate, inactive(true));
    assert_eq!(candidate.inputs, original.inputs);
    assert_eq!(original, untouched);
    // This actual consumer never requests the inactive source or its dormant instance claims.
    let split = construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut work).unwrap();
    let SplitSourceDocument::Prefab(after) = split.containing else {
        panic!("Prefab source")
    };
    let scene = SceneDocument::from_ordered_roots([host("h1"), SceneEntityRecord::new(id("b"))]);
    let before = compile(&scene, &basis, &[("p", &Arc::new(p.clone()))]);
    let SplitSourceDocument::Scene(scene) = rewrite_dependant(
        SourceDocumentInput::Scene(&scene),
        &key("active", SourceDocumentKind::Scene),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap()
    .document
    else {
        panic!("Scene source")
    };
    let mut forged = candidate.clone();
    forged.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .contextual_defaults
        .insert(field(), ComponentValue::EntityReference(local("z")));
    assert!(matches!(
        verify_dependant(
            SourceDocumentInput::Prefab(&original),
            &prefab_key("inactive"),
            SourceDocumentInput::Prefab(&forged),
            &relocation,
            &basis,
            false,
            &mut work,
        ),
        Err(ExtractionError::InvalidSource)
    ));
    let (after, q) = (Arc::new(after), Arc::new(split.extracted));
    let document = compile(
        &scene,
        &basis,
        &[("p", &after), ("q", &q), ("inactive", &Arc::new(candidate))],
    );
    let equally_effective = compile(
        &scene,
        &basis,
        &[("p", &after), ("q", &q), ("inactive", &Arc::new(forged))],
    );
    assert_eq!(document.entities(), equally_effective.entities());
    assert_eq!(document.roots(), equally_effective.roots());
    assert_eq!(document.entities().len(), 3);
    assert_eq!(before.entities().len(), document.entities().len());
}

#[test]
fn unmoved_p_context_and_cloned_references_do_not_report_source_changes() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let mut entity = host("host");
    let overlay = entity.prefab.as_mut().unwrap().overrides.as_mut().unwrap();
    overlay.components = vec![properties(target(&[], "k"), local("b"))];
    let source = SceneDocument::from_ordered_roots([
        entity,
        SceneEntityRecord::new(id("b")).with_component(component(), record(local("b"), local("b"))),
    ]);
    let rewritten = rewrite_dependant(
        SourceDocumentInput::Scene(&source),
        &key("unmoved", SourceDocumentKind::Scene),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap();
    assert!(!rewritten.source_changed);
    let SplitSourceDocument::Scene(candidate) = rewritten.document else {
        panic!("Scene source")
    };
    assert!(
        !verify_dependant(
            SourceDocumentInput::Scene(&source),
            &key("unmoved", SourceDocumentKind::Scene),
            SourceDocumentInput::Scene(&candidate),
            &relocation,
            &basis,
            false,
            &mut work,
        )
        .unwrap()
    );
    assert_eq!(candidate, source);
    assert!(!std::ptr::eq(
        candidate.entities.as_ptr(),
        source.entities.as_ptr(),
    ));
}

#[test]
fn binding_alone_reports_a_source_change_without_reference_payloads() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let mut entity = SceneEntityRecord::new(id("host"));
    entity.prefab = Some(PrefabInstance::root(
        request("p"),
        pk.clone(),
        target(&[], "b"),
    ));
    let source = PrefabDocument::from_ordered_roots([entity]);
    let rewritten = rewrite_dependant(
        SourceDocumentInput::Prefab(&source),
        &prefab_key("binding-only"),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap();
    assert!(rewritten.source_changed);
    let SplitSourceDocument::Prefab(candidate) = rewritten.document else {
        panic!("Prefab source")
    };
    assert!(
        verify_dependant(
            SourceDocumentInput::Prefab(&source),
            &prefab_key("binding-only"),
            SourceDocumentInput::Prefab(&candidate),
            &relocation,
            &basis,
            false,
            &mut work,
        )
        .unwrap()
    );
    let mut expected = source.clone();
    expected.entities[0].prefab.as_mut().unwrap().binding = crate::PrefabInstanceBinding::Root {
        base: pk,
        root: target(&[("k", "q")], "b"),
    };
    assert_eq!(candidate, expected);
}

#[test]
fn properties_target_alone_reports_a_source_change_without_reference_payloads() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let mut entity = host("host");
    let overlay = entity.prefab.as_mut().unwrap().overrides.as_mut().unwrap();
    let PrefabComponentIntent::Properties { claims, .. } = &mut overlay.components[0] else {
        panic!("Properties intent")
    };
    claims[0].intent = PrefabPropertyIntent::Absent;
    let source = PrefabDocument::from_ordered_roots([entity]);
    let rewritten = rewrite_dependant(
        SourceDocumentInput::Prefab(&source),
        &prefab_key("target-only"),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap();
    assert!(rewritten.source_changed);
    let SplitSourceDocument::Prefab(candidate) = rewritten.document else {
        panic!("Prefab source")
    };
    assert!(
        verify_dependant(
            SourceDocumentInput::Prefab(&source),
            &prefab_key("target-only"),
            SourceDocumentInput::Prefab(&candidate),
            &relocation,
            &basis,
            false,
            &mut work,
        )
        .unwrap()
    );
    let mut expected = source.clone();
    let overlay = expected.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    let PrefabComponentIntent::Properties {
        target: rewritten_target,
        ..
    } = &mut overlay.components[0]
    else {
        panic!("Properties intent")
    };
    *rewritten_target = target(&[("k", "q")], "b");
    assert_eq!(candidate, expected);
}

#[test]
fn source_placement_alone_reports_a_source_change_without_reference_payloads() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let mut entity = SceneEntityRecord::new(id("host"));
    entity.prefab = Some(PrefabInstance::root(
        request("p"),
        pk.clone(),
        target(&[], "k"),
    ));
    let mut source = SceneDocument::from_ordered_roots([entity, SceneEntityRecord::new(id("pin"))]);
    source.placements[1] = SceneEntityPlacement::new(
        target(&[], "pin"),
        ScenePlacement::last(ScenePlacementParent::Entity(target(&[("host", "p")], "b"))),
    );
    let rewritten = rewrite_dependant(
        SourceDocumentInput::Scene(&source),
        &key("placement-only", SourceDocumentKind::Scene),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap();
    assert!(rewritten.source_changed);
    let SplitSourceDocument::Scene(candidate) = rewritten.document else {
        panic!("Scene source")
    };
    assert!(
        verify_dependant(
            SourceDocumentInput::Scene(&source),
            &key("placement-only", SourceDocumentKind::Scene),
            SourceDocumentInput::Scene(&candidate),
            &relocation,
            &basis,
            false,
            &mut work,
        )
        .unwrap()
    );
    let mut expected = source.clone();
    expected.placements[1].placement.parent =
        ScenePlacementParent::Entity(target(&[("host", "p"), ("k", "q")], "b"));
    assert_eq!(candidate, expected);
}

#[test]
fn known_dormant_default_alone_reports_a_reference_and_source_change() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    // No active instance enters P. The understood hidden default still owns a route rewrite.
    let source = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("dormant")).with_component(
            component(),
            record(local("dormant"), reference(&[("host", "p")], "b")),
        ),
        SceneEntityRecord::new(id("host")),
    ]);
    let rewritten = rewrite_dependant(
        SourceDocumentInput::Prefab(&source),
        &prefab_key("reference-only"),
        &relocation,
        &basis,
        true,
        &mut work,
    )
    .unwrap();
    assert!(rewritten.source_changed);
    let SplitSourceDocument::Prefab(candidate) = rewritten.document else {
        panic!("Prefab source")
    };
    assert!(
        verify_dependant(
            SourceDocumentInput::Prefab(&source),
            &prefab_key("reference-only"),
            SourceDocumentInput::Prefab(&candidate),
            &relocation,
            &basis,
            true,
            &mut work,
        )
        .unwrap()
    );
    let mut expected = source.clone();
    expected.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .contextual_defaults
        .insert(
            field(),
            ComponentValue::EntityReference(reference(&[("host", "p"), ("k", "q")], "b")),
        );
    assert_eq!(candidate, expected);
}

#[test]
fn preserved_unknown_payload_does_not_report_a_source_change() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let source = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("opaque"))
        .with_component(
            ComponentTypeId::new("missing.Preserved"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("opaque", ComponentValue::String("retained".into()))]),
            ),
        )]);
    let rewritten = rewrite_dependant(
        SourceDocumentInput::Prefab(&source),
        &prefab_key("unknown"),
        &relocation,
        &basis,
        true,
        &mut work,
    )
    .unwrap();
    assert!(!rewritten.source_changed);
    let SplitSourceDocument::Prefab(candidate) = rewritten.document else {
        panic!("Prefab source")
    };
    assert!(
        !verify_dependant(
            SourceDocumentInput::Prefab(&source),
            &prefab_key("unknown"),
            SourceDocumentInput::Prefab(&candidate),
            &relocation,
            &basis,
            true,
            &mut work,
        )
        .unwrap()
    );
    assert_eq!(candidate, source);
}

#[test]
fn wrong_source_kind_and_affected_unknown_schema_refuse_without_mutating_input() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let mut entity = host("host");
    let overlay = entity.prefab.as_mut().unwrap().overrides.as_mut().unwrap();
    overlay.components = vec![PrefabComponentIntent::Properties {
        target: target(&[], "b"),
        component: ComponentTypeId::new("game.UnknownAffected"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![],
    }];
    let source = SceneDocument::from_ordered_roots([entity]);
    let untouched = source.clone();
    let usage = work.usage();
    for after in [
        SourceDocumentInput::Scene(&source),
        SourceDocumentInput::Prefab(&p),
    ] {
        assert!(matches!(
            verify_dependant(
                SourceDocumentInput::Scene(&source),
                &pk,
                after,
                &relocation,
                &basis,
                false,
                &mut work,
            ),
            Err(ExtractionError::InvalidSource)
        ));
        assert_eq!(work.usage(), usage);
    }
    assert!(matches!(
        rewrite_dependant(
            SourceDocumentInput::Scene(&source),
            &pk,
            &relocation,
            &basis,
            false,
            &mut work
        ),
        Err(ExtractionError::InvalidSource)
    ));
    assert_eq!(work.usage(), usage);
    assert!(matches!(
        rewrite_dependant(
            SourceDocumentInput::Scene(&source),
            &key("scene", SourceDocumentKind::Scene),
            &relocation,
            &basis,
            false,
            &mut work
        ),
        Err(ExtractionError::UnknownSchema)
    ));
    assert_eq!(source, untouched);
}

#[test]
fn stored_pairs_match_actual_ids_after_independent_record_permutations() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    for changed in [false, true] {
        let mut before = if changed {
            inactive(false)
        } else {
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("z")),
                SceneEntityRecord::new(id("b"))
                    .with_component(component(), record(local("b"), local("b"))),
                SceneEntityRecord::new(id("a")),
            ])
        };
        before.entities.reverse();
        let rewritten = rewrite_dependant(
            SourceDocumentInput::Prefab(&before),
            &prefab_key("permuted"),
            &relocation,
            &basis,
            false,
            &mut work,
        )
        .unwrap();
        assert_eq!(rewritten.source_changed, changed);
        let SplitSourceDocument::Prefab(mut after) = rewritten.document else {
            panic!("Prefab source")
        };
        after.canonicalize();
        for _ in 0..2 {
            assert_eq!(
                verify_dependant(
                    SourceDocumentInput::Prefab(&before),
                    &prefab_key("permuted"),
                    SourceDocumentInput::Prefab(&after),
                    &relocation,
                    &basis,
                    false,
                    &mut work,
                )
                .unwrap(),
                changed
            );
            before.entities.rotate_left(1);
            after.entities.reverse();
        }
    }
}

#[test]
fn stored_pairs_reject_duplicate_missing_extra_and_replaced_entity_ids() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let before = inactive(false);
    let SplitSourceDocument::Prefab(after) = rewrite_dependant(
        SourceDocumentInput::Prefab(&before),
        &prefab_key("ids"),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap()
    .document
    else {
        panic!("Prefab source")
    };
    for case in 0..5 {
        let mut forged = after.clone();
        match case {
            0 => {
                forged.entities.pop();
            }
            1 => forged.entities.push(SceneEntityRecord::new(id("extra"))),
            2 => forged.entities[1] = forged.entities[0].clone(),
            3 => forged.entities[1].id = id("replacement"),
            4 => forged.entities.push(forged.entities[0].clone()),
            _ => unreachable!(),
        }
        assert!(
            matches!(
                verify_dependant(
                    SourceDocumentInput::Prefab(&before),
                    &prefab_key("ids"),
                    SourceDocumentInput::Prefab(&forged),
                    &relocation,
                    &basis,
                    false,
                    &mut work,
                ),
                Err(ExtractionError::InvalidSource)
            ),
            "case {case}"
        );
    }
    let mut duplicate_before = before.clone();
    duplicate_before.entities[1] = duplicate_before.entities[0].clone();
    assert!(matches!(
        verify_dependant(
            SourceDocumentInput::Prefab(&duplicate_before),
            &prefab_key("ids"),
            SourceDocumentInput::Prefab(&after),
            &relocation,
            &basis,
            false,
            &mut work,
        ),
        Err(ExtractionError::InvalidSource)
    ));
}

#[test]
fn stored_pairs_preserve_complete_input_sets_and_exact_placement_vectors() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let before = inactive(false);
    let SplitSourceDocument::Prefab(after) = rewrite_dependant(
        SourceDocumentInput::Prefab(&before),
        &prefab_key("declarations"),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap()
    .document
    else {
        panic!("Prefab source")
    };
    for case in 0..8 {
        let mut forged = after.clone();
        match case {
            0 => {
                forged.placements.pop();
            }
            1 => forged.placements.push(forged.placements[0].clone()),
            2 => forged.placements[1] = forged.placements[0].clone(),
            3 => forged.placements.swap(0, 1),
            4 => forged.inputs.clear(),
            5 => {
                forged
                    .inputs
                    .insert(ReferenceInputId::new("extra").unwrap());
            }
            6 => {
                forged.inputs.clear();
                forged.inputs.insert(ReferenceInputId::new("v").unwrap());
            }
            7 => forged.placements[0].placement.slot = ScenePlacementSlot::First,
            _ => unreachable!(),
        }
        assert!(
            matches!(
                verify_dependant(
                    SourceDocumentInput::Prefab(&before),
                    &prefab_key("declarations"),
                    SourceDocumentInput::Prefab(&forged),
                    &relocation,
                    &basis,
                    false,
                    &mut work,
                ),
                Err(ExtractionError::InvalidSource)
            ),
            "case {case}"
        );
    }
}

#[test]
fn stored_pairs_reject_unrelated_and_dormant_record_property_argument_and_structural_changes() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let before = inactive(false);
    let SplitSourceDocument::Prefab(after) = rewrite_dependant(
        SourceDocumentInput::Prefab(&before),
        &prefab_key("dormant"),
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap()
    .document
    else {
        panic!("Prefab source")
    };
    for case in 0..10 {
        let mut forged = after.clone();
        if case < 2 {
            let record = forged.entities[0].components.get_mut(&component()).unwrap();
            if case == 0 {
                record.value =
                    ComponentValue::map([("target", ComponentValue::EntityReference(local("z")))]);
            } else {
                record
                    .contextual_defaults
                    .insert(field(), ComponentValue::EntityReference(local("z")));
            }
        } else {
            let overlay = forged
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("host"))
                .unwrap()
                .prefab
                .as_mut()
                .unwrap()
                .overrides
                .as_mut()
                .unwrap();
            match case {
                2 => {
                    overlay.additions[0]
                        .components
                        .get_mut(&component())
                        .unwrap()
                        .contextual_defaults
                        .insert(field(), ComponentValue::EntityReference(local("z")));
                }
                3 => {
                    overlay.additions[0]
                        .prefab
                        .as_mut()
                        .unwrap()
                        .arguments
                        .as_mut()
                        .unwrap()
                        .values
                        .insert(ReferenceInputId::new("crossing").unwrap(), local("z"));
                }
                4 | 5 => {
                    let PrefabComponentIntent::Properties { target, claims, .. } =
                        &mut overlay.components[0]
                    else {
                        panic!("Properties intent")
                    };
                    if case == 4 {
                        *target = PrefabEntityTarget::source(id("k"));
                    } else {
                        claims[0].intent = PrefabPropertyIntent::Absent;
                    }
                }
                6 => overlay.instances[0].value = None,
                7 => overlay.placements[0].placement.parent = ScenePlacementParent::Root,
                8 => overlay.suppressions.clear(),
                9 => overlay.base = prefab_key("unrelated"),
                _ => unreachable!(),
            }
        }
        assert!(
            matches!(
                verify_dependant(
                    SourceDocumentInput::Prefab(&before),
                    &prefab_key("dormant"),
                    SourceDocumentInput::Prefab(&forged),
                    &relocation,
                    &basis,
                    false,
                    &mut work,
                ),
                Err(ExtractionError::InvalidSource)
            ),
            "case {case}"
        );
    }
}

#[test]
fn preserved_unknown_storage_stays_exact_at_direct_addition_set_and_property_sites() {
    let (basis, p) = (basis(), original());
    let (cut, mut work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let unknown = ComponentTypeId::new("missing.Preserved");
    let opaque = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "opaque",
            ComponentValue::List(vec![
                ComponentValue::String("retained\0\n\\\" bytes 中文".into()),
                ComponentValue::U64(17),
                ComponentValue::f64(-0.0).unwrap(),
            ]),
        )]),
    )
    .with_contextual_default(field(), ComponentValue::String("masked".into()));
    let mut before = inactive(false);
    before.entities[0]
        .components
        .insert(unknown.clone(), opaque.clone());
    let overlay = before
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("host"))
        .unwrap()
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    overlay.additions[0]
        .components
        .insert(unknown.clone(), opaque.clone());
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: target(&[], "k"),
        component: unknown.clone(),
        record: opaque.clone(),
    });
    overlay.components.push(PrefabComponentIntent::Properties {
        target: target(&[], "b"),
        component: unknown.clone(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(field()),
            intent: PrefabPropertyIntent::Set(opaque.value.clone()),
        }],
    });
    let SplitSourceDocument::Prefab(after) = rewrite_dependant(
        SourceDocumentInput::Prefab(&before),
        &prefab_key("opaque"),
        &relocation,
        &basis,
        true,
        &mut work,
    )
    .unwrap()
    .document
    else {
        panic!("Prefab source")
    };
    assert!(
        verify_dependant(
            SourceDocumentInput::Prefab(&before),
            &prefab_key("opaque"),
            SourceDocumentInput::Prefab(&after),
            &relocation,
            &basis,
            true,
            &mut work,
        )
        .unwrap()
    );
    assert_eq!(after.entities[0].components[&unknown], opaque);
    for case in 0..8 {
        let mut forged = after.clone();
        if case < 3 || case == 7 {
            let record = forged.entities[0].components.get_mut(&unknown).unwrap();
            match case {
                0 => record.value = ComponentValue::String("mutated".into()),
                1 => {
                    record
                        .contextual_defaults
                        .insert(field(), ComponentValue::Null);
                }
                2 => {
                    record.owner =
                        Some(PersistentComponentSchemaOwnerId::try_new("missing.other").unwrap())
                }
                7 => {
                    let ComponentValue::Map(map) = &mut record.value else {
                        panic!("Opaque map")
                    };
                    let ComponentValue::List(values) = map.get_mut("opaque").unwrap() else {
                        panic!("Opaque list")
                    };
                    values[2] = ComponentValue::f64(0.0).unwrap();
                }
                _ => unreachable!(),
            }
        } else {
            let overlay = forged
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("host"))
                .unwrap()
                .prefab
                .as_mut()
                .unwrap()
                .overrides
                .as_mut()
                .unwrap();
            match case {
                3 => {
                    overlay.additions[0]
                        .components
                        .get_mut(&unknown)
                        .unwrap()
                        .value = ComponentValue::Null;
                }
                4 => {
                    let PrefabComponentIntent::Set { record, .. } = &mut overlay.components[1]
                    else {
                        panic!("Set intent")
                    };
                    record.contextual_defaults.clear();
                }
                5 | 6 => {
                    let PrefabComponentIntent::Properties { target, claims, .. } =
                        &mut overlay.components[2]
                    else {
                        panic!("Properties intent")
                    };
                    if case == 5 {
                        claims[0].intent = PrefabPropertyIntent::Set(ComponentValue::Null);
                    } else {
                        // Unknown payloads still retain the understood structural relocation.
                        *target = PrefabEntityTarget::source(id("b"));
                    }
                }
                _ => unreachable!(),
            }
        }
        if case == 7 {
            assert_eq!(
                forged, after,
                "ordinary value equality collapses signed zero"
            );
        }
        assert!(
            matches!(
                verify_dependant(
                    SourceDocumentInput::Prefab(&before),
                    &prefab_key("opaque"),
                    SourceDocumentInput::Prefab(&forged),
                    &relocation,
                    &basis,
                    true,
                    &mut work,
                ),
                Err(ExtractionError::InvalidSource)
            ),
            "case {case}"
        );
    }
    assert!(matches!(
        verify_dependant(
            SourceDocumentInput::Prefab(&before),
            &prefab_key("opaque"),
            SourceDocumentInput::Prefab(&after),
            &relocation,
            &basis,
            false,
            &mut work,
        ),
        Err(ExtractionError::UnknownSchema)
    ));
}

#[test]
fn stored_verification_shares_exact_allowances_and_keeps_refused_attempts_spent() {
    let (basis, p) = (basis(), original());
    let (cut, mut construction_work) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let before = inactive(false);
    let SplitSourceDocument::Prefab(after) = rewrite_dependant(
        SourceDocumentInput::Prefab(&before),
        &prefab_key("bounded"),
        &relocation,
        &basis,
        false,
        &mut construction_work,
    )
    .unwrap()
    .document
    else {
        panic!("Prefab source")
    };
    let run = |limits| {
        let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
        work.visit(3, 11, 17).unwrap();
        let result = verify_dependant(
            SourceDocumentInput::Prefab(&before),
            &prefab_key("bounded"),
            SourceDocumentInput::Prefab(&after),
            &relocation,
            &basis,
            false,
            &mut work,
        );
        (work, result)
    };
    let defaults = SceneCompilationLimits::default();
    let (baseline, result) = run(defaults);
    assert!(result.unwrap());
    let (nodes, bytes) = baseline.usage();
    let reference_bytes = defaults.reference_work_bytes().get()
        - baseline
            .compiler_options(0)
            .unwrap()
            .limits
            .reference_work_bytes()
            .get();
    let exact = defaults
        .with_reference_work_nodes(ItemLimit::new(nodes).unwrap())
        .with_reference_work_bytes(ByteLimit::new(reference_bytes).unwrap())
        .with_working_bytes(ByteLimit::new(bytes).unwrap());
    let (exact_work, result) = run(exact);
    assert!(result.unwrap());
    assert_eq!(exact_work.usage(), baseline.usage());
    for short in [
        exact.with_reference_work_nodes(ItemLimit::new(nodes - 1).unwrap()),
        exact.with_reference_work_bytes(ByteLimit::new(reference_bytes - 1).unwrap()),
        exact.with_working_bytes(ByteLimit::new(bytes - 1).unwrap()),
    ] {
        assert!(matches!(run(short).1, Err(ExtractionError::Capacity)));
    }

    let mut forged = after.clone();
    forged.inputs.clear();
    forged.inputs.insert(ReferenceInputId::new("v").unwrap());
    let mut work = ExtractionWork::new(exact, Some(0)).unwrap();
    work.visit(3, 11, 17).unwrap();
    assert!(matches!(
        verify_dependant(
            SourceDocumentInput::Prefab(&before),
            &prefab_key("bounded"),
            SourceDocumentInput::Prefab(&forged),
            &relocation,
            &basis,
            false,
            &mut work,
        ),
        Err(ExtractionError::InvalidSource)
    ));
    let refused = work.usage();
    assert!(refused.0 > 3 && refused.1 > 28);
    assert!(matches!(
        verify_dependant(
            SourceDocumentInput::Prefab(&before),
            &prefab_key("bounded"),
            SourceDocumentInput::Prefab(&after),
            &relocation,
            &basis,
            false,
            &mut work,
        ),
        Err(ExtractionError::Capacity)
    ));
    assert!(work.usage().0 >= refused.0 && work.usage().1 >= refused.1);
}

#[test]
fn either_raw_side_refuses_before_clone_or_equality_and_retains_its_failed_prefix() {
    let (basis, p) = (basis(), original());
    let (cut, _) = capture(&p, &basis);
    let (pk, qk) = (prefab_key("p"), prefab_key("q"));
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let unknown = ComponentTypeId::new("missing.Raw");
    let limits = SceneCompilationLimits::default()
        .with_reference_work_nodes(ItemLimit::new(10_000).unwrap());
    for before_is_large in [true, false] {
        let mut before = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("raw"))
            .with_component(
                unknown.clone(),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
            )]);
        let mut after = before.clone();
        let large = if before_is_large {
            &mut before
        } else {
            &mut after
        };
        large.entities[0]
            .components
            .get_mut(&unknown)
            .unwrap()
            .value = ComponentValue::List(vec![
            ComponentValue::List(vec![ComponentValue::String(
                "x".into()
            )]);
            8_192
        ]);
        let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
        let mut first_nodes = 0;
        for attempt in 0..2 {
            let prior = work.usage();
            assert!(matches!(
                verify_dependant(
                    SourceDocumentInput::Scene(&before),
                    &key("raw", SourceDocumentKind::Scene),
                    SourceDocumentInput::Scene(&after),
                    &relocation,
                    &basis,
                    true,
                    &mut work,
                ),
                Err(ExtractionError::Capacity)
            ));
            let spent = work.usage();
            assert!(spent.0 > prior.0 && spent.1 >= prior.1);
            if attempt == 0 {
                first_nodes = spent.0;
                assert!(spent.0 > 3_000 && spent.1 > 0);
            } else {
                assert!(spent.0 - prior.0 < first_nodes);
            }
        }
    }
}
