#![cfg(feature = "editor")]

#[path = "support/editor_persistence_driver.rs"]
mod editor_persistence_driver;
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;
#[path = "support/reference_project_copy.rs"]
mod reference_project_copy;

use std::{
    fs,
    sync::{
        Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    prelude::{App, Assets, Children, ImageAsset, Parent, Resource, World},
    project_host::{EditorProjectSession, TrustedCompiledEditorProjectIntent},
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{PrefabDocument, PrefabProjectionStep, SceneEntityId, SceneEntitySource},
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentKind,
        EditorDocumentMutation, EditorGestureAction, EditorGestureOutcome, EditorNavigationAction,
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorPlayAction, EditorPlayRequestResult, EditorPlayState, EditorProjectContentView,
        PrefabCompositionOutcome,
    },
    ui::{
        __private::{
            UiAuthoringCanvasProjection, UiAuthoringDirectEdit, UiAuthoringFieldValue,
            UiAuthoringHierarchyKind, UiBrushAuthoringKind, UiCreateParameters, UiStructureIntent,
        },
        UiColor, UiImageId, UiNode, UiVisual,
    },
};
use nara_reference_game::{REFERENCE_RUNTIME_UI_PLUGIN_ID, ReferenceUiObservation, wave_recipe};
use project_content_fixture::try_load_project_content_from_path;
use reference_project_copy::ReferenceProjectCopy;

