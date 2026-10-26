use super::*;

use nara_app::CoreStage;
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
use nara_tasks::TaskTerminal;
use nara_tasks::{TaskReturnState, TaskShutdownReport, TaskStats};
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
use nara_tooling::EditorPersistenceRequestResult;
use nara_tooling::{
    EditorPlayCommand, EditorPlayFailure, EditorPlayOperation, EditorPlayOperationResult,
    EditorPlayRequestResult, EditorPlayState,
};

// Reuse the existing body/disposal gates without widening the Editor's private test API.
mod held_tasks {
    include!("../editor/test_fixtures.rs");
}

use held_tasks::HeldCompute;

const WAIT: Duration = Duration::from_secs(5);
const CLOSE_OBSERVATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.editor-play-task-report");

#[derive(Clone, Copy, Debug)]
enum TaskReportTampering {
    Missing,
    Incomplete,
}

#[derive(Default)]
struct PlayTaskProbe {
    held: Mutex<Option<HeldCompute>>,
    before: Mutex<Option<TaskStats>>,
    created: Mutex<Option<TaskStats>>,
    closed: Mutex<Option<TaskShutdownReport>>,
    builds: AtomicUsize,
    frames: AtomicUsize,
    finishes: AtomicUsize,
    shutdowns: AtomicUsize,
    released_worlds: AtomicUsize,
    report_tampering: Option<TaskReportTampering>,
    tampered_reports: AtomicUsize,
    fail_finish: bool,
    fail_first_shutdown: bool,
    fail_shutdown: bool,
}

#[derive(Resource)]
struct PlayTaskWorld(Arc<PlayTaskProbe>);

impl Drop for PlayTaskWorld {
    fn drop(&mut self) {
        self.0.released_worlds.fetch_add(1, Ordering::SeqCst);
    }
}

fn observe_play_frame(probe: Res<PlayTaskWorld>) {
    probe.0.frames.fetch_add(1, Ordering::SeqCst);
}

struct PlayTaskConfiguration(Arc<PlayTaskProbe>);

impl crate::ProductConfiguration for PlayTaskConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"editor-held-play-task-close-v1");
        output.push(u8::from(self.0.fail_shutdown));
        output.push(u8::from(self.0.fail_finish));
        output.push(u8::from(self.0.fail_first_shutdown));
        output.push(match self.0.report_tampering {
            None => 0,
            Some(TaskReportTampering::Missing) => 1,
            Some(TaskReportTampering::Incomplete) => 2,
        });
    }
}

struct PlayTaskPlugin(Arc<PlayTaskProbe>);

impl Plugin for PlayTaskPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.editor-held-play-task"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[TASK_PLUGIN_ID])
        .shutdown_obligations(&[CLOSE_OBSERVATION]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        self.0.builds.fetch_add(1, Ordering::SeqCst);
        let pools = app.world().resource::<TaskPools>();
        *self.0.before.lock().unwrap() = Some(pools.stats());
        *self.0.held.lock().unwrap() = Some(HeldCompute::start(pools, || {}));
        *self.0.created.lock().unwrap() = Some(pools.stats());
        app.insert_resource(PlayTaskWorld(Arc::clone(&self.0)))?
            .add_systems(CoreStage::Update, observe_play_frame)?;
        app.register_plugin_runtime_close_participant(
            CLOSE_OBSERVATION,
            RuntimeCloseParticipantId::new("nara.test.editor-play-task-report"),
            ObservePlayTaskClose(Arc::clone(&self.0)),
        )?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        self.0.finishes.fetch_add(1, Ordering::SeqCst);
        if self.0.fail_finish {
            assert!(app.world().contains_resource::<TaskPools>());
            assert!(self.0.held.lock().unwrap().is_some());
            return Err(PluginDomainError::new(
                "nara.test.editor-play-finish-failed",
                "Play plugin Finish failed after acquiring Tasks",
            )
            .unwrap()
            .into());
        }
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        let previous = self.0.shutdowns.fetch_add(1, Ordering::SeqCst);
        if self.0.fail_shutdown || (self.0.fail_first_shutdown && previous == 0) {
            return Err(plugin_probe_error(Self::declaration().id));
        }
        Ok(())
    }
}

fn held_play_recipe(probe: &Arc<PlayTaskProbe>) -> crate::ProductRecipe {
    crate::ProductRecipe::new()
        .add_configured_plugin(
            PlayTaskConfiguration(Arc::clone(probe)),
            |configuration: &PlayTaskConfiguration| PlayTaskPlugin(Arc::clone(&configuration.0)),
        )
        .unwrap()
}

// Observe the real TaskPlugin report in the runtime before the Session releases that runtime.
// This participant neither stops tasks nor joins workers on behalf of the product owner.
// Malicious modes invalidate only the published evidence after physical completion.
struct ObservePlayTaskClose(Arc<PlayTaskProbe>);

impl RuntimeCloseParticipant for ObservePlayTaskClose {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let Some(report) = context.world().get_resource::<TaskShutdownReport>() else {
            return Ok(RuntimeCloseProgress::Pending);
        };
        if !report.is_complete() {
            return Ok(RuntimeCloseProgress::Pending);
        }
        *self.0.closed.lock().unwrap() = Some(report.clone());
        match self.0.report_tampering {
            None => {}
            Some(TaskReportTampering::Missing) => {
                let removed = context
                    .remove_resource::<TaskShutdownReport>()
                    .expect("the public close context permits removing the task report")
                    .expect("the real complete task report is still present");
                assert!(removed.is_complete());
                self.0.tampered_reports.fetch_add(1, Ordering::SeqCst);
            }
            Some(TaskReportTampering::Incomplete) => {
                let replacement = TaskShutdownReport::default();
                assert!(!replacement.is_complete());
                context
                    .insert_resource(replacement)
                    .expect("the public close context permits replacing the task report");
                self.0.tampered_reports.fetch_add(1, Ordering::SeqCst);
            }
        }
        Ok(RuntimeCloseProgress::Complete)
    }
}

