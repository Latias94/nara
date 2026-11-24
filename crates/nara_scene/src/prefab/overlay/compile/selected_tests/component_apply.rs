use nara_identity::ReferenceInputId;
use nara_reflect::__private::component_values_exactly_match;

use super::*;
use crate::{
    __private::{
        CapturedPrefabComponentApplySources, PrefabComponentApplyPreparationError,
        PrefabComponentApplyRequest, PrefabComponentApplySource, PrefabComponentApplySourceMut,
        PreparedPrefabComponentApply, bind_prepared_prefab_component_apply,
        capture_prefab_component_apply_sources, try_prepare_prefab_component_apply_with_loader,
    },
    PrefabAuthoringSession, PrefabDocumentCandidate, PrefabReferenceArguments, SceneFileEncoding,
    ScenePatchDocument, ScenePatchOperation,
};

fn prefab_session(
    name: &str,
    document: &PrefabDocument,
    basis: &AuthoringSchemaBasis,
) -> PrefabAuthoringSession {
    let published = PrefabDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(AuthoringDocumentSource::new(request(name)), basis)
        .unwrap()
        .publish();
    PrefabAuthoringSession::from_published_authoring_document(published)
}

fn containing_document(intent: PrefabComponentIntent) -> SceneDocument {
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(intent);
    SceneDocument::from_ordered_roots([anchor("mount", "base", Some(overlay))])
}

fn set(mode: PrefabComponentSetMode, record: SceneComponentRecord) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode,
        target: PrefabEntityTarget::source(id("base-root")),
        component: component(),
        record,
    }
}

fn suppression() -> PrefabComponentIntent {
    PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(id("base-root")),
        component: component(),
    }
}

fn capture_apply(
    containing: PrefabComponentApplySource<'_>,
    destination: &PrefabAuthoringSession,
    destination_key: &SourceDocumentKey,
) -> CapturedPrefabComponentApplySources {
    let containing_key = match containing {
        PrefabComponentApplySource::Scene(_) => SourceDocumentKey::named(
            "862aa6e4-e568-408a-abd4-69e7d7d5a53a".parse().unwrap(),
            "containing-scene",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
        PrefabComponentApplySource::Prefab(_) => key("containing"),
    };
    capture_prefab_component_apply_sources(
        containing,
        &containing_key,
        destination,
        destination_key,
        |_| Ok(()),
    )
    .unwrap()
}

fn read_apply(
    containing: PrefabComponentApplySource<'_>,
    destination: &PrefabAuthoringSession,
    selected: &PrefabEntityTarget,
    destination_route: &PrefabEntityTarget,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> Result<PreparedPrefabComponentApply, PrefabComponentApplyPreparationError<Infallible>> {
    let inspection = match containing {
        PrefabComponentApplySource::Scene(session) => inspect(
            PrefabInspectionInput::WorkingScene(&session.shared_document(), session.revision()),
            basis,
            sources,
        ),
        PrefabComponentApplySource::Prefab(session) => inspect(
            PrefabInspectionInput::WorkingPrefab(&session.shared_document(), session.revision()),
            basis,
            sources,
        ),
    };
    let (name, _) = sources
        .iter()
        .find(|(_, document)| std::ptr::eq(document.as_ref(), destination.document()))
        .unwrap();
    let captured = capture_apply(containing, destination, &key(name));
    try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            sources: &captured,
            inspection: &inspection,
            selected,
            destination_route,
            component: &component(),
            declaration_limits: Default::default(),
            compilation: Default::default(),
        },
        |asset, _| {
            let (name, document) = sources
                .iter()
                .find(|(name, _)| &request(name) == asset)
                .unwrap();
            Ok(Some(ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(document),
                basis.clone(),
                key(name),
            )))
        },
    )
}

