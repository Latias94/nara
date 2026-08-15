use nara::{
    identity::{EntityLookup, SpawnedSceneInstance},
    prelude::*,
    reflect::ComponentValueKind,
    scene::{
        SceneAuthoringHistoryLimits, SceneAuthoringHistoryStatus, ScenePatchDocument,
        ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    },
};

#[derive(Clone, Debug, PartialEq, Component)]
struct TestLabel {
    text: String,
}

#[test]
fn authoring_session_applies_patch_syncs_world_and_preserves_runtime_entities() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session =
        SceneAuthoringSession::new(SceneDocument::from_ordered_roots([labeled_entity(
            player.clone(),
            "Player",
        )]));
    let mut world = World::new();
    let runtime_entity = world.spawn_empty().id();

    let first_sync = session.sync_world(&mut world, &registry);

    assert!(first_sync.synced);
    assert!(!first_sync.diagnostics.has_errors());
    assert!(!session.is_live_dirty());
    assert_eq!(first_sync.live_instance.as_ref().unwrap().len(), 1);
    assert!(world.get_entity(runtime_entity).is_ok());
    let first_player = resolve_scene_entity(session.live_instance().unwrap(), &world, &player);
    assert_eq!(world.get::<TestLabel>(first_player).unwrap().text, "Player");

    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player.clone(),
        component: label_type_id(),
        component_version: ComponentSchemaVersion(1),
        field: ComponentFieldId::new("text"),
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
    let second_player = resolve_scene_entity(session.live_instance().unwrap(), &world, &player);
    assert_eq!(world.get::<TestLabel>(second_player).unwrap().text, "Hero");
}

#[test]
fn authoring_session_undo_redo_are_transactional() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session =
        SceneAuthoringSession::new(SceneDocument::from_ordered_roots([labeled_entity(
            player.clone(),
            "Player",
        )]));
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player.clone(),
        component: label_type_id(),
        component_version: ComponentSchemaVersion(1),
        field: ComponentFieldId::new("text"),
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
fn content_state_tracks_undo_redo_and_divergent_branches() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session =
        SceneAuthoringSession::new(SceneDocument::from_ordered_roots([labeled_entity(
            player.clone(),
            "Player",
        )]));
    let initial_state = session.content_state();
    let initial_revision = session.revision();

    assert!(
        session
            .apply_patch(&set_label_patch(&player, "Hero"), &registry)
            .applied
    );
    let edited_state = session.content_state();
    assert_ne!(edited_state, initial_state);

    assert!(session.undo(&registry).applied);
    assert_eq!(session.content_state(), initial_state);
    assert!(session.revision().generation() > initial_revision.generation());

    assert!(session.redo(&registry).applied);
    assert_eq!(session.content_state(), edited_state);

    assert!(session.undo(&registry).applied);
    assert!(
        session
            .apply_patch(&set_label_patch(&player, "Pilot"), &registry)
            .applied
    );
    let divergent_state = session.content_state();
    assert_ne!(divergent_state, initial_state);
    assert_ne!(divergent_state, edited_state);
    assert!(!session.redo(&registry).applied);
    assert_eq!(document_label(session.document(), &player), "Pilot");
}

#[test]
fn history_limits_evict_the_oldest_undo_record() {
    let registry = scene_registry();
    let player = scene_id("player");
    let limits =
        SceneAuthoringHistoryLimits::new(2, 1_024, 1_024, 1_024, 1_024, 1_024, 1_024).unwrap();
    let mut session = SceneAuthoringSession::new_with_history_limits(
        SceneDocument::from_ordered_roots([labeled_entity(player.clone(), "Player")]),
        limits,
    );

    assert!(
        session
            .apply_patch(&set_label_patch(&player, "One"), &registry)
            .applied
    );
    assert!(
        session
            .apply_patch(&set_label_patch(&player, "Two"), &registry)
            .applied
    );
    assert!(
        session
            .apply_patch(&set_label_patch(&player, "Three"), &registry)
            .applied
    );
    let newest_state = session.content_state();

    assert_eq!(session.history_status().undo_depth, 2);
    assert_eq!(session.history_usage().records, 2);
    assert_eq!(session.content_state(), newest_state);
    assert!(session.undo(&registry).applied);
    assert_eq!(document_label(session.document(), &player), "Two");
    assert!(session.undo(&registry).applied);
    assert_eq!(document_label(session.document(), &player), "One");
    assert!(!session.undo(&registry).applied);
}

