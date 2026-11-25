#![cfg(feature = "serde")]

#[path = "support/prefab_source.rs"]
mod prefab_source;

use nara_asset::AssetRef;
use nara_ecs::identity::AuthoredEntityReference;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, EntityReference,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringSchemaBasis, PrefabAuthoringSession, PrefabComponentIntent,
    PrefabComponentSetMode, PrefabDocument, PrefabDocumentCandidate, PrefabEntityTarget,
    PrefabInstance, PrefabOverlay, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityPlacement, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, ScenePatchReport, ScenePlacement,
    ScenePlacementParent, SchemaReclassificationKind, SchemaReclassificationWork,
    authoring::AuthoringAdapterReceipt,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn adapters(bundle: &AuthoringSchemaBundle) -> Vec<AuthoringAdapterReceipt> {
    bundle
        .owners()
        .iter()
        .map(|owner| {
            AuthoringAdapterReceipt::from_schema_owner(owner, &Default::default()).unwrap()
        })
        .collect()
}

fn scene(document: &SceneDocument, bundle: &AuthoringSchemaBundle) -> SceneAuthoringSession {
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("scenes/structure.scene.json").unwrap(),
                ),
                bundle,
                &adapters(bundle),
            )
            .unwrap()
            .publish(),
    )
}

fn prefab(document: &SceneDocument, bundle: &AuthoringSchemaBundle) -> PrefabAuthoringSession {
    let document = PrefabDocument::new(document.placements.clone(), document.entities.clone());
    PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/structure.prefab.json").unwrap(),
                ),
                bundle,
                &adapters(bundle),
            )
            .unwrap()
            .publish(),
    )
}

fn removal(document: &SceneDocument, entity: &str) -> ScenePatchDocument {
    let hierarchy = document.local_hierarchy(Default::default()).unwrap();
    let mut pending = vec![id(entity)];
    let mut removed = Vec::new();
    while let Some(entity) = pending.pop() {
        pending.extend(hierarchy.children(&entity).unwrap().iter().cloned());
        removed.push(entity);
    }
    let mut operations = removed
        .iter()
        .map(|entity| ScenePatchOperation::RemoveEntity {
            entity: entity.clone(),
        })
        .collect::<Vec<_>>();
    operations.push(ScenePatchOperation::SetPlacements {
        expected: document.placements.clone(),
        value: document
            .placements
            .iter()
            .filter(|entry| {
                !removed
                    .iter()
                    .any(|entity| entry.target == PrefabEntityTarget::source(entity.clone()))
            })
            .cloned()
            .collect(),
    });
    ScenePatchDocument::new(operations)
}

