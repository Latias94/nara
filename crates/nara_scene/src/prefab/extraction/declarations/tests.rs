//! Physical declaration coverage using the real cut, relocation and Reflect Schema owners.

use std::convert::Infallible;

use nara_asset::AssetRef;
use nara_core::ByteLimit;
use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget, ReferenceInputId};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::AuthoringAdapterReceipt,
    PrefabComponentSetMode, PrefabDocument, PrefabEntityOrigin, PrefabInstanceHop,
    PrefabPropertyClaim, PrefabPropertyIntent, ResolvedPrefabSource, SceneCompilationLimits,
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePlacement, SourceDocumentKind,
    prefab::{
        SourceDocumentInput,
        extraction::{
            SourceCut, relocation::MovedReferenceInputs, try_capture_source_cut_with_loader,
        },
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "43ba66e6-ac69-41d3-a27a-303c60ea93b6".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.ExtractionDeclarationLink")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.extraction-declarations").unwrap()
}

fn local(entity: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(entity) }
}

fn field() -> ComponentFieldId {
    ComponentFieldId::new("target")
}

fn basis() -> AuthoringSchemaBasis {
    let field = ComponentFieldSchema::optional_with_default(
        field(),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueShape::EntityRef,
        ComponentValue::EntityReference(local("x")),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let schema = ComponentSchema::new(component(), "Link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field]);
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
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn source() -> PrefabDocument {
    PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("b")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("k"),
                ))),
            ),
            SceneEntityPlacement::root(id("x")),
        ],
        ["k", "b", "x"].map(|name| SceneEntityRecord::new(id(name))),
    )
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
        |_, _| -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("a direct-only source cut must not load a Prefab")
        },
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    report.cut.unwrap()
}

fn instance(root: &str) -> PrefabInstance {
    PrefabInstance::root(
        AssetRef::path("prefabs/extraction-declarations/p.ron").unwrap(),
        key("p"),
        PrefabEntityTarget::source(id(root)),
    )
}

fn arguments() -> PrefabReferenceArguments {
    PrefabReferenceArguments {
        base: key("p"),
        values: [(ReferenceInputId::new("u").unwrap(), local("x"))].into(),
    }
}

fn record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(local("b")))]),
    )
    .with_owner(owner())
    .with_contextual_default(field(), ComponentValue::EntityReference(local("x")))
}

fn moved_target(entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("k")),
            expected_base: key("q"),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    }
}

fn deep_declaration(depth: usize, additions: bool) -> PrefabInstance {
    let mut declaration = instance("b");
    declaration.arguments = Some(PrefabReferenceArguments {
        // Payloads use the caller's lexical environment, independently of this base.
        base: key("other"),
        values: [(ReferenceInputId::new("u").unwrap(), local("b"))].into(),
    });
    let mut overlay = PrefabOverlay::new(key("p"));
    let mut addition = PrefabAddition::new(id("deep"));
    addition.components.insert(component(), record());
    overlay.additions.push(addition);
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("k")),
        value: None,
    });
    overlay.components = vec![
        PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: PrefabEntityTarget::source(id("b")),
            component: component(),
            record: record(),
        },
        PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("b")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(field()),
                intent: PrefabPropertyIntent::Set(ComponentValue::EntityReference(local("b"))),
            }],
        },
        PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(id("b")),
            component: component(),
        },
    ];
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("deep")),
        ScenePlacement::after(
            ScenePlacementParent::Entity(PrefabEntityTarget::source(id("b"))),
            PrefabEntityTarget::source(id("b")),
        ),
    ));
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("b")));
    declaration.overrides = Some(overlay);
    for _ in 1..depth {
        let mut parent = instance("k");
        let mut overlay = PrefabOverlay::new(key("other"));
        if additions {
            let mut addition = PrefabAddition::new(id("dormant"));
            addition.prefab = Some(declaration);
            overlay.additions.push(addition);
        } else {
            overlay.instances.push(PrefabInstanceClaim {
                target: PrefabEntityTarget::source(id("k")),
                value: Some(declaration),
            });
        }
        overlay
            .suppressions
            .push(PrefabEntityTarget::source(id("b")));
        parent.overrides = Some(overlay);
        declaration = parent;
    }
    declaration
}