const EDITED_RETRY_ACTION: &str = "reference-game.retry-saved";
const EDITED_RETRY_WIDTH: f64 = 216.0;
const EDITOR_FRAME: Duration = Duration::from_millis(20);
const DEADLINE: Duration = Duration::from_secs(10);
const UI_REFRESH_PROBE_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.editor-ui-refresh");
const UI_REFRESH_PROBE_REQUIREMENTS: &[PluginId] = &[REFERENCE_RUNTIME_UI_PLUGIN_ID];
const UI_REFRESH_PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(UI_REFRESH_PROBE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(UI_REFRESH_PROBE_REQUIREMENTS);

static UI_REFRESH_PROBE_SENDER: Mutex<Option<SyncSender<ReferenceUiObservation>>> =
    Mutex::new(None);
static UI_REFRESH_EXPECTED_SOURCE: Mutex<Option<SceneEntityId>> = Mutex::new(None);

#[test]
fn saved_ui_prefab_refreshes_project_content_and_explicit_restart() {
    let project = ReferenceProjectCopy::new("editor-ui-refresh");
    seed_legacy_modal_image(&project);
    let migrated_content = try_load_project_content_from_path(project.path())
        .expect("the legacy image brush must migrate through the ordinary loader");
    assert!(
        migrated_content
            .snapshot
            .images()
            .iter()
            .any(|image| image.path().as_str() == "textures/player.png")
    );
    assert!(migrated_content.snapshot.source_upgrade_required());
    let source_prefab = migrated_content
        .snapshot
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "reference-ui.prefab.json")
        .expect("the reference UI Prefab must remain in the source closure");
    let source_visual = source_prefab
        .document()
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "modal-dim")
        .and_then(|entity| {
            entity
                .components
                .get(&ComponentTypeId::new("nara.ui.UiVisual"))
        })
        .expect("the authored modal visual must remain present");
    assert_eq!(source_visual.version, ComponentSchemaVersion::ONE);
    assert_eq!(
        source_visual
            .value
            .get("background")
            .unwrap()
            .get("reference"),
        Some(&ComponentValue::String("textures/player.png".to_owned())),
        "source capture must preserve the original authored declaration"
    );
    let projected_id = migrated_content
        .snapshot
        .expanded_scene_provenance()
        .iter()
        .find_map(|(id, source)| {
            (source.local_entity_id().as_str() == "modal-dim"
                && source.projection_path().is_some_and(|path| {
                    path.steps().last().is_some_and(|step| {
                        matches!(step, PrefabProjectionStep::SourceEntity { canonical_source, .. }
                            if canonical_source == source_prefab.source_key())
                    })
                }))
            .then_some(id)
        })
        .expect("the modal must retain its exact source in effective compilation");
    let migrated_visual = migrated_content
        .snapshot
        .expanded_scene()
        .entities
        .iter()
        .find(|entity| &entity.id == projected_id)
        .and_then(|entity| {
            entity
                .components
                .get(&ComponentTypeId::new("nara.ui.UiVisual"))
        })
        .expect("effective compilation must include the migrated modal visual");
    assert_eq!(
        migrated_visual.version,
        ComponentSchemaVersion::new(2).unwrap()
    );
    let background = migrated_visual.value.get("background").unwrap();
    assert!(background.get("reference").is_none());
    assert_eq!(
        nara::reflect::decode_asset_reference_value(background.get("image").unwrap()).unwrap(),
        nara::asset::AssetRef::path("textures/player.png").unwrap()
    );
    drop(migrated_content);
    let stale_expected = UI_REFRESH_EXPECTED_SOURCE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    assert!(stale_expected.is_none());
    let (sender, receiver) = mpsc::sync_channel(4);
    let replaced = UI_REFRESH_PROBE_SENDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace(sender);
    assert!(replaced.is_none());

    let recipe = wave_recipe()
        .and_then(|recipe| recipe.add_plugin::<EditorUiRefreshProbePlugin>())
        .expect("the UI refresh product recipe must be statically valid");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )
    .expect("the guarded reference project must open in the Editor Host");
    let opened = editor.current_frame();
    let scene = opened
        .workspace()
        .active_scene
        .as_ref()
        .expect("the startup Scene must be active");
    let scene_document = scene.document;
    let initial_content_generation = opened
        .project_content()
        .current_generation()
        .expect("the trusted compiled Editor installs one project-content generation");

    assert!(matches!(
        dispatch(
            &mut editor,
            &opened,
            EditorAction::Play(EditorPlayAction::Play {
                document: scene_document,
                base_revision: scene.revision,
            }),
        ),
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    ));
    let _ = drive_until_play(&mut editor, EditorPlayState::Running);
    let initial_runtime = receive_runtime_ui(&mut editor, &receiver);
    let running = editor.current_frame();
    assert_eq!(
        initial_runtime.retry_source_document,
        "reference-ui.prefab.json"
    );
    assert_eq!(initial_runtime.retry_source_entity, "retry-button");
    assert_ne!(initial_runtime.retry_action, EDITED_RETRY_ACTION);
    assert_ne!(
        f64::from(initial_runtime.retry_authored_width),
        EDITED_RETRY_WIDTH
    );

    assert!(matches!(
        dispatch(
            &mut editor,
            &running,
            EditorAction::Play(EditorPlayAction::Pause),
        ),
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    ));
    let paused = drive_until_play(&mut editor, EditorPlayState::Paused);
    let reference_ui = SceneEntityId::new("reference-ui").unwrap();
    assert!(matches!(
        dispatch(
            &mut editor,
            &paused,
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document: scene_document,
                entity: Some(reference_ui.clone()),
            }),
        ),
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let selected = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        dispatch(
            &mut editor,
            &selected,
            EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                document: scene_document,
                base_revision: selected.workspace().scene(scene_document).unwrap().revision,
                entity: reference_ui,
            }),
        ),
        EditorActionOutcome::Workspace(report) if report.applied
    ));

    let prefab_frame = editor.drive_editor_frame(Duration::ZERO);
    let prefab_document = prefab_frame
        .workspace()
        .active_document
        .expect("the opened UI Prefab must become active");
    let prefab_tab = prefab_frame.workspace().document(prefab_document).unwrap();
    assert_eq!(prefab_tab.kind, EditorDocumentKind::Prefab);
    assert!(prefab_tab.dirty);
    let retry = SceneEntityId::new("retry-button").unwrap();
    let ui_frame = editor
        .current_ui_authoring_frame()
        .expect("the active UI Prefab must publish an authoring frame");
    let projection = ui_frame.projection().unwrap();
    let UiAuthoringCanvasProjection::Ready(canvas) = projection.canvas() else {
        panic!("the reference UI Prefab must produce a ready Canvas")
    };
    assert_eq!(canvas.logical_size().width(), 960.0);
    assert_eq!(canvas.logical_size().height(), 540.0);
    assert!(canvas.facts().iter().any(|fact| fact.entity() == &retry));
    let migrated_brush = projection
        .fields()
        .iter()
        .find(|field| {
            field.key().entity().as_str() == "modal-dim"
                && field.key().primary_field().as_str() == "background.kind"
        })
        .expect("the migrated modal must expose one semantic brush descriptor");
    assert!(matches!(
        migrated_brush.value(),
        UiAuthoringFieldValue::Brush {
            kind: UiBrushAuthoringKind::Image,
            reference,
            ..
        } if reference == "textures/player.png"
    ));
    let initial_authored_width = retry_authored_width(&editor, &retry);

    let ui_root = SceneEntityId::new("terminal-overlay").unwrap();
    let image_selection = ui_frame
        .image_choices()
        .expect("the current project image catalog must be available")
        .choices()
        .iter()
        .find(|choice| choice.project_path() == "textures/player.png")
        .expect("the otherwise unreferenced player image must be a current project choice")
        .selection(ui_frame.image_choice_generation());
    let create_placement = projection
        .structure_placement_candidates(None)
        .expect("the current UI source must expose exact placements")
        .into_iter()
        .filter(|candidate| candidate.parent() == &ui_root)
        .last()
        .expect("the primary HUD root must accept one exact child placement")
        .placement()
        .clone();
    let create_mutation = ui_frame
        .lower_structure_intent(&UiStructureIntent::Create {
            parameters: UiCreateParameters::Image {
                selection: Some(image_selection),
            },
            placement: create_placement,
        })
        .expect("the exact current Image recipe must lower through the Host frame");
    let created = match dispatch(
        &mut editor,
        &prefab_frame,
        EditorAction::Mutate(create_mutation),
    ) {
        EditorActionOutcome::Workspace(report) => match report.prefab_composition_outcome {
            Some(PrefabCompositionOutcome::Created { entity }) => entity,
            other => panic!("expected one created UI image, got {other:?}"),
        },
        other => panic!("expected one workspace create outcome, got {other:?}"),
    };
    let replaced_source = UI_REFRESH_EXPECTED_SOURCE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace(created.clone());
    assert!(replaced_source.is_none());

    let created_frame = editor.drive_editor_frame(Duration::ZERO);
    let created_order = ui_child_order(&editor, &ui_root);
    assert_eq!(
        created_order,
        vec![
            SceneEntityId::new("modal-dim").unwrap(),
            SceneEntityId::new("terminal-panel").unwrap(),
            created.clone(),
        ]
    );
    let move_mutation = {
        let frame = editor
            .current_ui_authoring_frame()
            .expect("the created Image must publish a successor authoring frame");
        let placement = frame
            .projection()
            .unwrap()
            .structure_placement_candidates(Some(&created))
            .unwrap()
            .into_iter()
            .find(|candidate| {
                candidate.parent() == &ui_root
                    && matches!(
                        &candidate.placement().anchor,
                        nara::scene::ScenePlacementAnchor::After { sibling }
                            if sibling.as_str() == "modal-dim"
                    )
            })
            .expect("the created Image must have one explicit alternate order")
            .placement()
            .clone();
        frame
            .lower_structure_intent(&UiStructureIntent::Move {
                target: created.clone(),
                placement,
            })
            .expect("the exact alternate placement must lower through the Host frame")
    };
    assert!(matches!(
        dispatch(
            &mut editor,
            &created_frame,
            EditorAction::Mutate(move_mutation),
        ),
        EditorActionOutcome::Workspace(report)
            if matches!(
                &report.prefab_composition_outcome,
                Some(PrefabCompositionOutcome::Moved { entity }) if entity == &created
            )
    ));
    let moved = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        ui_child_order(&editor, &ui_root),
        vec![
            SceneEntityId::new("modal-dim").unwrap(),
            created.clone(),
            SceneEntityId::new("terminal-panel").unwrap(),
        ]
    );

    let delete_frame = editor.current_ui_authoring_frame().unwrap();
    let delete_mutation = delete_frame
        .lower_structure_intent(&UiStructureIntent::Delete {
            target: SceneEntityId::new("modal-dim").unwrap(),
        })
        .expect("the migrated modal must be deletable as a source-local subtree");
    assert!(matches!(
        dispatch(
            &mut editor,
            &moved,
            EditorAction::Mutate(delete_mutation),
        ),
        EditorActionOutcome::Workspace(report)
            if matches!(
                &report.prefab_composition_outcome,
                Some(PrefabCompositionOutcome::Removed { entity, .. })
                    if entity.as_str() == "modal-dim"
            )
    ));
    let structure_deleted = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        ui_child_order(&editor, &ui_root),
        vec![
            created.clone(),
            SceneEntityId::new("terminal-panel").unwrap()
        ]
    );

    let action_ui = editor.current_ui_authoring_frame().unwrap();
    let action_descriptor = action_ui
        .projection()
        .unwrap()
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == &retry
                && field.key().primary_field() == &ComponentFieldId::new("action")
        })
        .expect("the Retry action must be a typed UI authoring field");
    let action_edit = action_ui
        .lower_field_edit(
            action_descriptor.key(),
            UiAuthoringFieldValue::Action(EDITED_RETRY_ACTION.to_owned()),
        )
        .expect("the typed Retry action must lower through its descriptor");
    assert!(matches!(
        dispatch(
            &mut editor,
            &structure_deleted,
            EditorAction::Mutate(EditorDocumentMutation::ApplyPrefabFieldEdit {
                edit: action_edit,
            }),
        ),
        EditorActionOutcome::Workspace(report) if report.applied
    ));

    let action_frame = editor.drive_editor_frame(Duration::ZERO);
    let action_ui = editor.current_ui_authoring_frame().unwrap();
    let baseline = action_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: retry.clone(),
            delta: [0.0, 0.0],
        })
        .unwrap();
    let history_before_gesture = action_frame
        .workspace()
        .document(prefab_document)
        .unwrap()
        .history;
    let began = dispatch(
        &mut editor,
        &action_frame,
        EditorAction::Gesture(EditorGestureAction::Begin { baseline }),
    );
    let EditorActionOutcome::Gesture(EditorGestureOutcome::Began(gesture)) = began else {
        panic!("the exact UI width baseline must begin one gesture")
    };
    let gesture_id = gesture.stamp().id();

    let preview_frame = editor.drive_editor_frame(Duration::ZERO);
    let preview_ui = editor.current_ui_authoring_frame().unwrap();
    let preview = preview_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: retry.clone(),
            delta: [EDITED_RETRY_WIDTH - 8.0 - initial_authored_width, 0.0],
        })
        .unwrap();
    assert!(matches!(
        dispatch(
            &mut editor,
            &preview_frame,
            EditorAction::Gesture(EditorGestureAction::Preview {
                gesture: gesture_id,
                edit: preview,
            }),
        ),
        EditorActionOutcome::Gesture(EditorGestureOutcome::Previewed(_))
    ));
    assert_eq!(
        editor
            .current_frame()
            .workspace()
            .document(prefab_document)
            .unwrap()
            .history,
        history_before_gesture
    );

    let commit_frame = editor.drive_editor_frame(Duration::ZERO);
    let commit_ui = editor.current_ui_authoring_frame().unwrap();
    let commit = commit_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: retry.clone(),
            delta: [8.0, 0.0],
        })
        .unwrap();
    assert!(matches!(
        dispatch(
            &mut editor,
            &commit_frame,
            EditorAction::Gesture(EditorGestureAction::Commit {
                gesture: gesture_id,
                edit: commit,
            }),
        ),
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let committed_frame = editor.drive_editor_frame(Duration::ZERO);
    let committed_tab = committed_frame
        .workspace()
        .document(prefab_document)
        .unwrap();
    assert_eq!(
        committed_tab.history.undo_depth,
        history_before_gesture.undo_depth + 1
    );
    assert!(committed_frame.play().is_out_of_date());
    assert_eq!(retry_authored_width(&editor, &retry), EDITED_RETRY_WIDTH);

    assert!(matches!(
        dispatch(
            &mut editor,
            &committed_frame,
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document: prefab_document,
                base_revision: committed_tab.revision,
            }),
        ),
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let undone = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        retry_authored_width(&editor, &retry),
        initial_authored_width
    );
    assert!(matches!(
        dispatch(
            &mut editor,
            &undone,
            EditorAction::Mutate(EditorDocumentMutation::Redo {
                document: prefab_document,
                base_revision: undone.workspace().document(prefab_document).unwrap().revision,
            }),
        ),
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let redone = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(retry_authored_width(&editor, &retry), EDITED_RETRY_WIDTH);

    let saved_revision = redone
        .workspace()
        .document(prefab_document)
        .unwrap()
        .revision;
    assert!(matches!(
        dispatch(
            &mut editor,
            &redone,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document: prefab_document,
                base_revision: saved_revision,
            }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    let saved = editor_persistence_driver::drive_until_result(&mut editor);
    let saved_digest = match saved.persistence().result() {
        Some(EditorPersistenceResult::Saved {
            document,
            revision,
            digest,
        }) if document == prefab_document && revision == saved_revision => digest,
        other => panic!("expected exact Prefab Save receipt, got {other:?}"),
    };
    assert!(matches!(
        saved.project_content(),
        EditorProjectContentView::RefreshPending { document, .. }
            if document == prefab_document
    ));
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );

    assert!(matches!(
        dispatch(
            &mut editor,
            &saved,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                attempt: saved.persistence().attempt().unwrap(),
            }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { .. })
    ));
    let refreshed = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        refreshed.project_content(),
        EditorProjectContentView::Current { generation }
            if generation > initial_content_generation
    ));
    assert!(refreshed.play().is_out_of_date());
    let refreshed_ui = editor
        .current_ui_authoring_frame()
        .expect("content refresh must republish the active UI source");
    assert_eq!(
        refreshed_ui
            .image_choices()
            .unwrap()
            .choices()
            .iter()
            .filter(|choice| choice.project_path() == "textures/player.png")
            .count(),
        1
    );

    assert!(matches!(
        dispatch(
            &mut editor,
            &refreshed,
            EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: refreshed.reopen_target(),
            }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    let reopened = editor_persistence_driver::drive_until_result(&mut editor);
    assert!(matches!(
        reopened.persistence().result(),
        Some(EditorPersistenceResult::Opened { document, digest, .. })
            if document == prefab_document && digest == saved_digest
    ));
    assert!(reopened.project_content().is_current());
    assert!(
        !reopened
            .workspace()
            .document(prefab_document)
            .unwrap()
            .dirty
    );
    let reopened_ui = editor
        .current_ui_authoring_frame()
        .expect("the reopened Prefab must publish its saved structure");
    let reopened_projection = reopened_ui.projection().unwrap();
    assert!(reopened_projection.hierarchy().iter().any(|node| {
        node.entity() == &created && node.kind() == UiAuthoringHierarchyKind::Image
    }));
    assert!(
        !reopened_projection
            .hierarchy()
            .iter()
            .any(|node| node.entity().as_str() == "modal-dim")
    );
    assert_eq!(
        ui_child_order(&editor, &ui_root),
        vec![
            created.clone(),
            SceneEntityId::new("terminal-panel").unwrap()
        ]
    );
    let image_brush = reopened_projection
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == &created
                && field.key().primary_field().as_str() == "background.kind"
        })
        .expect("the reopened Image must retain one semantic brush descriptor");
    assert!(matches!(
        image_brush.value(),
        UiAuthoringFieldValue::Brush {
            kind: UiBrushAuthoringKind::Image,
            reference,
            ..
        } if reference == "textures/player.png"
    ));
    assert!(matches!(
        dispatch(
            &mut editor,
            &reopened,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                attempt: reopened.persistence().attempt().unwrap(),
            }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { .. })
    ));
    let restart_frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        dispatch(
            &mut editor,
            &restart_frame,
            EditorAction::Play(EditorPlayAction::Restart),
        ),
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    ));
    let _ = drive_until_play(&mut editor, EditorPlayState::Running);
    let restarted_runtime = receive_runtime_ui(&mut editor, &receiver);
    let restarted = editor.current_frame();
    assert_eq!(
        restarted_runtime.retry_source_document,
        "reference-ui.prefab.json"
    );
    assert_eq!(restarted_runtime.retry_source_entity, "retry-button");
    assert_eq!(restarted_runtime.retry_action, EDITED_RETRY_ACTION);
    assert_eq!(restarted_runtime.retry_name, initial_runtime.retry_name);
    assert_eq!(
        f64::from(restarted_runtime.retry_authored_width),
        EDITED_RETRY_WIDTH
    );
    assert!(!restarted.play().is_out_of_date());

    assert!(matches!(
        dispatch(
            &mut editor,
            &restarted,
            EditorAction::Play(EditorPlayAction::Stop),
        ),
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    ));
    let _ = drive_until_play(&mut editor, EditorPlayState::Empty);
    let sender = UI_REFRESH_PROBE_SENDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    assert!(sender.is_some());
    assert_eq!(
        UI_REFRESH_EXPECTED_SOURCE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take(),
        Some(created)
    );
}

