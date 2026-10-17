#![cfg(feature = "editor")]

#[path = "support/reference_project_copy.rs"]
mod reference_project_copy;

use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginDefinitionId, PluginError, PluginId,
    },
    input::{ButtonDriverInput, KeyCode},
    project_host::{
        EditorGameInputBatch, EditorGameInputEnvelope, EditorGameInputEvent,
        EditorGameViewBridgeFailure, EditorGameViewCandidate, EditorGameViewCandidateIdentity,
        EditorGameViewCompletion, EditorProjectSession,
        advanced::{RuntimeProbe, open_trusted_compiled_editor_with_probe},
    },
    scene::{
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorCloseDecision,
        EditorDocumentMutation, EditorPlayAction, EditorPlayRejection, EditorPlayRequestResult,
        EditorPlayState, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
        EditorWorkspaceIntentPhase,
    },
};
use nara_reference_game::wave_game_view_intent;
use reference_project_copy::ReferenceProjectCopy;

#[derive(Debug)]
struct FixedTickProbe(Arc<AtomicU64>);

impl Plugin for FixedTickProbe {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("reference-game.test.editor-input-fixed-ticks"),
            PluginCategory::Tooling,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let ticks = Arc::clone(&self.0);
        app.add_systems(CoreStage::FixedUpdate, move || {
            ticks.fetch_add(1, Ordering::Relaxed);
        })?;
        Ok(())
    }
}

fn play(editor: &mut EditorProjectSession, action: EditorPlayAction) {
    let revision = editor.current_frame().revision();
    editor
        .dispatch_editor_actions(EditorActionBatch::new(
            revision,
            [EditorAction::Play(action)],
        ))
        .unwrap();
}