#[test]
fn whole_component_apply_prepares_exact_scene_and_prefab_pairs_for_each_intent() {
    let basis = edit_basis();
    let address = mount_target(PrefabEntityOrigin::SourceEntity(id("base-root")));
    let original_value = offset(4.5);
    let promoted_value = ComponentValue::map([("offset", ComponentValue::f64(-0.0).unwrap())]);
    for (intent, kind, had_component) in [
        (
            set(
                PrefabComponentSetMode::Add,
                owned_record(promoted_value.clone()),
            ),
            AuthoringPrefabComponentOverrideKind::Added,
            false,
        ),
        (
            set(
                PrefabComponentSetMode::Replace,
                owned_record(promoted_value.clone()),
            ),
            AuthoringPrefabComponentOverrideKind::Replaced,
            true,
        ),
        (
            suppression(),
            AuthoringPrefabComponentOverrideKind::Suppressed,
            true,
        ),
    ] {
        let scene = containing_document(intent);
        let scene_session = source_session(&scene, &basis);
        let prefab = PrefabDocument::from_ordered_roots(scene.entities.clone());
        let prefab_session = prefab_session("containing", &prefab, &basis);
        let mut root = SceneEntityRecord::new(id("base-root"));
        if had_component {
            root.components
                .insert(component(), owned_record(original_value.clone()));
        }
        let base = PrefabDocument::from_ordered_roots([root]);
        let destination = self::prefab_session("base", &base, &basis);
        let sources = [("base", destination.shared_document())];
        for containing in [
            PrefabComponentApplySource::Scene(&scene_session),
            PrefabComponentApplySource::Prefab(&prefab_session),
        ] {
            let result = read_apply(
                containing,
                &destination,
                &address,
                &address,
                &basis,
                &sources,
            )
            .unwrap();
            assert_eq!(result.kind(), kind);
            let entities = match result.containing_document() {
                SourceDocumentInput::Scene(document) => &document.entities,
                SourceDocumentInput::Prefab(document) => &document.entities,
            };
            assert!(
                entities[0]
                    .prefab
                    .as_ref()
                    .unwrap()
                    .overrides
                    .as_ref()
                    .is_none_or(|overlay| overlay.components.is_empty())
            );
            assert!(matches!(result.containing_patch().operations.as_slice(), [
                ScenePatchOperation::SetPrefabInstance { entity, .. }
            ] if entity == &id("mount")));
            assert_eq!(result.destination_revision(), destination.revision());
            let stored = result.destination_document().entities[0]
                .components
                .get(&component());
            if kind == AuthoringPrefabComponentOverrideKind::Suppressed {
                assert!(stored.is_none());
            } else {
                let stored = stored.unwrap();
                assert_eq!(stored.owner(), Some(&owner()));
                assert_eq!(stored.version, ComponentSchemaVersion::ONE);
                assert!(component_values_exactly_match(
                    stored.value(),
                    &promoted_value
                ));
                assert!(stored.value().get("labels").is_none());
            }
            match (
                kind,
                result.destination_patch().operations.as_slice(),
                result.destination_inverse().operations.as_slice(),
            ) {
                (
                    AuthoringPrefabComponentOverrideKind::Added,
                    [ScenePatchOperation::AddComponent { .. }],
                    [ScenePatchOperation::RemoveComponent { .. }],
                ) => {}
                (
                    AuthoringPrefabComponentOverrideKind::Replaced,
                    [ScenePatchOperation::ReplaceComponent { .. }],
                    [ScenePatchOperation::ReplaceComponent { value, .. }],
                )
                | (
                    AuthoringPrefabComponentOverrideKind::Suppressed,
                    [ScenePatchOperation::RemoveComponent { .. }],
                    [ScenePatchOperation::AddComponent { value, .. }],
                ) => assert!(component_values_exactly_match(
                    value.value(),
                    &original_value
                )),
                _ => panic!("expected exact forward and inverse for {kind:?}"),
            }
        }
        assert_eq!(scene_session.document(), &scene);
        assert_eq!(prefab_session.document(), &prefab);
        assert_eq!(destination.document(), &base);
        assert_eq!(scene_session.history_usage().records, 0);
        assert_eq!(prefab_session.history_usage().records, 0);
        assert_eq!(destination.history_usage().records, 0);
    }
}

