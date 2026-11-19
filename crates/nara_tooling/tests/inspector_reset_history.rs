use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldPathSegment,
    ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape,
};
use nara_scene::{
    PrefabAuthoringSession, PrefabDocument, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord,
};
use nara_tooling::{
    EditorDocumentId, EditorDocumentSourceBinding, EditorWorkspace, EditorWorkspaceCommand,
    EditorWorkspaceCommandReport, SceneInspectorCommand,
};

const SLOT_FIELDS: [&str; 3] = ["first-slot", "middle-slot", "last-slot"];

#[derive(nara_ecs::Component)]
struct Inventory(ComponentValue);

fn entity() -> SceneEntityId {
    SceneEntityId::new("inventory").unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.InspectorInventory")
}

fn registry() -> ComponentRegistrySnapshot {
    let optional = |name: &str, path| {
        ComponentFieldSchema::optional(
            ComponentFieldId::new(name),
            name,
            path,
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    };
    let slots = (0..).zip(SLOT_FIELDS).map(|(index, name)| {
        optional(
            name,
            ComponentFieldPath::new([
                ComponentFieldPathSegment::field("slots"),
                ComponentFieldPathSegment::index(index),
            ]),
        )
    });
    let schema = ComponentSchema::new(component(), "Inventory", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(
            slots.chain([
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("priority"),
                    "Priority",
                    ComponentFieldPath::from_fields(["priority"]),
                    ComponentValueKind::U64,
                    ComponentValue::U64(5),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                optional(
                    "row-amount",
                    ComponentFieldPath::new([
                        ComponentFieldPathSegment::field("rows"),
                        ComponentFieldPathSegment::index(0),
                        ComponentFieldPathSegment::field("amount"),
                    ]),
                ),
                ComponentFieldSchema::required(
                    ComponentFieldId::new("weights"),
                    "Weights",
                    ComponentFieldPath::from_fields(["weights"]),
                    ComponentValueShape::list(ComponentValueKind::U64.into()),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ]),
        );
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_with_codec::<Inventory, _, _>(
            schema,
            |value| Ok(Inventory(value.to_owned())),
            |inventory| Ok(inventory.0.clone()),
        )
        .unwrap();
    builder.freeze().unwrap()
}

fn list(values: &[u64]) -> ComponentValue {
    ComponentValue::List(values.iter().copied().map(ComponentValue::U64).collect())
}

fn inventory(
    priority: Option<u64>,
    amount: Option<u64>,
    weights: &[u64],
    middle: u64,
) -> ComponentValue {
    let row = ComponentValue::map(amount.map(|amount| ("amount", ComponentValue::U64(amount))));
    let mut fields = vec![
        ("slots", list(&[10, middle, 30])),
        ("rows", ComponentValue::List(vec![row])),
        ("weights", list(weights)),
    ];
    fields.extend(priority.map(|value| ("priority", ComponentValue::U64(value))));
    ComponentValue::map(fields)
}

fn records(value: &ComponentValue) -> [SceneEntityRecord; 2] {
    [
        SceneEntityRecord::new(SceneEntityId::new("untouched").unwrap()),
        SceneEntityRecord::new(entity()).with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value.clone()),
        ),
    ]
}

fn open_documents(value: &ComponentValue) -> (EditorWorkspace, [EditorDocumentId; 2]) {
    let mut workspace = EditorWorkspace::new();
    let scene = workspace
        .open_scene_session(
            "Inventory Scene",
            SceneAuthoringSession::new(SceneDocument::from_ordered_roots(records(value))),
        )
        .unwrap()
        .opened_document
        .unwrap();
    let prefab = workspace
        .open_prefab_session(
            "Inventory Prefab",
            EditorDocumentSourceBinding::new("inventory.prefab.json"),
            PrefabAuthoringSession::new(PrefabDocument::from_ordered_roots(records(value))),
        )
        .unwrap()
        .opened_document
        .unwrap();
    (workspace, [scene, prefab])
}

fn assert_source(workspace: &EditorWorkspace, document: EditorDocumentId, value: &ComponentValue) {
    if let Some(slot) = workspace.scene(document) {
        assert_eq!(
            slot.session().document(),
            &SceneDocument::from_ordered_roots(records(value))
        );
    } else {
        assert_eq!(
            workspace.prefab(document).unwrap().session().document(),
            &PrefabDocument::from_ordered_roots(records(value))
        );
    }
}

fn apply_field(
    workspace: &mut EditorWorkspace,
    registry: &ComponentRegistrySnapshot,
    document: EditorDocumentId,
    name: &str,
    value: Option<ComponentValue>,
) -> EditorWorkspaceCommandReport {
    let field = ComponentFieldId::new(name);
    let command = match value {
        Some(value) => SceneInspectorCommand::SetField {
            entity: entity(),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field,
            value,
        },
        None => SceneInspectorCommand::RemoveField {
            entity: entity(),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field,
        },
    };
    let base_revision = workspace.document_revision(document).unwrap();
    workspace.apply_command(
        registry,
        EditorWorkspaceCommand::ApplyInspectorCommand {
            document,
            base_revision,
            command,
        },
    )
}

fn assert_indexed_resets_refused(
    workspace: &mut EditorWorkspace,
    registry: &ComponentRegistrySnapshot,
    document: EditorDocumentId,
    expected_source: &ComponentValue,
) {
    let before = workspace.model(registry, None);
    let before = before.document(document).unwrap();
    for field in SLOT_FIELDS {
        let report = apply_field(workspace, registry, document, field, None);
        assert!(!report.applied, "{:?}: {field}", before.kind);
        assert_eq!(report.revision, Some(before.revision));
        assert_eq!(report.dirty, Some(before.dirty));
        let patch = report.inspector_report.unwrap().patch_report.unwrap();
        assert!(patch.inverse.is_none());
        assert!(
            patch
                .diagnostics
                .iter()
                .any(|entry| entry.code().as_str() == "scene.patch-invalid-field-path"),
            "{:?}: {field}: {:?}",
            before.kind,
            patch.diagnostics
        );
        assert_eq!(
            workspace.model(registry, None).document(document),
            Some(before)
        );
        assert_source(workspace, document, expected_source);
    }
}

#[test]
fn native_scene_and_prefab_refuse_indexed_inspector_resets_without_changing_history() {
    let registry = registry();
    let original = inventory(Some(7), Some(42), &[1, 2, 3], 20);
    let reset = inventory(None, Some(42), &[1, 2, 3], 20);
    let (mut workspace, documents) = open_documents(&original);

    for document in documents {
        let edit = apply_field(&mut workspace, &registry, document, "priority", None);
        assert!(edit.applied, "{:?}", edit.diagnostics);
        assert_eq!(edit.dirty, Some(true));
        assert_indexed_resets_refused(&mut workspace, &registry, document, &reset);

        let undo = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::Undo {
                document,
                base_revision: edit.revision.unwrap(),
            },
        );
        assert!(undo.applied, "{:?}", undo.diagnostics);
        assert_eq!(undo.dirty, Some(false));
        assert_indexed_resets_refused(&mut workspace, &registry, document, &original);

        let redo = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::Redo {
                document,
                base_revision: undo.revision.unwrap(),
            },
        );
        assert!(redo.applied, "{:?}", redo.diagnostics);
        assert_eq!(redo.dirty, Some(true));
        assert_source(&workspace, document, &reset);
    }
}

