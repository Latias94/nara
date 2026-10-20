#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, EntityReference,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, PrefabAuthoringSession,
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn scene(document: &SceneDocument, bundle: &AuthoringSchemaBundle) -> SceneAuthoringSession {
    let adapters = bundle
        .owners()
        .iter()
        .map(|owner| {
            AuthoringAdapterReceipt::from_schema_owner(owner, &Default::default()).unwrap()
        })
        .collect::<Vec<_>>();
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("scenes/structure.scene.json").unwrap(),
                ),
                bundle,
                &adapters,
            )
            .unwrap()
            .publish(),
    )
}

fn removal(document: &SceneDocument, entity: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::RemoveEntity {
        entity: id(entity),
        from: ScenePlacement::current(document, &id(entity)).unwrap(),
    }])
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
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        overrides: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
            entity: id("source-entity"),
            component: missing,
            value: SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::Bool(true),
            ),
        }]),
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
        let moved = session.apply_authoring_patch(&ScenePatchDocument::new([
            ScenePatchOperation::MoveEntity {
                entity: id(entity),
                from: ScenePlacement::current(&before, &id(entity)).unwrap(),
                to: ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: id("parent"),
                }),
            },
        ]));
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
        PrefabDocument::new(document.roots.clone(), document.entities.clone())
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
            .apply_authoring_patch(&ScenePatchDocument::new([
                ScenePatchOperation::MoveEntity {
                    entity: id("anchor"),
                    from: ScenePlacement::current_prefab(&before, &id("anchor")).unwrap(),
                    to: ScenePlacement::first(ScenePlacementOwner::Root),
                }
            ]))
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
        [id("root"), id("tail")],
        [
            SceneEntityRecord::new(id("root")).with_children([id("child")]),
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
    assert_eq!(session.document().roots, [id("tail")]);

    let mut ordinary_restore = scene(session.document(), &bundle);
    assert!(!ordinary_restore.apply_authoring_patch(&inverse).applied);
    let adapters =
        [
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap(),
        ];
    let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
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
fn deletion_rejects_effective_default_inbound_references_without_mutation() {
    let component = ComponentTypeId::new("game.Link");
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueKind::EntityRef,
        ComponentValue::EntityReference(EntityReference::SceneLocal {
            entity: id("target"),
        }),
    )
    .with_capabilities([
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::EntityRef,
    ]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.link").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component.clone(), "Link", ComponentSchemaVersion::ONE)
                    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
                    .with_fields([field]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("source")).with_component(
            component,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ),
        ),
        SceneEntityRecord::new(id("target")),
    ]);
    let mut session = scene(&document, &bundle);
    let revision = session.revision();
    let history = session.history_status();
    assert!(
        !session
            .apply_authoring_patch(&removal(&document, "target"))
            .applied
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
    assert!(
        session
            .apply_authoring_patch(&removal(&document, "source"))
            .applied
    );

    let mut subtree = document.clone();
    subtree.roots = vec![id("root")];
    subtree
        .entities
        .push(SceneEntityRecord::new(id("root")).with_children([id("source"), id("target")]));
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
                source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
                overrides: ScenePatchDocument::default(),
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
        [id("root"), id("tail")],
        [
            SceneEntityRecord::new(id("root")).with_children([id("child")]),
            SceneEntityRecord::new(id("child")),
            SceneEntityRecord::new(id("tail")),
        ],
    );
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let mut session = scene(&document, &bundle);
    let revision = session.revision();
    let history = session.history_status();
    for (entity, from, to) in [
        (
            id("root"),
            ScenePlacement::current(&document, &id("root")).unwrap(),
            ScenePlacement::last(ScenePlacementOwner::Parent {
                parent: id("child"),
            }),
        ),
        (
            id("tail"),
            ScenePlacement::first(ScenePlacementOwner::Root),
            ScenePlacement::last(ScenePlacementOwner::Parent { parent: id("root") }),
        ),
    ] {
        assert!(
            !session
                .apply_authoring_patch(&ScenePatchDocument::new([
                    ScenePatchOperation::MoveEntity { entity, from, to }
                ]))
                .applied
        );
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
    }
}
