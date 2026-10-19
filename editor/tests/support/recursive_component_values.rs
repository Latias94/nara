use super::*;
use nara::reflect::{ComponentNumericDomain, ComponentValueShape};

fn values(x: f64, optional: ComponentValue) -> ComponentValue {
    ComponentValue::map([
        ("optional", optional),
        (
            "vector",
            ComponentValue::map([
                ("x", ComponentValue::f64(x).unwrap()),
                ("y", ComponentValue::f64(2.0).unwrap()),
            ]),
        ),
        ("list", ComponentValue::List(vec![ComponentValue::U64(3)])),
        (
            "map",
            ComponentValue::map([("first", ComponentValue::String("before".into()))]),
        ),
    ])
}

fn prepare(project: &CompositionProject) {
    let number = ComponentValueShape::scalar(ComponentValueKind::F64)
        .with_numeric_domain(ComponentNumericDomain::F32);
    let shape = ComponentValueShape::record([
        (
            "optional".into(),
            ComponentValueShape::optional(number.clone()),
        ),
        (
            "vector".into(),
            ComponentValueShape::record([("x".into(), number.clone()), ("y".into(), number)]),
        ),
        (
            "list".into(),
            ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::U64)),
        ),
        (
            "map".into(),
            ComponentValueShape::map(ComponentValueShape::scalar(ComponentValueKind::String)),
        ),
    ]);
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM),
        "Recursive",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            shape,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("reference"),
            "Reference",
            ComponentFieldPath::from_fields(["reference"]),
            ComponentValueShape::EntityRef,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef),
    ]);
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
    let component = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            ("value", values(1.0, ComponentValue::Null)),
            (
                "reference",
                ComponentValue::EntityReference(nara::reflect::EntityReference::SceneLocal {
                    entity: SceneEntityId::new("unavailable").unwrap(),
                }),
            ),
        ]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap());
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(project.source(kind)).unwrap()).unwrap();
        let entity = SceneEntityRecord::new(SceneEntityId::new("editable").unwrap())
            .with_component(ComponentTypeId::new(CUSTOM), component.clone());
        // Preserve the fixture's unavailable record and its authored root placement.
        document["payload"]["entities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(entity).unwrap());
        document["payload"]["roots"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::String("editable".into()));
        fs::write(project.source(kind), serde_json::to_vec(&document).unwrap()).unwrap();
    }
}

fn write(
    app: &mut EditorApplication,
    field: &str,
    value: ComponentValue,
    base: Option<nara::scene::SceneAuthoringRevision>,
) {
    let current = frame(app);
    let scene = current.workspace().active_scene.as_ref().unwrap();
    let action = EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
        document: scene.document,
        base_revision: base.unwrap_or(scene.revision),
        command: SceneInspectorCommand::SetField {
            entity: SceneEntityId::new("editable").unwrap(),
            component: ComponentTypeId::new(CUSTOM),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new(field),
            value,
        },
    });
    dispatch(app, Some(action));
}

#[test]
fn recursive_root_transactions_keep_shape_revision_and_reference_authority_in_document_owner() {
    let project = CompositionProject::new();
    prepare(&project);
    let mut app = project.open();
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        if kind == EditorDocumentKind::Prefab {
            for _ in 0..128 {
                if !frame(&app).project_browser().has_pending_work() {
                    break;
                }
                dispatch(&mut app, None);
            }
            dispatch(
                &mut app,
                Some(EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Open {
                        relative_path: "blueprints/ordinary.prefab.json".into(),
                    },
                )),
            );
        }
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
                    entity: Some(SceneEntityId::new("editable").unwrap()),
                },
            )),
        );
        let original = values(1.0, ComponentValue::Null);
        assert_eq!(custom_value(&app), Some(original.clone()));
        let current = frame(&app);
        let scene = current.workspace().active_scene.as_ref().unwrap();
        let original_revision = scene.revision;
        let original_history = scene.editor.inspector.history;
        let manifest = app.active_authoring_unavailable_record_manifest().unwrap();
        write(&mut app, "value", values(1e100, ComponentValue::Null), None);
        write(
            &mut app,
            "reference",
            ComponentValue::EntityReference(nara::reflect::EntityReference::SceneLocal {
                entity: SceneEntityId::new("editable").unwrap(),
            }),
            None,
        );
        assert_eq!(custom_value(&app), Some(original.clone()));
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .revision,
            original_revision
        );
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .editor
                .inspector
                .history,
            original_history
        );
        let edited = values(9.0, ComponentValue::f64(7.0).unwrap());
        write(&mut app, "value", edited.clone(), None);
        assert_eq!(custom_value(&app), Some(edited.clone()));
        write(&mut app, "value", original.clone(), Some(original_revision));
        assert_eq!(
            custom_value(&app),
            Some(edited.clone()),
            "a stale root write cannot replace the successor"
        );
        for redo in [false, true] {
            let base_revision = frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .revision;
            dispatch(
                &mut app,
                Some(EditorAction::Mutate(if redo {
                    EditorDocumentMutation::Redo {
                        document,
                        base_revision,
                    }
                } else {
                    EditorDocumentMutation::Undo {
                        document,
                        base_revision,
                    }
                })),
            );
            assert_eq!(
                custom_value(&app),
                Some(if redo {
                    edited.clone()
                } else {
                    original.clone()
                })
            );
        }
        let base_revision = frame(&app)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision;
        dispatch(
            &mut app,
            Some(EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision,
            })),
        );
        settle_persistence(&mut app);
        assert!(matches!(
            frame(&app).persistence().result(),
            Some(nara::tooling::EditorPersistenceResult::Saved { .. })
        ));
        dispatch(
            &mut app,
            Some(EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult,
            )),
        );
        let base_revision = frame(&app)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision;
        dispatch(
            &mut app,
            Some(EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document,
                    base_revision,
                },
            })),
        );
        settle_persistence(&mut app);
        dispatch(
            &mut app,
            Some(EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(SceneEntityId::new("editable").unwrap()),
                },
            )),
        );
        assert_eq!(custom_value(&app), Some(edited));
        assert_eq!(
            app.active_authoring_unavailable_record_manifest(),
            Some(manifest)
        );
        dispatch(
            &mut app,
            Some(EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult,
            )),
        );
    }
    close(&mut app);
}