#[test]
fn whole_component_apply_rebases_shadowed_defaults_and_checks_their_input_ownership() {
    let basis = basis();
    let address = mount_target(PrefabEntityOrigin::SourceEntity(id("base-root")));
    let point = ReferenceInputId::new("point").unwrap();
    let outer = ReferenceInputId::new("outer").unwrap();
    let nested_hop = PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("socket-holder")),
        expected_base: key("leaf"),
    };
    let default = authored(AuthoredEntityReference {
        through: [address.through[0].clone(), nested_hop.clone()].into(),
        target: AuthoredReferenceTarget::Input(point.clone()),
    });
    let same_endpoint = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: address.through.clone(),
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("socket"))),
        },
    };
    let mut scene = containing_document(set(
        PrefabComponentSetMode::Add,
        owned_record(ComponentValue::map([("target", persistent())]))
            .with_contextual_default(ComponentFieldId::new("target"), default),
    ));
    scene.entities[0].prefab.as_mut().unwrap().arguments = Some(PrefabReferenceArguments {
        base: key("base"),
        values: [(outer.clone(), same_endpoint)].into(),
    });
    let session = source_session(&scene, &basis);
    let mut leaf = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf-root"))]);
    leaf.inputs.insert(point.clone());
    let leaf = Arc::new(leaf);
    for forwarded in [false, true] {
        let mut holder = anchor("socket-holder", "leaf", None);
        let argument = if forwarded {
            EntityReference::Authored {
                reference: AuthoredEntityReference::input(outer.clone()),
            }
        } else {
            EntityReference::SceneLocal {
                entity: id("socket"),
            }
        };
        holder.prefab.as_mut().unwrap().arguments = Some(PrefabReferenceArguments {
            base: key("leaf"),
            values: [(point.clone(), argument)].into(),
        });
        let mut base = PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("base-root")),
            holder,
            SceneEntityRecord::new(id("socket")),
        ]);
        base.inputs.insert(outer.clone());
        let destination = prefab_session("base", &base, &basis);
        let sources = [
            ("base", destination.shared_document()),
            ("leaf", Arc::clone(&leaf)),
        ];
        let result = read_apply(
            PrefabComponentApplySource::Scene(&session),
            &destination,
            &address,
            &address,
            &basis,
            &sources,
        );
        if forwarded {
            assert!(
                matches!(
                    result,
                    Err(PrefabComponentApplyPreparationError::Compilation(
                        AuthoringPrefabInspectionError::Compilation { .. }
                    ) | PrefabComponentApplyPreparationError::NonPortableReference)
                ),
                "{result:?}"
            );
        } else {
            let result = result.unwrap();
            let value = &result.destination_document().entities[0].components[&component()];
            assert_eq!(value.value().get("target"), Some(&persistent()));
            assert_eq!(
                value.contextual_defaults[&ComponentFieldId::new("target")],
                authored(AuthoredEntityReference {
                    through: vec![nested_hop.clone()],
                    target: AuthoredReferenceTarget::Input(point.clone()),
                })
            );
        }
    }
}

#[test]
fn whole_component_apply_rejects_a_different_root_facet_and_reappearing_lower_record() {
    let basis = edit_basis();
    let address = mount_target(PrefabEntityOrigin::SourceEntity(id("base-root")));
    let mut base = SceneEntityRecord::new(id("base-root"))
        .with_component(component(), owned_record(offset(1.0)));
    base.prefab = Some(PrefabInstance::root(
        request("leaf"),
        key("leaf"),
        PrefabEntityTarget::source(id("leaf-root")),
    ));
    let leaf = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf-root"))
        .with_component(component(), owned_record(offset(3.0)))]);
    let base = prefab_session("base", &PrefabDocument::from_ordered_roots([base]), &basis);
    let leaf = prefab_session("leaf", &leaf, &basis);
    let sources = [
        ("base", base.shared_document()),
        ("leaf", leaf.shared_document()),
    ];
    let session = source_session(&containing_document(suppression()), &basis);
    let leaf_address = PrefabEntityTarget {
        through: [
            address.through[0].clone(),
            PrefabInstanceHop {
                anchor: address.entity.clone(),
                expected_base: key("leaf"),
            },
        ]
        .into(),
        entity: PrefabEntityOrigin::SourceEntity(id("leaf-root")),
    };
    for (destination, route) in [(&leaf, &leaf_address), (&base, &address)] {
        let result = read_apply(
            PrefabComponentApplySource::Scene(&session),
            destination,
            &leaf_address,
            route,
            &basis,
            &sources,
        );
        assert!(
            matches!(&result, Err(PrefabComponentApplyPreparationError::Compilation(
            AuthoringPrefabInspectionError::Compilation { failure, .. }
        )) if failure.reason == SceneCompilationFailureKind::Component),
            "{result:?}"
        );
        assert_eq!(destination.history_usage().records, 0);
    }
    assert_eq!(session.history_usage().records, 0);
}

