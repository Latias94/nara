use super::*;

use nara_app::{CoreStage, RuntimeControl, RuntimeControlRequestResult, RuntimeState};
use nara_ecs::Commands;
use nara_tooling::{EditorPlayCommand, EditorPlayRequestResult, EditorPlayState};

use super::super::editor::{
    EditorGameStartRequest, EditorGameViewBridgeFailure, EditorGameViewCandidate,
    EditorGameViewCompletion, EditorGameViewCompletionDisposition, EditorProjectCloseProgress,
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
    let (mut editor, request) =
        open_game_awaiting_target(project, with_ui, crate::ProductRecipe::new());
    let activation = editor.prepare_game_start_activation(request).unwrap();
    editor.commit_game_start_activation(activation);
    editor
}

fn open_game_awaiting_target(
    project: &TestProject,
    with_ui: bool,
    recipe: crate::ProductRecipe,
) -> (EditorProjectSession, EditorGameStartRequest) {
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
        .with_recipe(recipe.add_plugin::<GameViewCameraPlugin>().unwrap());
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
    (editor, request)
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

#[derive(Clone, Default, Resource)]
struct GameViewLifetimeProbe {
    frames: Arc<AtomicUsize>,
    closes: Arc<AtomicUsize>,
}

impl crate::ProductConfiguration for GameViewLifetimeProbe {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"game-view-close-lifetime-v1");
    }
}

struct GameViewLifetimePlugin(GameViewLifetimeProbe);

impl Plugin for GameViewLifetimePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.game-view-close-lifetime"),
            PluginCategory::Runtime,
        )
        .shutdown_obligations(&[CLOSE_PROBE_OBLIGATION]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?
            .add_systems(CoreStage::Update, count_game_view_frames)?;
        app.register_plugin_runtime_close_participant(
            CLOSE_PROBE_OBLIGATION,
            CLOSE_PROBE_PARTICIPANT,
            CloseProbeParticipant {
                closes: Arc::clone(&self.0.closes),
            },
        )?;
        Ok(())
    }
}

fn count_game_view_frames(probe: Res<GameViewLifetimeProbe>) {
    probe.frames.fetch_add(1, Ordering::SeqCst);
}

fn game_view_lifetime_recipe(probe: &GameViewLifetimeProbe) -> crate::ProductRecipe {
    crate::ProductRecipe::new()
        .add_configured_plugin(probe.clone(), |probe: &GameViewLifetimeProbe| {
            GameViewLifetimePlugin(probe.clone())
        })
        .unwrap()
}

// Reconcile any unexpectedly admitted responsibility before reporting an admission failure.
fn observe_closing_game_admission(
    editor: &mut EditorProjectSession,
    request: EditorGameStartRequest,
) -> (bool, bool) {
    let activation_refused = match editor.prepare_game_start_activation(request) {
        Err(EditorGameViewBridgeFailure::RuntimeUnavailable) => true,
        Err(_) => false,
        Ok(activation) => {
            editor.commit_game_start_activation(activation);
            false
        }
    };
    let capture_refused = match editor.take_game_view_candidate() {
        Err(EditorGameViewBridgeFailure::RuntimeUnavailable) => true,
        Err(_) | Ok(None) => false,
        Ok(Some(candidate)) => {
            if editor
                .complete_game_view_candidate(candidate.content_rejected("Session is closing"))
                .is_err()
            {
                stop_lower_runtime(editor);
            }
            false
        }
    };
    (activation_refused, capture_refused)
}

#[test]
fn close_awaiting_game_target_retires_prepared_owner_without_activation_or_capture() {
    let project = TestProject::new("game-view-close-awaiting-target");
    let probe = GameViewLifetimeProbe::default();
    let (mut editor, request) =
        open_game_awaiting_target(&project, false, game_view_lifetime_recipe(&probe));
    let published = editor.current_frame();

    let _ = editor.advance_close();
    let admission = observe_closing_game_admission(&mut editor, request);
    let completed = drive_editor_close_to_completion(&mut editor);

    assert_eq!(admission, (true, true));
    assert_eq!(probe.frames.load(Ordering::SeqCst), 0);
    assert_eq!(probe.closes.load(Ordering::SeqCst), 1);
    assert!(editor.pending_game_start_request().is_none());
    assert!(editor.active_game_start_request().is_none());
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert!(editor.play_view().generation().is_none());
    assert!(Arc::ptr_eq(&published, &editor.current_frame()));
    assert!(matches!(completed, EditorProjectCloseProgress::Complete(_)));
    assert!(!completed.task_shutdown().has_cleanup_failure());
    assert_eq!(editor.advance_close(), completed);
    assert_eq!(probe.closes.load(Ordering::SeqCst), 1);
}