fn start(editor: &mut EditorProjectSession) -> EditorGameViewCandidate {
    let scene = editor
        .current_frame()
        .workspace()
        .active_scene
        .clone()
        .unwrap();
    play(
        editor,
        EditorPlayAction::Play {
            document: scene.document,
            base_revision: scene.revision,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "Game start did not complete");
        editor.drive_editor_frame(Duration::ZERO);
        if let Some(request) = editor.pending_game_start_request() {
            let activation = editor.prepare_game_start_activation(request).unwrap();
            editor.commit_game_start_activation(activation);
        }
        if editor.current_frame().play().state() == EditorPlayState::Running
            && let Some(candidate) = editor.take_game_view_candidate().unwrap()
        {
            return candidate;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

// This CPU contract fixture completes exact UI permits through a fake backend. Native GPU
// presentation is verified separately through the ordinary native Editor.
fn complete_fake_presentation(
    editor: &mut EditorProjectSession,
    candidate: EditorGameViewCandidate,
) {
    let identity = candidate.identity();
    let mut parts = candidate.into_parts();
    let ui = parts
        .take_ui_frame()
        .expect("the reference Game includes Runtime UI");
    assert!(ui.has_packets());
    let submissions = nara_ui_render::__private::bind_backend_submission_batch(
        ui.into_parts().1,
        nara_ui_render::UiPresentationBackendGeneration::new(77, 1).unwrap(),
    )
    .unwrap();
    editor
        .complete_game_view_candidate(EditorGameViewCompletion::with_ui(
            identity,
            submissions
                .into_iter()
                .map(|submission| submission.presented())
                .collect(),
        ))
        .unwrap();
}

fn input(
    identity: EditorGameViewCandidateIdentity,
    events: impl IntoIterator<Item = EditorGameInputEvent>,
) -> EditorGameInputBatch {
    EditorGameInputBatch::new(
        events
            .into_iter()
            .map(|event| EditorGameInputEnvelope::new(identity, 1, event)),
    )
}

#[test]
fn native_focus_release_interrupts_buffered_ui_without_stealing_host_authority() {
    let project = ReferenceProjectCopy::new("editor-buffered-input");
    let mut editor =
        EditorProjectSession::open_trusted_compiled(project.capability(), wave_game_view_intent())
            .unwrap();
    let first = start(&mut editor);
    let identity = first.identity();
    complete_fake_presentation(&mut editor, first);
    editor.drive_editor_frame(Duration::ZERO);
    editor
        .apply_game_view_input_batch(input(
            identity,
            [
                EditorGameInputEvent::Focused(true),
                EditorGameInputEvent::Keyboard(ButtonDriverInput::Press(KeyCode::Character('d'))),
            ],
        ))
        .unwrap();
    editor
        .apply_game_view_input_batch(input(
            identity,
            [
                EditorGameInputEvent::Keyboard(ButtonDriverInput::Release(KeyCode::Character('d'))),
                EditorGameInputEvent::Focused(false),
            ],
        ))
        .expect("native focus release must retire the buffered presentation reservation");
    assert!(editor.take_game_view_candidate().unwrap().is_none());

    editor.drive_editor_frame(Duration::ZERO);
    let in_flight = editor.take_game_view_candidate().unwrap().unwrap();
    assert!(matches!(
        editor.apply_game_view_input_batch(input(identity, [EditorGameInputEvent::Focused(false)])),
        Err(EditorGameViewBridgeFailure::CandidateBusy)
    ));
    complete_fake_presentation(&mut editor, in_flight);
    editor.drive_editor_frame(Duration::ZERO);
    editor
        .apply_game_view_input_batch(input(identity, [EditorGameInputEvent::SurfaceRetired]))
        .expect("physical endpoint retirement also releases buffered UI authority");
    assert!(editor.take_game_view_candidate().unwrap().is_none());
    play(&mut editor, EditorPlayAction::Stop);
    editor.drive_editor_frame(Duration::ZERO);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
}

#[test]
fn focus_release_during_step_retries_presentation_instead_of_failing_the_step() {
    let project = ReferenceProjectCopy::new("editor-step-input");
    let ticks = Arc::new(AtomicU64::new(0));
    let probe_ticks = Arc::clone(&ticks);
    let probe = RuntimeProbe::new(PluginDefinition::infallible::<FixedTickProbe, _>(
        PluginDefinitionId::new("reference-game.test.editor-input-fixed-ticks", 1),
        Vec::new(),
        move || FixedTickProbe(Arc::clone(&probe_ticks)),
    ));
    let mut editor = open_trusted_compiled_editor_with_probe(
        project.capability(),
        wave_game_view_intent(),
        probe,
    )
    .unwrap();
    let first = start(&mut editor);
    let identity = first.identity();
    complete_fake_presentation(&mut editor, first);
    play(&mut editor, EditorPlayAction::Pause);
    editor.drive_editor_frame(Duration::ZERO);
    if let Some(candidate) = editor.take_game_view_candidate().unwrap() {
        complete_fake_presentation(&mut editor, candidate);
    }
    let before_step = ticks.load(Ordering::Relaxed);
    play(&mut editor, EditorPlayAction::StepFixedTick);
    editor.drive_editor_frame(Duration::ZERO);
    editor.drive_editor_frame(Duration::ZERO);
    assert!(editor.game_step_requires_runtime_drive());
    editor
        .apply_game_view_input_batch(input(identity, [EditorGameInputEvent::Focused(false)]))
        .expect("focus release must abort only the unsubmitted presentation");
    assert!(editor.game_step_requires_runtime_drive());
    editor.drive_editor_frame(Duration::ZERO);
    let retry = editor.take_game_view_candidate().unwrap().unwrap();
    complete_fake_presentation(&mut editor, retry);
    editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );
    assert!(!editor.game_step_requires_runtime_drive());
    assert_eq!(
        ticks.load(Ordering::Relaxed),
        before_step + 1,
        "retrying presentation must not repeat the fixed update"
    );
    play(&mut editor, EditorPlayAction::Stop);
    editor.drive_editor_frame(Duration::ZERO);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
}

#[test]
fn gpu_recovery_pauses_runtime_without_resolving_dirty_exit() {
    let project = ReferenceProjectCopy::new("editor-recovery-dirty-exit");
    let source_path = project.file("scenes/startup.scene.json");
    let source_before = fs::read(&source_path).unwrap();
    let mut editor =
        EditorProjectSession::open_trusted_compiled(project.capability(), wave_game_view_intent())
            .unwrap();
    let first = start(&mut editor);
    complete_fake_presentation(&mut editor, first);
    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document: scene.document,
                    base_revision: scene.revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(
                            SceneEntityId::new("recovery-unsaved-entity").unwrap(),
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ))
        .unwrap();
    let dirty = editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    let dirty_scene = dirty.workspace().active_scene.as_ref().unwrap();
    assert!(dirty_scene.dirty);
    let document = dirty_scene.document;
    let revision = dirty_scene.revision;
    editor
        .dispatch_editor_actions(EditorActionBatch::new(
            dirty.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit),
            )],
        ))
        .unwrap();
    let awaiting = editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert_eq!(awaiting.play().state(), EditorPlayState::Running);
    assert_eq!(
        awaiting.workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    let ordinary_pause = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            awaiting.revision(),
            [EditorAction::Play(EditorPlayAction::Pause)],
        ))
        .unwrap();
    assert_eq!(
        ordinary_pause.outcomes(),
        &[EditorActionOutcome::Play(
            EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy,)
        )]
    );

    editor
        .set_game_view_source_active(false, "the Editor GPU owner was retired")
        .unwrap();
    assert_eq!(
        editor.request_runtime_pause_for_gpu_recovery(),
        EditorPlayRequestResult::Accepted
    );
    let recovered = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(recovered.play().state(), EditorPlayState::Paused);
    assert_eq!(
        recovered.workspace_intent().intent(),
        Some(EditorWorkspaceIntent::Exit)
    );
    assert_eq!(
        recovered.workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    let retained = recovered.workspace().active_scene.as_ref().unwrap();
    assert_eq!((retained.document, retained.revision), (document, revision));
    assert!(retained.dirty);
    assert_eq!(fs::read(source_path).unwrap(), source_before);
    let ordinary_resume = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            recovered.revision(),
            [EditorAction::Play(EditorPlayAction::Resume)],
        ))
        .unwrap();
    assert_eq!(
        ordinary_resume.outcomes(),
        &[EditorActionOutcome::Play(
            EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy,)
        )]
    );
    editor
        .dispatch_editor_actions(EditorActionBatch::new(
            recovered.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Resolve(EditorCloseDecision::Cancel),
            )],
        ))
        .unwrap();
    editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    play(&mut editor, EditorPlayAction::Stop);
    editor.drive_editor_frame(Duration::ZERO);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
}