struct ReleasePlayTask(Arc<PlayTaskProbe>);

impl Drop for ReleasePlayTask {
    fn drop(&mut self) {
        if let Some(held) = self
            .0
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            held.release();
        }
    }
}

#[test]
fn final_session_close_waits_for_active_play_body_and_late_result_then_joins_created_workers() {
    let project = TestProject::new("editor-final-close-held-play");
    configure_single_play_workers(&project);
    let probe = Arc::new(PlayTaskProbe::default());
    let recipe = held_play_recipe(&probe);
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )
    .unwrap();
    // Drop this guard before the Session even when startup or a close assertion unwinds.
    let _release = ReleasePlayTask(Arc::clone(&probe));
    start_play(&mut editor);
    let before = probe.before.lock().unwrap().clone().unwrap();
    let created = probe.created.lock().unwrap().clone().unwrap();
    let authoring = editor.test_authoring_task_stats();
    for kind in TaskPoolKind::ALL {
        assert_eq!(before.for_kind(kind).live_workers, 0, "unused Play {kind}");
    }
    assert_eq!(created.for_kind(TaskPoolKind::Compute).live_workers, 1);
    for kind in [TaskPoolKind::Io, TaskPoolKind::AsyncCompute] {
        assert_eq!(created.for_kind(kind).live_workers, 0, "unused Play {kind}");
    }
    assert_eq!(authoring.for_kind(TaskPoolKind::Compute).live_workers, 0);
    assert_eq!(
        authoring.for_kind(TaskPoolKind::AsyncCompute).live_workers,
        0
    );

    // Session close owns the active Play protocol: no preceding Stop, Exit or handle.cancel().
    assert_play_close_retains_body_and_late_result(&mut editor, &probe);
    probe
        .held
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .release_and_wait(TaskReturnState::Returned);
    let completed = drive_editor_close_to_completion(&mut editor);
    let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
        unreachable!("the close helper returns only complete progress");
    };
    let play =
        probe.closed.lock().unwrap().clone().expect(
            "Session completion requires the real Play TaskPlugin's physical shutdown report",
        );
    assert!(play.is_complete());
    assert!(!play.has_cleanup_failure());
    assert!(!receipt.has_authoring_cleanup_failure());
    assert!(!receipt.task_shutdown().has_cleanup_failure());
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            play.for_kind(kind).joined_workers,
            created.for_kind(kind).live_workers
        );
        assert_eq!(play.for_kind(kind).panicked_workers, 0);
        assert_eq!(
            receipt.task_shutdown().for_kind(kind).joined_workers,
            authoring.for_kind(kind).live_workers + created.for_kind(kind).live_workers,
            "Session completion must count each created {kind} worker exactly once"
        );
    }
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert_eq!(editor.advance_close(), completed);
}

fn cleanup_unexpected_play_close(editor: &mut editor::EditorProjectSession, probe: &PlayTaskProbe) {
    probe
        .held
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .release_and_wait(TaskReturnState::Returned);
    stop_play_after_failed_close(editor);
}

