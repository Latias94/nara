use std::collections::BTreeMap;

use nara::{advanced_prelude::*, tooling_prelude::*};

const PLAYER_STABLE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const UNKNOWN_STABLE_ID: &str = "4bf6d3ff-f6c6-47fb-9a39-4ab27598094f";

#[derive(Clone, Debug, PartialEq, Component)]
struct TestLabel {
    text: String,
}

#[test]
fn play_start_uses_isolated_world_and_stop_discards_runtime_mutations() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut preview_world = World::new();
    let preview_sync = session.sync_world(&mut preview_world, &registry);
    assert!(preview_sync.synced);
    let preview_entity = session.live_entity_map().get(&player).unwrap();

    let mut editor = SceneEditorState::new();
    let start = editor.start_play(&session, &registry);

    assert!(start.applied);
    assert_eq!(
        editor.mode(),
        SceneEditorMode::Play {
            source_revision: session.revision()
        }
    );
    assert_eq!(start.source_revision, Some(session.revision()));
    assert_eq!(editor.play_entity_map().unwrap().len(), 1);
    let play_entity = editor.play_entity_map().unwrap().get(&player).unwrap();
    assert_eq!(
        editor
            .play_world()
            .unwrap()
            .get::<TestLabel>(play_entity)
            .unwrap()
            .text,
        "Player"
    );

    editor
        .play_world_mut()
        .unwrap()
        .get_mut::<TestLabel>(play_entity)
        .unwrap()
        .text = "Runtime".to_string();

    assert_eq!(document_label(session.document(), &player), "Player");
    assert_eq!(
        preview_world.get::<TestLabel>(preview_entity).unwrap().text,
        "Player"
    );
    assert_eq!(
        editor
            .play_world()
            .unwrap()
            .get::<TestLabel>(play_entity)
            .unwrap()
            .text,
        "Runtime"
    );

    let stop = editor.stop_play();

    assert!(stop.applied);
    assert_eq!(editor.mode(), SceneEditorMode::Edit);
    assert!(editor.play_world().is_none());
    assert_eq!(document_label(session.document(), &player), "Player");
    assert_eq!(
        preview_world.get::<TestLabel>(preview_entity).unwrap().text,
        "Player"
    );
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
fn play_transitions_reject_invalid_modes_and_preserve_active_session() {
    let registry = scene_registry();
    let player = scene_id("player");
    let session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut editor = SceneEditorState::new();

    assert_invalid_transition(editor.pause_play(), "tooling.play-pause-invalid-mode");
    assert_invalid_transition(editor.resume_play(), "tooling.play-resume-invalid-mode");
    assert_invalid_transition(editor.stop_play(), "tooling.play-stop-invalid-mode");
    assert_eq!(editor.mode(), SceneEditorMode::Edit);

    let start = editor.start_play(&session, &registry);
    assert!(start.applied);
    let play_entity = editor.play_entity_map().unwrap().get(&player).unwrap();

    assert_invalid_transition(
        editor.start_play(&session, &registry),
        "tooling.play-start-invalid-mode",
    );
    assert_eq!(
        editor.play_entity_map().unwrap().get(&player),
        Some(play_entity)
    );

    let pause = editor.pause_play();
    assert!(pause.applied);
    assert!(editor.mode().is_paused());
    assert_invalid_transition(
        editor.start_play(&session, &registry),
        "tooling.play-start-invalid-mode",
    );
    assert_eq!(
        editor.play_entity_map().unwrap().get(&player),
        Some(play_entity)
    );
    assert_invalid_transition(editor.pause_play(), "tooling.play-pause-invalid-mode");
    assert_eq!(
        editor.play_entity_map().unwrap().get(&player),
        Some(play_entity)
    );

    let resume = editor.resume_play();
    assert!(resume.applied);
    assert!(editor.mode().is_play());
    assert_invalid_transition(editor.resume_play(), "tooling.play-resume-invalid-mode");
    assert_eq!(
        editor.play_entity_map().unwrap().get(&player),
        Some(play_entity)
    );
}

#[test]
fn failed_play_start_reports_diagnostics_without_entering_play() {
    let player = scene_id("player");
    let session =
        SceneAuthoringSession::new(SceneDocument::new([labeled_entity(player, "Player")]));
    let registry = ComponentRegistry::new();
    let mut editor = SceneEditorState::new();

    let start = editor.start_play(&session, &registry);

    assert!(!start.applied);
    assert!(start.diagnostics.has_errors());
    assert_eq!(editor.mode(), SceneEditorMode::Edit);
    assert!(editor.play_session().is_none());
    assert!(start.entity_map.is_none());
}