#[derive(Debug, Default)]
struct EditorUiRefreshProbePlugin;

impl Plugin for EditorUiRefreshProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &UI_REFRESH_PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let sender = UI_REFRESH_PROBE_SENDER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .expect("the Editor UI refresh test installs its observation channel before Play")
            .clone();
        app.insert_resource(EditorUiRefreshProbe {
            sender,
            reported: false,
        })?
        .add_systems(CoreStage::Cleanup, observe_runtime_ui)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct EditorUiRefreshProbe {
    sender: SyncSender<ReferenceUiObservation>,
    reported: bool,
}

fn observe_runtime_ui(world: &mut World) {
    if world.resource::<EditorUiRefreshProbe>().reported {
        return;
    }
    let expected_source = UI_REFRESH_EXPECTED_SOURCE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if expected_source.is_some_and(|expected| !runtime_structure_is_current(world, &expected)) {
        return;
    }
    let Some(observation) = ReferenceUiObservation::from_authoring_world(world) else {
        return;
    };
    let mut probe = world.resource_mut::<EditorUiRefreshProbe>();
    probe
        .sender
        .try_send(observation)
        .expect("the bounded UI refresh observation channel must have capacity");
    probe.reported = true;
}

fn runtime_structure_is_current(world: &World, expected: &SceneEntityId) -> bool {
    let Some(created) = world.iter_entities().find(|entity| {
        entity.get::<SceneEntitySource>().is_some_and(|source| {
            source.provenance().local_entity_id() == expected
                && source
                    .provenance()
                    .projection_path()
                    .and_then(|path| path.steps().last())
                    .is_some_and(|step| {
                        matches!(step, PrefabProjectionStep::SourceEntity { source_document, .. }
                        if source_document.as_path().is_some_and(|path| {
                            path.as_str() == "reference-ui.prefab.json"
                        }))
                    })
        }) && entity.contains::<UiNode>()
            && entity.contains::<UiVisual>()
    }) else {
        return false;
    };
    let expected_visual = UiVisual::image(
        UiImageId::new("textures/player.png").unwrap(),
        UiColor::rgba(1.0, 1.0, 1.0, 1.0),
    );
    if created.get::<UiVisual>() != Some(&expected_visual) {
        return false;
    }
    let Some(parent_id) = created.get::<Parent>().map(Parent::parent) else {
        return false;
    };
    let Ok(parent) = world.get_entity(parent_id) else {
        return false;
    };
    if !parent
        .get::<SceneEntitySource>()
        .is_some_and(|source| source.provenance().local_entity_id().as_str() == "terminal-overlay")
    {
        return false;
    }
    let Some(children) = parent.get::<Children>() else {
        return false;
    };
    let child_sources = children
        .iter()
        .filter_map(|child| {
            world
                .get_entity(child)
                .ok()?
                .get::<SceneEntitySource>()
                .map(|source| source.provenance().local_entity_id().clone())
        })
        .collect::<Vec<_>>();
    if child_sources
        != [
            expected.clone(),
            SceneEntityId::new("terminal-panel").unwrap(),
        ]
        || world.iter_entities().any(|entity| {
            entity
                .get::<SceneEntitySource>()
                .is_some_and(|source| source.provenance().local_entity_id().as_str() == "modal-dim")
        })
    {
        return false;
    }
    world
        .get_resource::<Assets<ImageAsset>>()
        .is_some_and(|images| {
            images.iter().any(|(_, image)| {
                image
                    .source()
                    .is_some_and(|source| source.path().as_str() == "textures/player.png")
            })
        })
}