fn assert_deep_rewrite(declaration: &PrefabInstance, depth: usize, additions: bool) {
    let mut leaf = declaration;
    for _ in 1..depth {
        let overlay = leaf.overrides.as_ref().unwrap();
        assert_eq!(overlay.base, key("other"));
        // Returning from a child must resume the parent's original structural environment.
        assert_eq!(overlay.suppressions, [PrefabEntityTarget::source(id("b"))]);
        leaf = if additions {
            overlay.additions[0].prefab.as_ref().unwrap()
        } else {
            assert_eq!(
                overlay.instances[0].target,
                PrefabEntityTarget::source(id("k")),
            );
            overlay.instances[0].value.as_ref().unwrap()
        };
    }
    assert_eq!(
        leaf.binding,
        PrefabInstanceBinding::Root {
            base: key("p"),
            root: moved_target("b"),
        },
    );
    let reference = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: moved_target("b").through,
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
        },
    };
    let arguments = leaf.arguments.as_ref().unwrap();
    assert_eq!(arguments.base, key("other"));
    assert_eq!(
        arguments.values[&ReferenceInputId::new("u").unwrap()],
        reference,
    );
    let overlay = leaf.overrides.as_ref().unwrap();
    assert_eq!(overlay.base, key("p"));
    let expected_record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(reference.clone()))]),
    )
    .with_owner(owner())
    .with_contextual_default(field(), ComponentValue::EntityReference(local("x")));
    assert_eq!(
        overlay.additions[0].components[&component()],
        expected_record
    );
    assert_eq!(overlay.instances[0].target, moved_target("k"));
    assert!(overlay.instances[0].value.is_none());
    let PrefabComponentIntent::Set { target, record, .. } = &overlay.components[0] else {
        panic!("expected complete Set")
    };
    assert_eq!(target, &moved_target("b"));
    assert_eq!(record, &expected_record);
    let PrefabComponentIntent::Properties { target, claims, .. } = &overlay.components[1] else {
        panic!("expected Properties")
    };
    assert_eq!(target, &moved_target("b"));
    assert_eq!(
        claims[0].intent,
        PrefabPropertyIntent::Set(ComponentValue::EntityReference(reference)),
    );
    let PrefabComponentIntent::Suppress { target, .. } = &overlay.components[2] else {
        panic!("expected Suppress")
    };
    assert_eq!(target, &moved_target("b"));
    assert_eq!(
        overlay.placements[0].target,
        PrefabEntityTarget::addition(id("deep")),
    );
    assert_eq!(
        overlay.placements[0].placement.parent,
        ScenePlacementParent::Entity(moved_target("b")),
    );
    assert_eq!(
        overlay.placements[0].placement.slot,
        ScenePlacementSlot::After(moved_target("b")),
    );
    assert_eq!(overlay.suppressions, [moved_target("b")]);
}

fn drop_chain(mut declaration: PrefabInstance, additions: bool) {
    // Avoid testing derived recursive Drop instead of the mutation walk's native-stack bound.
    while let Some(overlay) = &mut declaration.overrides {
        let nested = if additions {
            overlay.additions[0].prefab.take()
        } else {
            overlay.instances[0].value.take()
        };
        let Some(nested) = nested else {
            break;
        };
        declaration = nested;
    }
}

#[test]
fn depth_64_rewrites_complete_declarations_with_inclusive_working_byte_limits() {
    let basis = basis();
    let source = source();
    let cut = capture_cut(&source, &basis);
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    assert_eq!(MAX_PHYSICAL_DEPTH, 64);
    for additions in [true, false] {
        let mut declaration = deep_declaration(MAX_PHYSICAL_DEPTH, additions);
        let mut work = ExtractionWork::new(Default::default(), Some(17)).unwrap();
        let mut visited = Vec::new();
        let source_changed = rewrite_instance(
            &mut declaration,
            &relocation,
            &basis,
            false,
            &mut work,
            &mut |reference, work| {
                visited.push(reference.clone());
                let rewritten =
                    relocation.retained_reference(&p, reference, |bytes| work.admit(bytes))?;
                work.own_reference(rewritten)
            },
        )
        .unwrap();
        assert!(source_changed);
        assert_eq!(
            visited,
            [
                local("b"),
                local("b"),
                local("x"),
                local("b"),
                local("x"),
                local("b"),
            ],
        );
        assert_deep_rewrite(&declaration, MAX_PHYSICAL_DEPTH, additions);
        drop_chain(declaration, additions);
        let usage = work.usage();
        for (bytes, admitted) in [(usage.1, true), (usage.1 - 1, false)] {
            let limits = SceneCompilationLimits::default()
                .with_working_bytes(ByteLimit::new(bytes).unwrap());
            let mut work = ExtractionWork::new(limits, Some(17)).unwrap();
            let mut declaration = deep_declaration(MAX_PHYSICAL_DEPTH, additions);
            let result = rewrite_instance(
                &mut declaration,
                &relocation,
                &basis,
                false,
                &mut work,
                &mut |reference, work| {
                    let rewritten =
                        relocation.retained_reference(&p, reference, |bytes| work.admit(bytes))?;
                    work.own_reference(rewritten)
                },
            );
            if admitted {
                assert!(result.unwrap());
                assert_eq!(work.usage(), usage);
                assert_deep_rewrite(&declaration, MAX_PHYSICAL_DEPTH, additions);
            } else {
                assert!(
                    matches!(result, Err(ExtractionError::Capacity)),
                    "{result:?}"
                );
            }
            drop_chain(declaration, additions);
        }
    }
}

