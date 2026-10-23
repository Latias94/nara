use super::*;

use nara_app::{RuntimeControl, RuntimeControlRequestResult, RuntimeState};
use nara_ecs::Commands;
use nara_tooling::{EditorPlayCommand, EditorPlayRequestResult, EditorPlayState};

use super::super::editor::{
    EditorGameViewCandidate, EditorGameViewCompletion, EditorGameViewCompletionDisposition,
    EditorProjectSession, TrustedCompiledEditorProjectIntent,
};

const CAMERA_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("nara.test.game-view-outcomes"),
    PluginCategory::Tooling,
)
.requires_plugins(&[nara_render::RENDER_PLUGIN_ID]);

#[derive(Debug, Default)]
struct GameViewCameraPlugin;

impl Plugin for GameViewCameraPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CAMERA_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Scene, spawn_camera)?;
        Ok(())
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        nara_transform::Transform2d::default(),
        nara_render::Camera2d::default(),
    ));
}

fn open_game(project: &TestProject, with_ui: bool) -> EditorProjectSession {
    let capabilities = if with_ui {
        r#"["runtime-2d", "runtime-ui", "presentation-2d", "presentation-ui", "desktop-winit", "render-wgpu"]"#
    } else {
        r#"["runtime-2d", "presentation-2d", "desktop-winit", "render-wgpu"]"#
    };
    let path = project.root.join(PROJECT_MANIFEST);
    let mut manifest = fs::read_to_string(&path).unwrap();
    manifest.push_str(&format!(
        r#"
[profiles.desktop.capabilities]
requested = {capabilities}

[profiles.desktop.runtime]
preset = "local-headless"

[profiles.desktop.window]
enabled = true
width = 960
height = 540
resizable = true
"#
    ));
    if with_ui {
        manifest.push_str(
            "\n[profiles.desktop.ui.primary_surface]\nlogical_width = 960\nlogical_height = 540\n",
        );
    }
    fs::write(path, manifest).unwrap();
    let intent = TrustedCompiledEditorProjectIntent::new()
        .with_profile("desktop")
        .with_recipe(
            crate::ProductRecipe::new()
                .add_plugin::<GameViewCameraPlugin>()
                .unwrap(),
        );
    let mut editor =
        EditorProjectSession::open_trusted_compiled(project.capability(), intent).unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let request = loop {
        editor.drive_editor_frame(Duration::ZERO);
        if let Some(request) = editor.pending_game_start_request() {
            break request;
        }
        assert!(
            Instant::now() < deadline,
            "Game start did not reach target admission: {:?}",
            editor.play_view()
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    let activation = editor.prepare_game_start_activation(request).unwrap();
    editor.commit_game_start_activation(activation);
    editor
}

fn take_candidate(editor: &mut EditorProjectSession) -> EditorGameViewCandidate {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(editor.play_view().state(), EditorPlayState::Running);
        if let Some(candidate) = editor.take_game_view_candidate().unwrap() {
            return candidate;
        }
        assert!(
            Instant::now() < deadline,
            "Game did not publish a candidate"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn stop_game(editor: &mut EditorProjectSession) {
    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted,
        "a reconciled completion must not strand Stop behind CandidateBusy"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        if editor.play_view().state() == EditorPlayState::Empty {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Game retirement did not finish: {:?}",
            editor.play_view()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(editor.active_game_start_request().is_none());
    assert!(editor.play_view().generation().is_none());
    drive_editor_close_to_completion(editor);
}

// These negative fixtures deliberately retain the Session slot after using the lower owner.
// Retire that real Runtime directly for cleanup without fabricating a Session acknowledgement.
fn stop_lower_runtime(editor: &mut EditorProjectSession) {
    let runtime = editor.test_game_view_runtime();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.state() != RuntimeState::Stopped {
        let _ = runtime.drive(Duration::ZERO);
        assert!(Instant::now() < deadline, "the lower Runtime did not stop");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn real_completion_postfault_consumes_once_clears_session_and_allows_stop() {
    let project = TestProject::new("game-view-completion-postfault");
    let mut editor = open_game(&project, false);
    let candidate = take_candidate(&mut editor);
    let identity = candidate.identity();
    drop(candidate);
    assert_eq!(editor.test_game_view_in_flight(), Some(identity));
    assert!(!editor.test_game_view_bridge_is_idle());
    assert!(editor.test_game_view_runtime().fault().is_none());

    let _fault = editor.test_fault_after_game_view_completion();
    editor
        .complete_game_view_candidate(EditorGameViewCompletion::without_ui(identity))
        .expect("the real port consumed this completion before the post-operation fault");
    let fault = RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.game-view-post-completion",
    );
    assert_eq!(editor.test_game_view_runtime().fault(), Some(&fault));
    assert_eq!(editor.play_view().state(), EditorPlayState::Faulted);
    assert_eq!(editor.test_game_view_in_flight(), None);
    assert!(editor.test_game_view_bridge_is_idle());

    let duplicate = editor
        .complete_game_view_candidate(EditorGameViewCompletion::without_ui(identity))
        .expect_err("the Session cannot consume the same completion twice");
    assert_eq!(duplicate.identity(), identity);
    assert_eq!(
        duplicate.disposition(),
        EditorGameViewCompletionDisposition::Presented
    );
    assert!(editor.test_game_view_bridge_is_idle());
    assert_eq!(editor.test_game_view_runtime().fault(), Some(&fault));
    stop_game(&mut editor);
}

#[test]
fn real_domain_rejection_postfault_returns_exact_completion_and_ui_outcomes() {
    let project = TestProject::new("game-view-domain-rejection-postfault");
    #[cfg(feature = "presentation-ui")]
    project.write_ui_authoring_documents(None, None);
    let mut editor = open_game(&project, cfg!(feature = "presentation-ui"));
    let completion = take_candidate(&mut editor).content_rejected("retain this exact UI outcome");
    let identity = completion.identity();
    #[cfg(feature = "presentation-ui")]
    let (allocation, outcome_count, outcome_debug) = {
        let outcomes = completion.test_ui_outcomes();
        assert!(
            !outcomes.is_empty(),
            "the real Game candidate must carry UI outcomes"
        );
        (outcomes.as_ptr(), outcomes.len(), format!("{outcomes:?}"))
    };

    // Consume the real bridge slot without acknowledging the still-owned Session completion.
    editor
        .test_complete_game_view_bridge(EditorGameViewCompletion::without_ui_deferred(identity))
        .unwrap();
    assert!(editor.test_game_view_bridge_is_idle());
    assert_eq!(editor.test_game_view_in_flight(), Some(identity));
    assert!(editor.test_game_view_runtime().fault().is_none());

    let _fault = editor.test_fault_after_game_view_completion();
    let returned = editor
        .complete_game_view_candidate(completion)
        .expect_err("the real completion port must reject its already-consumed bridge identity");
    assert_eq!(returned.identity(), identity);
    assert_eq!(
        returned.disposition(),
        EditorGameViewCompletionDisposition::ContentRejected
    );
    #[cfg(feature = "presentation-ui")]
    {
        assert_eq!(returned.test_ui_outcomes().as_ptr(), allocation);
        assert_eq!(returned.test_ui_outcomes().len(), outcome_count);
        assert_eq!(format!("{:?}", returned.test_ui_outcomes()), outcome_debug);
    }
    assert_eq!(editor.test_game_view_in_flight(), Some(identity));
    assert!(editor.test_game_view_bridge_is_idle());
    assert_eq!(
        editor.test_game_view_runtime().fault(),
        Some(&RuntimeFault::engine(
            RuntimeFaultKind::RequiredService,
            "nara.test.game-view-post-completion",
        ))
    );
    drop(returned);
    stop_lower_runtime(&mut editor);
    drive_editor_close_to_completion(&mut editor);
}

#[test]
fn stopped_scope_entry_returns_uninvoked_completion_and_ui_outcomes() {
    let project = TestProject::new("game-view-completion-scope-rejection");
    #[cfg(feature = "presentation-ui")]
    project.write_ui_authoring_documents(None, None);
    let mut editor = open_game(&project, cfg!(feature = "presentation-ui"));
    let completion =
        take_candidate(&mut editor).content_rejected("uninvoked scope retains custody");
    let identity = completion.identity();
    #[cfg(feature = "presentation-ui")]
    let (allocation, outcome_count, outcome_debug) = {
        let outcomes = completion.test_ui_outcomes();
        assert!(!outcomes.is_empty());
        (outcomes.as_ptr(), outcomes.len(), format!("{outcomes:?}"))
    };
    stop_lower_runtime(&mut editor);
    assert!(!editor.test_game_view_bridge_is_idle());
    assert!(editor.test_game_view_runtime().fault().is_none());

    let _fault = editor.test_fault_after_game_view_completion();
    let returned = editor
        .complete_game_view_candidate(completion)
        .expect_err("a stopped Runtime must return the uninvoked owned completion");
    assert_eq!(returned.identity(), identity);
    assert_eq!(
        returned.disposition(),
        EditorGameViewCompletionDisposition::ContentRejected
    );
    #[cfg(feature = "presentation-ui")]
    {
        assert_eq!(returned.test_ui_outcomes().as_ptr(), allocation);
        assert_eq!(returned.test_ui_outcomes().len(), outcome_count);
        assert_eq!(format!("{:?}", returned.test_ui_outcomes()), outcome_debug);
    }
    assert_eq!(editor.test_game_view_in_flight(), Some(identity));
    assert!(!editor.test_game_view_bridge_is_idle());
    assert!(
        editor.test_game_view_runtime().fault().is_none(),
        "the post-port injection cannot run when scope entry rejects the input"
    );
    drop(returned);
    drive_editor_close_to_completion(&mut editor);
}