#[test]
fn move_preserves_unknown_payloads_and_exact_history_in_both_document_kinds() {
    let missing = ComponentTypeId::new("game.Missing");
    let unknown = SceneEntityRecord::new(id("unknown")).with_component(
        missing.clone(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("opaque".into()),
        ),
    );
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    let mut overlay = PrefabOverlay::new(prefab_source::key("structural.base"));
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: PrefabEntityTarget::source(id("source-entity")),
        component: missing,
        record: SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Bool(true)),
    });
    anchor.prefab = Some(PrefabInstance {
        binding: nara_scene::PrefabInstanceBinding::Forest,
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        arguments: None,
        overrides: Some(overlay),
    });
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("plain")),
        unknown,
        anchor,
        SceneEntityRecord::new(id("parent")),
    ]);
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let mut session = scene(&document, &bundle);
    let original_manifest = session.unavailable_record_manifest().unwrap().clone();
    for entity in ["plain", "unknown", "anchor"] {
        let before = session.document().clone();
        let before_manifest = session.unavailable_record_manifest().unwrap().clone();
        let moved = session.apply_authoring_patch(&ScenePatchDocument::set_placement(
            &before.placements,
            PrefabEntityTarget::source(id(entity)),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id("parent"),
            ))),
        ));
        assert!(moved.applied, "{:?}", moved.diagnostics);
        let after = session.document().clone();
        let after_manifest = session.unavailable_record_manifest().unwrap().clone();
        assert_eq!(
            after_manifest.total_count(),
            original_manifest.total_count()
        );
        for record in &before.entities {
            assert_eq!(
                after
                    .entities
                    .iter()
                    .find(|next| next.id == record.id)
                    .unwrap()
                    .components,
                record.components
            );
            assert_eq!(
                after
                    .entities
                    .iter()
                    .find(|next| next.id == record.id)
                    .unwrap()
                    .prefab,
                record.prefab
            );
        }
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &before);
        assert_eq!(
            session.unavailable_record_manifest(),
            Some(&before_manifest)
        );
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &after);
        assert_eq!(session.unavailable_record_manifest(), Some(&after_manifest));
    }
    let published = PrefabDocumentCandidate::decode_json_bytes(
        PrefabDocument::new(document.placements.clone(), document.entities.clone())
            .to_json_string()
            .unwrap()
            .as_bytes(),
    )
    .unwrap()
    .admit_authoring(
        AuthoringDocumentSource::new(AssetRef::path("prefabs/structure.prefab.json").unwrap()),
        &bundle,
        &[],
    )
    .unwrap()
    .publish();
    let mut prefab = PrefabAuthoringSession::from_published_authoring_document(published);
    let before = prefab.document().clone();
    assert!(
        prefab
            .apply_authoring_patch(&ScenePatchDocument::set_placement(
                &before.placements,
                PrefabEntityTarget::source(id("anchor")),
                ScenePlacement::first(ScenePlacementParent::Root),
            ))
            .applied
    );
    assert!(prefab.undo_authoring().applied);
    assert_eq!(prefab.document(), &before);
}

#[test]
fn deleting_a_complete_subtree_restores_exact_records_without_component_edit_permission() {
    let component = ComponentTypeId::new("game.ReadOnly");
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.data").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component.clone(), "Read only", ComponentSchemaVersion::ONE)
                    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("value"),
                        "Value",
                        ComponentFieldPath::from_fields(["value"]),
                        ComponentValueKind::U64,
                    )
                    .with_capabilities([
                        ComponentCapability::Scene,
                        ComponentCapability::Inspect,
                    ])]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let document = SceneDocument::new(
        [
            SceneEntityPlacement::root(id("root")),
            SceneEntityPlacement::root(id("tail")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("child")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("root"),
                ))),
            ),
        ],
        [
            SceneEntityRecord::new(id("root")),
            SceneEntityRecord::new(id("child")).with_component(
                component,
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("value", ComponentValue::U64(7))]),
                ),
            ),
            SceneEntityRecord::new(id("tail")),
        ],
    );
    let mut session = scene(&document, &bundle);
    let report = session.apply_authoring_patch(&removal(&document, "root"));
    assert!(report.applied, "{:?}", report.diagnostics);
    let inverse = report.inverse.unwrap();
    assert_eq!(session.document().entities.len(), 1);
    assert_eq!(session.history_status().undo_depth, 1);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert!(session.redo_authoring().applied);
    assert_eq!(
        session.document().placements,
        [SceneEntityPlacement::root(id("tail"))]
    );

    let mut ordinary_restore = scene(session.document(), &bundle);
    assert!(!ordinary_restore.apply_authoring_patch(&inverse).applied);
    let adapters =
        [
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap(),
        ];
    let prefab = PrefabDocument::new(document.placements.clone(), document.entities.clone());
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/complete.prefab.json").unwrap(),
                ),
                &bundle,
                &adapters,
            )
            .unwrap()
            .publish();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    assert!(
        session
            .apply_authoring_patch(&removal(&document, "root"))
            .applied
    );
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &prefab);
}