#[test]
fn depth_65_refuses_before_rewriting_the_deepest_declared_node() {
    let basis = basis();
    let source = source();
    let cut = capture_cut(&source, &basis);
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let depth = MAX_PHYSICAL_DEPTH + 1;
    for additions in [true, false] {
        let mut declaration = deep_declaration(depth, additions);
        let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
        let result = rewrite_instance(
            &mut declaration,
            &relocation,
            &basis,
            false,
            &mut work,
            &mut |_, _| panic!("the only reference payloads are at refused depth 65"),
        );
        assert!(
            matches!(result, Err(ExtractionError::InvalidSource)),
            "{result:?}",
        );
        let mut leaf = &declaration;
        for _ in 1..depth {
            let overlay = leaf.overrides.as_ref().unwrap();
            leaf = if additions {
                overlay.additions[0].prefab.as_ref().unwrap()
            } else {
                overlay.instances[0].value.as_ref().unwrap()
            };
        }
        assert_eq!(leaf.binding, instance("b").binding);
        assert_eq!(
            leaf.arguments.as_ref().unwrap().values[&ReferenceInputId::new("u").unwrap()],
            local("b"),
        );
        assert_eq!(
            leaf.overrides.as_ref().unwrap().components[0],
            PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Replace,
                target: PrefabEntityTarget::source(id("b")),
                component: component(),
                record: record(),
            },
        );
        drop_chain(declaration, additions);
    }
}

#[test]
fn ancestor_capacity_is_reused_for_siblings_and_charges_full_growth_requests() {
    let basis = basis();
    let source = source();
    let cut = capture_cut(&source, &basis);
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let empty = || {
        let mut declaration = PrefabInstance::forest(instance("k").source);
        declaration.overrides = Some(PrefabOverlay::new(p.clone()));
        declaration
    };
    let mut chain = empty();
    let mut wide = empty();
    for _ in 1..9 {
        let mut addition = PrefabAddition::new(id("dormant"));
        addition.prefab = Some(chain);
        let mut parent = empty();
        parent.overrides.as_mut().unwrap().additions.push(addition);
        chain = parent;
        let mut addition = PrefabAddition::new(id("dormant"));
        addition.prefab = Some(empty());
        wide.overrides.as_mut().unwrap().additions.push(addition);
    }
    let mut wide_work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    let wide_changed = rewrite_instance(
        &mut wide,
        &relocation,
        &basis,
        false,
        &mut wide_work,
        &mut |_, _| panic!("no reference payload exists"),
    )
    .unwrap();
    assert!(!wide_changed);
    let mut chain_work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    let chain_changed = rewrite_instance(
        &mut chain,
        &relocation,
        &basis,
        false,
        &mut chain_work,
        &mut |_, _| panic!("no reference payload exists"),
    )
    .unwrap();
    assert!(!chain_changed);
    // Both candidates contain nine identical instances/overlays and eight equal-length
    // additions. Width reuses capacity four; depth additionally requests capacities 8 and 16.
    assert_eq!(
        chain_work.usage().1 - wide_work.usage().1,
        (8 + 16) * size_of::<DeclarationFrame<'_>>(),
    );
    assert_eq!(chain_work.usage().0 - wide_work.usage().0, 2);
    // This chain's empty leaf needs no indexing during teardown.
    while let Some(nested) = chain
        .overrides
        .as_mut()
        .unwrap()
        .additions
        .first_mut()
        .and_then(|addition| addition.prefab.take())
    {
        chain = nested;
    }
}

