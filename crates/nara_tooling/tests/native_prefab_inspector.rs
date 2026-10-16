use nara_reflect::{
    ComponentFieldId, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};
use nara_scene::{
    PrefabAuthoringSession, PrefabDocument, SceneComponentRecord, SceneEntityId, SceneEntityRecord,
};
use nara_tooling::{
    EditorDocumentSourceBinding, EditorWorkspace, EditorWorkspaceCommand, SceneInspectorCommand,
};

#[test]
fn native_prefab_shared_inspector_edits_its_own_document_and_undoes() {
    let mut registry = ComponentRegistry::new();
    nara_scene::register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    let entity = SceneEntityId::new("native-prefab").unwrap();
    let component = ComponentTypeId::new("nara.scene.Name");
    let original = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
        .with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Before".into()),
            ),
        )]);
    let mut workspace = EditorWorkspace::new();
    let document = workspace
        .open_prefab_session(
            "Native",
            EditorDocumentSourceBinding::new("native.prefab.json"),
            PrefabAuthoringSession::new(original.clone()),
        )
        .unwrap()
        .opened_document
        .unwrap();
    let _ = workspace.apply_command(
        &registry,
        EditorWorkspaceCommand::SelectEntity {
            document,
            entity: Some(entity.clone()),
        },
    );
    let model = workspace.model(&registry, None).active_scene.unwrap();
    assert_eq!(model.kind, nara_tooling::EditorDocumentKind::Prefab);
    assert!(
        model
            .editor
            .inspector
            .selected_entity_view
            .as_ref()
            .unwrap()
            .components[0]
            .access
            .is_editable()
    );
    let edit = workspace.apply_command(
        &registry,
        EditorWorkspaceCommand::ApplyInspectorCommand {
            document,
            base_revision: model.revision,
            command: SceneInspectorCommand::SetField {
                entity,
                component,
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("value"),
                value: ComponentValue::String("After".into()),
            },
        },
    );
    assert!(edit.applied, "{:?}", edit.diagnostics);
    assert_eq!(
        workspace
            .prefab(document)
            .unwrap()
            .session()
            .history_status()
            .undo_depth,
        1
    );
    assert!(
        workspace
            .apply_command(
                &registry,
                EditorWorkspaceCommand::Undo {
                    document,
                    base_revision: edit.revision.unwrap()
                }
            )
            .applied
    );
    assert_eq!(
        workspace.prefab(document).unwrap().session().document(),
        &original
    );
}
