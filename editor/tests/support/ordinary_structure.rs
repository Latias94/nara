use super::*;
use nara::tooling::{EditorPersistenceResult, SceneInspectorEntityRow};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn write_documents(project: &CompositionProject, document: SceneDocument) {
    fs::write(
        project.source(EditorDocumentKind::Scene),
        document.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.source(EditorDocumentKind::Prefab),
        PrefabDocument::new(document.roots, document.entities)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
}

fn open_kind(app: &mut EditorApplication, kind: EditorDocumentKind) {
    if kind == EditorDocumentKind::Prefab {
        let deadline = Instant::now() + Duration::from_secs(10);
        while frame(app).project_browser().has_pending_work() && Instant::now() < deadline {
            dispatch(app, None);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            !frame(app).project_browser().has_pending_work(),
            "the Browser index must be ready before document admission"
        );
        dispatch(
            app,
            Some(EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Open {
                    relative_path: "blueprints/ordinary.prefab.json".into(),
                },
            )),
        );
    }
    assert_eq!(
        frame(app).workspace().active_scene.as_ref().unwrap().kind,
        kind
    );
}

fn compose(app: &mut EditorApplication, command: SceneCompositionCommand) {
    let current = frame(app);
    let scene = current.workspace().active_scene.as_ref().unwrap();
    let mutation = match scene.kind {
        EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
            document: scene.document,
            base_revision: scene.revision,
            command,
        },
        EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
            document: scene.document,
            base_revision: scene.revision,
            command: match command {
                SceneCompositionCommand::MoveEntity { entity, to } => {
                    PrefabCompositionCommand::MoveEntity { entity, to }
                }
                SceneCompositionCommand::RemoveEntity { entity } => {
                    PrefabCompositionCommand::RemoveEntity { entity }
                }
                _ => unreachable!("this structural consumer only moves and removes"),
            },
        },
        EditorDocumentKind::Extension(_) => {
            unreachable!("this consumer exercises Scene and Prefab documents")
        }
    };
    dispatch(app, Some(EditorAction::Mutate(mutation)));
}

fn history(app: &mut EditorApplication, redo: bool) {
    let current = frame(app);
    let scene = current.workspace().active_scene.as_ref().unwrap();
    dispatch(
        app,
        Some(EditorAction::Mutate(if redo {
            EditorDocumentMutation::Redo {
                document: scene.document,
                base_revision: scene.revision,
            }
        } else {
            EditorDocumentMutation::Undo {
                document: scene.document,
                base_revision: scene.revision,
            }
        })),
    );
}

fn tree(app: &EditorApplication) -> Vec<SceneInspectorEntityRow> {
    frame(app)
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .entities
        .clone()
}