#[test]
fn dormant_declarations_use_original_structural_bases_and_the_supplied_moved_lexical_scope() {
    let basis = basis();
    let source = source();
    let cut = capture_cut(&source, &basis);
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let mut nested = instance("b");
    nested.arguments = Some(arguments());
    let mut addition = PrefabAddition::new(id("b"));
    addition.components.insert(component(), record());
    addition.prefab = Some(nested.clone());
    let mut overlay = PrefabOverlay::new(p.clone());
    overlay.additions = vec![
        addition,
        PrefabAddition::new(id("before")),
        PrefabAddition::new(id("after")),
    ];
    overlay.instances = vec![
        PrefabInstanceClaim {
            target: PrefabEntityTarget::source(id("k")),
            value: Some(nested),
        },
        PrefabInstanceClaim {
            target: PrefabEntityTarget::addition(id("b")),
            value: None,
        },
    ];
    overlay.components = vec![
        PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: PrefabEntityTarget::source(id("b")),
            component: component(),
            record: record(),
        },
        PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("x")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(field()),
                intent: PrefabPropertyIntent::Set(ComponentValue::EntityReference(local("x"))),
            }],
        },
        PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(id("k")),
            component: component(),
        },
    ];
    overlay.placements = vec![
        SceneEntityPlacement::new(
            PrefabEntityTarget::addition(id("b")),
            ScenePlacement::first(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id("b"),
            ))),
        ),
        SceneEntityPlacement::new(
            PrefabEntityTarget::addition(id("before")),
            ScenePlacement::before(
                ScenePlacementParent::Entity(PrefabEntityTarget::source(id("k"))),
                PrefabEntityTarget::source(id("b")),
            ),
        ),
        SceneEntityPlacement::new(
            PrefabEntityTarget::addition(id("after")),
            ScenePlacement::after(
                ScenePlacementParent::Entity(PrefabEntityTarget::source(id("k"))),
                PrefabEntityTarget::source(id("b")),
            ),
        ),
    ];
    overlay.suppressions = vec![
        PrefabEntityTarget::addition(id("b")),
        PrefabEntityTarget::source(id("b")),
    ];
    let mut original_instance = instance("k");
    original_instance.arguments = Some(arguments());
    original_instance.overrides = Some(overlay);
    let mut host = SceneEntityRecord::new(id("host"));
    host.prefab = Some(original_instance);
    let original = SceneDocument::from_ordered_roots([host]);
    let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    work.source_copy(SourceDocumentInput::Scene(&original))
        .unwrap();
    let mut candidate = original.clone();
    let rewritten = candidate.entities[0].prefab.as_mut().unwrap();
    let mut inputs = MovedReferenceInputs::new(&relocation);
    let source_changed = rewrite_instance(
        rewritten,
        &relocation,
        &basis,
        false,
        &mut work,
        &mut |reference, work| {
            let reference = inputs.rewrite(reference, |bytes| work.admit(bytes))?;
            work.own_reference(reference)
        },
    )
    .unwrap();
    assert!(source_changed);

    let input = EntityReference::Authored {
        reference: AuthoredEntityReference::input(ReferenceInputId::new("extracted.0").unwrap()),
    };
    assert_eq!(
        inputs.arguments().values().collect::<Vec<_>>(),
        vec![&local("x")],
    );
    let original_instance = original.entities[0].prefab.as_ref().unwrap();
    assert_eq!(rewritten.binding, original_instance.binding);
    assert_eq!(rewritten.source, original_instance.source);
    assert_eq!(rewritten.arguments.as_ref().unwrap().base, p);
    assert_eq!(
        rewritten.arguments.as_ref().unwrap().values[&ReferenceInputId::new("u").unwrap()],
        input,
    );
    let overlay = rewritten.overrides.as_ref().unwrap();
    assert_eq!(overlay.base, p);
    let addition = &overlay.additions[0];
    assert_eq!(addition.id, id("b"));
    let mut expected_record = record();
    expected_record
        .contextual_defaults
        .insert(field(), ComponentValue::EntityReference(input.clone()));
    assert_eq!(addition.components[&component()], expected_record);
    for nested in [
        addition.prefab.as_ref().unwrap(),
        overlay.instances[0].value.as_ref().unwrap(),
    ] {
        assert_eq!(nested.source, instance("b").source);
        assert_eq!(
            nested.binding,
            PrefabInstanceBinding::Root {
                base: p.clone(),
                root: moved_target("b"),
            },
        );
        assert_eq!(nested.arguments.as_ref().unwrap().base, p);
        assert_eq!(
            nested.arguments.as_ref().unwrap().values[&ReferenceInputId::new("u").unwrap()],
            input,
        );
    }
    assert_eq!(overlay.instances[0].target, moved_target("k"));
    assert_eq!(
        overlay.instances[1].target,
        PrefabEntityTarget::addition(id("b")),
    );
    assert!(overlay.instances[1].value.is_none());
    let PrefabComponentIntent::Set { target, record, .. } = &overlay.components[0] else {
        panic!("expected complete Set")
    };
    assert_eq!(target, &moved_target("b"));
    assert_eq!(record, &expected_record);
    let PrefabComponentIntent::Properties { target, claims, .. } = &overlay.components[1] else {
        panic!("expected Properties")
    };
    assert_eq!(target, &PrefabEntityTarget::source(id("x")));
    assert_eq!(
        claims[0].intent,
        PrefabPropertyIntent::Set(ComponentValue::EntityReference(input)),
    );
    assert_eq!(
        overlay.placements[0].target,
        PrefabEntityTarget::addition(id("b")),
    );
    assert_eq!(
        overlay.placements[0].placement.parent,
        ScenePlacementParent::Entity(moved_target("b")),
    );
    assert_eq!(
        overlay.placements[1].placement.slot,
        ScenePlacementSlot::Before(moved_target("b")),
    );
    assert_eq!(
        overlay.placements[2].placement.slot,
        ScenePlacementSlot::After(moved_target("b")),
    );
    assert_eq!(
        overlay.suppressions,
        vec![PrefabEntityTarget::addition(id("b")), moved_target("b")],
    );
    let untouched = original_instance.overrides.as_ref().unwrap();
    assert_eq!(
        untouched.additions[0].components[&component()].contextual_defaults[&field()],
        ComponentValue::EntityReference(local("x")),
    );
    assert_eq!(
        untouched.instances[0].target,
        PrefabEntityTarget::source(id("k")),
    );
}