#[test]
fn whole_component_apply_rejects_a_destination_key_loaded_from_two_allocations() {
    let basis = edit_basis();
    let mut route = PrefabEntityTarget::source(id("base-root"));
    route.through.push(PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("selected")),
        expected_base: key("base"),
    });
    let mut outer = PrefabOverlay::new(key("outer"));
    outer.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: route.clone(),
        component: component(),
        record: owned_record(offset(1.0)),
    });
    route.through.insert(
        0,
        PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("mount")),
            expected_base: key("outer"),
        },
    );
    let scene = SceneDocument::from_ordered_roots([anchor("mount", "outer", Some(outer))]);
    let session = source_session(&scene, &basis);
    let base = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("base-root"))]);
    let destination = prefab_session("base", &base, &basis);
    let mut sibling = PrefabOverlay::new(key("base"));
    sibling
        .components
        .push(set(PrefabComponentSetMode::Add, owned_record(offset(2.0))));
    let outer = PrefabDocument::from_ordered_roots([
        anchor("selected", "base", None),
        anchor("sibling", "base-alias", Some(sibling)),
    ]);
    let alias = request("base-alias");
    let sources = [
        (request("outer"), key("outer"), Arc::new(outer)),
        (request("base"), key("base"), destination.shared_document()),
        (
            alias.clone(),
            key("base"),
            Arc::new(destination.document().clone()),
        ),
    ];
    let load = |asset: &AssetRef, _: &mut crate::SceneCompilationWorkingSet| {
        let (_, key, document) = sources
            .iter()
            .find(|(lookup, _, _)| lookup == asset)
            .unwrap();
        Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
            Arc::clone(document),
            basis.clone(),
            key.clone(),
        )))
    };
    let revision = session.revision();
    let inspection = try_inspect_authoring_prefab_instance_with_loader(
        PrefabInspectionInput::WorkingScene(&session.shared_document(), revision),
        &id("mount"),
        &basis,
        Default::default(),
        load,
    )
    .unwrap();
    let captured = capture_apply(
        PrefabComponentApplySource::Scene(&session),
        &destination,
        &key("base"),
    );
    let mut loaded_alias = false;
    let result = try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            sources: &captured,
            inspection: &inspection,
            selected: &route,
            destination_route: &route,
            component: &component(),
            declaration_limits: Default::default(),
            compilation: Default::default(),
        },
        |asset, working| {
            loaded_alias |= asset == &alias;
            load(asset, working)
        },
    );
    assert!(loaded_alias);
    assert!(
        matches!(
            result,
            Err(PrefabComponentApplyPreparationError::InvalidDestination)
        ),
        "{result:?}"
    );
}

