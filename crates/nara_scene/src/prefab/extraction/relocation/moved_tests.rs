//! Moved-declaration proposals and one manually authored P/Q compiler consumer.
//! These cases do not constitute an extraction builder or publication proof.

use std::{borrow::Cow, collections::BTreeMap, convert::Infallible, io, sync::Arc};

use nara_asset::AssetRef;
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, ReferenceInputId,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

use super::{BranchRelocation, MovedReferenceInputs};
use crate::{
    __private::AuthoringAdapterReceipt,
    AuthoringSchemaBasis, ExpandedSceneDocument, PrefabDocument, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, PrefabProjectionStep,
    PrefabReferenceArguments, ResolvedPrefabSource, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
    SourceDocumentKey, SourceDocumentKind,
    prefab::{
        SourceDocumentInput,
        extraction::{SourceCut, try_capture_source_cut_with_loader},
        try_compile_authoring_scene_with_loader,
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn input(value: &str) -> ReferenceInputId {
    ReferenceInputId::new(value).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "a96df619-ea77-4fb0-b039-2ef1b22b1d55".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn hop(anchor: &str, base: &str) -> PrefabInstanceHop {
    PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
        expected_base: key(base),
    }
}

fn local(entity: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(entity) }
}

fn input_reference(name: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference::input(input(name)),
    }
}

fn reference(through: Vec<PrefabInstanceHop>, entity: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through,
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(entity))),
        },
    }
}

fn source() -> PrefabDocument {
    PrefabDocument {
        inputs: [input("u")].into(),
        placements: vec![
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("b")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("k"),
                ))),
            ),
            SceneEntityPlacement::root(id("x")),
        ],
        entities: vec![
            SceneEntityRecord::new(id("b")),
            SceneEntityRecord::new(id("x")),
            SceneEntityRecord::new(id("k")),
        ],
    }
}

fn empty_basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn capture_cut<'source>(
    source: &'source PrefabDocument,
    basis: &AuthoringSchemaBasis,
) -> SourceCut<'source> {
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(source),
        &id("k"),
        basis,
        Default::default(),
        None,
        |reference, _| -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("the direct source cut must not load a Prefab: {reference:?}")
        },
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.budget_failed);
    let cut = report.cut.expect("the source-owned k/b branch must exist");
    assert_eq!(cut.entities().len(), 2);
    assert!(std::ptr::eq(cut.root(), &source.entities[2]));
    cut
}

#[test]
fn moved_internal_references_keep_their_slots_and_addition_namespace_stays_distinct() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(&source, &empty_basis());
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let mut inputs = MovedReferenceInputs::new(&relocation);
    let persistent = EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("moved-reference-tests").unwrap(),
            PersistentRuntimeId::parse_str("310e50e8-7768-4299-ad89-f6e0acc7e69c").unwrap(),
        ),
    };
    for original in [
        local("k"),
        local("b"),
        reference(vec![], "k"),
        reference(vec![], "b"),
        // The root's old instance slot moves too: k@R is relative to Q now.
        reference(vec![hop("k", "r")], "r-root"),
        reference(vec![hop("b", "r")], "r-root"),
        persistent,
    ] {
        let before = original.clone();
        let rewritten = inputs
            .rewrite(&original, |_| Ok::<_, Infallible>(()))
            .unwrap();
        assert!(matches!(rewritten, Cow::Borrowed(value) if std::ptr::eq(value, &original)));
        assert_eq!(original, before);
        assert!(inputs.arguments().is_empty());
    }

    // Equal spelling never turns a current-scope addition into the moved Source(b).
    let addition = PrefabEntityOrigin::InstanceAddition(id("b"));
    for (name, original) in [
        (
            "extracted.0",
            EntityReference::Authored {
                reference: AuthoredEntityReference {
                    through: vec![],
                    target: AuthoredReferenceTarget::Entity(addition.clone()),
                },
            },
        ),
        (
            "extracted.1",
            reference(
                vec![PrefabInstanceHop {
                    anchor: addition,
                    expected_base: key("r"),
                }],
                "r-root",
            ),
        ),
    ] {
        let before = original.clone();
        let rewritten = inputs
            .rewrite(&original, |_| Ok::<_, Infallible>(()))
            .unwrap();
        assert!(matches!(&rewritten, Cow::Owned(_)));
        assert_eq!(rewritten.as_ref(), &input_reference(name));
        assert_eq!(inputs.arguments()[&input(name)], original);
        assert_eq!(original, before);
    }
    assert_eq!(inputs.arguments().len(), 2);
    assert_eq!(source, before);
}