#[test]
fn nested_properties_target_changes_report_without_reference_payloads() {
    let basis = basis();
    let source = source();
    let cut = capture_cut(&source, &basis);
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    for additions in [true, false] {
        let mut nested = instance("k");
        let mut overlay = PrefabOverlay::new(p.clone());
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("b")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(field()),
                intent: PrefabPropertyIntent::Absent,
            }],
        });
        nested.overrides = Some(overlay);
        let mut declaration = instance("k");
        let mut overlay = PrefabOverlay::new(key("other"));
        if additions {
            let mut addition = PrefabAddition::new(id("dormant"));
            addition.prefab = Some(nested);
            overlay.additions.push(addition);
        } else {
            overlay.instances.push(PrefabInstanceClaim {
                target: PrefabEntityTarget::source(id("k")),
                value: Some(nested),
            });
        }
        declaration.overrides = Some(overlay);
        let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
        let source_changed = rewrite_instance(
            &mut declaration,
            &relocation,
            &basis,
            false,
            &mut work,
            &mut |_, _| panic!("Absent has no reference payload"),
        )
        .unwrap();
        assert!(source_changed);
        assert_eq!(declaration.binding, instance("k").binding);
        let overlay = declaration.overrides.as_ref().unwrap();
        let nested = if additions {
            overlay.additions[0].prefab.as_ref().unwrap()
        } else {
            assert_eq!(
                overlay.instances[0].target,
                PrefabEntityTarget::source(id("k")),
            );
            overlay.instances[0].value.as_ref().unwrap()
        };
        assert_eq!(nested.binding, instance("k").binding);
        let PrefabComponentIntent::Properties { target, .. } =
            &nested.overrides.as_ref().unwrap().components[0]
        else {
            panic!("Properties target")
        };
        assert_eq!(target, &moved_target("b"));
    }
}

#[test]
fn empty_absent_and_suppress_intents_still_require_schema_without_reference_values() {
    let basis = basis();
    let source = source();
    let cut = capture_cut(&source, &basis);
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let unknown = ComponentTypeId::new("game.UnknownExtractionLink");
    for intent in [
        PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("b")),
            component: unknown.clone(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![],
        },
        PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("b")),
            component: unknown.clone(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(field()),
                intent: PrefabPropertyIntent::Absent,
            }],
        },
        PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(id("b")),
            component: unknown,
        },
    ] {
        let mut overlay = PrefabOverlay::new(p.clone());
        overlay.components.push(intent);
        let mut declaration = instance("k");
        declaration.overrides = Some(overlay);
        let mut host = SceneEntityRecord::new(id("host"));
        host.prefab = Some(declaration);
        let original = SceneDocument::from_ordered_roots([host]);
        let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
        work.source_copy(SourceDocumentInput::Scene(&original))
            .unwrap();
        let mut candidate = original.clone();
        let result = rewrite_instance(
            candidate.entities[0].prefab.as_mut().unwrap(),
            &relocation,
            &basis,
            false,
            &mut work,
            &mut |_, _| panic!("no reference payload exists"),
        );
        assert!(
            matches!(result, Err(ExtractionError::UnknownSchema)),
            "{result:?}",
        );
        let untouched = original.entities[0].prefab.as_ref().unwrap();
        assert_eq!(untouched.overrides.as_ref().unwrap().base, p);
    }
}
