use nara_reflect::ComponentPropertyReadBudget;

use super::*;
use crate::{
    __private::{
        AuthoringPrefabComponentEditError as EditError, AuthoringPrefabInspectionError,
        try_inspect_authoring_prefab_instance_with_loader,
    },
    AuthoringDocumentSource, AuthoringPrefabComponentOverrideKind, AuthoringPrefabInstanceSnapshot,
    AuthoringPrefabPropertyValue, AuthoringPrefabRecordOrigin, PrefabComponentEdit,
    PrefabComponentSetMode, PrefabDeclarationLimits, PrefabDeclarationSource, PrefabOverlaySlot,
    SceneAuthoringSession, SceneDocumentCandidate,
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
    try_inspect(input, basis, sources).unwrap()
}

fn try_inspect(
    input: PrefabInspectionInput<'_>,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> Result<AuthoringPrefabInstanceSnapshot, AuthoringPrefabInspectionError<Infallible>> {
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
                let target = observed.target(address).unwrap();
                assert_eq!(target.component(&component()).is_err(), hidden);
                let declaration = target.component_override(&component());
                assert_eq!(declaration.is_some(), has_declaration);
                if let Some(declaration) = declaration {
                    let origin = declaration.underlying_record().unwrap();
                    if addition_owns_component {
                        assert!(matches!(
                            origin,
                            AuthoringPrefabRecordOrigin::Overlay {
                                location,
                                slot: PrefabOverlaySlot::Addition { index: 0 },
                            } if location.source() == &PrefabDeclarationSource::Containing
                                && location.path().anchor == id("mount")
                                && location.path().steps.is_empty()
                        ));
                    } else {
                        assert_eq!(
                            origin,
                            &AuthoringPrefabRecordOrigin::SourceEntity {
                                source: PrefabDeclarationSource::Prefab(key("leaf")),
                                entity: id("leaf-root"),
                            }
                        );
                    }
                }
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
fn component_apply_suppression_can_reveal_another_record_under_a_bound_root() {
    let basis = edit_basis();
    let component = component();
    let mut base_root = SceneEntityRecord::new(id("base-root"))
        .with_component(component.clone(), owned_record(offset(1.0)));
    base_root.prefab = Some(PrefabInstance::root(
        request("leaf"),
        key("leaf"),
        PrefabEntityTarget::source(id("leaf-root")),
    ));
    let leaf = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf-root"))
        .with_component(component.clone(), owned_record(offset(3.0)))]);
    let original_sources = [
        (
            "base",
            Arc::new(PrefabDocument::from_ordered_roots([base_root.clone()])),
        ),
        ("leaf", Arc::new(leaf)),
    ];
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(id("base-root")),
        component: component.clone(),
    });
    let containing = SceneDocument::from_ordered_roots([anchor("mount", "base", Some(overlay))]);
    let session = source_session(&containing, &basis);
    let working = session.shared_document();
    let original = inspect(
        PrefabInspectionInput::WorkingScene(&working, session.revision()),
        &basis,
        &original_sources,
    );
    let address = mount_target(PrefabEntityOrigin::SourceEntity(id("base-root")));
    let leaf_address = PrefabEntityTarget {
        through: address
            .through
            .iter()
            .cloned()
            .chain([PrefabInstanceHop {
                anchor: address.entity.clone(),
                expected_base: key("leaf"),
            }])
            .collect(),
        entity: PrefabEntityOrigin::SourceEntity(id("leaf-root")),
    };
    let expected_record = AuthoringPrefabRecordOrigin::SourceEntity {
        source: PrefabDeclarationSource::Prefab(key("base")),
        entity: id("base-root"),
    };
    for address in [&address, &leaf_address] {
        let target = original.target(address).unwrap();
        assert!(target.component(&component).is_err());
        let suppressed = target.component_override(&component).unwrap();
        assert_eq!(
            suppressed.kind(),
            AuthoringPrefabComponentOverrideKind::Suppressed
        );
        assert_eq!(suppressed.underlying_record(), Some(&expected_record));
    }
    let revert = original
        .plan_component_edit(
            SourceDocumentInput::Scene(&working),
            &leaf_address,
            &component,
            PrefabComponentEdit::Revert,
            Default::default(),
        )
        .unwrap();
    let cleared = session
        .prepare_prefab_component_edit(session.revision(), &revert, Default::default())
        .unwrap();
    base_root.components.remove(&component).unwrap();
    let promoted_sources = [
        (
            "base",
            Arc::new(PrefabDocument::from_ordered_roots([base_root])),
        ),
        ("leaf", Arc::clone(&original_sources[1].1)),
    ];
    let candidate = inspect(
        PrefabInspectionInput::Borrowed(SourceDocumentInput::Scene(cleared.document())),
        &basis,
        &promoted_sources,
    );
    let target = candidate.target(&leaf_address).unwrap();
    assert!(target.component_override(&component).is_none());
    assert_eq!(
        target.component(&component).unwrap().fact().record(),
        &AuthoringPrefabRecordOrigin::SourceEntity {
            source: PrefabDeclarationSource::Prefab(key("leaf")),
            entity: id("leaf-root"),
        }
    );
}