#[test]
fn gpu_recovery_consumes_queued_step_once_without_waiting_for_a_retired_source() {
    let project = ReferenceProjectCopy::new("editor-recovery-queued-step");
    let ticks = Arc::new(AtomicU64::new(0));
    let probe_ticks = Arc::clone(&ticks);
    let probe = RuntimeProbe::new(PluginDefinition::infallible::<FixedTickProbe, _>(
        PluginDefinitionId::new("reference-game.test.editor-input-fixed-ticks", 1),
        Vec::new(),
        move || FixedTickProbe(Arc::clone(&probe_ticks)),
    ));
    let mut editor = open_trusted_compiled_editor_with_probe(
        project.capability(),
        wave_game_view_intent(),
        probe,
    )
    .unwrap();
    let first = start(&mut editor);
    complete_fake_presentation(&mut editor, first);
    play(&mut editor, EditorPlayAction::Pause);
    editor.drive_editor_frame(Duration::ZERO);
    if let Some(candidate) = editor.take_game_view_candidate().unwrap() {
        complete_fake_presentation(&mut editor, candidate);
    }
    let before_step = ticks.load(Ordering::Relaxed);
    play(&mut editor, EditorPlayAction::StepFixedTick);
    editor
        .set_game_view_source_active(false, "the Editor GPU owner was retired")
        .unwrap();
    assert_eq!(
        editor.request_runtime_pause_for_gpu_recovery(),
        EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy)
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(editor.game_step_requires_runtime_drive());
    assert_eq!(ticks.load(Ordering::Relaxed), before_step + 1);
    assert_eq!(
        editor.request_runtime_pause_for_gpu_recovery(),
        EditorPlayRequestResult::Accepted
    );
    let recovered = editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert_eq!(recovered.play().state(), EditorPlayState::Paused);
    assert!(!editor.game_step_requires_runtime_drive());
    assert!(editor.take_game_view_candidate().unwrap().is_none());
    assert_eq!(ticks.load(Ordering::Relaxed), before_step + 1);

    editor
        .set_game_view_source_active(true, "the Editor GPU owner was recreated")
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    let fresh = editor.take_game_view_candidate().unwrap().unwrap();
    complete_fake_presentation(&mut editor, fresh);
    assert_eq!(ticks.load(Ordering::Relaxed), before_step + 1);
    play(&mut editor, EditorPlayAction::Stop);
    editor.drive_editor_frame(Duration::ZERO);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
}
