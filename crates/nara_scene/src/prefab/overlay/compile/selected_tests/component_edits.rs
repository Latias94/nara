use super::*;
use crate::{
    __private::{
        AuthoringPrefabComponentEditError as EditError,
        try_inspect_authoring_prefab_instance_with_loader,
    },
    AuthoringDocumentSource, AuthoringPrefabInstanceSnapshot, PrefabComponentEdit,
    PrefabComponentSetMode, PrefabDeclarationLimits, SceneAuthoringSession, SceneDocumentCandidate,
};

fn edit_basis() -> AuthoringSchemaBasis {
    let capabilities = ComponentCapability::SCENE_AUTHORING;
    basis_with_schema(
        ComponentSchema::new(component(), "Complete record", ComponentSchemaVersion::ONE)
            .with_capabilities(capabilities)
            .with_fields([
                ComponentFieldSchema::optional(
                    ComponentFieldId::new("offset"),
                    "Offset",
                    ComponentFieldPath::from_fields(["offset"]),
                    ComponentValueKind::F64,
                )
                .with_capabilities(capabilities),
                ComponentFieldSchema::optional(
                    ComponentFieldId::new("labels"),
                    "Labels",
                    ComponentFieldPath::from_fields(["labels"]),
                    ComponentValueShape::list(ComponentValueKind::String.into()),
                )
                .with_capabilities(capabilities),
            ]),
    )
}

fn source_session(scene: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_str(&scene.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(
                AssetRef::path("scenes/component-edit.scene.json").unwrap(),
            ),
            basis,
        )
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document(published)
}

fn inspect(
    input: PrefabInspectionInput<'_>,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> AuthoringPrefabInstanceSnapshot {
    try_inspect_authoring_prefab_instance_with_loader(
        input,
        &id("mount"),
        basis,
        Default::default(),
        |asset, _| {
            let (name, prefab) = sources
                .iter()
                .find(|(name, _)| asset == &request(name))
                .unwrap();
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(prefab),
                basis.clone(),
                key(name),
            )))
        },
    )
    .unwrap()
}

fn mount_target(entity: PrefabEntityOrigin) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("mount")),
            expected_base: key("base"),
        }],
        entity,
    }
}

fn offset(value: f64) -> ComponentValue {
    ComponentValue::map([("offset", ComponentValue::f64(value).unwrap())])
}

fn owned_record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
}

