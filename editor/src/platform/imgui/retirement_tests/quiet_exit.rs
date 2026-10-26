use std::sync::{Arc, mpsc::Sender};

use dear_imgui_rs::ContextBinding;
use nara_tasks::{TaskPoolKind, WorkerSpawnFailureProbe};
use winit::event::StartCause;

use super::*;

pub(super) fn qualify() {
    let (application, release, probe) =
        crate::app::startup_retirement_tests::held_startup_application();
    let mut test = QuietExitHost {
        host: NativeEditorHost::new(
            application,
            SupervisedPlayWake::default(),
            ImguiEditorFrontend::default(),
        ),
        release: Some(release),
        probe,
        owner: None,
        phase: Phase::Starting,
        delivered_redraws: 0,
        presented_frames: 0,
        close_presents: 0,
        manager_focus_released: false,
        manager_escape_down: false,
        settling: None,
        maintenance_deadline: None,
        deadline_wake: false,
        quiet_wakes: 0,
        show_after: None,
        shown: false,
        deadline: Instant::now() + RETIREMENT_TIMEOUT,
        completed: false,
        failure: None,
    };
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_result = builder
        .build()
        .and_then(|event_loop| event_loop.run_app(&mut test));

    // Release on every failure path before the shared finite cleanup driver joins the worker.
    if let Some(release) = test.release.take() {
        let _ = release.send(());
    }
    let cleanup = cleanup_host(&mut test.host);
    event_result.unwrap();
    cleanup.unwrap();
    assert!(test.failure.is_none(), "{:?}", test.failure);
    assert!(
        test.completed,
        "production Host did not complete native exit"
    );
    assert!(test.quiet_wakes >= 3);
    eprintln!(
        "quiet native exit: {} actual maintenance wakes; redraws={}; presents={}; visible fallback={}; physical Open Host and graphics retirement complete",
        test.quiet_wakes, test.delivered_redraws, test.presented_frames, test.shown,
    );
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Starting,
    Closing,
    Quiet(Presentation),
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Presentation {
    delivered_redraws: u64,
    frame_index: u64,
    presented_frames: u64,
    transaction_presents: u64,
}

struct NativeOwner {
    binding: ContextBinding,
    backend: u64,
    window: WindowId,
}

struct QuietExitHost {
    host: NativeEditorHost,
    release: Option<Sender<()>>,
    probe: WorkerSpawnFailureProbe,
    owner: Option<NativeOwner>,
    phase: Phase,
    delivered_redraws: u64,
    presented_frames: u64,
    close_presents: u64,
    manager_focus_released: bool,
    manager_escape_down: bool,
    settling: Option<Presentation>,
    maintenance_deadline: Option<Instant>,
    deadline_wake: bool,
    quiet_wakes: u32,
    show_after: Option<Instant>,
    shown: bool,
    deadline: Instant,
    completed: bool,
    failure: Option<String>,
}

impl QuietExitHost {
    fn callback(
        &mut self,
        event_loop: &ActiveEventLoop,
        callback: impl FnOnce(&mut Self) -> Result<(), String>,
    ) {
        if self.failure.is_some() {
            return;
        }
        // As in the other retirement proofs, keep actual owners outside the unwind boundary.
        let result = catch_unwind(AssertUnwindSafe(|| {
            callback(self)?;
            if let Some(error) = &self.host.error {
                return Err(error.to_string());
            }
            if event_loop.exiting() {
                self.observe_exit()?;
            }
            Ok(())
        }))
        .unwrap_or_else(|panic| {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic");
            Err(format!("quiet native exit callback panicked: {message}"))
        });
        if let Err(error) = result {
            self.failure = Some(error);
            event_loop.exit();
        }
    }

    fn presentation(&self) -> Result<Presentation, String> {
        let native = self
            .host
            .native
            .as_ref()
            .ok_or("the live native owner was lost")?;
        Ok(Presentation {
            delivered_redraws: self.delivered_redraws,
            frame_index: native.frame_index,
            presented_frames: self.presented_frames,
            transaction_presents: native.gpu.frame_transaction_stats().presents(),
        })
    }

    fn retained_worker_and_owner(&self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let owner = self
            .owner
            .as_ref()
            .ok_or("the native identity was not recorded")?;
        let native = self
            .host
            .native
            .as_ref()
            .ok_or("graphics retired before worker return")?;
        check(
            !event_loop.exiting()
                && self.probe.started() == 1
                && self.probe.exited() == 0
                && !self
                    .host
                    .application
                    .open_host_tasks_close_report()
                    .is_complete()
                && self.host.retiring.is_none()
                && native.context.id() == owner.binding.id()
                && owner.binding.lifecycle() == ContextLifecycle::Alive
                && native.gpu.backend_instance_id() == owner.backend
                && native.window.id() == owner.window,
            "held startup worker lost its physical custody or exact live native owner",
        )
    }

    fn check_quiet(&self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if let Phase::Quiet(baseline) = self.phase {
            self.retained_worker_and_owner(event_loop)?;
            let current = self.presentation()?;
            check(
                current == baseline,
                &format!(
                    "unchanged retirement delivered a redraw, frame or presentation after settling: baseline={baseline:?}, current={current:?}, wakes={}, deadline_wake={}",
                    self.quiet_wakes, self.deadline_wake
                ),
            )?;
        }
        Ok(())
    }

    fn observe_exit(&mut self) -> Result<(), String> {
        let owner = self
            .owner
            .as_ref()
            .ok_or("exit preceded native initialization")?;
        let report = self.host.application.open_host_tasks_close_report();
        check(
            matches!(self.phase, Phase::Released)
                && self.quiet_wakes >= 3
                && self.probe.exited() == 1
                && report.is_complete()
                && report.tasks().for_kind(TaskPoolKind::Io).joined_workers == 1
                && self.host.application.request_exit().exit_ready()
                && self.host.retirement_action == NativeRetirementAction::Exit
                && self.host.native.is_none()
                && self.host.retiring.is_none()
                && owner.binding.lifecycle() == ContextLifecycle::NativeDestroyed
                && self.host.retirement_errors.is_empty(),
            "process exit preceded physical Open Host join or native graphics retirement",
        )?;
        self.completed = true;
        Ok(())
    }

    fn advance(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let now = Instant::now();
        if now >= self.deadline {
            return Err(format!(
                "quiet native exit timed out: phase={:?}, state={:?}, redraws={}, presents={}, close_presents={}, wakes={}, deadline={:?}, native={:?}, repaint={:?}",
                self.phase,
                self.host.application.state(),
                self.delivered_redraws,
                self.presented_frames,
                self.close_presents,
                self.quiet_wakes,
                event_loop.control_flow(),
                self.host.native.as_ref().map(|native| (
                    native.frame_index,
                    native.presented,
                    native.ui_followup,
                    native.context.io().want_text_input(),
                )),
                self.host.repaint_deadline,
            ));
        }
        self.check_quiet(event_loop)?;
        if self.show_after.is_some_and(|deadline| now >= deadline) {
            // Hidden Windows HWNDs can suppress redraw delivery (see external_cadence_tests).
            // Show the same inactive window only if startup/close redraws have not settled.
            if matches!(self.phase, Phase::Starting | Phase::Closing) {
                self.host
                    .window
                    .as_ref()
                    .ok_or("the test window was lost")?
                    .set_visible(true);
                self.shown = true;
            }
            self.show_after = None;
        }

        self.host.about_to_wait(event_loop);
        self.check_quiet(event_loop)?;
        if event_loop.exiting() {
            return Ok(());
        }
        if matches!(self.phase, Phase::Closing | Phase::Quiet(_)) {
            self.retained_worker_and_owner(event_loop)?;
            let native = self.host.native.as_ref().unwrap();
            let settled = self.host.application.current_frame().is_none()
                && self.host.application.has_pending_exit()
                && self.host.application.project_open_retirement_pending()
                && self.host.retirement_action == NativeRetirementAction::AwaitingExitReady
                && self.presented_frames > self.close_presents
                && native.presented
                && !native.ui_followup
                && !native.context.io().want_text_input()
                && self.host.repaint_deadline.is_none();
            let maintenance = match event_loop.control_flow() {
                ControlFlow::WaitUntil(deadline) if deadline > Instant::now() => Some(deadline),
                _ => None,
            };
            if matches!(self.phase, Phase::Quiet(_)) {
                check(
                    settled && maintenance.is_some(),
                    "quiet exit lost its maintenance-only deadline or exit latch",
                )?;
                if self.deadline_wake {
                    self.quiet_wakes += 1;
                }
            } else if settled && maintenance.is_some() {
                let presentation = self.presentation()?;
                if self.deadline_wake && self.settling == Some(presentation) {
                    self.phase = Phase::Quiet(presentation);
                    self.show_after = None;
                } else {
                    self.settling = Some(presentation);
                }
            } else {
                self.settling = None;
            }
            self.maintenance_deadline = maintenance;
        } else {
            self.maintenance_deadline = None;
        }
        if matches!(self.phase, Phase::Quiet(_)) && self.quiet_wakes >= 3 {
            // The third actual maintenance wake completes the unchanged-state proof. Waiting
            // for a fourth can cross the real join deadline and legitimately change its report.
            self.check_quiet(event_loop)?;
            check(
                !self.host.application.request_exit().exit_ready(),
                "Exit became ready while the startup worker was held",
            )?;
            self.release
                .take()
                .ok_or("the worker was already released")?
                .send(())
                .map_err(|error| error.to_string())?;
            self.phase = Phase::Released;
            self.deadline = Instant::now() + RETIREMENT_TIMEOUT;
        }
        self.deadline_wake = false;

        // The watchdog/visibility fallback must never masquerade as a production deadline wake.
        let mut watchdog = self.deadline;
        if let Some(show_after) = self.show_after {
            watchdog = watchdog.min(show_after);
        }
        let deadline = match event_loop.control_flow() {
            ControlFlow::WaitUntil(deadline) => deadline.min(watchdog),
            _ => watchdog,
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        Ok(())
    }
}

impl ApplicationHandler for QuietExitHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.callback(event_loop, |test| {
            if test.owner.is_some() {
                return Ok(());
            }
            test.host.window = Some(Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Nara quiet native exit proof")
                            .with_visible(false)
                            .with_active(false)
                            .with_inner_size(LogicalSize::new(800.0, 600.0)),
                    )
                    .map_err(|error| error.to_string())?,
            ));
            initialize(&mut test.host, event_loop)?;
            let native = test
                .host
                .native
                .as_mut()
                .ok_or("the native owner did not initialize")?;
            let mut flags = native.context.io().config_flags();
            flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
            native.context.io_mut().set_config_flags(flags);
            test.owner = Some(NativeOwner {
                binding: native.context.binding(),
                backend: native.gpu.backend_instance_id(),
                window: native.window.id(),
            });
            test.show_after = Some(Instant::now() + Duration::from_millis(500));
            Ok(())
        });
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.callback(event_loop, |test| {
            test.deadline_wake = matches!(
                cause,
                StartCause::ResumeTimeReached { start, requested_resume }
                    if test.maintenance_deadline == Some(requested_resume)
                        && requested_resume > start
                        && Instant::now() >= requested_resume
            );
            test.check_quiet(event_loop)?;
            Ok(())
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window: WindowId, event: WindowEvent) {
        self.callback(event_loop, |test| {
            let redraw = matches!(event, WindowEvent::RedrawRequested)
                && test
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.window == window);
            if redraw {
                test.delivered_redraws += 1;
            }
            test.host.window_event(event_loop, window, event);
            if redraw
                && test
                    .host
                    .native
                    .as_ref()
                    .is_some_and(|native| native.presented)
            {
                test.presented_frames += 1;
                if matches!(test.phase, Phase::Starting) {
                    test.close_presents = test.presented_frames;
                    test.phase = Phase::Closing;
                    test.host
                        .window_event(event_loop, window, WindowEvent::CloseRequested);
                }
            }
            if redraw
                && matches!(test.phase, Phase::Closing)
                && test.host.application.current_frame().is_none()
                && !test.manager_focus_released
                && (test.manager_escape_down
                    || test
                        .host
                        .native
                        .as_ref()
                        .is_some_and(|native| native.context.io().want_text_input()))
            {
                let native = test
                    .host
                    .native
                    .as_mut()
                    .ok_or("the native owner was lost")?;
                let io = native.context.io_mut();
                if test.manager_escape_down {
                    io.add_key_event(dear_imgui_rs::Key::Escape, false);
                    test.manager_focus_released = true;
                } else {
                    io.add_focus_event(true);
                    io.add_key_event(dear_imgui_rs::Key::Escape, true);
                    test.manager_escape_down = true;
                }
                test.host
                    .window
                    .as_ref()
                    .ok_or("the test window was lost")?
                    .request_redraw();
            }
            test.check_quiet(event_loop)
        });
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.callback(event_loop, |test| test.advance(event_loop));
    }
}