#[test]
fn moved_outward_references_collect_exact_p_arguments_and_reuse_equal_dependencies() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(&source, &empty_basis());
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let mut inputs = MovedReferenceInputs::new(&relocation);
    let outward = local("x");
    let p_input = input_reference("u");
    let outside_slot = reference(vec![hop("x", "r")], "b");
    for (original, name) in [
        (&outward, "extracted.0"),
        (&p_input, "extracted.1"),
        (&outside_slot, "extracted.2"),
        (&outward, "extracted.0"),
        (&p_input, "extracted.1"),
        (&outside_slot, "extracted.2"),
    ] {
        let before = original.clone();
        let rewritten = inputs
            .rewrite(original, |_| Ok::<_, Infallible>(()))
            .unwrap();
        assert!(matches!(&rewritten, Cow::Owned(_)));
        assert_eq!(rewritten.as_ref(), &input_reference(name));
        assert_eq!(original, &before);
    }
    assert_eq!(
        inputs.arguments(),
        &BTreeMap::from([
            (input("extracted.0"), outward),
            (input("extracted.1"), p_input),
            (input("extracted.2"), outside_slot),
        ]),
    );
    assert_eq!(source, before);
}

#[test]
fn moved_routes_rewrite_explicit_later_p_reentries_in_local_and_outward_paths() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(&source, &empty_basis());
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let mut inputs = MovedReferenceInputs::new(&relocation);

    for (first, external) in [("k", false), ("x", true)] {
        let original = reference(
            vec![
                hop(first, "r"),
                hop("return", "p"),
                hop("k", "r"),
                hop("return", "p"),
            ],
            "b",
        );
        let before = original.clone();
        let expected = reference(
            vec![
                hop(first, "r"),
                hop("return", "p"),
                hop("k", "q"),
                hop("k", "r"),
                hop("return", "p"),
                hop("k", "q"),
            ],
            "b",
        );
        let rewritten = inputs
            .rewrite(&original, |_| Ok::<_, Infallible>(()))
            .unwrap();
        assert!(matches!(&rewritten, Cow::Owned(_)));
        if external {
            assert_eq!(rewritten.as_ref(), &input_reference("extracted.0"));
            assert_eq!(
                inputs.arguments(),
                &BTreeMap::from([(input("extracted.0"), expected)]),
            );
        } else {
            assert_eq!(rewritten.as_ref(), &expected);
            assert!(inputs.arguments().is_empty());
        }
        assert_eq!(original, before);
    }
    assert_eq!(source, before);
}