fn save_and_reopen(app: &mut EditorApplication) {
    let current = frame(app);
    let scene = current.workspace().active_scene.as_ref().unwrap();
    let document = scene.document;
    let revision = scene.revision;
    dispatch(
        app,
        Some(EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: revision,
        })),
    );
    settle_persistence(app);
    assert!(
        matches!(frame(app).persistence().result(), Some(EditorPersistenceResult::Saved {
        document: actual, revision: saved, ..
    }) if actual == document && saved == revision)
    );
    assert!(!frame(app).workspace().document(document).unwrap().dirty);
    dispatch(
        app,
        Some(EditorAction::Persistence(
            EditorPersistenceAction::AcknowledgeResult,
        )),
    );
    dispatch(
        app,
        Some(EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: revision,
            },
        })),
    );
    settle_persistence(app);
    assert!(matches!(
        frame(app).persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
}

#[test]
fn ordinary_scene_and_prefab_move_unknown_records_restore_exact_history_and_reopen() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let project = CompositionProject::new();
        let unavailable = SceneEntityRecord::new(id("unavailable")).with_component(
            ComponentTypeId::new(UNKNOWN),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "opaque",
                        ComponentValue::String(
                            "parent-a is payload, not a remap instruction".into(),
                        ),
                    ),
                    (
                        "reference",
                        ComponentValue::EntityReference(
                            nara::reflect::EntityReference::SceneLocal {
                                entity: id("parent-a"),
                            },
                        ),
                    ),
                ]),
            )
            .with_owner(
                PersistentComponentSchemaOwnerId::try_new("studio.missing.components").unwrap(),
            ),
        );
        write_documents(
            &project,
            SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(id("parent-a")),
                unavailable,
                SceneEntityRecord::new(id("parent-b")),
            ]),
        );
        let mut app = project.open();
        open_kind(&mut app, kind);
        let before = frame(&app);
        let model = before.workspace().active_scene.as_ref().unwrap();
        let document = model.document;
        let original_tree = tree(&app);
        let original_manifest = app.active_authoring_unavailable_record_manifest().unwrap();
        assert_eq!(original_manifest.record_count(), 1);
        let stale = match kind {
            EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
                document,
                base_revision: model.revision,
                command: SceneCompositionCommand::RemoveEntity {
                    entity: id("parent-b"),
                },
            },
            EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
                document,
                base_revision: model.revision,
                command: PrefabCompositionCommand::RemoveEntity {
                    entity: id("parent-b"),
                },
            },
            EditorDocumentKind::Extension(_) => {
                unreachable!("this consumer exercises Scene and Prefab documents")
            }
        };
        compose(
            &mut app,
            SceneCompositionCommand::MoveEntity {
                entity: id("unavailable"),
                to: ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: id("parent-a"),
                }),
            },
        );
        assert_eq!(
            tree(&app)
                .iter()
                .find(|row| row.id == id("unavailable"))
                .unwrap()
                .parent,
            Some(id("parent-a"))
        );
        let moved_tree = tree(&app);
        let moved_manifest = app.active_authoring_unavailable_record_manifest().unwrap();
        assert_ne!(moved_manifest, original_manifest);
        let unchanged = frame(&app).workspace().document(document).unwrap().clone();
        dispatch(&mut app, Some(EditorAction::Mutate(stale)));
        assert_eq!(frame(&app).workspace().document(document), Some(&unchanged));
        history(&mut app, false);
        assert_eq!(tree(&app), original_tree);
        assert_eq!(
            app.active_authoring_unavailable_record_manifest(),
            Some(original_manifest.clone())
        );
        history(&mut app, true);
        assert_eq!(tree(&app), moved_tree);
        assert_eq!(
            app.active_authoring_unavailable_record_manifest(),
            Some(moved_manifest.clone())
        );
        compose(
            &mut app,
            SceneCompositionCommand::MoveEntity {
                entity: id("parent-b"),
                to: ScenePlacement::first(ScenePlacementOwner::Root),
            },
        );
        assert_eq!(tree(&app)[0].id, id("parent-b"));
        let arranged = tree(&app);
        let manifest = app.active_authoring_unavailable_record_manifest().unwrap();
        save_and_reopen(&mut app);
        assert_eq!(tree(&app), arranged);
        assert_eq!(
            app.active_authoring_unavailable_record_manifest(),
            Some(manifest)
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(project.source(kind)).unwrap()).unwrap();
        assert!(
            saved["payload"]["entities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entity| {
                    entity["id"] == "unavailable"
                        && entity["components"][UNKNOWN]["value"]["value"]["opaque"]["value"]
                            == "parent-a is payload, not a remap instruction"
                })
        );
        close(&mut app);
    }
}