#[test]
fn deletion_and_prefab_attachment_replay_exact_history_in_both_document_kinds() {
    use nara_scene::AuthoringDocumentReadiness::{Complete, Degraded};

    let name = ComponentTypeId::new("game.Name");
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.names").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(name.clone(), "Name", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("value"),
                        "Name",
                        ComponentFieldPath::empty(),
                        ComponentValueKind::String,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let name_record = |value: &str| {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(value.into()),
        )
    };
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("old")).with_component(name.clone(), name_record("Old")),
        SceneEntityRecord::new(id("anchor")),
    ]);

    for (override_component, applied_readiness) in [
        (None, Complete),
        (Some(name), Complete),
        (Some(ComponentTypeId::new("game.Missing")), Degraded),
    ] {
        let overrides = override_component.map(|component| {
            let mut overlay = PrefabOverlay::new(prefab_source::key("structural.base"));
            overlay.components.push(PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Add,
                target: PrefabEntityTarget::source(id("base-root")),
                component,
                record: name_record("Override"),
            });
            overlay
        });
        let instance = PrefabInstance {
            binding: nara_scene::PrefabInstanceBinding::Forest,
            source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
            arguments: None,
            overrides,
        };
        let mut patch = removal(&original, "old");
        patch
            .operations
            .push(ScenePatchOperation::SetPrefabInstance {
                entity: id("anchor"),
                expected: None,
                value: Some(instance.clone()),
            });
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(instance);
        let edited = SceneDocument::from_ordered_roots([anchor]);
        let mut scene = scene(&original, &bundle);
        let mut prefab = prefab(&original, &bundle);

        for (generation, document, readiness, depths) in [
            (1, &edited, applied_readiness, (1, 0)),
            (2, &original, Complete, (0, 1)),
            (3, &edited, applied_readiness, (1, 0)),
        ] {
            let (scene_report, prefab_report) = match generation {
                1 => (
                    scene.apply_authoring_patch(&patch),
                    prefab.apply_authoring_patch(&patch),
                ),
                2 => (scene.undo_authoring(), prefab.undo_authoring()),
                _ => (scene.redo_authoring(), prefab.redo_authoring()),
            };
            for (kind, report) in [("Scene", &scene_report), ("Prefab", &prefab_report)] {
                assert!(
                    report.applied,
                    "{kind} revision {generation} with {applied_readiness:?} attachment failed: {:?}",
                    report.diagnostics
                );
            }
            if generation == 1 {
                for report in [
                    scene.apply_authoring_patch(scene_report.inverse.as_ref().unwrap()),
                    prefab.apply_authoring_patch(prefab_report.inverse.as_ref().unwrap()),
                ] {
                    assert!(!report.applied, "Public Apply accepted a history inverse");
                    assert!(report.inverse.is_none());
                }
            }
            assert_eq!(scene.document(), document);
            assert_eq!(&prefab.document().instantiate(), document);
            for (revision, actual_readiness, history) in [
                (
                    scene.revision(),
                    scene.authoring_readiness(),
                    scene.history_status(),
                ),
                (
                    prefab.revision(),
                    prefab.authoring_readiness(),
                    prefab.history_status(),
                ),
            ] {
                assert_eq!(revision.generation(), generation);
                assert_eq!(actual_readiness, Some(readiness));
                assert_eq!((history.undo_depth, history.redo_depth), depths);
            }
        }
    }
}

fn direct_target_references() -> [EntityReference; 2] {
    [
        EntityReference::SceneLocal {
            entity: id("target"),
        },
        EntityReference::Authored {
            reference: AuthoredEntityReference::source(id("target")),
        },
    ]
}

