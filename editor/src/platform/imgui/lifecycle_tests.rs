#![cfg(target_os = "windows")]

use super::*;
use crate::{EditorLaunchOptions, frontend::imgui::UiProject};
use nara::tooling::{EditorAction, EditorPlayAction};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, ShowWindow,
};
use winit::{
    platform::windows::EventLoopBuilderExtWindows,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
};

struct NativeLifecycleProof {
    started: Instant,
    phase: LifecyclePhase,
    primary_frames: u32,
    secondary_frames: u32,
    minimized_secondary_frames: u32,
    returned_frames: u32,
    game_closed_frames: u32,
    scene_closed_frames: u32,
    reopened_frames: u32,
    recovered_frames: u32,
    retired_backend: Option<u64>,
    first_present_ms: Option<u128>,
    stale_receipt_rejected: bool,
    last_game: Option<WgpuSubmittedImageReceipt>,
    initial_scene_view: Option<WgpuEditorViewId>,
    initial_game_view: Option<WgpuEditorViewId>,
    reopened_game_view: Option<WgpuEditorViewId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecyclePhase {
    Primary,
    Detached,
    Minimized,
    RestoringPrimary,
    Returned,
    GameClosed,
    GameReopening,
    SceneClosed,
    Reopened,
    Recovering,
    Recovered,
    Stopping,
    Complete,
}

impl NativeLifecycleProof {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            phase: LifecyclePhase::Primary,
            primary_frames: 0,
            secondary_frames: 0,
            minimized_secondary_frames: 0,
            returned_frames: 0,
            game_closed_frames: 0,
            scene_closed_frames: 0,
            reopened_frames: 0,
            recovered_frames: 0,
            retired_backend: None,
            first_present_ms: None,
            stale_receipt_rejected: false,
            last_game: None,
            initial_scene_view: None,
            initial_game_view: None,
            reopened_game_view: None,
        }
    }
}