#[test]
fn ordinary_complete_scene_and_prefab_delete_subtree_restore_records_and_parent_selection() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let project = CompositionProject::new();
        let component = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", ComponentValue::I64(42))]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap());
        write_documents(
            &project,
            SceneDocument::new(
                [id("parent"), id("loose")],
                [
                    SceneEntityRecord::new(id("parent")).with_children([id("child")]),
                    SceneEntityRecord::new(id("child"))
                        .with_children([id("grandchild")])
                        .with_component(ComponentTypeId::new(CUSTOM), component),
                    SceneEntityRecord::new(id("grandchild")),
                    SceneEntityRecord::new(id("loose")),
                ],
            ),
        );
        let mut app = project.open();
        open_kind(&mut app, kind);
        assert_eq!(
            app.active_authoring_unavailable_record_manifest()
                .unwrap()
                .total_count(),
            0
        );
        let document = frame(&app)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .document;
        dispatch(
            &mut app,
            Some(EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(id("grandchild")),
                },
            )),
        );
        let original = tree(&app);
        compose(
            &mut app,
            SceneCompositionCommand::RemoveEntity {
                entity: id("child"),
            },
        );
        let removed = tree(&app);
        assert_eq!(
            removed.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
            [id("parent"), id("loose")]
        );
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .selection
                .top_entity(),
            Some(&id("parent"))
        );
        history(&mut app, false);
        let restored = tree(&app);
        assert_eq!(
            restored
                .iter()
                .map(|row| (&row.id, &row.parent))
                .collect::<Vec<_>>(),
            original
                .iter()
                .map(|row| (&row.id, &row.parent))
                .collect::<Vec<_>>()
        );
        dispatch(
            &mut app,
            Some(EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(id("child")),
                },
            )),
        );
        assert_eq!(custom_value(&app), Some(ComponentValue::I64(42)));
        history(&mut app, true);
        assert_eq!(
            tree(&app)
                .iter()
                .map(|row| row.id.clone())
                .collect::<Vec<_>>(),
            [id("parent"), id("loose")]
        );
        save_and_reopen(&mut app);
        assert_eq!(tree(&app).len(), 2);
        close(&mut app);
    }
}

#[test]
fn ordinary_delete_rejects_unknown_survivors_and_declared_inbound_references_without_history() {
    for unknown in [true, false] {
        let project = CompositionProject::new();
        let component = if unknown { UNKNOWN } else { CUSTOM };
        if !unknown {
            let schema = ComponentSchema::new(
                ComponentTypeId::new(CUSTOM),
                "Reference",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)]);
            let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
                PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
                ComponentSchemaCatalog {
                    components: vec![schema],
                    ..Default::default()
                },
                None,
            )])
            .unwrap();
            fs::write(
                project.root.join("catalogs/authoring.bundle.json"),
                bundle.to_json_string().unwrap(),
            )
            .unwrap();
        }
        write_documents(
            &project,
            SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(id("target")),
                SceneEntityRecord::new(id("referrer")).with_component(
                    ComponentTypeId::new(component),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([(
                            "target",
                            if unknown {
                                ComponentValue::String("target".into())
                            } else {
                                ComponentValue::EntityReference(
                                    nara::reflect::EntityReference::SceneLocal {
                                        entity: id("target"),
                                    },
                                )
                            },
                        )]),
                    )
                    .with_owner(
                        PersistentComponentSchemaOwnerId::try_new(if unknown {
                            "studio.missing.components"
                        } else {
                            OWNER
                        })
                        .unwrap(),
                    ),
                ),
            ]),
        );
        let mut app = project.open();
        let before = frame(&app);
        let scene = before.workspace().active_scene.as_ref().unwrap();
        let tab = before.workspace().document(scene.document).unwrap().clone();
        let original_tree = tree(&app);
        let manifest = app.active_authoring_unavailable_record_manifest();
        compose(
            &mut app,
            SceneCompositionCommand::RemoveEntity {
                entity: id("target"),
            },
        );
        assert_eq!(frame(&app).workspace().document(scene.document), Some(&tab));
        assert_eq!(tree(&app), original_tree);
        assert_eq!(app.active_authoring_unavailable_record_manifest(), manifest);
        close(&mut app);
    }
}