fn link_fixture(
    reference: &EntityReference,
    contextual: bool,
) -> (AuthoringSchemaBundle, SceneDocument, ComponentTypeId) {
    let component = ComponentTypeId::new("game.Link");
    let field_id = ComponentFieldId::new("target");
    let path = ComponentFieldPath::from_fields(["target"]);
    let field = if contextual {
        ComponentFieldSchema::optional_with_default(
            field_id.clone(),
            "Target",
            path,
            ComponentValueKind::EntityRef,
            ComponentValue::Null,
        )
    } else {
        ComponentFieldSchema::required(
            field_id.clone(),
            "Target",
            path,
            ComponentValueKind::EntityRef,
        )
    }
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.link").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component.clone(), "Link", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([field]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let value = ComponentValue::EntityReference(reference.clone());
    let record = if contextual {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::Map(Default::default()),
        )
        .with_contextual_default(field_id, value)
    } else {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("target", value)]),
        )
    };
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("source")).with_component(component.clone(), record),
        SceneEntityRecord::new(id("target")),
    ]);
    (bundle, document, component)
}

fn assert_local_reference_refusal(report: ScenePatchReport) {
    assert!(!report.applied, "{report:?}");
    assert!(report.inverse.is_none());
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-local-reference-invalid"
        }),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn deletion_rejects_both_direct_reference_encodings_without_changing_either_draft() {
    for reference in direct_target_references() {
        for contextual in [false, true] {
            let (bundle, document, component) = link_fixture(&reference, contextual);
            let mut scene = scene(&document, &bundle);
            let mut prefab = prefab(&document, &bundle);
            let original_prefab = prefab.document().clone();
            let remove_target = removal(&document, "target");
            let mut coordinated = remove_target.clone();
            let replacement = ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id("source"),
            });
            coordinated.operations.insert(
                0,
                ScenePatchOperation::SetField {
                    entity: id("source"),
                    component: component.clone(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("target"),
                    value: replacement.clone(),
                },
            );
            for report in [
                scene.apply_authoring_patch(&coordinated),
                prefab.apply_authoring_patch(&coordinated),
            ] {
                assert!(report.applied, "{:?}", report.diagnostics);
            }
            let coordinated_scene = scene.document().clone();
            let coordinated_prefab = prefab.document().clone();
            assert_eq!(coordinated_prefab.instantiate(), coordinated_scene);
            let [survivor] = coordinated_scene.entities.as_slice() else {
                panic!("only the retargeted source should survive");
            };
            assert_eq!(survivor.id, id("source"));
            assert_eq!(
                survivor.components[&component].value().get("target"),
                Some(&replacement)
            );
            assert!(scene.undo_authoring().applied);
            assert!(prefab.undo_authoring().applied);
            let scene_revision = scene.revision();
            let prefab_revision = prefab.revision();
            let scene_content = scene.content_state();
            let prefab_content = prefab.content_state();
            let scene_history = (scene.history_status(), scene.history_usage());
            let prefab_history = (prefab.history_status(), prefab.history_usage());
            assert_eq!(scene_history.0.redo_depth, 1);
            assert_eq!(prefab_history.0.redo_depth, 1);

            assert_local_reference_refusal(scene.apply_authoring_patch(&remove_target));
            assert_local_reference_refusal(prefab.apply_authoring_patch(&remove_target));
            assert_eq!(scene.document(), &document);
            assert_eq!(prefab.document(), &original_prefab);
            assert_eq!(scene.revision(), scene_revision);
            assert_eq!(prefab.revision(), prefab_revision);
            assert_eq!(scene.content_state(), scene_content);
            assert_eq!(prefab.content_state(), prefab_content);
            assert_eq!(
                (scene.history_status(), scene.history_usage()),
                scene_history
            );
            assert_eq!(
                (prefab.history_status(), prefab.history_usage()),
                prefab_history
            );

            assert!(scene.redo_authoring().applied);
            assert!(prefab.redo_authoring().applied);
            assert_eq!(scene.document(), &coordinated_scene);
            assert_eq!(prefab.document(), &coordinated_prefab);
        }
    }
}