#[test]
fn aggregate_history_cost_limits_evict_the_oldest_undo_record() {
    let cases = [
        (
            SceneAuthoringHistoryLimits::new(8, 2, 1_024, 1_024, 2, 1_024, 1_024).unwrap(),
            2,
            (|usage: nara::scene::SceneAuthoringHistoryUsage| usage.operations)
                as fn(nara::scene::SceneAuthoringHistoryUsage) -> usize,
        ),
        (
            SceneAuthoringHistoryLimits::new(8, 1_024, 2, 1_024, 1_024, 2, 1_024).unwrap(),
            2,
            (|usage: nara::scene::SceneAuthoringHistoryUsage| usage.value_nodes)
                as fn(nara::scene::SceneAuthoringHistoryUsage) -> usize,
        ),
        (
            SceneAuthoringHistoryLimits::new(8, 1_024, 1_024, 64, 1_024, 1_024, 64).unwrap(),
            64,
            (|usage: nara::scene::SceneAuthoringHistoryUsage| usage.logical_bytes)
                as fn(nara::scene::SceneAuthoringHistoryUsage) -> usize,
        ),
    ];

    for (limits, expected_maximum, measured_cost) in cases {
        let registry = scene_registry();
        let player = scene_id("player");
        let mut session = SceneAuthoringSession::new_with_history_limits(
            SceneDocument::from_ordered_roots([labeled_entity(player.clone(), "Player")]),
            limits,
        );

        for label in ["One", "Two", "Three"] {
            assert!(
                session
                    .apply_patch(&set_label_patch(&player, label), &registry)
                    .applied
            );
        }

        assert_eq!(session.history_status().undo_depth, 2);
        assert!(measured_cost(session.history_usage()) <= expected_maximum);
        assert!(session.undo(&registry).applied);
        assert_eq!(document_label(session.document(), &player), "Two");
        assert!(session.redo(&registry).applied);
        assert_eq!(document_label(session.document(), &player), "Three");
    }
}

#[test]
fn oversized_history_records_reject_before_document_publication() {
    let registry = scene_registry();
    let player = scene_id("player");
    let byte_limited =
        SceneAuthoringHistoryLimits::new(4, 1_024, 1_024, 1_024, 1_024, 1_024, 4).unwrap();
    let mut session = SceneAuthoringSession::new_with_history_limits(
        SceneDocument::from_ordered_roots([labeled_entity(player.clone(), "Player")]),
        byte_limited,
    );
    let before_document = session.document().clone();
    let before_revision = session.revision();
    let before_state = session.content_state();
    let before_live_dirty = session.is_live_dirty();

    let report = session.apply_patch(&set_label_patch(&player, "H"), &registry);

    assert!(!report.applied);
    assert_eq!(
        report.diagnostics.iter().next().unwrap().code().as_str(),
        "scene.authoring-history-entry-limit-exceeded"
    );
    assert_eq!(session.document(), &before_document);
    assert_eq!(session.revision(), before_revision);
    assert_eq!(session.content_state(), before_state);
    assert_eq!(session.history_usage().records, 0);
    assert_eq!(session.is_live_dirty(), before_live_dirty);

    let operation_limited =
        SceneAuthoringHistoryLimits::new(4, 1, 1_024, 1_024, 1, 1_024, 1_024).unwrap();
    let mut structural =
        SceneAuthoringSession::new_with_history_limits(SceneDocument::default(), operation_limited);
    let structural_revision = structural.revision();
    let structural_state = structural.content_state();
    let report = structural.apply_patch(
        &ScenePatchDocument::new([
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(scene_id("first")),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(scene_id("second")),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        ]),
        &registry,
    );
    assert!(!report.applied);
    assert!(structural.document().entities.is_empty());
    assert_eq!(structural.revision(), structural_revision);
    assert_eq!(structural.content_state(), structural_state);

    let root = scene_id("root");
    let children = [
        scene_id("child-a"),
        scene_id("child-b"),
        scene_id("child-c"),
    ];
    let subtree = SceneDocument::new(
        [root.clone()],
        [
            SceneEntityRecord::new(root.clone()).with_children(children.clone()),
            SceneEntityRecord::new(children[0].clone()),
            SceneEntityRecord::new(children[1].clone()),
            SceneEntityRecord::new(children[2].clone()),
        ],
    );
    let subtree_limits =
        SceneAuthoringHistoryLimits::new(4, 7, 1_024, 1_024, 7, 1_024, 1_024).unwrap();
    let mut structural = SceneAuthoringSession::new_with_history_limits(subtree, subtree_limits);
    let before_document = structural.document().clone();
    let before_revision = structural.revision();

    let report = structural.apply_patch(
        &ScenePatchDocument::new([ScenePatchOperation::RemoveEntity {
            entity: root,
            from: ScenePlacement::first(ScenePlacementOwner::Root),
        }]),
        &registry,
    );

    assert!(!report.applied);
    assert_eq!(structural.document(), &before_document);
    assert_eq!(structural.revision(), before_revision);
    assert_eq!(structural.history_usage().records, 0);
}

