use nara::{prelude::*, scene::ScenePatchDocument, scene::ScenePatchOperation};

#[derive(Clone, Debug, PartialEq, Component)]
struct TestLabel {
    text: String,
}

#[test]
fn authoring_session_applies_patch_syncs_world_and_preserves_runtime_entities() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut world = World::new();
    let runtime_entity = world.spawn_empty().id();

    let first_sync = session.sync_world(&mut world, &registry);

    assert!(first_sync.synced);
    assert!(!first_sync.diagnostics.has_errors());
    assert!(!session.is_live_dirty());
    assert_eq!(first_sync.entity_map.len(), 1);
    assert!(world.get_entity(runtime_entity).is_ok());
    let first_player = session.live_entity_map().get(&player).unwrap();
    assert_eq!(world.get::<TestLabel>(first_player).unwrap().text, "Player");

    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player.clone(),
        component: label_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["text"]),
        value: ComponentValue::String("Hero".to_string()),
    }]);
    let patch_report = session.apply_patch(&patch, &registry);

    assert!(patch_report.applied);
    assert!(session.is_live_dirty());
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 1,
            redo_depth: 0,
        }
    );
    assert_eq!(document_label(session.document(), &player), "Hero");

    let second_sync = session.sync_world(&mut world, &registry);

    assert!(second_sync.synced);
    assert_eq!(second_sync.removed_entities, 1);
    assert!(world.get_entity(first_player).is_err());
    assert!(world.get_entity(runtime_entity).is_ok());
    let second_player = session.live_entity_map().get(&player).unwrap();
    assert_eq!(world.get::<TestLabel>(second_player).unwrap().text, "Hero");
}

#[test]
fn authoring_session_undo_redo_are_transactional() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player.clone(),
        component: label_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["text"]),
        value: ComponentValue::String("Hero".to_string()),
    }]);

    assert!(session.apply_patch(&patch, &registry).applied);
    assert_eq!(document_label(session.document(), &player), "Hero");

    let undo_report = session.undo(&registry);

    assert!(undo_report.applied);
    assert_eq!(document_label(session.document(), &player), "Player");
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 0,
            redo_depth: 1,
        }
    );

    let redo_report = session.redo(&registry);

    assert!(redo_report.applied);
    assert_eq!(document_label(session.document(), &player), "Hero");
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 1,
            redo_depth: 0,
        }
    );
}

#[test]
fn failed_authoring_patch_does_not_dirty_live_world_or_enter_history() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut world = World::new();
    let sync = session.sync_world(&mut world, &registry);
    assert!(sync.synced);
    assert!(!session.is_live_dirty());

    let invalid_patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player.clone(),
        component: label_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["missing"]),
        value: ComponentValue::String("Hero".to_string()),
    }]);
    let report = session.apply_patch(&invalid_patch, &registry);

    assert!(!report.applied);
    assert!(report.diagnostics.has_errors());
    assert_eq!(document_label(session.document(), &player), "Player");
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 0,
            redo_depth: 0,
        }
    );
    assert!(!session.is_live_dirty());
}

#[test]
fn failed_world_sync_keeps_existing_live_projection() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut world = World::new();
    let sync = session.sync_world(&mut world, &registry);
    assert!(sync.synced);
    let live_player = session.live_entity_map().get(&player).unwrap();

    session.replace_document(SceneDocument::new([SceneEntityRecord::new(player.clone())
        .with_component(
            ComponentTypeId::new("nara.test.Unregistered"),
            SceneComponentRecord::new(
                ComponentSchemaVersion(1),
                ComponentValue::String("invalid".to_string()),
            ),
        )]));
    let failed_sync = session.sync_world(&mut world, &registry);

    assert!(!failed_sync.synced);
    assert!(failed_sync.diagnostics.has_errors());
    assert_eq!(session.live_entity_map().get(&player), Some(live_player));
    assert!(world.get_entity(live_player).is_ok());
    assert!(session.is_live_dirty());
}

