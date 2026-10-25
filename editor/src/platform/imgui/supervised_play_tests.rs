#![cfg(target_os = "windows")]

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{ConfigFlags, MouseButton};
use nara::tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorCloseDecision, EditorDocumentKind,
    EditorPlayAction, EditorPlayOperation, EditorPlayOperationResult, EditorWorkspaceIntentPhase,
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

use super::{
    NativeImgui,
    retirement::{finish_graphics_for_test, initialize_native_for_test},
    supervised_play_support::SupervisedPlayTestFixture,
};
use crate::{
    EditorApplication, EditorLaunchOptions,
    frontend::imgui::{clear_controls, control_clip, control_rect, control_visible},
};

#[test]
#[ignore = "requires a Windows desktop, a WGPU adapter, and real supervised Cargo builds"]
fn native_imgui_supervised_play_preserves_last_good_and_successor_generations() {
    let options =
        EditorLaunchOptions::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../reference-game"))
            .with_profile("desktop");
    // The existing canary owns its guarded project copy and document edits. Play controls use
    // synthetic input on the actual widgets; no OS input or focus request is injected.
    let (application, canary) = SupervisedPlayTestFixture::new(options).unwrap();
    let mut host = SupervisedPlayTestHost {
        application,
        canary: Some(canary),
        native: None,
        failure: None,
        primary_present_count: 0,
        manager_present_count: 0,
        workspace_present_count: 0,
        last_drive: Instant::now(),
        next_redraw: Instant::now() + Duration::from_millis(16),
        exit_requested: false,
        exit_ready: false,
        pending_play_click: None,
        play_button_clicks: Vec::new(),
    };
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    match builder.build() {
        Ok(event_loop) => {
            if let Err(error) = event_loop.run_app(&mut host) {
                host.failure.get_or_insert(error.to_string());
            }
        }
        Err(error) => {
            host.failure.get_or_insert(error.to_string());
        }
    }
    // Every assertion follows normal graphics shutdown and physical Session/process retirement,
    // including errors from native construction, presentation, canary actions, or the event loop.
    host.cleanup();
    let evidence = host
        .canary
        .take()
        .expect("the existing canary owner remains installed")
        .finish()
        .map_err(|error| error.to_string());
    let failure = host.failure.take();
    let primary_present_count = host.primary_present_count;
    let manager_present_count = host.manager_present_count;
    let workspace_present_count = host.workspace_present_count;
    let play_button_clicks = std::mem::take(&mut host.play_button_clicks);
    let exit_ready = host.exit_ready;
    drop(host);
    assert!(
        failure.is_none(),
        "native supervised Play failed after cleanup: {failure:?}"
    );
    let evidence = evidence.unwrap();
    assert!(
        exit_ready,
        "the application must finish its real exit before releasing the fixture"
    );
    assert!(primary_present_count > 0);
    assert!(
        manager_present_count > 0,
        "the actual Project Manager must be presented before Open"
    );
    assert!(
        workspace_present_count > 0,
        "the ordinary project workspace must actually be presented"
    );
    assert_eq!(evidence.first_generation(), evidence.last_good_generation());
    assert_ne!(evidence.first_generation(), evidence.successor_generation());
    assert!(evidence.successor_saved_revision() > evidence.first_saved_revision());
    assert_eq!(
        play_button_clicks,
        [
            "project:Play",
            "project:Stop",
            "project:Play",
            "project:Run Last Good",
            "project:Stop",
            "project:Play",
            "project:Stop",
        ]
    );
    eprintln!(
        "{}",
        serde_json::json!({
            "kind": "nara_imgui_supervised_play_native_test",
            "actions": "semantic canary document edits and saves; actual Play widgets through synthetic local IO; no injected OS input",
            "play_button_clicks": play_button_clicks,
            "primary_present_count": primary_present_count,
            "manager_present_count": manager_present_count,
            "workspace_present_count": workspace_present_count,
            "first_saved_revision": evidence.first_saved_revision(),
            "successor_saved_revision": evidence.successor_saved_revision(),
            "first_generation": evidence.first_generation(),
            "last_good_generation": evidence.last_good_generation(),
            "successor_generation": evidence.successor_generation(),
            "first_output_sequence": evidence.first_output_sequence(),
            "successor_output_sequence": evidence.successor_output_sequence(),
            "first_output_truncated": evidence.first_output_truncated(),
            "successor_output_truncated": evidence.successor_output_truncated(),
            "cleanup": "complete",
        })
    );
}