#[test]
fn close_retains_accepted_game_activation_until_matching_commit_then_retires_without_frame() {
    let project = TestProject::new("game-view-close-accepted-activation");
    let probe = GameViewLifetimeProbe::default();
    let (mut editor, request) =
        open_game_awaiting_target(&project, false, game_view_lifetime_recipe(&probe));
    let activation = editor.prepare_game_start_activation(request).unwrap();
    let published = editor.current_frame();

    // Keep assertions after the caller returns its accepted activation to the Session.
    let first = editor.advance_close();
    let second = editor.advance_close();
    let closes_before_commit = probe.closes.load(Ordering::SeqCst);
    let commit = catch_unwind(AssertUnwindSafe(|| {
        editor.commit_game_start_activation(activation);
    }));
    let committed = editor.active_game_start_request();
    let generation = editor.play_view().generation();
    let bridge_idle = committed.is_some() && editor.test_game_view_bridge_is_idle();
    let admission = observe_closing_game_admission(&mut editor, request);
    let _ = editor.advance_close();
    let active_after_next_close = editor.active_game_start_request();
    let completed = drive_editor_close_to_completion(&mut editor);
    let active_at_completion = editor.active_game_start_request();
    if active_at_completion.is_some() {
        // A false Complete must not leave a live Runtime to quarantine during assertion unwind.
        stop_lower_runtime(&mut editor);
        drive_editor_close_to_completion(&mut editor);
    }

    for progress in [first, second] {
        assert!(
            matches!(
                progress,
                EditorProjectCloseProgress::Incomplete {
                    play_cleanup_pending: true,
                    ..
                }
            ),
            "Close must retain the caller's accepted activation: {progress:?}"
        );
        assert!(
            !progress.task_shutdown().is_complete(),
            "the accepted activation still owns unretired task facilities"
        );
    }
    assert_eq!(closes_before_commit, 0);
    assert!(
        commit.is_ok(),
        "Close must preserve the matching activation commit"
    );
    assert_eq!(committed, Some(request));
    assert_eq!(generation, Some(request.runtime_generation().get()));
    assert!(bridge_idle, "the commit must not capture a gameplay frame");
    assert_eq!(admission, (true, true));
    assert!(active_after_next_close.is_none());
    assert!(active_at_completion.is_none());
    assert_eq!(probe.frames.load(Ordering::SeqCst), 0);
    assert_eq!(probe.closes.load(Ordering::SeqCst), 1);
    assert!(editor.pending_game_start_request().is_none());
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert!(editor.play_view().generation().is_none());
    assert!(Arc::ptr_eq(&published, &editor.current_frame()));
    assert!(matches!(completed, EditorProjectCloseProgress::Complete(_)));
    assert_eq!(editor.advance_close(), completed);
    assert_eq!(probe.closes.load(Ordering::SeqCst), 1);
}

#[test]
fn close_waits_for_exact_in_flight_game_completion_and_refuses_fresh_work() {
    let project = TestProject::new("game-view-close-in-flight-candidate");
    let probe = GameViewLifetimeProbe::default();
    let (mut editor, request) =
        open_game_awaiting_target(&project, false, game_view_lifetime_recipe(&probe));
    let activation = editor.prepare_game_start_activation(request).unwrap();
    editor.commit_game_start_activation(activation);
    let previous = take_candidate(&mut editor);
    let stale_identity = previous.identity();
    editor
        .complete_game_view_candidate(previous.content_rejected("prepare the next real candidate"))
        .unwrap();
    let completion = take_candidate(&mut editor).content_rejected("return the held Game candidate");
    let identity = completion.identity();
    let frames_before_close = probe.frames.load(Ordering::SeqCst);
    let published = editor.current_frame();

    // Return the exact completion and drain the owner before asserting these observations.
    let first = editor.advance_close();
    let admission = observe_closing_game_admission(&mut editor, request);
    let stale = editor.complete_game_view_candidate(EditorGameViewCompletion::without_ui_deferred(
        stale_identity,
    ));
    let second = editor.advance_close();
    let retained = editor.test_game_view_in_flight();
    let active = editor.active_game_start_request();
    let closes_while_held = probe.closes.load(Ordering::SeqCst);
    let returned = editor.complete_game_view_candidate(completion);
    if returned.is_err()
        && editor.active_game_start_request().is_some()
        && editor.test_game_view_runtime().state() != RuntimeState::Stopped
    {
        stop_lower_runtime(&mut editor);
    }
    let in_flight_after_return = editor.test_game_view_in_flight();
    let completed = drive_editor_close_to_completion(&mut editor);
    let active_at_completion = editor.active_game_start_request();
    if active_at_completion.is_some() {
        stop_lower_runtime(&mut editor);
        drive_editor_close_to_completion(&mut editor);
    }

    for progress in [first, second] {
        assert!(
            matches!(
                progress,
                EditorProjectCloseProgress::Incomplete {
                    play_cleanup_pending: true,
                    ..
                }
            ),
            "Close must wait for the exact existing Game completion: {progress:?}"
        );
    }
    assert_eq!(admission, (true, true));
    assert_ne!(identity, stale_identity);
    assert_eq!(stale.unwrap_err().identity(), stale_identity);
    assert_eq!(retained, Some(identity));
    assert_eq!(active, Some(request));
    assert_eq!(closes_while_held, 0);
    assert!(
        returned.is_ok(),
        "the exact in-flight completion must remain valid"
    );
    assert!(in_flight_after_return.is_none());
    assert_eq!(probe.frames.load(Ordering::SeqCst), frames_before_close);
    assert_eq!(probe.closes.load(Ordering::SeqCst), 1);
    assert!(active_at_completion.is_none());
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert!(editor.play_view().generation().is_none());
    assert!(Arc::ptr_eq(&published, &editor.current_frame()));
    assert!(matches!(completed, EditorProjectCloseProgress::Complete(_)));
    assert_eq!(editor.advance_close(), completed);
    assert_eq!(probe.closes.load(Ordering::SeqCst), 1);
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