#[test]
fn whole_component_apply_refuses_a_record_over_the_work_budget_before_loading() {
    let basis = edit_basis();
    let value = ComponentValue::map([(
        "labels",
        ComponentValue::List(vec![ComponentValue::String("x".repeat(96 * 1024))]),
    )]);
    let scene = containing_document(set(PrefabComponentSetMode::Add, owned_record(value)));
    let session = source_session(&scene, &basis);
    let base = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("base-root"))]);
    let destination = prefab_session("base", &base, &basis);
    let revision = session.revision();
    let inspection = inspect(
        PrefabInspectionInput::WorkingScene(&session.shared_document(), revision),
        &basis,
        &[("base", destination.shared_document())],
    );
    let route = mount_target(PrefabEntityOrigin::SourceEntity(id("base-root")));
    let captured = capture_apply(
        PrefabComponentApplySource::Scene(&session),
        &destination,
        &key("base"),
    );
    let result = try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            sources: &captured,
            inspection: &inspection,
            selected: &route,
            destination_route: &route,
            component: &component(),
            declaration_limits: Default::default(),
            compilation: SceneCompilationOptions::default().with_limits(
                SceneCompilationLimits::default()
                    .with_working_bytes(ByteLimit::new(64 * 1024).unwrap()),
            ),
        },
        |_, _| -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("the over-budget declaration must be refused before loading")
        },
    );
    assert!(
        matches!(result, Err(PrefabComponentApplyPreparationError::WorkLimit)),
        "{result:?}"
    );
}

#[test]
fn whole_component_apply_keeps_the_loader_limit_without_requiring_an_undo_entry() {
    let basis = edit_basis();
    let scene = containing_document(set(PrefabComponentSetMode::Add, owned_record(offset(2.0))));
    let admission = SceneDocumentCandidate::decode_json_str(&scene.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/one.scene.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let session = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        admission,
        crate::SceneAuthoringHistoryLimits::new(1, 1, 1, 1, 1, 1, 1).unwrap(),
    );
    let base = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("base-root"))]);
    let destination = prefab_session("base", &base, &basis);
    let revision = session.revision();
    let inspection = inspect(
        PrefabInspectionInput::WorkingScene(&session.shared_document(), revision),
        &basis,
        &[("base", destination.shared_document())],
    );
    let route = mount_target(PrefabEntityOrigin::SourceEntity(id("base-root")));
    let captured = capture_apply(
        PrefabComponentApplySource::Scene(&session),
        &destination,
        &key("base"),
    );
    let loader_limit = ByteLimit::new(2 * 1024 * 1024).unwrap();
    let mut calls = 0;
    let prepared = try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            sources: &captured,
            inspection: &inspection,
            selected: &route,
            destination_route: &route,
            component: &component(),
            declaration_limits: Default::default(),
            compilation: Default::default(),
        },
        |_, working| {
            if calls == 0 {
                working.restrict_working_bytes(loader_limit);
            } else {
                assert_eq!(working.working_byte_limit(), loader_limit);
            }
            calls += 1;
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                captured.shared_destination(),
                basis.clone(),
                key("base"),
            )))
        },
    )
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(prepared.containing_revision(), revision);
    assert_eq!(session.history_usage().records, 0);
    assert_ne!(prepared.containing_future_revision(), revision);
    assert_ne!(
        prepared.destination_future_revision(),
        destination.revision()
    );
}