struct SupervisedPlayTestHost {
    application: EditorApplication,
    canary: Option<SupervisedPlayTestFixture>,
    native: Option<NativeImgui>,
    failure: Option<String>,
    primary_present_count: u64,
    manager_present_count: u64,
    workspace_present_count: u64,
    last_drive: Instant,
    next_redraw: Instant,
    exit_requested: bool,
    exit_ready: bool,
    pending_play_click: Option<PlayButtonClick>,
    play_button_clicks: Vec<&'static str>,
}

struct PlayButtonClick {
    control: &'static str,
    operation: EditorPlayOperation,
    point: [f32; 2],
    frame: u8,
}

impl PlayButtonClick {
    fn new(action: EditorPlayAction) -> Result<Self, String> {
        let (control, operation) = match action {
            EditorPlayAction::Play { .. } => ("project:Play", EditorPlayOperation::Play),
            EditorPlayAction::Stop => ("project:Stop", EditorPlayOperation::Stop),
            EditorPlayAction::RunLastGood => {
                ("project:Run Last Good", EditorPlayOperation::RunLastGood)
            }
            other => {
                return Err(format!(
                    "the canary requested an unsupported Play widget: {other:?}"
                ));
            }
        };
        Ok(Self {
            control,
            operation,
            point: [-1000.0, -1000.0],
            frame: 0,
        })
    }

    fn check_result(&self, application: &EditorApplication) -> Result<(), String> {
        let result = application
            .current_frame()
            .and_then(|frame| frame.play().result());
        let operation = match result {
            Some(
                EditorPlayOperationResult::Pending { operation }
                | EditorPlayOperationResult::Applied { operation, .. }
                | EditorPlayOperationResult::Failed { operation, .. },
            ) => operation,
            // A real build failure belongs to the canary's existing last-good checks. Rejected,
            // cancelled, or missing widget dispatch must fail here instead of being retried.
            _ => {
                return Err(format!(
                    "Play widget {:?} did not dispatch its action: {result:?}",
                    self.control
                ));
            }
        };
        if operation != self.operation {
            return Err(format!(
                "Play widget {:?} dispatched another operation: {result:?}",
                self.control
            ));
        }
        Ok(())
    }

    fn refresh_point(&mut self) -> Result<(), String> {
        let rect = control_rect(self.control)
            .ok_or_else(|| format!("the queued Play widget {:?} disappeared", self.control))?;
        self.point = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        if !control_visible(self.control)
            || !control_clip(self.control).is_some_and(|clip| clip.contains(self.point))
        {
            return Err(format!(
                "Play widget {:?} has no visible click center; rect={rect:?}; clip={:?}; visible={}",
                self.control,
                control_clip(self.control),
                control_visible(self.control),
            ));
        }
        Ok(())
    }

    fn trace(&self, application: &EditorApplication, phase: &str) {
        let Some(frame) = application.current_frame() else {
            return;
        };
        let read_only = application.ordinary_input_locked();
        let can_request = !read_only
            && frame.workspace_intent().phase().is_none()
            && frame.persistence().operation().is_none();
        let content_ready = frame.project_content().is_current()
            && !frame
                .workspace()
                .documents
                .iter()
                .any(|document| document.kind == EditorDocumentKind::Prefab && document.dirty);
        eprintln!(
            "{}",
            serde_json::json!({
                "kind": "nara_imgui_supervised_play_widget",
                "phase": phase,
                "control": self.control,
                "click_frame": self.frame,
                "pointer": self.point,
                "rect": format!("{:?}", control_rect(self.control)),
                "clip": format!("{:?}", control_clip(self.control)),
                "visible": control_visible(self.control),
                "can_request": can_request,
                "content_ready": content_ready,
                "project_content": format!("{:?}", frame.project_content()),
                "runtime_availability": format!("{:?}", frame.runtime_availability()),
                "read_only": read_only,
                "persistence": format!("{:?}", frame.persistence()),
                "workspace_intent": format!("{:?}", frame.workspace_intent().phase()),
                "documents": frame.workspace().documents.iter().map(|document| {
                    format!("{:?}:kind={:?}:dirty={}:history={:?}", document.document, document.kind, document.dirty, document.history)
                }).collect::<Vec<_>>(),
                "play_result": format!("{:?}", frame.play().result()),
                "external": frame.play().external().map(|external| format!("{:?}", external.state())),
            })
        );
    }
}