#[test]
fn empty_undo_redo_report_non_applied_info_diagnostics() {
    let registry = scene_registry();
    let mut session = SceneAuthoringSession::new(SceneDocument::default());
    let revision = session.revision();

    let undo = session.undo(&registry);
    let redo = session.redo(&registry);

    assert!(!undo.applied);
    assert!(!redo.applied);
    assert!(!undo.diagnostics.has_errors());
    assert!(!redo.diagnostics.has_errors());
    assert_eq!(
        undo.diagnostics.diagnostics()[0].code.as_str(),
        "scene.undo-empty"
    );
    assert_eq!(
        redo.diagnostics.diagnostics()[0].code.as_str(),
        "scene.redo-empty"
    );
    assert_eq!(session.revision(), revision);
}

#[test]
fn authoring_revision_changes_only_for_successful_document_mutations() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let initial = session.revision();
    let mut world = World::new();

    let sync = session.sync_world(&mut world, &registry);
    assert!(sync.synced);
    assert_eq!(session.revision(), initial);

    let invalid_patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player.clone(),
        component: label_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["missing"]),
        value: ComponentValue::String("Hero".to_string()),
    }]);
    let invalid_report = session.apply_patch(&invalid_patch, &registry);
    assert!(!invalid_report.applied);
    assert_eq!(session.revision(), initial);

    let empty_report = session.apply_patch(&ScenePatchDocument::default(), &registry);
    assert!(empty_report.applied);
    assert_eq!(session.revision(), initial);

    let patch_report = session.apply_patch(&set_label_patch(&player, "Hero"), &registry);
    assert!(patch_report.applied);
    let after_patch = session.revision();
    assert_eq!(after_patch.source_id(), initial.source_id());
    assert_eq!(after_patch.generation(), initial.generation() + 1);

    session.clear_history();
    assert_eq!(session.revision(), after_patch);

    let removed_entities = session.clear_live_world(&mut world);
    assert_eq!(removed_entities, 1);
    assert_eq!(session.revision(), after_patch);

    session.replace_document(SceneDocument::new([labeled_entity(
        player.clone(),
        "Replacement",
    )]));
    let after_replace = session.revision();
    assert_eq!(after_replace.source_id(), initial.source_id());
    assert_eq!(after_replace.generation(), after_patch.generation() + 1);
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 0,
            redo_depth: 0,
        }
    );

    let failed_sync = session.sync_world(&mut world, &ComponentRegistry::new());
    assert!(!failed_sync.synced);
    assert_eq!(session.revision(), after_replace);
}

#[test]
fn authoring_revision_advances_for_successful_undo_and_redo() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let initial = session.revision();

    let patch_report = session.apply_patch(&set_label_patch(&player, "Hero"), &registry);
    assert!(patch_report.applied);
    let after_patch = session.revision();
    assert_eq!(after_patch.generation(), initial.generation() + 1);

    let undo_report = session.undo(&registry);
    assert!(undo_report.applied);
    let after_undo = session.revision();
    assert_eq!(after_undo.source_id(), initial.source_id());
    assert_eq!(after_undo.generation(), after_patch.generation() + 1);

    let empty_undo = session.undo(&registry);
    assert!(!empty_undo.applied);
    assert_eq!(session.revision(), after_undo);

    let redo_report = session.redo(&registry);
    assert!(redo_report.applied);
    let after_redo = session.revision();
    assert_eq!(after_redo.source_id(), initial.source_id());
    assert_eq!(after_redo.generation(), after_undo.generation() + 1);

    let empty_redo = session.redo(&registry);
    assert!(!empty_redo.applied);
    assert_eq!(session.revision(), after_redo);
}

#[test]
fn authoring_revision_source_identity_prevents_cross_session_equality() {
    let first = SceneAuthoringSession::new(SceneDocument::default());
    let second = SceneAuthoringSession::new(SceneDocument::default());

    assert_eq!(
        first.revision().generation(),
        second.revision().generation()
    );
    assert_ne!(first.revision(), second.revision());
}

fn scene_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let label_id = label_type_id();
    registry
        .register_scene_component_with_fields::<TestLabel, _, _>(
            label_id.clone(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::required(
                ComponentFieldPath::from_fields(["text"]),
                ComponentValueKind::String,
            )],
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

fn set_label_patch(entity: &SceneEntityId, text: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: entity.clone(),
        component: label_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["text"]),
        value: ComponentValue::String(text.to_string()),
    }])
}

fn label_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Label")
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}