#[test]
fn component_apply_mixed_generations_expose_presence_and_value_conflicts() {
    let basis = edit_basis();
    let address = mount_target(PrefabEntityOrigin::SourceEntity(id("base-root")));
    let component = component();
    let document_with_component = |stored: Option<f64>| {
        let mut root = SceneEntityRecord::new(id("base-root"));
        if let Some(value) = stored {
            root.components
                .insert(component.clone(), owned_record(offset(value)));
        }
        [("base", Arc::new(PrefabDocument::from_ordered_roots([root])))]
    };
    let set = |mode| PrefabComponentIntent::Set {
        mode,
        target: PrefabEntityTarget::source(id("base-root")),
        component: component.clone(),
        record: owned_record(offset(2.0)),
    };
    let inspect_source = |scene: &SceneDocument, sources: &[(&str, Arc<PrefabDocument>)]| {
        try_inspect(
            PrefabInspectionInput::Borrowed(SourceDocumentInput::Scene(scene)),
            &basis,
            sources,
        )
    };
    let value_at = |snapshot: &AuthoringPrefabInstanceSnapshot| {
        let target = snapshot.target(&address).unwrap();
        let Ok(record) = target.component(&component) else {
            return None;
        };
        let observed = record
            .property_value(
                &ComponentPropertyPath::new(ComponentFieldId::new("offset")),
                &mut ComponentPropertyReadBudget::new(
                    ItemLimit::new(8).unwrap(),
                    ByteLimit::new(512).unwrap(),
                ),
            )
            .unwrap();
        let AuthoringPrefabPropertyValue::Present(value) = observed.value else {
            panic!("the complete record must expose its offset");
        };
        Some(value.copy_value(Default::default()).unwrap())
    };
    for (kind, intent, original, promoted) in [
        (
            AuthoringPrefabComponentOverrideKind::Added,
            set(PrefabComponentSetMode::Add),
            None,
            Some(2.0),
        ),
        (
            AuthoringPrefabComponentOverrideKind::Replaced,
            set(PrefabComponentSetMode::Replace),
            Some(1.0),
            Some(2.0),
        ),
        (
            AuthoringPrefabComponentOverrideKind::Suppressed,
            PrefabComponentIntent::Suppress {
                target: PrefabEntityTarget::source(id("base-root")),
                component: component.clone(),
            },
            Some(1.0),
            None,
        ),
    ] {
        let mut overlay = PrefabOverlay::new(key("base"));
        overlay.components.push(intent);
        let containing =
            SceneDocument::from_ordered_roots([anchor("mount", "base", Some(overlay))]);
        let session = source_session(&containing, &basis);
        let original_sources = document_with_component(original);
        let promoted_sources = document_with_component(promoted);
        let working = session.shared_document();
        let inspected = inspect(
            PrefabInspectionInput::WorkingScene(&working, session.revision()),
            &basis,
            &original_sources,
        );
        assert_eq!(
            inspected
                .target(&address)
                .unwrap()
                .component_override(&component)
                .unwrap()
                .kind(),
            kind
        );
        assert_eq!(
            value_at(&inspected),
            promoted.map(|value| ComponentValue::f64(value).unwrap())
        );
        let revert = inspected
            .plan_component_edit(
                SourceDocumentInput::Scene(&working),
                &address,
                &component,
                PrefabComponentEdit::Revert,
                Default::default(),
            )
            .unwrap();
        let cleared = session
            .prepare_prefab_component_edit(session.revision(), &revert, Default::default())
            .unwrap();

        let destination_only = inspect_source(&containing, &promoted_sources);
        if kind == AuthoringPrefabComponentOverrideKind::Replaced {
            assert_eq!(value_at(&destination_only.unwrap()), value_at(&inspected));
        } else {
            assert!(matches!(
                destination_only,
                Err(AuthoringPrefabInspectionError::Compilation { failure, .. })
                    if failure.reason == SceneCompilationFailureKind::Component
            ));
        }
        let containing_only = inspect_source(cleared.document(), &original_sources).unwrap();
        assert_ne!(value_at(&containing_only), value_at(&inspected));
        let complete = inspect_source(cleared.document(), &promoted_sources).unwrap();
        assert_eq!(value_at(&complete), value_at(&inspected));
        let target = complete.target(&address).unwrap();
        assert!(target.component_override(&component).is_none());
        if let Some(value) = promoted {
            assert_eq!(
                target.component(&component).unwrap().fact().record(),
                &AuthoringPrefabRecordOrigin::SourceEntity {
                    source: PrefabDeclarationSource::Prefab(key("base")),
                    entity: id("base-root"),
                },
                "promoted value {value}"
            );
        } else {
            assert!(target.component(&component).is_err());
        }
        assert_eq!(session.document(), &containing);
        assert_eq!(session.history_usage().records, 0);
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

#[test]
fn whole_component_edit_admits_the_shadowed_default_in_the_complete_patch_budget() {
    use crate::{PrefabDeclarationError, ScenePatchOperation, prefab::PrefabOverlayCostLimits};

    let basis = basis();
    let sources = [(
        "base",
        Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("root")),
        ])),
    )];
    let original = SceneDocument::from_ordered_roots([
        anchor("mount", "base", None),
        SceneEntityRecord::new(id("socket")),
        SceneEntityRecord::new(id("other")),
    ]);
    let mut session = source_session(&original, &basis);
    let revision = session.revision();
    let history = session.history_status();
    let shared = session.shared_document();
    let observed = inspect(
        PrefabInspectionInput::WorkingScene(&shared, revision),
        &basis,
        &sources,
    );
    let value = ComponentValue::map([("target", local("other"))]);
    let plan = observed
        .plan_component_edit(
            SourceDocumentInput::Scene(&shared),
            &mount_target(PrefabEntityOrigin::SourceEntity(id("root"))),
            &component(),
            PrefabComponentEdit::Set {
                mode: PrefabComponentSetMode::Add,
                value: value.clone(),
            },
            Default::default(),
        )
        .unwrap();

    let original_instance = original.entities[0].prefab.as_ref().unwrap();
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: PrefabEntityTarget::source(id("root")),
        component: component(),
        record: record(value),
    });
    let mut next_instance = original_instance.clone();
    next_instance.overrides = Some(overlay);
    let raw_cost = |instance| {
        crate::prefab::prefab_instance_cost(instance, PrefabOverlayCostLimits::UNBOUNDED).unwrap()
    };
    let previous = raw_cost(original_instance);
    let next = raw_cost(&next_instance);
    let nodes = 1 + previous.nodes + next.nodes;
    let bytes = "mount".len() + previous.logical_bytes() + next.logical_bytes();
    for (nodes, bytes, depth) in [
        (nodes - 1, bytes, next.depth),
        (nodes, bytes - 1, next.depth),
        (nodes, bytes, next.depth - 1),
    ] {
        let report = session
            .prepare_prefab_component_edit(
                revision,
                &plan,
                PrefabDeclarationLimits::new(nodes, bytes, depth).unwrap(),
            )
            .unwrap_err();
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == PrefabDeclarationError::BudgetExceeded.diagnostic_code()
        }));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
        assert_eq!(session.document(), &original);
    }

    let prepared = session
        .prepare_prefab_component_edit(
            revision,
            &plan,
            PrefabDeclarationLimits::new(nodes, bytes, next.depth).unwrap(),
        )
        .unwrap();
    let forward = ScenePatchOperation::SetPrefabInstance {
        entity: id("mount"),
        expected: Some(original_instance.clone()),
        value: Some(next_instance.clone()),
    };
    let inverse = ScenePatchOperation::SetPrefabInstance {
        entity: id("mount"),
        expected: Some(next_instance.clone()),
        value: Some(original_instance.clone()),
    };
    assert_eq!(prepared.patch().operations.as_slice(), &[forward]);
    assert_eq!(prepared.inverse().operations.as_slice(), &[inverse]);
    let source = SourceDocumentInput::Scene(prepared.document());
    assert!(
        inspect(PrefabInspectionInput::Borrowed(source), &basis, &sources)
            .verifies_component_edit(source, &plan)
    );
    let mut different_default = prepared.document().clone();
    let intents = &mut different_default.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
        .components;
    let [PrefabComponentIntent::Set { record, .. }] = intents.as_mut_slice() else {
        panic!("complete Set expected")
    };
    record
        .contextual_defaults
        .insert(ComponentFieldId::new("target"), local("other"));
    let source = SourceDocumentInput::Scene(&different_default);
    assert!(
        !inspect(PrefabInspectionInput::Borrowed(source), &basis, &sources)
            .verifies_component_edit(source, &plan)
    );
    let mut expected = original.clone();
    expected.entities[0].prefab = Some(next_instance);
    assert!(session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document(), &expected);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &expected);
}
