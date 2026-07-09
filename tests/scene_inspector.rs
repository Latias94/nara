use nara::{prelude::*, tooling_prelude::*};

#[derive(Clone, Debug, PartialEq, Component)]
struct TestLabel {
    text: String,
}

#[test]
fn inspector_model_lists_scene_entities_schema_fields_and_live_snapshot() {
    let registry = test_registry();
    let player = scene_id("player");
    let child = scene_id("player/child");
    let document = SceneDocument::new([
        labeled_entity(player.clone(), "Player"),
        SceneEntityRecord::new(child.clone()).with_parent(player.clone()),
    ]);
    let mut session = SceneAuthoringSession::new(document);
    let mut inspector = SceneInspectorState::new();
    inspector.select_entity(Some(player.clone()));
    let mut world = World::new();
    let runtime_entity = world.spawn_empty().id();
    assert!(session.sync_world(&mut world, &registry).synced);
    let snapshot = WorldSnapshot::capture(&mut world);

    let model = inspector.model(&session, &registry, Some(&snapshot));

    assert!(model.world_snapshot.as_ref().unwrap().entity_count >= 3);
    assert!(
        model
            .world_snapshot
            .as_ref()
            .unwrap()
            .entities
            .contains(&runtime_entity)
    );
    assert_eq!(model.schema_catalog.components.len(), 1);
    assert_eq!(model.history.undo_depth, 0);
    assert!(!model.live_dirty);
    assert!(!model.diagnostics.has_errors());
    assert_eq!(model.entities.len(), 2);
    assert!(model.entities.iter().any(|row| {
        row.id == player && row.selected && row.component_count == 1 && row.live_entity.is_some()
    }));
    assert!(model.entities.iter().any(|row| {
        row.id == child && row.parent.as_ref() == Some(&player) && row.live_entity.is_some()
    }));

    let entity_view = model.selected_entity_view.unwrap();
    assert_eq!(entity_view.id, player);
    assert_eq!(entity_view.components.len(), 1);
    let component = &entity_view.components[0];
    assert!(component.schema_known);
    assert_eq!(component.component, label_type_id());
    assert_eq!(component.document_version, ComponentSchemaVersion(1));
    assert_eq!(component.schema_version, Some(ComponentSchemaVersion(1)));
    assert_eq!(component.fields.len(), 2);

    let text = field(&component.fields, "text");
    assert_eq!(text.state, SceneInspectorFieldState::Present);
    assert_eq!(text.value.as_ref().unwrap().as_str(), Some("Player"));

    let note = field(&component.fields, "note");
    assert_eq!(note.state, SceneInspectorFieldState::Missing);
    assert!(!note.required);
}

#[test]
fn inspector_set_field_command_applies_patch_and_selects_target() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut inspector = SceneInspectorState::new();

    let report = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::SetField {
            entity: player.clone(),
            component: label_type_id(),
            component_version: ComponentSchemaVersion(1),
            path: ComponentFieldPath::from_fields(["text"]),
            value: ComponentValue::String("Hero".to_string()),
        },
    );

    assert!(report.applied);
    assert!(report.patch.is_some());
    assert!(report.patch_report.as_ref().unwrap().applied);
    assert_eq!(report.selected_entity.as_ref(), Some(&player));
    assert_eq!(inspector.selected_entity(), Some(&player));
    assert_eq!(document_label(session.document(), &player), "Hero");
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 1,
            redo_depth: 0,
        }
    );
    assert!(session.is_live_dirty());
}

#[test]
fn inspector_selection_rejects_missing_entities_without_losing_selection() {
    let registry = test_registry();
    let player = scene_id("player");
    let missing = scene_id("missing");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut inspector = SceneInspectorState::new();
    assert!(
        inspector
            .apply_command(
                &mut session,
                &registry,
                SceneInspectorCommand::SelectEntity {
                    entity: Some(player.clone()),
                },
            )
            .applied
    );

    let report = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::SelectEntity {
            entity: Some(missing),
        },
    );

    assert!(!report.applied);
    assert!(report.diagnostics.has_errors());
    assert_eq!(
        report.diagnostics.diagnostics()[0].code.as_str(),
        "tooling.inspector-missing-entity"
    );
    assert_eq!(
        report.diagnostics.diagnostics()[0]
            .context
            .entity_id
            .as_deref(),
        Some("missing")
    );
    assert_eq!(report.selected_entity.as_ref(), Some(&player));
    assert_eq!(inspector.selected_entity(), Some(&player));
}

#[test]
fn inspector_failed_patch_command_does_not_enter_history() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut inspector = SceneInspectorState::new();
    inspector.select_entity(Some(player.clone()));

    let report = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::RemoveField {
            entity: player.clone(),
            component: label_type_id(),
            component_version: ComponentSchemaVersion(1),
            path: ComponentFieldPath::from_fields(["text"]),
        },
    );

    assert!(!report.applied);
    assert!(report.patch.is_some());
    assert!(
        report
            .patch_report
            .as_ref()
            .unwrap()
            .diagnostics
            .has_errors()
    );
    assert_eq!(report.selected_entity.as_ref(), Some(&player));
    assert_eq!(document_label(session.document(), &player), "Player");
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 0,
            redo_depth: 0,
        }
    );
}

fn test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let label_id = label_type_id();
    registry
        .register_scene_component_with_fields::<TestLabel, _, _>(
            label_id.clone(),
            ComponentSchemaVersion(1),
            [
                ComponentFieldSchema::required(
                    ComponentFieldPath::from_fields(["text"]),
                    ComponentValueKind::String,
                ),
                ComponentFieldSchema::optional(
                    ComponentFieldPath::from_fields(["note"]),
                    ComponentValueKind::String,
                ),
            ],
            |value| {
                Ok(TestLabel {
                    text: value.field_str("text")?.to_string(),
                })
            },
            |label| {
                Ok(ComponentValue::map([(
                    "text",
                    ComponentValue::String(label.text.clone()),
                )]))
            },
        )
        .unwrap();
    registry
}

fn labeled_entity(id: SceneEntityId, label: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id).with_component(
        label_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([("text", ComponentValue::String(label.to_string()))]),
        ),
    )
}

fn field<'a>(fields: &'a [SceneInspectorFieldView], path: &str) -> &'a SceneInspectorFieldView {
    fields
        .iter()
        .find(|field| field.path.to_string() == path)
        .unwrap()
}

fn document_label(document: &SceneDocument, id: &SceneEntityId) -> String {
    document
        .entities
        .iter()
        .find(|entity| entity.id == *id)
        .and_then(|entity| entity.components.get(&label_type_id()))
        .and_then(|component| component.value.field_str("text").ok())
        .unwrap()
        .to_string()
}

fn label_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Label")
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}
