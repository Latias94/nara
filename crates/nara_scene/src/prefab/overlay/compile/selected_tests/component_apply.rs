use nara_identity::ReferenceInputId;
use nara_reflect::__private::component_values_exactly_match;

use super::*;
use crate::{
    __private::{
        PrefabComponentApplyPreparationError, PrefabComponentApplyRequest,
        PrefabComponentApplySource, PreparedPrefabComponentApply,
        try_prepare_prefab_component_apply_with_loader,
    },
    PrefabAuthoringSession, PrefabDocumentCandidate, PrefabReferenceArguments, ScenePatchOperation,
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

fn read_apply(
    containing: PrefabComponentApplySource<'_>,
    destination: &PrefabAuthoringSession,
    selected: &PrefabEntityTarget,
    destination_route: &PrefabEntityTarget,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> Result<PreparedPrefabComponentApply, PrefabComponentApplyPreparationError<Infallible>> {
    let (inspection, revision) = match containing {
        PrefabComponentApplySource::Scene(session) => (
            inspect(
                PrefabInspectionInput::WorkingScene(&session.shared_document(), session.revision()),
                basis,
                sources,
            ),
            session.revision(),
        ),
        PrefabComponentApplySource::Prefab(session) => (
            inspect(
                PrefabInspectionInput::WorkingPrefab(
                    &session.shared_document(),
                    session.revision(),
                ),
                basis,
                sources,
            ),
            session.revision(),
        ),
    };
    try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            containing,
            containing_revision: revision,
            inspection: &inspection,
            selected,
            destination_route,
            destination,
            destination_revision: destination.revision(),
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
    let mut loaded_alias = false;
    let result = try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            containing: PrefabComponentApplySource::Scene(&session),
            containing_revision: revision,
            inspection: &inspection,
            selected: &route,
            destination_route: &route,
            destination: &destination,
            destination_revision: destination.revision(),
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
    let result = try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            containing: PrefabComponentApplySource::Scene(&session),
            containing_revision: revision,
            inspection: &inspection,
            selected: &route,
            destination_route: &route,
            destination: &destination,
            destination_revision: destination.revision(),
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
    let loader_limit = ByteLimit::new(2 * 1024 * 1024).unwrap();
    let mut calls = 0;
    let prepared = try_prepare_prefab_component_apply_with_loader(
        PrefabComponentApplyRequest {
            containing: PrefabComponentApplySource::Scene(&session),
            containing_revision: revision,
            inspection: &inspection,
            selected: &route,
            destination_route: &route,
            destination: &destination,
            destination_revision: destination.revision(),
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
                destination.shared_document(),
                basis.clone(),
                key("base"),
            )))
        },
    )
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(prepared.containing_revision(), revision);
    assert_eq!(session.history_usage().records, 0);
}