fn stop_play_after_failed_close(editor: &mut editor::EditorProjectSession) {
    // A failed Startup may already be retiring; release and drain either owner before asserting.
    let _ = editor.request_play(EditorPlayCommand::Stop);
    let deadline = Instant::now() + WAIT;
    while editor.play_view().state() != EditorPlayState::Empty {
        assert!(
            Instant::now() < deadline,
            "unexpected Play cleanup did not finish"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn final_session_close_preserves_failed_startup_with_clean_physical_cleanup() {
    assert_failed_startup_session_close(false);
}

#[test]
fn final_session_close_preserves_failed_startup_and_terminal_shutdown_evidence() {
    assert_failed_startup_session_close(true);
}

fn assert_failed_startup_session_close(fail_shutdown: bool) {
    let project = TestProject::new("editor-final-close-failed-startup-held-play");
    configure_single_play_workers(&project);
    let probe = Arc::new(PlayTaskProbe {
        fail_shutdown,
        ..Default::default()
    });
    let recipe = held_play_recipe(&probe)
        .add_plugin::<ClassifiedStartupFailurePlugin>()
        .unwrap();
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )
    .unwrap();
    let _release = ReleasePlayTask(Arc::clone(&probe));
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.play_view().state(), EditorPlayState::Starting);
    let failed_start = editor.drive_editor_frame(Duration::ZERO);
    assert!(report_has_code(
        failed_start.diagnostics(),
        "nara.test.startup-configuration-invalid"
    ));
    assert_eq!(editor.play_view().state(), EditorPlayState::RetiringPlay);
    assert_eq!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Pending {
            operation: EditorPlayOperation::Play,
        })
    );
    assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
    assert_eq!(probe.frames.load(Ordering::SeqCst), 0);
    let created = probe.created.lock().unwrap().clone().unwrap();
    let authoring = editor.test_authoring_task_stats();

    // Close the whole Session directly while the rejected candidate still owns real work.
    assert_play_close_retains_body_and_late_result(&mut editor, &probe);
    probe
        .held
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .release_and_wait(TaskReturnState::Returned);
    let completed = drive_editor_close_to_completion(&mut editor);
    let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
        unreachable!("the close helper returns only complete progress");
    };
    let play = probe.closed.lock().unwrap().clone().expect(
        "failed Startup must retain the TaskPlugin report until physical cleanup completes",
    );
    assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 1);
    assert!(play.is_complete());
    assert!(!play.has_cleanup_failure());
    assert!(!receipt.has_authoring_cleanup_failure());
    assert!(!receipt.task_shutdown().has_cleanup_failure());
    assert_eq!(receipt.has_play_cleanup_failure(), fail_shutdown);
    assert_eq!(receipt.has_cleanup_failure(), fail_shutdown);
    if fail_shutdown {
        let evidence = receipt
            .play_close_evidence()
            .expect("terminal plugin shutdown evidence must survive World release");
        assert!(evidence.plugin_shutdown_failed());
        assert!(evidence.has_terminal_failure());
        assert!(evidence.pending_participants().is_empty());
        assert_eq!(
            evidence
                .causes()
                .iter()
                .filter(|cause| matches!(cause, RuntimeCloseCause::PluginShutdown))
                .count(),
            1
        );
    } else {
        assert!(
            receipt.play_close_evidence().is_none(),
            "a primary Startup fault must not become a cleanup failure"
        );
    }
    assert_play_worker_counts(receipt, &play, &created, &authoring);
    let startup_failure = Some(EditorPlayOperationResult::Failed {
        operation: EditorPlayOperation::Play,
        failure: EditorPlayFailure::Start,
    });
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert_eq!(editor.play_view().result(), startup_failure);
    for _ in 0..3 {
        assert_eq!(editor.advance_close(), completed);
        let frame = editor.drive_editor_frame(Duration::from_millis(16));
        assert_eq!(frame.play().result(), startup_failure);
        assert!(report_has_code(
            frame.diagnostics(),
            "nara.test.startup-configuration-invalid"
        ));
        assert_eq!(probe.closed.lock().unwrap().as_ref(), Some(&play));
        assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
        assert_eq!(probe.shutdowns.load(Ordering::SeqCst), 1);
        assert_eq!(probe.frames.load(Ordering::SeqCst), 0);
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn final_session_close_cancels_pending_restart_without_successor_or_gameplay_frame() {
    let project = TestProject::new("editor-final-close-pending-restart-held-play");
    configure_single_play_workers(&project);
    let probe = Arc::new(PlayTaskProbe::default());
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(held_play_recipe(&probe)),
    )
    .unwrap();
    let _release = ReleasePlayTask(Arc::clone(&probe));
    start_play(&mut editor);
    // Prove the frame counter observes this actual Play runtime before testing close suppression.
    editor.drive_editor_frame(Duration::from_millis(16));
    assert!(probe.frames.load(Ordering::SeqCst) > 0);
    let frames = probe.frames.load(Ordering::SeqCst);
    let created = probe.created.lock().unwrap().clone().unwrap();
    let authoring = editor.test_authoring_task_stats();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Restart),
        EditorPlayRequestResult::Accepted
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::Stopping);
    assert_eq!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Pending {
            operation: EditorPlayOperation::Restart,
        })
    );

    assert_play_close_retains_body_and_late_result(&mut editor, &probe);
    probe
        .held
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .release_and_wait(TaskReturnState::Returned);
    let completed = drive_editor_close_to_completion(&mut editor);
    let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
        unreachable!("the close helper returns only complete progress");
    };
    let play = probe.closed.lock().unwrap().clone().unwrap();
    assert!(!receipt.has_play_cleanup_failure());
    assert!(!receipt.has_cleanup_failure());
    assert!(receipt.play_close_evidence().is_none());
    assert_play_worker_counts(receipt, &play, &created, &authoring);
    let cancelled = Some(EditorPlayOperationResult::Cancelled {
        operation: EditorPlayOperation::Restart,
    });
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert_eq!(editor.play_view().result(), cancelled);
    for _ in 0..3 {
        assert_eq!(editor.advance_close(), completed);
        let frame = editor.drive_editor_frame(Duration::from_millis(16));
        assert_eq!(frame.play().state(), EditorPlayState::Empty);
        assert_eq!(frame.play().result(), cancelled);
        assert_eq!(probe.closed.lock().unwrap().as_ref(), Some(&play));
        assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
        assert_eq!(probe.shutdowns.load(Ordering::SeqCst), 1);
        assert_eq!(probe.frames.load(Ordering::SeqCst), frames);
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn final_session_close_cancels_preparing_and_starting_before_runtime_construction() {
    // These two adjacent cuts own no constructed runtime; held-candidate cleanup is covered above.
    for starting in [false, true] {
        let project = TestProject::new("editor-final-close-unconstructed-play");
        configure_single_play_workers(&project);
        let probe = Arc::new(PlayTaskProbe::default());
        let mut editor = editor::EditorProjectSession::open_trusted_compiled(
            project.writable_capability(),
            editor::TrustedCompiledEditorProjectIntent::new().with_recipe(held_play_recipe(&probe)),
        )
        .unwrap();
        let _release = ReleasePlayTask(Arc::clone(&probe));
        assert_eq!(
            editor.request_play(EditorPlayCommand::Play),
            EditorPlayRequestResult::Accepted
        );
        assert_eq!(editor.play_view().state(), EditorPlayState::PreparingPlay);
        if starting {
            editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(editor.play_view().state(), EditorPlayState::Starting);
        }
        let authoring = editor.test_authoring_task_stats();
        let completed = drive_editor_close_to_completion(&mut editor);
        let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
            unreachable!("the close helper returns only complete progress");
        };
        assert!(!receipt.has_cleanup_failure());
        assert!(!receipt.has_play_cleanup_failure());
        assert!(receipt.play_close_evidence().is_none());
        for kind in TaskPoolKind::ALL {
            assert_eq!(
                receipt.task_shutdown().for_kind(kind).joined_workers,
                authoring.for_kind(kind).live_workers
            );
        }
        for _ in 0..3 {
            assert_eq!(editor.advance_close(), completed);
            let frame = editor.drive_editor_frame(Duration::from_millis(16));
            assert_eq!(frame.play().state(), EditorPlayState::Empty);
            assert_eq!(
                frame.play().result(),
                Some(EditorPlayOperationResult::Cancelled {
                    operation: EditorPlayOperation::Play,
                })
            );
            assert_eq!(probe.builds.load(Ordering::SeqCst), 0);
            assert_eq!(probe.frames.load(Ordering::SeqCst), 0);
            assert_eq!(probe.shutdowns.load(Ordering::SeqCst), 0);
            assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 0);
            assert!(probe.held.lock().unwrap().is_none());
            assert!(probe.closed.lock().unwrap().is_none());
        }
    }
}