fn dispatch(
    editor: &mut EditorProjectSession,
    frame: &nara::tooling::EditorFrameModel,
    action: EditorAction,
) -> EditorActionOutcome {
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), [action]))
        .expect("the product trajectory dispatches only current Editor frames");
    let mut outcomes = report.into_outcomes();
    assert_eq!(outcomes.len(), 1);
    outcomes.pop().unwrap()
}

fn drive_until_play(
    editor: &mut EditorProjectSession,
    expected: EditorPlayState,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        let frame = editor.current_frame();
        if frame.play().state() == expected {
            return frame;
        }
        editor.drive_editor_frame(Duration::ZERO);
    }
    let frame = editor.current_frame();
    panic!(
        "expected Play state {expected:?}, got {:?}: {:?}",
        frame.play().state(),
        frame.diagnostics()
    );
}

fn receive_runtime_ui(
    editor: &mut EditorProjectSession,
    receiver: &Receiver<ReferenceUiObservation>,
) -> ReferenceUiObservation {
    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        if let Ok(observation) = receiver.try_recv() {
            return observation;
        }
        editor.drive_editor_frame(EDITOR_FRAME);
    }
    panic!("the real Runtime UI product did not publish its authored observation")
}

fn retry_authored_width(editor: &EditorProjectSession, retry: &SceneEntityId) -> f64 {
    let frame = editor
        .current_ui_authoring_frame()
        .expect("the UI Prefab remains the active authoring source");
    let field = frame
        .projection()
        .unwrap()
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == retry
                && field.key().primary_field().as_str() == "style.width.kind"
        })
        .expect("the Retry node must retain its authored width descriptor");
    match field.value() {
        UiAuthoringFieldValue::Dimension { value, .. } => *value,
        other => panic!("expected a dimension width, got {other:?}"),
    }
}