#[test]
fn reclassification_rejects_both_dangling_direct_reference_encodings() {
    let empty = AuthoringSchemaBundle::new([]).unwrap();
    for reference in direct_target_references() {
        for contextual in [false, true] {
            let (bundle, mut document, _) = link_fixture(&reference, contextual);
            document.entities.retain(|entity| entity.id != id("target"));
            document
                .placements
                .retain(|placement| placement.target != PrefabEntityTarget::source(id("target")));
            let successor = AuthoringSchemaBasis::admit(&bundle, &adapters(&bundle)).unwrap();
            let scene = scene(&document, &empty);
            let prefab = prefab(&document, &empty);
            let kind = SchemaReclassificationKind::DeclarationExtension;
            assert_local_reference_refusal(
                scene
                    .prepare_schema_reclassification(
                        &successor,
                        &kind,
                        &mut SchemaReclassificationWork::default(),
                    )
                    .unwrap_err(),
            );
            assert_local_reference_refusal(
                prefab
                    .prepare_schema_reclassification(
                        &successor,
                        &kind,
                        &mut SchemaReclassificationWork::default(),
                    )
                    .unwrap_err(),
            );
        }
    }
}

#[test]
fn deletion_accepts_a_subtree_containing_both_ends_of_a_default_reference() {
    let (bundle, mut subtree, _) = link_fixture(&direct_target_references()[0], true);
    subtree.placements = vec![
        SceneEntityPlacement::root(id("root")),
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(id("source")),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id("root"),
            ))),
        ),
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(id("target")),
            ScenePlacement::after(
                ScenePlacementParent::Entity(PrefabEntityTarget::source(id("root"))),
                PrefabEntityTarget::source(id("source")),
            ),
        ),
    ];
    subtree.entities.push(SceneEntityRecord::new(id("root")));
    subtree.canonicalize();
    let mut session = scene(&subtree, &bundle);
    assert!(
        session
            .apply_authoring_patch(&removal(&subtree, "root"))
            .applied
    );
    assert!(session.document().entities.is_empty());
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &subtree);
}

#[test]
fn deletion_does_not_guess_unknown_references_or_prefab_namespaces() {
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    for prefab in [false, true] {
        let mut blocker = SceneEntityRecord::new(id("blocker"));
        if prefab {
            blocker.prefab = Some(PrefabInstance {
                binding: nara_scene::PrefabInstanceBinding::Forest,
                source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
                arguments: None,
                overrides: None,
            });
        } else {
            blocker.components.insert(
                ComponentTypeId::new("game.Unknown"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("target".into()),
                ),
            );
        }
        let document =
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("target")), blocker]);
        let mut session = scene(&document, &bundle);
        let revision = session.revision();
        assert!(
            !session
                .apply_authoring_patch(&removal(&document, "target"))
                .applied
        );
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn move_rejects_cycles_and_stale_placement_without_history_or_payload_changes() {
    let document = SceneDocument::new(
        [
            SceneEntityPlacement::root(id("root")),
            SceneEntityPlacement::root(id("tail")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("child")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("root"),
                ))),
            ),
        ],
        [
            SceneEntityRecord::new(id("root")),
            SceneEntityRecord::new(id("child")),
            SceneEntityRecord::new(id("tail")),
        ],
    );
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let mut session = scene(&document, &bundle);
    let revision = session.revision();
    let history = session.history_status();
    let mut stale = document.placements.clone();
    stale[1].placement = ScenePlacement::first(ScenePlacementParent::Root);
    for (entity, expected, to) in [
        (
            id("root"),
            document.placements.clone(),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id("child"),
            ))),
        ),
        (
            id("tail"),
            stale,
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id("root"),
            ))),
        ),
    ] {
        assert!(
            !session
                .apply_authoring_patch(&ScenePatchDocument::set_placement(
                    &expected,
                    PrefabEntityTarget::source(entity),
                    to,
                ))
                .applied
        );
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
    }
}
