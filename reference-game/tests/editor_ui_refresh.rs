#![cfg(feature = "editor-game-view-canary")]

#[path = "support/reference_project_copy.rs"]
mod reference_project_copy;

use std::{
    sync::{
        Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    prelude::{App, Resource, World},
    project_host::{EditorProjectIntent, EditorProjectSession},
    reflect::ComponentFieldId,
    scene::SceneEntityId,
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentKind,
        EditorDocumentMutation, EditorGestureAction, EditorGestureOutcome, EditorNavigationAction,
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorPlayAction, EditorPlayRequestResult, EditorPlayState, EditorProjectContentView,
    },
    ui::__private::{UiAuthoringCanvasProjection, UiAuthoringDirectEdit, UiAuthoringFieldValue},
};
use nara_reference_game::{REFERENCE_RUNTIME_UI_PLUGIN_ID, ReferenceUiObservation, wave_recipe};
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

#[test]
fn saved_ui_prefab_refreshes_project_content_and_explicit_restart() {
    let project = ReferenceProjectCopy::new("editor-ui-refresh");
    let (sender, receiver) = mpsc::sync_channel(4);
    let replaced = UI_REFRESH_PROBE_SENDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace(sender);
    assert!(replaced.is_none());

    let recipe = wave_recipe()
        .and_then(|recipe| recipe.add_plugin::<EditorUiRefreshProbePlugin>())
        .expect("the UI refresh product recipe must be statically valid");
    let mut editor = EditorProjectSession::open(
        project.capability(),
        EditorProjectIntent::new().with_recipe(recipe),
    )
    .expect("the guarded reference project must open in the Editor Host");
    let opened = editor.current_frame();
    let scene = opened
        .workspace()
        .active_scene
        .as_ref()
        .expect("the startup Scene must be active");
    let scene_document = scene.document;
    let initial_content_generation = opened.project_content().current_generation();

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
    let initial_authored_width = retry_authored_width(&editor, &retry);

    let action_descriptor = projection
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == &retry
                && field.key().primary_field() == &ComponentFieldId::new("action")
        })
        .expect("the Retry action must be a typed UI authoring field");
    let action_edit = ui_frame
        .lower_field_edit(
            action_descriptor.key(),
            UiAuthoringFieldValue::Action(EDITED_RETRY_ACTION.to_owned()),
        )
        .expect("the typed Retry action must lower through its descriptor");
    assert!(matches!(
        dispatch(
            &mut editor,
            &prefab_frame,
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
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
    ));
    let saved = editor.drive_editor_frame(Duration::ZERO);
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
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
    ));
    let refreshed = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        refreshed.project_content(),
        EditorProjectContentView::Current { generation }
            if generation > initial_content_generation
    ));
    assert!(refreshed.play().is_out_of_date());

    assert!(matches!(
        dispatch(
            &mut editor,
            &refreshed,
            EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: refreshed.reopen_target(),
            }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
    ));
    let reopened = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        reopened.persistence().result(),
        Some(EditorPersistenceResult::Opened { document, digest, .. })
            if document == prefab_document && digest == saved_digest
    ));
    assert!(reopened.project_content().is_current());
    assert!(matches!(
        dispatch(
            &mut editor,
            &reopened,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
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