fn seed_legacy_modal_image(project: &ReferenceProjectCopy) {
    let loaded = try_load_project_content_from_path(project.path())
        .expect("the temporary reference project must load before the source edit");
    assert!(
        !loaded
            .snapshot
            .images()
            .iter()
            .any(|image| image.path().as_str() == "textures/player.png")
    );
    let source = loaded
        .snapshot
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "reference-ui.prefab.json")
        .expect("the reference UI Prefab must be available for the source edit");
    let mut prefab = source.document().clone();
    let visual_id = ComponentTypeId::new("nara.ui.UiVisual");
    let modal = prefab
        .entities
        .iter_mut()
        .find(|entity| entity.id.as_str() == "modal-dim")
        .expect("the reference modal source entity must exist");
    let visual = modal
        .components
        .get_mut(&visual_id)
        .expect("the reference modal must own a visual component");
    let border_width = visual
        .value
        .get("border_width")
        .cloned()
        .expect("the visual border width must be present");
    let radii = visual
        .value
        .get("radii")
        .cloned()
        .expect("the visual radii must be present");
    visual.version = ComponentSchemaVersion::ONE;
    visual.value = ComponentValue::map([
        (
            "background",
            legacy_brush("image", [0.0, 0.0, 0.0, 0.65], "textures/player.png"),
        ),
        ("border_width", border_width),
        (
            "border_brush",
            legacy_brush("solid", [0.0, 0.0, 0.0, 0.0], ""),
        ),
        ("radii", radii),
    ]);
    let encoded = PrefabDocument::new(prefab.roots, prefab.entities)
        .to_json_string()
        .expect("the legacy source edit must use the canonical Prefab writer");
    fs::write(
        project.file("prefabs/reference-ui.prefab.json"),
        format!("{encoded}\n"),
    )
    .expect("the guarded source copy must accept the canonical Prefab edit");
}

fn legacy_brush(kind: &str, color: [f64; 4], reference: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_owned())),
        (
            "red",
            ComponentValue::f64(color[0]).expect("legacy red channel must be finite"),
        ),
        (
            "green",
            ComponentValue::f64(color[1]).expect("legacy green channel must be finite"),
        ),
        (
            "blue",
            ComponentValue::f64(color[2]).expect("legacy blue channel must be finite"),
        ),
        (
            "alpha",
            ComponentValue::f64(color[3]).expect("legacy alpha channel must be finite"),
        ),
        ("reference", ComponentValue::String(reference.to_owned())),
    ])
}

fn ui_child_order(editor: &EditorProjectSession, parent: &SceneEntityId) -> Vec<SceneEntityId> {
    editor
        .current_ui_authoring_frame()
        .expect("the UI Prefab remains the active authoring source")
        .projection()
        .unwrap()
        .hierarchy()
        .iter()
        .filter(|node| node.parent() == Some(parent))
        .map(|node| node.entity().clone())
        .collect()
}