impl NativeLifecycleProof {
    fn observe(
        &mut self,
        native: &mut NativeImgui,
        application: &mut EditorApplication,
        outcome: WgpuEditorFrameOutcome,
        secondary_outcomes: Vec<WgpuEditorFrameOutcome>,
    ) -> Result<bool, EditorRunError> {
        let window_packet = editor_window_packet(&native.window);
        for panel in application.panel_workspace().layout().instances() {
            let target = match panel.kind() {
                "nara.authoring-view" => Some(&native.scene),
                "nara.game-view" => Some(&native.game),
                _ => None,
            };
            let expected = target
                .filter(|_| panel.is_visible())
                .and_then(|target| target.view.filter(|_| target.identity.is_some()))
                .map(|view| crate::workspace::EditorRenderViewId(view.get()));
            if panel.render_view() != expected {
                return Err(imgui_error(
                    "the panel did not publish its exact live GPU view identity",
                ));
            }
        }
        if let Some(old) = self.last_game.filter(|old| {
            native.game.submitted.is_some_and(|new| new != *old)
                && native
                    .game
                    .identity
                    .is_some_and(|target| target.matches_submission(*old))
        }) {
            let stale = native.gpu.present_window(
                window_packet,
                native.frame_index,
                &[old],
                |_| panic!("a stale image receipt reached paint"),
                || {},
            );
            if !matches!(stale, Err(WgpuEditorExecutionError::StaleImageSubmission)) {
                return Err(imgui_error(
                    "a repeated image allocation accepted a superseded receipt",
                ));
            }
            self.stale_receipt_rejected = true;
        }
        self.last_game = native.game.submitted;
        let mut action = None;
        let proof = self;
        {
            if proof.started.elapsed() > Duration::from_secs(60) {
                return Err(imgui_error(format!(
                    "native view lifecycle timed out at phase {:?}: Scene {:?}, Game {:?}, Play {:?}, backend {} (retired {:?})",
                    proof.phase,
                    native.scene.submitted,
                    native.game.submitted,
                    application
                        .current_frame()
                        .map(|frame| frame.play().state()),
                    native.gpu.backend_instance_id(),
                    proof.retired_backend,
                )));
            }
            if outcome.window().is_some() && proof.first_present_ms.is_none() {
                proof.first_present_ms = Some(proof.started.elapsed().as_millis());
            }
            let scene = native.scene.submitted;
            let game = native.game.submitted;
            let primary_scene = outcome
                .composites()
                .iter()
                .any(|receipt| Some(receipt.image()) == scene);
            let primary_game = outcome
                .composites()
                .iter()
                .any(|receipt| Some(receipt.image()) == game);
            let secondary_game = secondary_outcomes
                .iter()
                .flat_map(WgpuEditorFrameOutcome::composites)
                .any(|receipt| Some(receipt.image()) == game);
            if let Some(frame) = application.current_frame() {
                if frame.play().state() == EditorPlayState::Empty
                    && proof.phase == LifecyclePhase::Primary
                {
                    if let Some(scene) = &frame.workspace().active_scene {
                        action = Some(EditorAction::Play(EditorPlayAction::Play {
                            document: scene.document,
                            base_revision: scene.revision,
                        }));
                    }
                }
            }
            match proof.phase {
                LifecyclePhase::Primary if primary_scene && primary_game => {
                    proof.initial_scene_view.get_or_insert(
                        native
                            .scene
                            .view
                            .expect("a presented Scene has its GPU view"),
                    );
                    proof.initial_game_view.get_or_insert(
                        native.game.view.expect("a presented Game has its GPU view"),
                    );
                    proof.primary_frames += 1;
                    if proof.primary_frames >= 4 {
                        native.frontend.place_test_game(true);
                        proof.phase = LifecyclePhase::Detached;
                    }
                }
                LifecyclePhase::Detached if primary_scene && secondary_game => {
                    proof.secondary_frames += 1;
                    if proof.secondary_frames >= 4 {
                        set_minimized_without_activation(&native.window, true)?;
                        proof.phase = LifecyclePhase::Minimized;
                    }
                }
                LifecyclePhase::Minimized
                    if secondary_game && native.window.is_minimized() == Some(true) =>
                {
                    proof.minimized_secondary_frames += 1;
                    if proof.minimized_secondary_frames >= 4 {
                        set_minimized_without_activation(&native.window, false)?;
                        proof.phase = LifecyclePhase::RestoringPrimary;
                    }
                }
                LifecyclePhase::RestoringPrimary
                    if primary_scene
                        && secondary_game
                        && native.window.is_minimized() == Some(false) =>
                {
                    native.frontend.place_test_game(false);
                    proof.phase = LifecyclePhase::Returned;
                }
                LifecyclePhase::Returned if primary_scene && primary_game => {
                    proof.returned_frames += 1;
                    if proof.returned_frames >= 4 {
                        if !proof.stale_receipt_rejected {
                            return Err(imgui_error(
                                "the native lifecycle did not reject an overwritten image receipt",
                            ));
                        }
                        proof.phase = LifecyclePhase::GameClosed;
                        set_panel_open(application.panel_workspace_mut(), "nara.game-view", false);
                    }
                }
                LifecyclePhase::GameClosed
                    if primary_scene
                        && native.game.view.is_none()
                        && native.game_candidate.is_none() =>
                {
                    if !application
                        .current_frame()
                        .is_some_and(|frame| frame.play().state() == EditorPlayState::Running)
                    {
                        return Err(imgui_error("closing Game stopped its Runtime"));
                    }
                    if native.scene.view != proof.initial_scene_view {
                        return Err(imgui_error(
                            "closing Game replaced Scene's independent view",
                        ));
                    }
                    proof.game_closed_frames += 1;
                    if proof.game_closed_frames >= 4 {
                        set_panel_open(application.panel_workspace_mut(), "nara.game-view", true);
                        proof.phase = LifecyclePhase::GameReopening;
                    }
                }
                LifecyclePhase::GameReopening if primary_scene && primary_game => {
                    if native.game.view == proof.initial_game_view
                        || native.scene.view != proof.initial_scene_view
                    {
                        return Err(imgui_error(
                            "reopening Game reused its retired identity or replaced Scene",
                        ));
                    }
                    proof.reopened_game_view = native.game.view;
                    set_panel_open(
                        application.panel_workspace_mut(),
                        "nara.authoring-view",
                        false,
                    );
                    proof.phase = LifecyclePhase::SceneClosed;
                }
                LifecyclePhase::SceneClosed if primary_game && native.scene.view.is_none() => {
                    if native.game.view != proof.reopened_game_view {
                        return Err(imgui_error(
                            "closing Scene replaced Game's independent view",
                        ));
                    }
                    proof.scene_closed_frames += 1;
                    if proof.scene_closed_frames >= 4 {
                        set_panel_open(
                            application.panel_workspace_mut(),
                            "nara.authoring-view",
                            true,
                        );
                        proof.phase = LifecyclePhase::Reopened;
                    }
                }
                LifecyclePhase::Reopened if primary_scene && primary_game => {
                    if native.scene.view == proof.initial_scene_view
                        || native.game.view != proof.reopened_game_view
                    {
                        return Err(imgui_error(
                            "reopening Scene reused its retired identity or replaced Game",
                        ));
                    }
                    proof.reopened_frames += 1;
                    if proof.reopened_frames >= 4 {
                        proof.phase = LifecyclePhase::Recovering;
                        proof.retired_backend = native
                            .game
                            .submitted
                            .map(|image| image.backend_instance_id());
                        native.gpu.wait_for_idle().map_err(platform_gpu_error)?;
                        native
                            .gpu
                            .with_device(window_packet, |scope| scope.device().destroy())
                            .map_err(platform_gpu_error)?;
                    }
                }
                LifecyclePhase::Recovering
                    if primary_scene
                        && primary_game
                        && native.game.submitted.is_some_and(|image| {
                            Some(image.backend_instance_id()) != proof.retired_backend
                        }) =>
                {
                    if !application
                        .current_frame()
                        .is_some_and(|frame| frame.play().state() == EditorPlayState::Paused)
                    {
                        return Err(imgui_error(
                            "GPU replacement did not pause the active Runtime",
                        ));
                    }
                    proof.phase = LifecyclePhase::Recovered;
                    action = Some(EditorAction::Play(EditorPlayAction::Resume));
                }
                LifecyclePhase::Recovered if primary_scene && primary_game => {
                    proof.recovered_frames += 1;
                    if proof.recovered_frames >= 4 {
                        proof.phase = LifecyclePhase::Stopping;
                        action = Some(EditorAction::Play(EditorPlayAction::Stop));
                    }
                }
                LifecyclePhase::Stopping
                    if application.active_game_start_request().is_none()
                        && application.pending_game_start_request().is_none() =>
                {
                    if native.game.submitted.is_some()
                        || native.game_candidate.is_some()
                        || native.game.identity.is_some()
                    {
                        return Err(imgui_error(
                            "Stop retained an interactive Game image lifetime",
                        ));
                    }
                    proof.phase = LifecyclePhase::Complete;
                    return Ok(true);
                }
                _ => {}
            }
        }

        if let Some(action) = action {
            let frame = application
                .current_frame()
                .ok_or_else(|| imgui_error("the lifecycle action lost its workspace"))?;
            let dispatch = application
                .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), [action]));
            if dispatch.invalidates_game_input() {
                native
                    .input
                    .invalidate_after_control(application.active_game_start_request());
                native.game_candidate = None;
            }
            application.drive_editor_frame(
                Duration::ZERO,
                dispatch.actions_observed(),
                dispatch.requires_runtime_drive() || dispatch.requests_step(),
            );
        }
        Ok(false)
    }
}