#[test]
fn prefab_play_start_variants_follow_spawn_preflight() {
    let registry = scene_registry();
    let source = AssetRef::path("prefabs/enemy.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([labeled_entity(scene_id("visual"), "Enemy")]),
    );
    let session = SceneAuthoringSession::new(SceneDocument::new([prefab_anchor("enemy", source)]));
    let mut editor = SceneEditorState::new();

    let start = editor.start_play_with_prefab_resolver(&session, &registry, &resolver);

    assert!(start.applied);
    assert_eq!(editor.play_entity_map().unwrap().len(), 2);
    let visual = editor
        .play_entity_map()
        .unwrap()
        .get(&scene_id("enemy/visual"))
        .unwrap();
    assert_eq!(
        editor
            .play_world()
            .unwrap()
            .get::<TestLabel>(visual)
            .unwrap()
            .text,
        "Enemy"
    );

    let stop = editor.stop_play();
    assert!(stop.applied);

    let missing_source = AssetRef::path("prefabs/missing.ron").unwrap();
    let missing_session =
        SceneAuthoringSession::new(SceneDocument::new([prefab_anchor("enemy", missing_source)]));
    let failed = editor.start_play_with_prefab_resolver(
        &missing_session,
        &registry,
        &InMemoryPrefabSourceResolver::new(),
    );

    assert!(!failed.applied);
    assert!(failed.diagnostics.has_errors());
    assert_eq!(editor.mode(), SceneEditorMode::Edit);
    assert!(editor.play_session().is_none());
}

#[test]
fn asset_and_combined_play_start_variants_follow_spawn_preflight() {
    let registry = sprite_registry();
    let database = asset_database(PLAYER_STABLE_ID, "textures/player.png");
    let player = scene_id("player");
    let session = SceneAuthoringSession::new(SceneDocument::new([sprite_entity(
        player.clone(),
        asset_ref_value("stable_id", PLAYER_STABLE_ID),
    )]));
    let mut editor = SceneEditorState::new();

    let start = editor.start_play_with_asset_database(&session, &registry, &database);

    assert!(start.applied);
    let play_entity = editor.play_entity_map().unwrap().get(&player).unwrap();
    let sprite = editor
        .play_world()
        .unwrap()
        .get::<Sprite>(play_entity)
        .unwrap();
    let texture = sprite
        .material
        .image
        .expect("sprite texture should resolve");
    assert_eq!(
        editor
            .play_world()
            .unwrap()
            .resource::<AssetServer>()
            .path(texture.id()),
        Some("textures/player.png")
    );
    assert!(editor.stop_play().applied);

    let source = AssetRef::path("prefabs/player.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([sprite_entity(
            scene_id("visual"),
            asset_ref_value("stable_id", PLAYER_STABLE_ID),
        )]),
    );
    let combined_session =
        SceneAuthoringSession::new(SceneDocument::new([prefab_anchor("player", source)]));
    let combined = editor.start_play_with_prefab_resolver_and_asset_database(
        &combined_session,
        &registry,
        &resolver,
        &database,
    );

    assert!(combined.applied);
    let visual = editor
        .play_entity_map()
        .unwrap()
        .get(&scene_id("player/visual"))
        .unwrap();
    assert!(
        editor
            .play_world()
            .unwrap()
            .get::<Sprite>(visual)
            .unwrap()
            .material
            .image
            .is_some()
    );
    assert!(editor.stop_play().applied);

    let invalid_session = SceneAuthoringSession::new(SceneDocument::new([sprite_entity(
        scene_id("missing"),
        asset_ref_value("stable_id", UNKNOWN_STABLE_ID),
    )]));
    let invalid = editor.start_play_with_asset_database(
        &invalid_session,
        &registry,
        &ProjectAssetDatabase::default(),
    );

    assert!(!invalid.applied);
    assert!(invalid.diagnostics.has_errors());
    assert!(editor.play_session().is_none());
}

#[test]
fn mode_aware_inspector_rejects_persistent_play_edits_and_models_play_snapshot() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut editor = SceneEditorState::new();

    let edit_report =
        editor.apply_inspector_command(&mut session, &registry, set_label_command(&player, "Hero"));
    assert!(edit_report.applied);
    assert_eq!(document_label(session.document(), &player), "Hero");

    let start = editor.start_play(&session, &registry);
    assert!(start.applied);
    let select = editor.apply_inspector_command(
        &mut session,
        &registry,
        SceneInspectorCommand::SelectEntity {
            entity: Some(player.clone()),
        },
    );
    assert!(select.applied);
    assert_eq!(editor.inspector().selected_entity(), Some(&player));

    let rejected = editor.apply_inspector_command(
        &mut session,
        &registry,
        set_label_command(&player, "Runtime"),
    );
    assert!(!rejected.applied);
    assert!(rejected.patch.is_none());
    assert_eq!(
        rejected.diagnostics.diagnostics()[0].code.as_str(),
        "tooling.inspector-persistent-command-in-play-mode"
    );
    assert_eq!(document_label(session.document(), &player), "Hero");

    let pause = editor.pause_play();
    assert!(pause.applied);
    let paused_rejected = editor.apply_inspector_command(
        &mut session,
        &registry,
        set_label_command(&player, "PausedRuntime"),
    );
    assert!(!paused_rejected.applied);
    assert_eq!(document_label(session.document(), &player), "Hero");

    let model = editor.model(&session, &registry, None);
    assert!(model.mode.is_paused());
    assert_eq!(model.inspector.selected_entity, Some(player));
    assert!(model.play_world_snapshot.is_some());
}