#[test]
fn whole_component_apply_binding_is_reversible_and_installs_the_same_checkpoints() {
    let basis = edit_basis();
    let intent = set(PrefabComponentSetMode::Add, owned_record(offset(4.0)));
    let spare =
        SceneEntityRecord::new(id("spare")).with_component(component(), owned_record(offset(1.0)));
    let scene = SceneDocument::from_ordered_roots(
        containing_document(intent)
            .entities
            .into_iter()
            .chain([spare.clone()]),
    );
    let base = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("base-root")), spare]);
    let mut session = source_session(&scene, &basis);
    let mut destination = prefab_session("base", &base, &basis);
    for value in [2.0, 3.0] {
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: id("spare"),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("offset"),
            value: ComponentValue::f64(value).unwrap(),
        }]);
        assert!(session.apply_authoring_patch(&patch).applied);
        assert!(destination.apply_authoring_patch(&patch).applied);
    }
    assert!(session.undo_authoring().applied);
    assert!(destination.undo_authoring().applied);
    let containing_history = session.history_status();
    let destination_history = destination.history_status();
    assert_eq!(containing_history.undo_depth, 1);
    assert_eq!(containing_history.redo_depth, 1);
    assert_eq!(destination_history, containing_history);
    let containing_content_state = session.content_state();
    let destination_content_state = destination.content_state();
    let captured = capture_apply(
        PrefabComponentApplySource::Scene(&session),
        &destination,
        &key("base"),
    );
    let original_scene = captured.shared_containing_scene().unwrap();
    let original_destination = captured.shared_destination();
    let inspection = inspect(
        PrefabInspectionInput::WorkingScene(&original_scene, captured.containing_revision()),
        &basis,
        &[("base", Arc::clone(&original_destination))],
    );
    let address = mount_target(PrefabEntityOrigin::SourceEntity(id("base-root")));
    let prepared = try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            sources: &captured,
            inspection: &inspection,
            selected: &address,
            destination_route: &address,
            component: &component(),
            declaration_limits: Default::default(),
            compilation: Default::default(),
        },
        |_, _| {
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                captured.shared_destination(),
                captured.schema_basis(),
                captured.destination_key().clone(),
            )))
        },
    )
    .unwrap();
    assert!(captured.owns_prepared(&prepared));
    let identical_capture = capture_apply(
        PrefabComponentApplySource::Scene(&session),
        &destination,
        &key("base"),
    );
    assert!(!identical_capture.owns_prepared(&prepared));
    assert_eq!(
        prepared.containing_file_encoding(),
        Some(SceneFileEncoding::Json)
    );
    assert_eq!(
        prepared.destination_file_encoding(),
        Some(SceneFileEncoding::Ron)
    );
    let future_scene = prepared.shared_containing_scene().unwrap();
    let future_destination = prepared.shared_destination();
    let future_revisions = (
        prepared.containing_future_revision(),
        prepared.destination_future_revision(),
    );
    let future_states = (
        prepared.containing_content_state(),
        prepared.destination_content_state(),
    );

    let mut replacement = prefab_session("base", destination.document(), &basis);
    assert_eq!(destination.document(), replacement.document());
    let failure = bind_prepared_prefab_component_apply(
        PrefabComponentApplySourceMut::Scene(&mut session),
        &mut replacement,
        prepared,
    )
    .unwrap_err();
    assert!(!failure.report().applied);
    let (prepared, _) = failure.into_parts();
    let prepared = bind_prepared_prefab_component_apply(
        PrefabComponentApplySourceMut::Scene(&mut session),
        &mut destination,
        prepared,
    )
    .unwrap()
    .into_prepared();
    assert!(captured.owns_prepared(&prepared));
    assert_eq!(prepared.containing_future_revision(), future_revisions.0);
    assert_eq!(prepared.destination_future_revision(), future_revisions.1);
    assert_eq!(prepared.containing_content_state(), future_states.0);
    assert_eq!(prepared.destination_content_state(), future_states.1);
    assert_eq!(session.revision(), captured.containing_revision());
    assert_eq!(destination.revision(), captured.destination_revision());
    assert_eq!(session.content_state(), containing_content_state);
    assert_eq!(destination.content_state(), destination_content_state);
    assert_eq!(session.history_status(), containing_history);
    assert_eq!(destination.history_status(), destination_history);
    assert!(Arc::ptr_eq(&original_scene, &session.shared_document()));
    assert!(Arc::ptr_eq(
        &original_destination,
        &destination.shared_document()
    ));

    let (containing, destination) = bind_prepared_prefab_component_apply(
        PrefabComponentApplySourceMut::Scene(&mut session),
        &mut destination,
        prepared,
    )
    .unwrap()
    .commit();
    let PrefabComponentApplySourceMut::Scene(session) = containing else {
        panic!("commit must return the installed Scene session")
    };
    assert!(Arc::ptr_eq(&future_scene, &session.shared_document()));
    assert!(Arc::ptr_eq(
        &future_destination,
        &destination.shared_document()
    ));
    assert_eq!(session.revision(), future_revisions.0);
    assert_eq!(destination.revision(), future_revisions.1);
    assert_eq!(session.content_state(), future_states.0);
    assert_eq!(destination.content_state(), future_states.1);
    assert_eq!(session.history_usage().records, 0);
    assert_eq!(destination.history_usage().records, 0);
    assert!(session.is_live_dirty());
    assert!(session.acknowledge_source_saved(future_states.0));
    assert!(destination.acknowledge_source_saved(future_states.1));
}