#[test]
fn authoring_session_undo_redo_preserve_exact_root_order() {
    let registry = scene_registry();
    let first = scene_id("first");
    let middle = scene_id("middle");
    let last = scene_id("last");
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(first.clone()),
        SceneEntityRecord::new(middle.clone()),
        SceneEntityRecord::new(last.clone()),
    ]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::MoveEntity {
        entity: last.clone(),
        from: ScenePlacement::after(ScenePlacementOwner::Root, middle.clone()),
        to: ScenePlacement::first(ScenePlacementOwner::Root),
    }]);
    let mut session = SceneAuthoringSession::new(original.clone());

    assert!(session.apply_patch(&patch, &registry).applied);
    assert_eq!(
        session.document().roots,
        [last.clone(), first.clone(), middle.clone()]
    );
    assert!(session.undo(&registry).applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo(&registry).applied);
    assert_eq!(session.document().roots, [last, first, middle]);
}

#[test]
fn failed_authoring_patch_does_not_dirty_live_world_or_enter_history() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session =
        SceneAuthoringSession::new(SceneDocument::from_ordered_roots([labeled_entity(
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
        field: ComponentFieldId::new("missing"),
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
    let mut session =
        SceneAuthoringSession::new(SceneDocument::from_ordered_roots([labeled_entity(
            player.clone(),
            "Player",
        )]));
    let mut world = World::new();
    let sync = session.sync_world(&mut world, &registry);
    assert!(sync.synced);
    let live_player = resolve_scene_entity(session.live_instance().unwrap(), &world, &player);

    session.replace_document(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        player.clone(),
    )
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
    assert_eq!(
        resolve_scene_entity(session.live_instance().unwrap(), &world, &player),
        live_player
    );
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
        undo.diagnostics.iter().next().unwrap().code().as_str(),
        "scene.undo-empty"
    );
    assert_eq!(
        redo.diagnostics.iter().next().unwrap().code().as_str(),
        "scene.redo-empty"
    );
    assert_eq!(session.revision(), revision);
}

#[test]
fn authoring_revision_changes_only_for_successful_document_mutations() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session =
        SceneAuthoringSession::new(SceneDocument::from_ordered_roots([labeled_entity(
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
        field: ComponentFieldId::new("missing"),
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

    let clear = session.clear_live_world(&mut world);
    assert!(clear.cleared);
    assert_eq!(clear.removed_entities, 1);
    assert!(clear.live_instance.is_none());
    assert!(session.live_instance().is_none());
    assert_eq!(session.revision(), after_patch);

    session.replace_document(SceneDocument::from_ordered_roots([labeled_entity(
        player.clone(),
        "Replacement",
    )]));
    let after_replace = session.revision();
    assert_ne!(after_replace.source_id(), initial.source_id());
    assert_eq!(after_replace.generation(), 0);
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
    let mut session =
        SceneAuthoringSession::new(SceneDocument::from_ordered_roots([labeled_entity(
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
    let schema = ComponentSchema::new(label_id, "Label", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("text"),
            "Text",
            ComponentFieldPath::from_fields(["text"]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    registry
        .register_persistent_component_with_codec::<TestLabel, _, _>(
            schema,
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
    registry.freeze().unwrap();
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
        field: ComponentFieldId::new("text"),
        value: ComponentValue::String(text.to_string()),
    }])
}

fn label_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Label")
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

fn resolve_scene_entity(
    instance: &SpawnedSceneInstance,
    world: &World,
    id: &SceneEntityId,
) -> Entity {
    match instance.resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected resolved scene entity, got {lookup:?}"),
    }
}