fn assert_play_close_retains_body_and_late_result(
    editor: &mut editor::EditorProjectSession,
    probe: &PlayTaskProbe,
) {
    let deadline = Instant::now() + WAIT;
    loop {
        let progress = editor.advance_close();
        let retained = matches!(
            progress,
            editor::EditorProjectCloseProgress::Incomplete {
                play_cleanup_pending: true,
                ..
            }
        );
        if !retained {
            cleanup_unexpected_play_close(editor, probe);
        }
        assert!(
            retained,
            "Session must retain Play's held body: {progress:?}"
        );
        assert!(probe.closed.lock().unwrap().is_none());
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 0);
        let cancelled = {
            let held = probe.held.lock().unwrap();
            let held = held.as_ref().unwrap();
            assert_eq!(held.returned.state(), TaskReturnState::Pending);
            held.handle.cancellation_token().is_cancelled()
        };
        if cancelled {
            break;
        }
        if Instant::now() >= deadline {
            cleanup_unexpected_play_close(editor, probe);
            panic!("Session close never cancelled the owned Play task");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    {
        let mut held = probe.held.lock().unwrap();
        let held = held.as_mut().unwrap();
        held.release_body_until_drop();
        assert!(held.handle.is_finished());
        assert_eq!(held.returned.state(), TaskReturnState::Pending);
    }
    for _ in 0..4 {
        let progress = editor.advance_close();
        let retained = matches!(
            progress,
            editor::EditorProjectCloseProgress::Incomplete {
                play_cleanup_pending: true,
                ..
            }
        );
        if !retained {
            cleanup_unexpected_play_close(editor, probe);
        }
        assert!(
            retained,
            "Session must retain Play's disposing late result: {progress:?}"
        );
        assert!(probe.closed.lock().unwrap().is_none());
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 0);
    }
}

fn assert_play_worker_counts(
    receipt: &editor::EditorProjectCloseReceipt,
    play: &TaskShutdownReport,
    created: &TaskStats,
    authoring: &TaskStats,
) {
    assert!(play.is_complete());
    assert!(!play.has_cleanup_failure());
    assert!(receipt.task_shutdown().is_complete());
    assert_eq!(created.for_kind(TaskPoolKind::Compute).live_workers, 1);
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            play.for_kind(kind).joined_workers,
            created.for_kind(kind).live_workers
        );
        assert_eq!(play.for_kind(kind).panicked_workers, 0);
        assert_eq!(
            receipt.task_shutdown().for_kind(kind).joined_workers,
            authoring.for_kind(kind).live_workers + created.for_kind(kind).live_workers,
            "Session must count each actually created {kind} worker once"
        );
    }
}

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
struct BlockingIoConfiguration(Arc<BlockingTaskControl>);

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
impl crate::ProductConfiguration for BlockingIoConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"editor-authoring-while-play-io-held-v1");
    }
}

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
struct ReleaseIoTask(Arc<BlockingTaskControl>);

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
impl Drop for ReleaseIoTask {
    fn drop(&mut self) {
        if let Some(release) = self
            .0
            .release
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = release.send(());
        }
    }
}

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
#[test]
fn held_play_io_does_not_block_authoring_prefab_edit_save_and_reopen() {
    let project = TestProject::new("editor-authoring-with-saturated-play-io");
    configure_single_play_workers(&project);
    let source = project.root.join("prefabs/shared.prefab.json");
    fs::write(
        &source,
        editable_authoring_prefab("original")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let anchor_id = nara_identity::SceneEntityId::new("prefab-anchor").unwrap();
    let mut anchor = SceneEntityRecord::new(anchor_id.clone());
    anchor.prefab = Some(nara_scene::PrefabInstance {
        source: AssetRef::path("shared.prefab.json").unwrap(),
        overrides: None,
    });
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let control = Arc::new(BlockingTaskControl::default());
    let recipe = crate::ProductRecipe::new()
        .add_configured_plugin(
            BlockingIoConfiguration(Arc::clone(&control)),
            |configuration: &BlockingIoConfiguration| BlockingTaskPlugin {
                control: Arc::clone(&configuration.0),
            },
        )
        .unwrap();
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )
    .unwrap();
    // The existing Io fixture has no timeout in its body; release it before Session Drop.
    let _release = ReleaseIoTask(Arc::clone(&control));
    let scene = editor.workspace().active_document().unwrap();
    start_play(&mut editor);
    let play_created = editor
        .test_game_view_runtime()
        .world()
        .resource::<TaskPools>()
        .stats();
    assert_eq!(control.instances.load(Ordering::SeqCst), 1);
    assert_eq!(play_created.for_kind(TaskPoolKind::Io).live_workers, 1);
    assert_eq!(play_created.for_kind(TaskPoolKind::Io).running, 1);
    for kind in [TaskPoolKind::Compute, TaskPoolKind::AsyncCompute] {
        assert_eq!(play_created.for_kind(kind).live_workers, 0);
        assert_eq!(
            editor
                .test_authoring_task_stats()
                .for_kind(kind)
                .live_workers,
            0
        );
    }
    let opened = open_prefab_from_scene(&mut editor, scene, &anchor_id);
    assert!(opened.applied, "{:?}", opened.diagnostics);
    let prefab = opened.opened_document.unwrap();
    let edit_prefab = |editor: &mut editor::EditorProjectSession, value: &str| {
        let base_revision = editor.workspace().document_revision(prefab).unwrap();
        let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyPrefabPatch {
            document: prefab,
            base_revision,
            patch: ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: nara_identity::SceneEntityId::new("editable-name").unwrap(),
                component: ComponentTypeId::new("nara.scene.Name"),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("value"),
                value: ComponentValue::String(value.to_owned()),
            }]),
        });
        assert!(report.applied, "{:?}", report.diagnostics);
    };
    edit_prefab(&mut editor, "saved-while-play-held");
    let saved_revision = editor.workspace().document_revision(prefab).unwrap();
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(prefab)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    let saved = drive_persistence_to_terminal(&mut editor);
    let bytes = fs::read(&source).unwrap();
    let digest =
        crate::project_host::persistence::editor_digest(nara_fs::ContentDigest::of_bytes(&bytes));
    assert_eq!(
        saved.persistence().result(),
        Some(EditorPersistenceResult::Saved {
            document: prefab,
            revision: saved_revision,
            digest,
        })
    );
    assert!(
        String::from_utf8(bytes.clone())
            .unwrap()
            .contains("saved-while-play-held")
    );
    assert!(!editor.workspace().prefab(prefab).unwrap().is_dirty());
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
            attempt: editor.persistence_view().attempt().unwrap()
        }),
        EditorPersistenceRequestResult::Acknowledged { .. }
    ));
    edit_prefab(&mut editor, "discarded-by-reopen");
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(prefab)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    let reopened = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        reopened.persistence().result(),
        Some(EditorPersistenceResult::Opened { document, .. }) if document == prefab
    ));
    let slot = editor.workspace().prefab(prefab).unwrap();
    let reopened_source = slot.session().document().to_json_string().unwrap();
    assert!(reopened_source.contains("saved-while-play-held"));
    assert!(!reopened_source.contains("discarded-by-reopen"));
    assert!(!slot.is_dirty());
    assert_eq!(fs::read(&source).unwrap(), bytes);
    let authoring_created = editor.test_authoring_task_stats();
    assert!(authoring_created.for_kind(TaskPoolKind::Io).live_workers > 0);
    let play_held = editor
        .test_game_view_runtime()
        .world()
        .resource::<TaskPools>()
        .stats();
    assert_eq!(editor.play_view().state(), EditorPlayState::Running);
    assert_eq!(play_held.for_kind(TaskPoolKind::Io).running, 1);
    assert_eq!(play_held.for_kind(TaskPoolKind::Io).completed, 0);
    for kind in [TaskPoolKind::Compute, TaskPoolKind::AsyncCompute] {
        assert_eq!(authoring_created.for_kind(kind).live_workers, 0);
        assert_eq!(play_held.for_kind(kind).live_workers, 0);
    }

    control.release();
    let completed = drive_editor_close_to_completion(&mut editor);
    let play_at_completion = editor.play_view().state();
    if play_at_completion != EditorPlayState::Empty {
        stop_play_after_failed_close(&mut editor);
    }
    assert_eq!(play_at_completion, EditorPlayState::Empty);
    assert!(!completed.task_shutdown().has_cleanup_failure());
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            completed.task_shutdown().for_kind(kind).joined_workers,
            authoring_created.for_kind(kind).live_workers + play_held.for_kind(kind).live_workers
        );
    }
}