fn set_panel_open(workspace: &mut crate::workspace::EditorPanelWorkspace, kind: &str, open: bool) {
    let ids = workspace
        .layout()
        .instances()
        .iter()
        .filter(|instance| instance.kind() == kind)
        .map(|instance| instance.id())
        .collect::<Vec<_>>();
    for id in ids {
        if open {
            workspace.layout_mut().reopen(id);
        } else {
            workspace.layout_mut().close(id);
        }
    }
}

fn set_minimized_without_activation(
    window: &Window,
    minimized: bool,
) -> Result<(), EditorRunError> {
    let handle = window.window_handle().map_err(imgui_error)?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err(imgui_error(
            "the Windows lifecycle test did not receive its owned HWND",
        ));
    };
    // Winit uses SW_RESTORE, which can take focus from the user's foreground app. This test
    // changes only its owned HWND with the non-activating Win32 minimize/restore commands.
    unsafe {
        ShowWindow(
            handle.hwnd.get() as _,
            if minimized {
                SW_SHOWMINNOACTIVE
            } else {
                SW_SHOWNOACTIVATE
            },
        );
    }
    Ok(())
}

pub(super) fn hidden_window(
    event_loop: &ActiveEventLoop,
    title: &str,
) -> Result<Arc<Window>, EditorRunError> {
    event_loop
        .create_window(
            Window::default_attributes()
                .with_title(title)
                .with_visible(false)
                .with_active(false)
                .with_inner_size(LogicalSize::new(1600.0, 1100.0)),
        )
        .map(Arc::new)
        .map_err(imgui_error)
}