#[test]
fn whole_component_edit_cannot_replace_or_suppress_an_additions_own_record_through_a_root_alias() {
    let basis = edit_basis();
    for addition_owns_component in [true, false] {
        let base = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("base-root"))]);
        let mut leaf = SceneEntityRecord::new(id("leaf-root"));
        let mut addition = PrefabAddition::new(id("graft"));
        if addition_owns_component {
            addition
                .components
                .insert(component(), owned_record(offset(12.0)));
        } else {
            leaf = leaf.with_component(component(), owned_record(offset(12.0)));
        }
        addition.prefab = Some(PrefabInstance::root(
            request("leaf"),
            key("leaf"),
            PrefabEntityTarget::source(id("leaf-root")),
        ));
        let sources = [
            ("base", Arc::new(base)),
            ("leaf", Arc::new(PrefabDocument::from_ordered_roots([leaf]))),
        ];
        let mut overlay = PrefabOverlay::new(key("base"));
        overlay.placements.push(placement(
            PrefabEntityTarget::addition(id("graft")),
            ScenePlacementParent::Root,
        ));
        overlay.additions.push(addition);
        let addition = mount_target(PrefabEntityOrigin::InstanceAddition(id("graft")));
        let mut alias = addition.clone();
        alias.through.push(PrefabInstanceHop {
            anchor: alias.entity,
            expected_base: key("leaf"),
        });
        alias.entity = PrefabEntityOrigin::SourceEntity(id("leaf-root"));
        let relative = PrefabEntityTarget {
            through: alias.through[1..].to_vec(),
            entity: alias.entity.clone(),
        };
        for declaration in [
            None,
            Some(PrefabComponentIntent::Suppress {
                target: relative.clone(),
                component: component(),
            }),
            Some(PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Replace,
                target: relative.clone(),
                component: component(),
                record: owned_record(offset(18.0)),
            }),
        ] {
            let hidden = matches!(&declaration, Some(PrefabComponentIntent::Suppress { .. }));
            let mut layer = overlay.clone();
            let has_declaration = declaration.is_some();
            layer.components.extend(declaration);
            let original =
                SceneDocument::from_ordered_roots([anchor("mount", "base", Some(layer))]);
            let session = source_session(&original, &basis);
            let shared = session.shared_document();
            let observed = inspect(
                PrefabInspectionInput::WorkingScene(&shared, session.revision()),
                &basis,
                &sources,
            );
            for address in [&addition, &alias] {
                assert_eq!(
                    observed
                        .target(address)
                        .unwrap()
                        .component(&component())
                        .is_err(),
                    hidden
                );
                for edit in [
                    if has_declaration {
                        PrefabComponentEdit::Revert
                    } else {
                        PrefabComponentEdit::Suppress
                    },
                    PrefabComponentEdit::Set {
                        mode: PrefabComponentSetMode::Replace,
                        value: offset(27.0),
                    },
                ] {
                    let restorative = matches!(&edit, PrefabComponentEdit::Revert);
                    let plan = observed.plan_component_edit(
                        SourceDocumentInput::Scene(&shared),
                        address,
                        &component(),
                        edit,
                        Default::default(),
                    );
                    if addition_owns_component && !restorative {
                        assert!(matches!(plan, Err(EditError::InvalidComponent)), "{plan:?}");
                    } else {
                        let plan = plan.unwrap();
                        let candidate = session
                            .prepare_prefab_component_edit(
                                session.revision(),
                                &plan,
                                Default::default(),
                            )
                            .unwrap();
                        let source = SourceDocumentInput::Scene(candidate.document());
                        assert!(
                            inspect(PrefabInspectionInput::Borrowed(source), &basis, &sources)
                                .verifies_component_edit(source, &plan)
                        );
                    }
                }
            }
            assert_eq!(session.document(), &original);
            assert_eq!(session.history_usage().records, 0);
        }
    }
}

#[test]
fn whole_component_edit_rejects_a_different_source_allocation_and_replays_signed_zero_exactly() {
    let basis = edit_basis();
    let sources = [(
        "base",
        Arc::new(PrefabDocument::from_ordered_roots(["first", "second"].map(
            |name| {
                SceneEntityRecord::new(id(name))
                    .with_component(component(), owned_record(offset(1.0)))
            },
        ))),
    )];
    let document = |target: &str, value: f64| {
        let mut overlay = PrefabOverlay::new(key("base"));
        overlay.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: PrefabEntityTarget::source(id(target)),
            component: component(),
            record: owned_record(offset(value)),
        });
        SceneDocument::from_ordered_roots([anchor("mount", "base", Some(overlay))])
    };
    let mut session = source_session(&document("first", 0.0), &basis);
    let shared = session.shared_document();
    let observed = inspect(
        PrefabInspectionInput::WorkingScene(&shared, session.revision()),
        &basis,
        &sources,
    );
    let address = mount_target(PrefabEntityOrigin::SourceEntity(id("first")));
    for wrong in [document("second", 0.0), document("first", -0.0)] {
        assert!(matches!(
            observed.plan_component_edit(
                SourceDocumentInput::Scene(&wrong),
                &address,
                &component(),
                PrefabComponentEdit::Revert,
                Default::default(),
            ),
            Err(EditError::StaleObservedIntent)
        ));
    }
    let plan = observed
        .plan_component_edit(
            SourceDocumentInput::Scene(&shared),
            &address,
            &component(),
            PrefabComponentEdit::Set {
                mode: PrefabComponentSetMode::Replace,
                value: offset(-0.0),
            },
            Default::default(),
        )
        .unwrap();
    let candidate = session
        .prepare_prefab_component_edit(session.revision(), &plan, Default::default())
        .unwrap();
    let source = SourceDocumentInput::Scene(candidate.document());
    assert!(
        inspect(PrefabInspectionInput::Borrowed(source), &basis, &sources)
            .verifies_component_edit(source, &plan)
    );
    let assert_bits = |scene: &SceneDocument, expected: f64| {
        let [PrefabComponentIntent::Set { record, .. }] = scene.entities[0]
            .prefab
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .components
            .as_slice()
        else {
            panic!("one complete component declaration expected");
        };
        let ComponentValue::Map(fields) = record.value() else {
            panic!("record map expected")
        };
        let ComponentValue::F64(actual) = &fields["offset"] else {
            panic!("offset expected")
        };
        assert_eq!(actual.get().to_bits(), expected.to_bits());
    };
    assert_bits(candidate.document(), -0.0);
    assert!(session.commit_authoring_edit(candidate).applied);
    assert_bits(session.document(), -0.0);
    assert_eq!(session.history_usage().records, 1);
    assert!(session.undo_authoring().applied);
    assert_bits(session.document(), 0.0);
    let history = session.history_status();
    let obsolete = session
        .prepare_prefab_component_edit(session.revision(), &plan, Default::default())
        .unwrap_err();
    assert!(
        obsolete
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.authoring-edit-stale-basis" })
    );
    assert_eq!(session.history_status(), history);
    assert!(session.redo_authoring().applied);
    assert_bits(session.document(), -0.0);
}