fn configure_single_play_workers(project: &TestProject) {
    let manifest_path = project.root.join(PROJECT_MANIFEST);
    let mut manifest = fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str(
        "\n[tasks.io]\nworkers = 1\npending_capacity = 8\n\
         [tasks.compute]\nworkers = 1\npending_capacity = 8\n\
         [tasks.async_compute]\nworkers = 1\npending_capacity = 8\n\
         [tasks.shutdown]\ndrain_timeout_ms = 1\ncancel_timeout_ms = 5000\njoin_timeout_ms = 5000\n",
    );
    fs::write(manifest_path, manifest).unwrap();
}

fn start_play(editor: &mut editor::EditorProjectSession) {
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + WAIT;
    while editor.play_view().state() != EditorPlayState::Running {
        assert!(Instant::now() < deadline, "trusted Play did not start");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn stop_fails_when_required_task_report_is_missing_after_physical_completion() {
    assert_invalid_play_task_report(TaskReportTampering::Missing, false);
}

#[test]
fn stop_fails_when_required_task_report_is_incomplete_after_physical_completion() {
    assert_invalid_play_task_report(TaskReportTampering::Incomplete, false);
}

#[test]
fn restart_has_no_successor_when_required_task_report_is_missing_after_physical_completion() {
    assert_invalid_play_task_report(TaskReportTampering::Missing, true);
}

#[test]
fn restart_has_no_successor_when_required_task_report_is_incomplete_after_physical_completion() {
    assert_invalid_play_task_report(TaskReportTampering::Incomplete, true);
}

fn assert_invalid_play_task_report(tampering: TaskReportTampering, restart: bool) {
    let project = TestProject::new("editor-play-invalid-task-report");
    configure_single_play_workers(&project);
    let probe = Arc::new(PlayTaskProbe {
        report_tampering: Some(tampering),
        ..Default::default()
    });
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(held_play_recipe(&probe)),
    )
    .unwrap();
    let _release = ReleasePlayTask(Arc::clone(&probe));
    start_play(&mut editor);
    editor.drive_editor_frame(Duration::from_millis(16));
    assert!(probe.frames.load(Ordering::SeqCst) > 0);
    let created = probe.created.lock().unwrap().clone().unwrap();
    let (command, operation) = if restart {
        (EditorPlayCommand::Restart, EditorPlayOperation::Restart)
    } else {
        (EditorPlayCommand::Stop, EditorPlayOperation::Stop)
    };
    assert_eq!(
        editor.request_play(command),
        EditorPlayRequestResult::Accepted
    );
    assert_eq!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Pending { operation })
    );

    // Drive the ordinary control path while the real executor still owns its blocked body.
    let deadline = Instant::now() + WAIT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(probe.tampered_reports.load(Ordering::SeqCst), 0);
        assert!(probe.closed.lock().unwrap().is_none());
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 0);
        let held = probe.held.lock().unwrap();
        let held = held.as_ref().unwrap();
        assert_eq!(held.returned.state(), TaskReturnState::Pending);
        if held.handle.cancellation_token().is_cancelled() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Play close did not cancel its task"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    probe
        .held
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .release_and_wait(TaskReturnState::Returned);
    drive_play_control_to_terminal(&mut editor);
    let failed = Some(EditorPlayOperationResult::Failed {
        operation,
        failure: EditorPlayFailure::Close,
    });
    assert_eq!(editor.play_view().result(), failed, "{tampering:?}");
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert!(report_has_code(
        editor.current_frame().diagnostics(),
        "project.run.task-shutdown-report-invalid"
    ));
    let play = probe
        .closed
        .lock()
        .unwrap()
        .clone()
        .expect("tampering requires the real complete TaskPlugin report");
    assert!(play.is_complete());
    assert!(!play.has_cleanup_failure());
    assert_eq!(created.for_kind(TaskPoolKind::Compute).live_workers, 1);
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            play.for_kind(kind).joined_workers,
            created.for_kind(kind).live_workers
        );
        assert_eq!(play.for_kind(kind).panicked_workers, 0);
    }
    assert_eq!(
        probe.builds.load(Ordering::SeqCst),
        1,
        "Restart must not construct a successor"
    );
    assert_eq!(probe.shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 1);
    assert_eq!(probe.tampered_reports.load(Ordering::SeqCst), 1);

    let frames = probe.frames.load(Ordering::SeqCst);
    let authoring = editor.test_authoring_task_stats();
    let completed = drive_editor_close_to_completion(&mut editor);
    let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
        unreachable!("the close helper returns only complete progress");
    };
    assert!(receipt.has_play_cleanup_failure());
    assert!(receipt.has_cleanup_failure());
    assert!(!receipt.has_authoring_cleanup_failure());
    assert!(receipt.task_shutdown().is_complete());
    assert!(!receipt.task_shutdown().has_cleanup_failure());
    assert!(
        receipt
            .play_close_evidence()
            .expect("the stopped runtime's close evidence survives World release")
            .is_empty(),
        "report validation alone fails, without plugin or participant cleanup failure"
    );
    for kind in TaskPoolKind::ALL {
        // Invalid Play evidence must not manufacture joined-worker counts in the receipt.
        assert_eq!(
            receipt.task_shutdown().for_kind(kind).joined_workers,
            authoring.for_kind(kind).live_workers
        );
    }
    for _ in 0..3 {
        assert_eq!(editor.advance_close(), completed);
        let frame = editor.drive_editor_frame(Duration::from_millis(16));
        assert_eq!(frame.play().state(), EditorPlayState::Empty);
        assert_eq!(frame.play().result(), failed);
        assert!(report_has_code(
            frame.diagnostics(),
            "project.run.task-shutdown-report-invalid"
        ));
        assert_eq!(probe.closed.lock().unwrap().as_ref(), Some(&play));
        assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
        assert_eq!(probe.shutdowns.load(Ordering::SeqCst), 1);
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 1);
        assert_eq!(probe.tampered_reports.load(Ordering::SeqCst), 1);
        assert_eq!(probe.frames.load(Ordering::SeqCst), frames);
    }
}

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
#[test]
fn ordinary_play_task_error_does_not_fail_stop_or_session_cleanup() {
    let project = TestProject::new("editor-play-task-business-error-clean-close");
    configure_single_play_workers(&project);
    let probe = Arc::new(PlayTaskProbe::default());
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(held_play_recipe(&probe)),
    )
    .unwrap();
    let _release = ReleasePlayTask(Arc::clone(&probe));
    start_play(&mut editor);
    probe
        .held
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .release_and_wait(TaskReturnState::Returned);
    let mut task = editor
        .test_game_view_runtime()
        .world()
        .resource::<TaskPools>()
        .spawn(
            TaskPoolKind::Compute,
            TaskSpawnRequest::new(1, TaskDomainKey::new(2)),
            |_| Err::<(), _>("ordinary Play task error"),
        )
        .into_handle()
        .expect("the real Play executor admits the business-error task");
    let returned = task.return_observer();
    let deadline = Instant::now() + WAIT;
    while returned.state() == TaskReturnState::Pending {
        assert!(
            Instant::now() < deadline,
            "the business-error task did not return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(returned.state(), TaskReturnState::Returned);
    assert_eq!(
        task.try_take(),
        Some(TaskTerminal::Completed(Err("ordinary Play task error")))
    );
    let created = probe.created.lock().unwrap().clone().unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_play_control_to_terminal(&mut editor);
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert!(matches!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::Stop,
            ..
        })
    ));
    assert!(!report_has_code(
        editor.current_frame().diagnostics(),
        "project.run.task-shutdown-report-invalid"
    ));
    let play = probe.closed.lock().unwrap().clone().unwrap();
    let authoring = editor.test_authoring_task_stats();
    let completed = drive_editor_close_to_completion(&mut editor);
    let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
        unreachable!("the close helper returns only complete progress");
    };
    assert!(!receipt.has_cleanup_failure());
    assert!(!receipt.has_play_cleanup_failure());
    assert!(receipt.play_close_evidence().is_none());
    assert_play_worker_counts(receipt, &play, &created, &authoring);
}