struct LifecycleTestHost {
    host: NativeEditorHost,
    proof: NativeLifecycleProof,
    finished: bool,
    wake_events: u32,
    quiet_wakes_preserved_frames: bool,
    wakes_acknowledged: bool,
}

impl LifecycleTestHost {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: EditorRunError) {
        self.host.error.get_or_insert(error);
        event_loop.exit();
    }
}

impl ApplicationHandler<EditorUserEvent> for LifecycleTestHost {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EditorUserEvent) {
        let frame = self.host.application.current_frame();
        let was_pending = self.host.supervised_play_wake.pending();
        self.host.user_event(event_loop, event);
        self.wake_events += 1;
        self.wakes_acknowledged &= was_pending && !self.host.supervised_play_wake.pending();
        self.quiet_wakes_preserved_frames &= frame
            .zip(self.host.application.current_frame())
            .is_some_and(|(before, after)| Arc::ptr_eq(&before, &after));
        if self.wake_events == 1 {
            // A real acknowledged proxy event must rearm the same coalesced notification.
            self.host.supervised_play_wake.notify();
            self.host.supervised_play_wake.notify();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.host.window.is_none() {
            match hidden_window(event_loop, "Nara native view lifecycle proof") {
                Ok(window) => self.host.window = Some(window),
                Err(error) => {
                    self.fail(event_loop, imgui_error(error));
                    return;
                }
            }
        }
        self.host.resumed(event_loop);
        if let Some(native) = &mut self.host.native {
            native.layout_loaded = true;
            let flags = native.context.io().config_flags()
                | dear_imgui_rs::ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
            native.context.io_mut().set_config_flags(flags);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let redraw = matches!(event, WindowEvent::RedrawRequested);
        if let Some(native) = &mut self.host.native {
            let flags = native.context.io().config_flags()
                | dear_imgui_rs::ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
            native.context.io_mut().set_config_flags(flags);
        }
        self.host.window_event(event_loop, window_id, event);
        if self.host.error.is_some() {
            return;
        }
        if redraw && let Some(native) = &mut self.host.native {
            if let Some((outcome, secondary)) = native.lifecycle_frame.take() {
                // The backend must receive this policy before showing a new secondary window.
                // No global input, explicit focus request, or foreground activation is used.
                let unfocused =
                    native
                        .context
                        .platform_io()
                        .viewports_iter()
                        .skip(1)
                        .all(|viewport| {
                            viewport
                                .flags()
                                .contains(dear_imgui_rs::ViewportFlags::NO_FOCUS_ON_APPEARING)
                        });
                if !unfocused {
                    self.fail(
                        event_loop,
                        imgui_error("a test secondary viewport lost NO_FOCUS_ON_APPEARING"),
                    );
                    return;
                }
                match self
                    .proof
                    .observe(native, &mut self.host.application, outcome, secondary)
                {
                    Ok(true) => {
                        self.finished = true;
                        event_loop.exit();
                    }
                    Ok(false) => {}
                    Err(error) => self.fail(event_loop, error),
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.host.about_to_wait(event_loop);
        if self.proof.started.elapsed() > Duration::from_secs(60) {
            let frame = self.host.application.current_frame();
            let native = self.host.native.as_ref().map(|native| {
                format!(
                    "frame={}, presented={}, Scene={:?}, Game={:?}, Game candidate={:?}, composites={:?}",
                    native.frame_index,
                    native.presented,
                    native.scene.submitted,
                    native.game.submitted,
                    native.game_candidate,
                    native.lifecycle_frame.as_ref().map(|(primary, secondary)| (
                        primary.window(), primary.composites().len(),
                        secondary.iter().map(|frame| frame.composites().len()).collect::<Vec<_>>()
                    ))
                )
            });
            self.fail(
                event_loop,
                imgui_error(format!(
                    "the native lifecycle timed out at {:?}; application={:?}; Play={:?}; result={:?}; availability={:?}; active={:?}; pending={:?}; native={native:?}",
                    self.proof.phase,
                    self.host.application.state(),
                    frame.as_ref().map(|frame| frame.play().state()),
                    frame.as_ref().and_then(|frame| frame.play().result()),
                    frame.as_ref().map(|frame| frame.runtime_availability()),
                    self.host.application.active_game_start_request(),
                    self.host.application.pending_game_start_request(),
                )),
            );
            return;
        }
        if let Some(window) = &self.host.window {
            // Hidden Winit windows need not receive RedrawRequested. Drive the same owned-window
            // production handler explicitly, as the other hidden native proofs drive rendering.
            // This still executes actual GPU submission/presentation and recovery; it supplies
            // no receipt or lifecycle result and never shows or activates the primary window.
            self.window_event(event_loop, window.id(), WindowEvent::RedrawRequested);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}

#[test]
#[ignore = "requires Windows native secondary windows, a WGPU adapter, and real device replacement"]
fn native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts() {
    let project = UiProject::new();
    let application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project.0).with_profile("desktop"),
        super::game_input_tests::game_view_intent().unwrap(),
    )
    .unwrap_or_else(|error| {
        panic!(
            "trusted native lifecycle Open rejected: {error:?}; diagnostics={:?}",
            error.diagnostics()
        )
    });
    let mut test = LifecycleTestHost {
        host: NativeEditorHost {
            application,
            supervised_play_wake: SupervisedPlayWake::default(),
            native: None,
            retiring: None,
            retirement_action: NativeRetirementAction::Hold,
            retirement_errors: NativeCloseErrors::default(),
            window: None,
            retained: None,
            gpu_recovery: EditorGpuRecovery::Ready,
            error: None,
            last_drive: None,
            repaint_deadline: None,
        },
        proof: NativeLifecycleProof::new(),
        finished: false,
        wake_events: 0,
        quiet_wakes_preserved_frames: true,
        wakes_acknowledged: true,
    };
    let mut builder = EventLoop::<EditorUserEvent>::with_user_event();
    builder.with_any_thread(true);
    match builder.build() {
        Ok(event_loop) => {
            let proxy = event_loop.create_proxy();
            let wake = SupervisedPlayWake::new(move || {
                proxy
                    .send_event(EditorUserEvent::SupervisedPlayWake)
                    .is_ok()
            });
            test.host
                .application
                .install_supervised_play_wake(wake.clone());
            test.host.supervised_play_wake = wake;
            test.host.supervised_play_wake.notify();
            test.host.supervised_play_wake.notify();
            if let Err(error) = event_loop.run_app(&mut test) {
                test.host.error.get_or_insert_with(|| imgui_error(error));
            }
        }
        Err(error) => {
            test.host.error.get_or_insert_with(|| imgui_error(error));
        }
    }
    // All behavior assertions follow the real renderer -> platform -> Session cleanup.
    let graphics = retirement::finish_host_graphics_for_test(&mut test.host);
    let semantic_views_retired = test
        .host
        .application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .all(|panel| panel.render_view().is_none());
    let cleanup_order = (test.host.native.is_some() || test.host.retiring.is_some())
        && test
            .host
            .native
            .iter()
            .chain(test.host.retiring.iter())
            .all(|native| {
                native.close.is_complete()
                    && native.scene.view.is_none()
                    && native.game.view.is_none()
                    && native.context.platform_io().viewports_iter().count() == 1
            });
    test.host.native.take();
    test.host.retiring.take();
    test.host.window.take();
    let retirement = super::canvas_tests::retire_application(&mut test.host.application);
    let failure = test.host.error.take();
    assert!(
        failure.is_none(),
        "native lifecycle failed after cleanup: {failure:?}"
    );
    graphics.unwrap();
    retirement.unwrap();
    assert!(test.finished);
    assert_eq!(
        test.wake_events, 2,
        "two notification bursts must yield exactly two acknowledged native proxy events"
    );
    assert!(test.wakes_acknowledged);
    assert!(test.quiet_wakes_preserved_frames);
    assert!(!test.host.supervised_play_wake.pending());
    assert_eq!(test.proof.phase, LifecyclePhase::Complete);
    assert!(test.proof.primary_frames >= 4);
    assert!(test.proof.secondary_frames >= 4);
    assert!(test.proof.minimized_secondary_frames >= 4);
    assert!(test.proof.returned_frames >= 4);
    assert!(test.proof.game_closed_frames >= 4);
    assert!(test.proof.scene_closed_frames >= 4);
    assert!(test.proof.reopened_frames >= 4);
    assert!(test.proof.recovered_frames >= 4);
    assert!(test.proof.stale_receipt_rejected);
    assert!(test.proof.retired_backend.is_some());
    assert_ne!(test.proof.initial_scene_view, test.proof.initial_game_view);
    assert_ne!(test.proof.initial_game_view, test.proof.reopened_game_view);
    assert!(semantic_views_retired);
    assert!(test.proof.first_present_ms.is_some());
    assert!(
        cleanup_order,
        "all graphics/native view leases must retire in owner order"
    );
}