#[test]
fn moved_admission_is_inclusive_and_late_refusal_preserves_prior_arguments_and_borrows() {
    let source = source();
    let before = source.clone();
    let source_entities = source.entities.as_ptr();
    let cut = capture_cut(&source, &empty_basis());
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let original = reference(vec![hop("x", "r"), hop("return", "p")], "b");
    let reference_before = original.clone();
    let EntityReference::Authored {
        reference: authored,
    } = &original
    else {
        unreachable!()
    };
    let original_hops = authored.through.as_ptr();
    let seed = || {
        let mut inputs = MovedReferenceInputs::new(&relocation);
        let outward = local("x");
        assert_eq!(
            inputs
                .rewrite(&outward, |_| Ok::<_, Infallible>(()))
                .unwrap()
                .as_ref(),
            &input_reference("extracted.0"),
        );
        inputs
    };
    let mut measured = seed();
    let mut charges = Vec::new();
    let expected = measured
        .rewrite(&original, |bytes| {
            charges.push(bytes.expect("this bounded rewrite has a known byte cost"));
            Ok::<_, Infallible>(())
        })
        .unwrap()
        .into_owned();
    let used: usize = charges.iter().sum();
    assert!(used > 1);
    assert!(charges.len() > 2);
    assert_eq!(expected, input_reference("extracted.1"));

    for (allowance, accepted) in [(used, true), (used - 1, false)] {
        let mut inputs = seed();
        let previous = inputs.arguments().clone();
        let previous_ptr = std::ptr::from_ref(&inputs.arguments()[&input("extracted.0")]);
        let mut remaining = allowance;
        let result = inputs.rewrite(&original, |bytes| {
            remaining = bytes
                .and_then(|bytes| remaining.checked_sub(bytes))
                .ok_or(())?;
            Ok(())
        });
        if accepted {
            assert_eq!(result.unwrap().as_ref(), &expected);
            assert_eq!(remaining, 0);
            assert_eq!(inputs.arguments(), measured.arguments());
        } else {
            assert_eq!(result, Err(()));
            assert_eq!(inputs.arguments(), &previous);
            assert_eq!(
                std::ptr::from_ref(&inputs.arguments()[&input("extracted.0")]),
                previous_ptr,
            );
            // A failed proposal cannot consume the next generated input name.
            assert_eq!(
                inputs
                    .rewrite(&original, |_| Ok::<_, Infallible>(()))
                    .unwrap()
                    .as_ref(),
                &expected,
            );
            assert_eq!(inputs.arguments(), measured.arguments());
        }
    }

    // Exercise refusal at both final callback boundaries, after route construction work.
    for denied_call in [charges.len() - 1, charges.len()] {
        let mut inputs = seed();
        let previous = inputs.arguments().clone();
        let previous_ptr = std::ptr::from_ref(&inputs.arguments()[&input("extracted.0")]);
        let error = Box::new(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "moved input denied",
        ));
        let error_ptr = std::ptr::from_ref(error.as_ref());
        let mut error = Some(error);
        let mut calls = 0;
        let result = inputs.rewrite(&original, |bytes| {
            assert!(bytes.is_some());
            calls += 1;
            if calls == denied_call {
                Err(error
                    .take()
                    .expect("admission must stop at its first error"))
            } else {
                Ok(())
            }
        });
        let Err(returned) = result else {
            panic!("late admission refusal must not publish a Q input");
        };
        assert_eq!(calls, denied_call);
        assert!(std::ptr::eq(returned.as_ref(), error_ptr));
        assert_eq!(inputs.arguments(), &previous);
        assert_eq!(
            std::ptr::from_ref(&inputs.arguments()[&input("extracted.0")]),
            previous_ptr,
        );
        assert_eq!(original, reference_before);
        assert_eq!(authored.through.as_ptr(), original_hops);
        assert_eq!(source, before);
        assert_eq!(source.entities.as_ptr(), source_entities);
        assert!(std::ptr::eq(cut.root(), &source.entities[2]));
        let SourceDocumentInput::Prefab(borrowed_source) = cut.source() else {
            panic!("refusal must retain the actual Prefab source borrow");
        };
        assert!(std::ptr::eq(borrowed_source, &source));
    }
}

#[test]
fn outward_input_retry_admits_the_prior_route_scan_before_cost_comparison() {
    let source = source();
    let basis = empty_basis();
    let cut = capture_cut(&source, &basis);
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let mut inputs = MovedReferenceInputs::new(&relocation);
    let hops = 8_192;
    let external = reference(vec![hop("x", "outside"); hops], "external");
    inputs
        .rewrite(&external, |_| Ok::<_, Infallible>(()))
        .unwrap();
    let before = inputs.arguments().clone();
    let pending = input_reference("u");
    let mut first = None;
    let result = inputs.rewrite(&pending, |bytes| {
        assert!(
            first.replace(bytes).is_none(),
            "refusal stops at the first gate"
        );
        Err::<(), _>(())
    });
    assert_eq!(result, Err(()));
    assert_eq!(
        first,
        Some(Some((hops + 2) * std::mem::size_of::<PrefabInstanceHop>())),
        "both cost walks require length-based admission before consulting their logical bytes",
    );
    assert_eq!(inputs.arguments(), &before);
}