#[test]
fn failed_finish_with_acquired_tasks_rejects_missing_shutdown_report() {
    assert_failed_finish_task_report(TaskReportTampering::Missing);
}

#[test]
fn failed_finish_with_acquired_tasks_rejects_incomplete_shutdown_report() {
    assert_failed_finish_task_report(TaskReportTampering::Incomplete);
}

fn assert_failed_finish_task_report(tampering: TaskReportTampering) {
    let project = TestProject::new("editor-failed-finish-acquired-tasks-invalid-report");
    configure_single_play_workers(&project);
    let probe = Arc::new(PlayTaskProbe {
        fail_finish: true,
        report_tampering: Some(tampering),
        ..Default::default()
    });
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(held_play_recipe(&probe)),
    )
    .unwrap();
    let _release = ReleasePlayTask(Arc::clone(&probe));
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.play_view().state(), EditorPlayState::Starting);
    let failed_start = editor.drive_editor_frame(Duration::ZERO);
    assert!(report_has_code(
        failed_start.diagnostics(),
        "nara.test.editor-play-finish-failed"
    ));
    assert_eq!(
        identifier_field(
            failed_start.diagnostics(),
            "nara.test.editor-play-finish-failed",
            "hook"
        ),
        Some("finish")
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::RetiringPlay);
    assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
    assert_eq!(probe.finishes.load(Ordering::SeqCst), 1);
    let created = probe.created.lock().unwrap().clone().unwrap();
    let authoring = editor.test_authoring_task_stats();

    assert_play_close_retains_body_and_late_result(&mut editor, &probe);
    assert_eq!(probe.tampered_reports.load(Ordering::SeqCst), 0);
    probe
        .held
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .release_and_wait(TaskReturnState::Returned);
    let completed = drive_editor_close_to_completion(&mut editor);
    let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
        unreachable!("the close helper returns only complete progress");
    };
    let play = probe.closed.lock().unwrap().clone().unwrap();
    assert!(play.is_complete());
    assert!(!play.has_cleanup_failure());
    assert_eq!(created.for_kind(TaskPoolKind::Compute).live_workers, 1);
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            play.for_kind(kind).joined_workers,
            created.for_kind(kind).live_workers
        );
        assert_eq!(play.for_kind(kind).panicked_workers, 0);
        assert_eq!(
            receipt.task_shutdown().for_kind(kind).joined_workers,
            authoring.for_kind(kind).live_workers,
            "invalid Play evidence must not manufacture joined {kind} workers"
        );
    }
    assert!(receipt.task_shutdown().is_complete());
    assert!(!receipt.task_shutdown().has_cleanup_failure());
    assert!(!receipt.has_authoring_cleanup_failure());
    assert!(receipt.has_play_cleanup_failure(), "{tampering:?}");
    assert!(receipt.has_cleanup_failure());
    let evidence = receipt
        .play_close_evidence()
        .expect("report validation failure retains the physically retired runtime evidence");
    assert!(!evidence.plugin_shutdown_failed());
    assert!(!evidence.has_terminal_failure());
    assert!(evidence.pending_participants().is_empty());
    let failed = Some(EditorPlayOperationResult::Failed {
        operation: EditorPlayOperation::Play,
        failure: EditorPlayFailure::Start,
    });
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert_eq!(editor.play_view().result(), failed);
    for _ in 0..3 {
        assert_eq!(editor.advance_close(), completed);
        let frame = editor.drive_editor_frame(Duration::from_millis(16));
        assert_eq!(frame.play().state(), EditorPlayState::Empty);
        assert_eq!(frame.play().result(), failed);
        assert!(report_has_code(
            frame.diagnostics(),
            "nara.test.editor-play-finish-failed"
        ));
        assert!(report_has_code(
            frame.diagnostics(),
            "project.run.task-shutdown-report-invalid"
        ));
        assert_eq!(probe.closed.lock().unwrap().as_ref(), Some(&play));
        assert_eq!(probe.builds.load(Ordering::SeqCst), 1);
        assert_eq!(probe.finishes.load(Ordering::SeqCst), 1);
        assert_eq!(probe.shutdowns.load(Ordering::SeqCst), 1);
        assert_eq!(probe.tampered_reports.load(Ordering::SeqCst), 1);
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 1);
        assert_eq!(probe.frames.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn preparation_rejection_before_tasks_acquisition_does_not_fail_session_cleanup() {
    let project = TestProject::new("editor-pre-tasks-preparation-rejection-clean-close");
    configure_single_play_workers(&project);
    let probe = Arc::new(PlayTaskProbe::default());
    let builds = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let recipe = held_play_recipe(&probe)
        .add_configured_plugin(PlayTaskConfiguration(Arc::clone(&probe)), {
            let builds = Arc::clone(&builds);
            let closes = Arc::clone(&closes);
            move |_: &PlayTaskConfiguration| CloseProbePlugin {
                builds: Arc::clone(&builds),
                closes: Arc::clone(&closes),
            }
        })
        .unwrap();
    // Satisfy PrepareFailurePlugin's dependency so this exercises its real fallible factory.
    let overlay =
        crate::project_host::composition::runtime_plugin_overlay(prepare_failure_definition());
    let mut editor = editor::EditorProjectSession::open_trusted_compiled_with_overlay(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
        overlay,
    )
    .unwrap();
    let _release = ReleasePlayTask(Arc::clone(&probe));
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.play_view().state(), EditorPlayState::Starting);
    let rejected = editor.drive_editor_frame(Duration::ZERO);
    assert!(report_has_code(
        rejected.diagnostics(),
        "nara.test.project-prepare-failure"
    ));
    assert_eq!(
        identifier_field(
            rejected.diagnostics(),
            "nara.test.project-prepare-failure",
            "phase"
        ),
        Some("prepare")
    );
    let authoring = editor.test_authoring_task_stats();
    let completed = drive_editor_close_to_completion(&mut editor);
    let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
        unreachable!("the close helper returns only complete progress");
    };
    assert!(!receipt.has_cleanup_failure());
    assert!(!receipt.has_play_cleanup_failure());
    assert!(!receipt.has_authoring_cleanup_failure());
    assert!(receipt.play_close_evidence().is_none());
    assert!(receipt.task_shutdown().is_complete());
    let failed = Some(EditorPlayOperationResult::Failed {
        operation: EditorPlayOperation::Play,
        failure: EditorPlayFailure::Start,
    });
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            receipt.task_shutdown().for_kind(kind).joined_workers,
            authoring.for_kind(kind).live_workers
        );
    }
    for _ in 0..3 {
        assert_eq!(editor.advance_close(), completed);
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(frame.play().state(), EditorPlayState::Empty);
        assert_eq!(frame.play().result(), failed);
        assert!(!report_has_code(
            frame.diagnostics(),
            "project.run.task-shutdown-report-invalid"
        ));
        assert_eq!(probe.builds.load(Ordering::SeqCst), 0);
        assert_eq!(probe.finishes.load(Ordering::SeqCst), 0);
        assert_eq!(probe.shutdowns.load(Ordering::SeqCst), 0);
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 0);
        assert_eq!(builds.load(Ordering::SeqCst), 0);
        assert_eq!(closes.load(Ordering::SeqCst), 0);
        assert!(probe.held.lock().unwrap().is_none());
        assert!(probe.created.lock().unwrap().is_none());
        assert!(probe.closed.lock().unwrap().is_none());
    }
}