#[test]
fn native_scene_and_prefab_replay_named_inspector_resets_and_typed_collection_replacement() {
    let registry = registry();
    let states = [
        inventory(Some(7), Some(42), &[1, 2, 3], 20),
        inventory(None, Some(42), &[1, 2, 3], 20),
        inventory(None, None, &[1, 2, 3], 20),
        inventory(None, Some(99), &[1, 2, 3], 20),
        inventory(None, Some(99), &[1, 2, 3], 99),
        inventory(None, Some(99), &[1, 3], 99),
    ];
    let (mut workspace, documents) = open_documents(&states[0]);

    for document in documents {
        let edits = [
            ("priority", None),
            ("row-amount", None),
            ("row-amount", Some(ComponentValue::U64(99))),
            ("middle-slot", Some(ComponentValue::U64(99))),
            ("weights", Some(list(&[1, 3]))),
        ];
        for ((field, value), expected) in edits.into_iter().zip(&states[1..]) {
            let base = workspace.document_revision(document).unwrap();
            let report = apply_field(&mut workspace, &registry, document, field, value);
            assert!(report.applied, "{field}: {:?}", report.diagnostics);
            assert_eq!(report.dirty, Some(true));
            assert_eq!(report.revision, Some(base.next()));
            assert_eq!(workspace.document_revision(document), report.revision);
            assert_source(&workspace, document, expected);
        }

        for (remaining, expected) in states[..states.len() - 1].iter().enumerate().rev() {
            let base_revision = workspace.document_revision(document).unwrap();
            let undo = workspace.apply_command(
                &registry,
                EditorWorkspaceCommand::Undo {
                    document,
                    base_revision,
                },
            );
            assert!(undo.applied, "{:?}", undo.diagnostics);
            assert_eq!(undo.dirty, Some(remaining != 0));
            assert_eq!(undo.revision, Some(base_revision.next()));
            assert_eq!(workspace.document_revision(document), undo.revision);
            assert_source(&workspace, document, expected);
        }

        for expected in &states[1..] {
            let base_revision = workspace.document_revision(document).unwrap();
            let redo = workspace.apply_command(
                &registry,
                EditorWorkspaceCommand::Redo {
                    document,
                    base_revision,
                },
            );
            assert!(redo.applied, "{:?}", redo.diagnostics);
            assert_eq!(redo.dirty, Some(true));
            assert_eq!(redo.revision, Some(base_revision.next()));
            assert_eq!(workspace.document_revision(document), redo.revision);
            assert_source(&workspace, document, expected);
        }
    }
}