#[test]
fn moved_outward_input_preserves_actual_x_targets_through_authoring_compilation() {
    fn request(name: &str) -> AssetRef {
        AssetRef::path(format!("prefabs/moved-reference-tests/{name}.ron")).unwrap()
    }

    fn projected(document: &ExpandedSceneDocument, path: &[(&str, &str)]) -> SceneEntityId {
        let steps = path
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
        let entity = matches
            .next()
            .expect("the typed occurrence must exist")
            .0
            .clone();
        assert!(matches.next().is_none());
        entity
    }

    let component = ComponentTypeId::new("game.MovedReferenceLink");
    let owner = PersistentComponentSchemaOwnerId::try_new("game.moved-reference-links").unwrap();
    let schema = ComponentSchema::new(component.clone(), "Moved link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::EntityRef,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
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
    assert!(basis.has_authoring_adapter(&component));
    assert!(!basis.has_native_preview_adapter(&component));
    let mut p = source();
    for entity in p.entities.iter_mut().filter(|entity| entity.id != id("x")) {
        entity.components.insert(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("target", ComponentValue::EntityReference(local("x")))]),
            )
            .with_owner(owner.clone()),
        );
    }
    let p = Arc::new(p);
    let original = p.as_ref().clone();
    let cut = capture_cut(&p, &basis);
    let (p_key, q_key) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p_key, &q_key).unwrap();
    let mut inputs = MovedReferenceInputs::new(&relocation);

    // This finite AFTER fixture moves exactly k/b, with an explicit input on Q.
    let mut q = PrefabDocument::new(
        [p.placements[0].clone(), p.placements[1].clone()],
        cut.entities().cloned(),
    );
    for entity in &mut q.entities {
        let ComponentValue::Map(fields) = &mut entity.components.get_mut(&component).unwrap().value
        else {
            panic!("the admitted data schema uses a map");
        };
        let ComponentValue::EntityReference(reference) = fields.get_mut("target").unwrap() else {
            panic!("the source field is a typed reference");
        };
        *reference = inputs
            .rewrite(reference, |_| Ok::<_, Infallible>(()))
            .unwrap()
            .into_owned();
    }
    assert_eq!(
        inputs.arguments(),
        &BTreeMap::from([(input("extracted.0"), local("x"))])
    );
    q.inputs = inputs.arguments().keys().cloned().collect();
    let q = Arc::new(q);
    let mut new_q = PrefabInstance::root(
        request("q"),
        q_key.clone(),
        PrefabEntityTarget::source(id("k")),
    );
    new_q.arguments = Some(PrefabReferenceArguments {
        base: q_key.clone(),
        values: inputs.arguments().clone(),
    });
    let mut anchor = SceneEntityRecord::new(id("k"));
    anchor.prefab = Some(new_q);
    let mut p_after = PrefabDocument::from_ordered_roots([
        anchor,
        p.entities
            .iter()
            .find(|entity| entity.id == id("x"))
            .unwrap()
            .clone(),
    ]);
    p_after.inputs = p.inputs.clone();
    let p_after = Arc::new(p_after);
    let mut host = SceneEntityRecord::new(id("host"));
    host.prefab = Some(PrefabInstance::forest(request("p")));
    let scene = SceneDocument::from_ordered_roots([host]);
    let scene_before = scene.clone();
    let compile = |sources: &[(&str, &Arc<PrefabDocument>)]| {
        let report = try_compile_authoring_scene_with_loader(
            &scene,
            &basis,
            Default::default(),
            None,
            |reference, _| {
                Ok::<_, Infallible>(sources.iter().find_map(|(name, source)| {
                    (request(name) == *reference).then(|| {
                        ResolvedPrefabSource::bound_authoring_source(
                            Arc::clone(source),
                            basis.clone(),
                            key(name),
                        )
                    })
                }))
            },
        )
        .unwrap();
        assert!(!report.diagnostics().has_errors(), "{report:?}");
        assert!(report.failure().is_none(), "{report:?}");
        assert!(!report.budget_exceeded());
        report
            .into_parts()
            .candidate
            .expect("the authoring scene must compile")
    };
    let targets = |document: &ExpandedSceneDocument, b_path: &[(&str, &str)]| {
        let k = projected(document, &[("p", "k")]);
        let b = projected(document, b_path);
        let x = projected(document, &[("p", "x")]);
        let correspondence = BTreeMap::from([(k.clone(), "k"), (b.clone(), "b"), (x, "x")]);
        assert_eq!(correspondence.len(), 3);
        [(k, "k"), (b, "b")]
            .into_iter()
            .map(|(entity, label)| {
                let entity = document
                    .entities()
                    .iter()
                    .find(|record| record.id() == &entity)
                    .unwrap();
                let value = &entity.components()[&component].value;
                let EntityReference::SceneLocal { entity: target } =
                    value.field_entity_reference("target").unwrap()
                else {
                    panic!(
                        "the compiler must resolve the authored input to an actual local target"
                    );
                };
                (label, correspondence[target])
            })
            .collect::<BTreeMap<_, _>>()
    };
    let before = compile(&[("p", &p)]);
    let after = compile(&[("p", &p_after), ("q", &q)]);
    let expected = BTreeMap::from([("k", "x"), ("b", "x")]);
    assert_eq!(targets(&before, &[("p", "b")]), expected);
    assert_eq!(targets(&after, &[("p", "k"), ("q", "b")]), expected);
    assert_eq!(p.as_ref(), &original);
    assert_eq!(scene, scene_before);
}