#[test]
fn apply_changes_exports_selected_component_and_keeps_revision_guards() {
    let registry = scene_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut editor = SceneEditorState::new();

    let start = editor.start_play(&session, &registry);
    assert!(start.applied);

    let status = editor.apply_changes_status(&session);
    assert!(!status.applied);
    assert!(status.supported);
    assert_eq!(status.source_revision, Some(session.revision()));

    let play_entity = editor.play_entity_map().unwrap().get(&player).unwrap();
    editor
        .play_world_mut()
        .unwrap()
        .get_mut::<TestLabel>(play_entity)
        .unwrap()
        .text = "Runtime".to_string();

    let export = editor.export_apply_changes(
        &session,
        &registry,
        SceneApplyChangesRequest::new(player.clone(), [label_type_id()]),
    );
    assert!(export.supported);
    assert!(!export.applied);
    assert_eq!(export.components.len(), 1);
    assert_eq!(
        export.components[0].status,
        SceneApplyChangesComponentStatus::Pending
    );
    assert_eq!(export.patch.as_ref().unwrap().operations.len(), 1);
    assert_eq!(session.history_status().undo_depth, 0);

    let apply = editor.apply_changes(
        &mut session,
        &registry,
        SceneApplyChangesRequest::new(player.clone(), [label_type_id()]),
    );
    assert!(apply.applied);
    assert!(apply.supported);
    assert_eq!(
        apply.components[0].status,
        SceneApplyChangesComponentStatus::Applied
    );
    assert_eq!(document_label(session.document(), &player), "Runtime");
    assert_eq!(session.history_status().undo_depth, 1);

    let mismatch = editor.apply_changes_status(&session);
    assert!(!mismatch.applied);
    assert_eq!(
        mismatch.diagnostics.diagnostics()[0].code.as_str(),
        "tooling.apply-changes-revision-mismatch"
    );

    let undo = session.undo(&registry);
    assert!(undo.applied);
    assert_eq!(document_label(session.document(), &player), "Player");
    let still_mismatch = editor.apply_changes_status(&session);
    assert_eq!(
        still_mismatch.diagnostics.diagnostics()[0].code.as_str(),
        "tooling.apply-changes-revision-mismatch"
    );

    let stop = editor.stop_play();
    assert!(stop.applied);
    assert_eq!(document_label(session.document(), &player), "Player");
    let no_play = editor.apply_changes_status(&session);
    assert_eq!(
        no_play.diagnostics.diagnostics()[0].code.as_str(),
        "tooling.apply-changes-not-in-play-mode"
    );
}

fn assert_invalid_transition(report: ScenePlayTransitionReport, code: &str) {
    assert!(!report.applied);
    assert_eq!(report.diagnostics.diagnostics()[0].code.as_str(), code);
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

fn sprite_registry() -> ComponentRegistry {
    let mut registry = scene_registry();
    nara::sprite::register_sprite_components(&mut registry)
        .expect("sprite components should register once");
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

fn sprite_entity(id: SceneEntityId, texture: ComponentValue) -> SceneEntityRecord {
    SceneEntityRecord::new(id).with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(ComponentSchemaVersion(1), sprite_value(texture)),
    )
}

fn prefab_anchor(id: &str, source: AssetRef) -> SceneEntityRecord {
    SceneEntityRecord {
        id: scene_id(id),
        parent: None,
        components: BTreeMap::new(),
        prefab: Some(PrefabInstance {
            source,
            overrides: ScenePatchDocument::default(),
        }),
    }
}

fn set_label_command(entity: &SceneEntityId, text: &str) -> SceneInspectorCommand {
    SceneInspectorCommand::SetField {
        entity: entity.clone(),
        component: label_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["text"]),
        value: ComponentValue::String(text.to_string()),
    }
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

fn asset_database(stable_id_value: &str, path: &str) -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            stable_id(stable_id_value),
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    database
}

fn sprite_value(image: ComponentValue) -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(16.0).unwrap()),
                ("y", ComponentValue::f64(16.0).unwrap()),
            ]),
        ),
        (
            "material",
            ComponentValue::map([
                ("image", image),
                (
                    "tint",
                    ComponentValue::map([
                        ("r", ComponentValue::f64(1.0).unwrap()),
                        ("g", ComponentValue::f64(1.0).unwrap()),
                        ("b", ComponentValue::f64(1.0).unwrap()),
                        ("a", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
    ])
}

fn asset_ref_value(kind: &str, value: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_string())),
        ("value", ComponentValue::String(value.to_string())),
    ])
}

fn stable_id(id: &str) -> StableAssetId {
    StableAssetId::parse_str(id).unwrap()
}

fn label_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Label")
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}