#[test]
fn whole_component_edit_checks_value_limits_and_retains_only_compact_value_storage() {
    let basis = edit_basis();
    let sources = [(
        "base",
        Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("root")),
        ])),
    )];
    let shared = Arc::new(SceneDocument::from_ordered_roots([anchor(
        "mount", "base", None,
    )]));
    let observed = inspect(
        PrefabInspectionInput::WorkingScene(&shared, crate::SceneAuthoringRevision::initial()),
        &basis,
        &sources,
    );
    let address = mount_target(PrefabEntityOrigin::SourceEntity(id("root")));
    let component = component();
    let mut label = String::with_capacity(2 * 1024 * 1024);
    label.push('x');
    let mut labels = Vec::with_capacity(8_192);
    labels.push(ComponentValue::String(label));
    let value = ComponentValue::map([("labels", ComponentValue::List(labels))]);
    let cost = value.cost();
    let bytes = address.logical_bytes_up_to(usize::MAX).unwrap()
        + component.as_str().len()
        + cost.logical_bytes();
    let plan = |value, nodes, bytes, depth| {
        observed.plan_component_edit(
            SourceDocumentInput::Scene(&shared),
            &address,
            &component,
            PrefabComponentEdit::Set {
                mode: PrefabComponentSetMode::Add,
                value,
            },
            PrefabDeclarationLimits::new(nodes, bytes, depth).unwrap(),
        )
    };
    for (nodes, bytes, depth) in [
        (cost.nodes() - 1, bytes, 3),
        (cost.nodes(), bytes - 1, 3),
        (cost.nodes(), bytes, 2),
    ] {
        assert!(matches!(
            plan(value.clone(), nodes, bytes, depth),
            Err(EditError::BudgetExceeded)
        ));
    }
    let edit = plan(value, cost.nodes(), bytes, 3).unwrap();
    let PrefabComponentEdit::Set {
        value: ComponentValue::Map(fields),
        ..
    } = edit.operation()
    else {
        panic!("complete component map expected")
    };
    let ComponentValue::List(labels) = &fields["labels"] else {
        panic!("label list expected")
    };
    let [ComponentValue::String(label)] = labels.as_slice() else {
        panic!("one label expected")
    };
    assert_eq!(label, "x");
    assert_eq!(label.capacity(), label.len());
    assert_eq!(labels.capacity(), labels.len());
    assert!(edit.checked_retained_bytes().unwrap() < 4096);
}