impl SupervisedPlayTestHost {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.failure.get_or_insert_with(|| error.to_string());
        event_loop.exit();
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let rendered_state = self.application.state();
        let workspace = self.application.current_frame().is_some();
        let native = self
            .native
            .as_mut()
            .ok_or("the native Editor is unavailable")?;
        if let Some(click) = &mut self.pending_play_click {
            click.trace(&self.application, "before input");
            // Use the most recently painted actual item each frame, while retaining exactly one
            // press and release. Missing or clipped geometry fails instead of retrying a click.
            if (1..=3).contains(&click.frame) {
                click.refresh_point()?;
            }
        }
        let pointer = self
            .pending_play_click
            .as_ref()
            .map_or([-1000.0, -1000.0], |click| click.point);
        let button = self
            .pending_play_click
            .as_ref()
            .and_then(|click| match click.frame {
                2 => Some(true),
                3 => Some(false),
                _ => None,
            });
        clear_controls();
        let exit_ready = native
            .redraw_with_input(event_loop, &mut self.application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                // Replace native polling events after prepare_frame, preserving held synthetic
                // buttons across frames without moving or activating any native window.
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event(pointer);
                io.add_mouse_viewport_event(viewport);
                if let Some(pressed) = button {
                    io.add_mouse_button_event(MouseButton::Left, pressed);
                }
            })
            .map_err(|error| error.to_string())?;
        let stats = native.gpu.frame_transaction_stats();
        // Delegated viewport presents also increment this transaction's presentation counter.
        // Only the primary backend acquires a surface; a successful redraw pairs that acquire
        // with its completed present, so secondary windows cannot inflate this primary count.
        let presents = stats.surface_acquires().min(stats.presents());
        self.primary_present_count += presents;
        if workspace {
            self.workspace_present_count += presents;
        } else {
            self.manager_present_count += presents;
        }
        if exit_ready {
            self.exit_ready = true;
            event_loop.exit();
            return Ok(());
        }
        if let Some(click) = &mut self.pending_play_click {
            click.trace(&self.application, "after redraw");
            if click.frame >= 3 {
                click.check_result(&self.application)?;
            }
            // First paint the owner's successor state, then position, press, release and finish
            // two follow-up frames. The last registry may still describe the predecessor state.
            click.frame += 1;
            if click.frame == 6 {
                self.play_button_clicks.push(click.control);
                self.pending_play_click = None;
            }
            native.window.request_redraw();
            return Ok(());
        }
        if !native.presented || self.exit_requested {
            return Ok(());
        }
        let canary = self
            .canary
            .as_mut()
            .ok_or("the semantic canary owner is unavailable")?;
        if workspace {
            let advance = canary
                .advance_workspace(&mut self.application, native.window.id())
                .map_err(|error| error.to_string())?;
            if let Some(action) = advance.action {
                let action = match action {
                    EditorAction::Play(action) => {
                        self.pending_play_click = Some(PlayButtonClick::new(action)?);
                        native.window.request_redraw();
                        return Ok(());
                    }
                    action => action,
                };
                let frame = self
                    .application
                    .current_frame()
                    .ok_or("the canary action lost its current workspace frame")?;
                let description = format!("{action:?}");
                let dispatch = self
                    .application
                    .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), [action]));
                let failed = match dispatch.feedback() {
                    Some(Ok(report)) => report.outcomes().iter().any(|outcome| match outcome {
                        EditorActionOutcome::Rejected(_) => true,
                        EditorActionOutcome::Workspace(report) => !report.applied,
                        _ => false,
                    }),
                    Some(Err(_)) | None => true,
                };
                if failed {
                    return Err(format!(
                        "semantic canary dispatch failed: action={description}; feedback={:?}",
                        dispatch.feedback()
                    ));
                }
                let update = self.application.drive_editor_frame(
                    Duration::ZERO,
                    dispatch.actions_observed(),
                    true,
                );
                if update.exit_ready() {
                    self.exit_ready = true;
                    event_loop.exit();
                    return Ok(());
                }
            }
            if advance.request_exit {
                self.exit_requested = true;
                if self.application.request_exit().exit_ready() {
                    self.exit_ready = true;
                    event_loop.exit();
                }
            }
            if advance.exit {
                event_loop.exit();
            }
            if advance.redraw {
                native.window.request_redraw();
            }
        } else if canary
            .advance_project_shell(rendered_state, &mut self.application, native.window.id())
            .map_err(|error| error.to_string())?
        {
            native.window.request_redraw();
        }
        Ok(())
    }

    fn cleanup(&mut self) {
        if let Some(mut native) = self.native.take() {
            if let Err(error) = finish_graphics_for_test(&mut native) {
                self.record_cleanup_failure(format!("native graphics cleanup failed: {error}"));
            }
            drop(native);
        }
        if self.exit_ready {
            return;
        }
        if self.application.request_exit().exit_ready() {
            self.exit_ready = true;
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let decision = self.application.current_frame().filter(|frame| {
                frame.workspace_intent().phase()
                    == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
            });
            let update = if let Some(frame) = decision {
                self.application
                    .submit_and_drive(
                        EditorActionBatch::new(
                            frame.revision(),
                            [EditorAction::WorkspaceIntent(
                                frame
                                    .workspace_intent()
                                    .decision_prompt()
                                    .unwrap()
                                    .resolve(EditorCloseDecision::Discard),
                            )],
                        ),
                        Duration::ZERO,
                    )
                    .update()
            } else {
                self.application
                    .drive_editor_frame(Duration::ZERO, false, true)
            };
            if update.exit_ready() {
                self.exit_ready = true;
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.record_cleanup_failure(format!(
            "the supervised Session did not finish physical cleanup; state={:?}; play={:?}",
            self.application.state(),
            self.application
                .current_frame()
                .map(|frame| format!("{:?}", frame.play())),
        ));
    }

    fn record_cleanup_failure(&mut self, error: String) {
        match &mut self.failure {
            Some(original) => {
                original.push_str("; ");
                original.push_str(&error);
            }
            None => self.failure = Some(error),
        }
    }
}

impl ApplicationHandler for SupervisedPlayTestHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native.is_some() || self.failure.is_some() {
            return;
        }
        let initialized = (|| {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Nara Dear ImGui supervised Play regression")
                            .with_active(false)
                            .with_inner_size(LogicalSize::new(1200.0, 800.0)),
                    )
                    .map_err(|error| error.to_string())?,
            );
            let mut native = initialize_native_for_test(event_loop, window)?;
            native.layout_loaded = true;
            let mut flags = native.context.io().config_flags();
            flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
            flags.insert(ConfigFlags::NO_MOUSE_CURSOR_CHANGE);
            native.context.io_mut().set_config_flags(flags);
            native.window.request_redraw();
            Ok::<_, String>(native)
        })();
        match initialized {
            Ok(native) => self.native = Some(native),
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(native) = self.native.as_mut() else {
            return;
        };
        let primary = native.window.id() == window_id;
        let routed = (|| {
            if native
                .input
                .handle_event(window_id, &event, &native.context)
            {
                native
                    .platform
                    .handle_window_event(&mut native.context, window_id, &event)
                    .map_err(|error| error.to_string())?;
            }
            native
                .input
                .flush(&mut self.application)
                .map_err(|error| error.to_string())
        })();
        if let Err(error) = routed {
            self.fail(event_loop, error);
            return;
        }
        match event {
            WindowEvent::CloseRequested if primary => self.fail(
                event_loop,
                "the native Editor was closed before its supervised Play trajectory finished",
            ),
            WindowEvent::RedrawRequested if primary => {
                if let Err(error) = self.redraw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if let Some(canary) = &self.canary
            && now >= canary.deadline()
        {
            self.fail(event_loop, canary.timeout_error(&self.application));
            return;
        }
        let delta = now.saturating_duration_since(self.last_drive);
        self.last_drive = now;
        let opened = self.application.advance_project_open();
        let update = self
            .application
            .drive_editor_frame(delta, opened.actions_observed(), true);
        if opened.exit_ready() || update.exit_ready() {
            self.exit_ready = true;
            event_loop.exit();
            return;
        }
        if now >= self.next_redraw {
            if let Some(native) = &self.native {
                native.window.request_redraw();
            }
            self.next_redraw = now + Duration::from_millis(16);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_redraw));
    }
}