#[test]
fn session_close_counts_two_play_owners_once_and_keeps_first_shutdown_failure() {
    let project = TestProject::new("editor-two-play-owners-sticky-first-shutdown-failure");
    configure_single_play_workers(&project);
    let probe = Arc::new(PlayTaskProbe {
        fail_first_shutdown: true,
        ..Default::default()
    });
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new().with_recipe(held_play_recipe(&probe)),
    )
    .unwrap();
    let _release = ReleasePlayTask(Arc::clone(&probe));
    let mut reports = Vec::new();
    let mut created = Vec::new();
    for owner in 0..2 {
        start_play(&mut editor);
        created.push(probe.created.lock().unwrap().clone().unwrap());
        probe
            .held
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .release_and_wait(TaskReturnState::Returned);
        assert_eq!(
            editor.request_play(EditorPlayCommand::Stop),
            EditorPlayRequestResult::Accepted
        );
        drive_play_control_to_terminal(&mut editor);
        assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
        let terminal = editor.play_view().result();
        if owner == 0 {
            assert_eq!(
                terminal,
                Some(EditorPlayOperationResult::Failed {
                    operation: EditorPlayOperation::Stop,
                    failure: EditorPlayFailure::Close,
                })
            );
            assert!(report_has_code(
                editor.current_frame().diagnostics(),
                "project.run.cleanup-plugin-shutdown"
            ));
        } else {
            assert!(matches!(
                terminal,
                Some(EditorPlayOperationResult::Applied {
                    operation: EditorPlayOperation::Stop,
                    ..
                })
            ));
        }
        let report = probe.closed.lock().unwrap().clone().unwrap();
        assert!(report.is_complete());
        assert!(!report.has_cleanup_failure());
        assert_eq!(
            created[owner].for_kind(TaskPoolKind::Compute).live_workers,
            1
        );
        for kind in TaskPoolKind::ALL {
            assert_eq!(
                report.for_kind(kind).joined_workers,
                created[owner].for_kind(kind).live_workers
            );
        }
        for _ in 0..3 {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(frame.play().state(), EditorPlayState::Empty);
            assert_eq!(frame.play().result(), terminal);
            assert_eq!(probe.closed.lock().unwrap().as_ref(), Some(&report));
            assert_eq!(probe.builds.load(Ordering::SeqCst), owner + 1);
            assert_eq!(probe.shutdowns.load(Ordering::SeqCst), owner + 1);
            assert_eq!(probe.released_worlds.load(Ordering::SeqCst), owner + 1);
        }
        reports.push(report);
    }
    let terminal = editor.play_view().result();
    let authoring = editor.test_authoring_task_stats();
    let completed = drive_editor_close_to_completion(&mut editor);
    let editor::EditorProjectCloseProgress::Complete(receipt) = &completed else {
        unreachable!("the close helper returns only complete progress");
    };
    assert!(receipt.has_play_cleanup_failure());
    assert!(receipt.has_cleanup_failure());
    assert!(!receipt.has_authoring_cleanup_failure());
    assert!(receipt.task_shutdown().is_complete());
    assert!(!receipt.task_shutdown().has_cleanup_failure());
    let evidence = receipt.play_close_evidence().unwrap();
    assert!(evidence.plugin_shutdown_failed());
    assert!(evidence.has_terminal_failure());
    assert!(evidence.pending_participants().is_empty());
    assert_eq!(
        evidence
            .causes()
            .iter()
            .filter(|cause| matches!(cause, RuntimeCloseCause::PluginShutdown))
            .count(),
        1
    );
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            receipt.task_shutdown().for_kind(kind).joined_workers,
            authoring.for_kind(kind).live_workers
                + reports[0].for_kind(kind).joined_workers
                + reports[1].for_kind(kind).joined_workers,
            "Session must count both actually created {kind} owners exactly once"
        );
    }
    for _ in 0..3 {
        assert_eq!(editor.advance_close(), completed);
        let frame = editor.drive_editor_frame(Duration::from_millis(16));
        assert_eq!(frame.play().state(), EditorPlayState::Empty);
        assert_eq!(frame.play().result(), terminal);
        assert_eq!(probe.closed.lock().unwrap().as_ref(), Some(&reports[1]));
        assert_eq!(probe.builds.load(Ordering::SeqCst), 2);
        assert_eq!(probe.finishes.load(Ordering::SeqCst), 2);
        assert_eq!(probe.shutdowns.load(Ordering::SeqCst), 2);
        assert_eq!(probe.released_worlds.load(Ordering::SeqCst), 2);
    }
}

fn drive_play_control_to_terminal(editor: &mut editor::EditorProjectSession) {
    let deadline = Instant::now() + WAIT;
    while matches!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Pending { .. })
    ) {
        assert!(Instant::now() < deadline, "Play control did not finish");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}
