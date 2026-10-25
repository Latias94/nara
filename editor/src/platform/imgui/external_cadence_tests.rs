use super::*;
use crate::EditorLaunchOptions;
use dear_imgui_rs::ConfigFlags;
use nara::tooling::{
    EditorAction, EditorActionOutcome, EditorExternalPlayState, EditorPlayAction,
    EditorPlayRequestResult,
};
use std::{fs, path::Path};
use winit::platform::windows::EventLoopBuilderExtWindows;

use super::supervised_play_support::fixture::GuardedSupervisedPlayProject;

const EXIT_MARKER: &str = ".nara-native-cadence-exit";

#[test]
#[ignore = "requires a Windows desktop, WGPU adapter, and one real supervised Cargo build"]
fn native_external_running_maintenance_stays_quiet_and_presents_child_exit() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../reference-game");
    let project = GuardedSupervisedPlayProject::prepare(&source).unwrap();
    let desktop = project.root().join("src/bin/desktop.rs");
    let source = fs::read_to_string(&desktop).unwrap();
    let exit_marker = project.root().join(EXIT_MARKER);
    let marker_literal = format!("{:?}", exit_marker.to_string_lossy().as_ref());
    assert!(source.contains("fn main() -> ExitCode {"));
    // Only the guarded copy gains a bounded natural-exit trigger. Play still uses its ordinary
    // Cargo product, inherited bootstrap, source admission, native Runtime and READY handshake.
    let source = source
        .replacen(
            "fn main() -> ExitCode {",
            r#"fn main() -> ExitCode {
    std::thread::spawn(|| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        while !std::path::Path::new(__NARA_EXIT_MARKER__).exists() {
            if std::time::Instant::now() >= deadline { std::process::exit(124); }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::process::exit(0);
    });"#,
            1,
        )
        .replace("__NARA_EXIT_MARKER__", &marker_literal);
    fs::write(desktop, source).unwrap();
    let application =
        EditorApplication::new(EditorLaunchOptions::new(project.root()).with_profile("desktop"));
    let mut test = ExternalCadenceHost {
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
        exit_marker,
        deadline: Instant::now() + Duration::from_secs(30),
        play_requested: false,
        running_started: false,
        last_phase: String::new(),
        quiet: None,
        maintenance_cycles: 0,
        exit_requested: false,
        exit_observed: false,
        exit_presented: false,
        failure: None,
    };
    let mut builder = EventLoop::<EditorUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let result = builder.build().and_then(|event_loop| {
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
        event_loop.run_app(&mut test)
    });
    // Release this one copied child on every error path before retiring its actual owner.
    let release = fs::write(&test.exit_marker, b"exit");
    let graphics = retirement::finish_host_graphics_for_test(&mut test.host);
    test.host.native.take();
    test.host.retiring.take();
    test.host.window.take();
    let retirement = super::canvas_tests::retire_application(&mut test.host.application);
    assert!(result.is_ok(), "native event loop failed: {result:?}");
    assert!(
        release.is_ok(),
        "the copied child's exit marker must be writable"
    );
    graphics.unwrap();
    retirement.unwrap();
    assert!(test.failure.is_none(), "{:?}", test.failure);
    assert!(test.maintenance_cycles >= 3);
    assert!(test.exit_observed);
    assert!(
        test.exit_presented,
        "the natural child exit must reach an actual native present"
    );
    eprintln!(
        "external cadence: {} quiet maintenance cycles; natural exit observed and presented; cleanup complete",
        test.maintenance_cycles
    );
}

struct ExternalCadenceHost {
    host: NativeEditorHost,
    exit_marker: std::path::PathBuf,
    deadline: Instant,
    play_requested: bool,
    running_started: bool,
    last_phase: String,
    quiet: Option<(u64, u64)>,
    maintenance_cycles: u32,
    exit_requested: bool,
    exit_observed: bool,
    exit_presented: bool,
    failure: Option<String>,
}

impl ExternalCadenceHost {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.failure.get_or_insert_with(|| error.to_string());
        event_loop.exit();
    }

    fn observe_exit(&mut self) {
        if self.exit_requested
            && self.host.application.current_frame().is_some_and(|frame| {
                frame
                    .play()
                    .external()
                    .is_some_and(|external| external.state() != EditorExternalPlayState::Running)
            })
        {
            self.exit_observed = true;
        }
    }

    fn trace_phase(&mut self) {
        let frame = self.host.application.current_frame();
        let phase = format!(
            "{:?}/{:?}",
            self.host.application.state(),
            frame
                .as_ref()
                .and_then(|frame| frame.play().external().map(|view| view.state()))
        );
        if self.last_phase != phase {
            eprintln!(
                "external cadence phase={phase}; availability={:?}; Play={:?}",
                frame.as_ref().map(|frame| frame.runtime_availability()),
                frame.as_ref().and_then(|frame| frame.play().result())
            );
            self.last_phase = phase;
        }
    }

    fn request_play(&mut self, event_loop: &ActiveEventLoop) {
        let Some(frame) = self.host.application.current_frame() else {
            return;
        };
        if frame.runtime_availability() != EditorRuntimeAvailability::ExternalSupervised {
            return self.fail(
                event_loop,
                format!(
                    "ordinary Open cannot run external Play: {:?}",
                    frame.runtime_availability()
                ),
            );
        }
        let Some(scene) = frame.workspace().active_scene.as_ref() else {
            return self.fail(event_loop, "ordinary Open has no startup Scene");
        };
        let dispatch = self
            .host
            .application
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Play(EditorPlayAction::Play {
                    document: scene.document,
                    base_revision: scene.revision,
                })],
            ));
        let accepted = matches!(dispatch.feedback(), Some(Ok(report))
            if matches!(report.outcomes(), [EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)]));
        if !accepted {
            return self.fail(
                event_loop,
                format!("ordinary Play was not admitted: {:?}", dispatch.feedback()),
            );
        }
        self.host
            .application
            .drive_editor_frame(Duration::ZERO, dispatch.actions_observed(), true);
        self.play_requested = true;
        self.deadline = Instant::now() + Duration::from_secs(600);
        self.trace_phase();
        // Match the ordinary UI-output tail: the accepted action publishes its successor,
        // requests presentation, and then the production about_to_wait schedules owner work.
        if let Some(native) = &self.host.native {
            native.window.request_redraw();
        }
    }
}

impl ApplicationHandler<EditorUserEvent> for ExternalCadenceHost {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EditorUserEvent) {
        self.host.user_event(event_loop, event);
        self.observe_exit();
        self.trace_phase();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Windows must deliver the production redraw requests while this trajectory waits.
        // A hidden HWND can suppress that delivery, so use a visible, inactive test window.
        match event_loop.create_window(
            Window::default_attributes()
                .with_title("Nara external cadence proof")
                .with_visible(true)
                .with_active(false)
                .with_inner_size(LogicalSize::new(1600.0, 1100.0)),
        ) {
            Ok(window) => self.host.window = Some(Arc::new(window)),
            Err(error) => return self.fail(event_loop, error),
        }
        self.host.resumed(event_loop);
        if let Some(native) = &mut self.host.native {
            native.layout_loaded = true;
            let mut flags =
                native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
            flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
            native.context.io_mut().set_config_flags(flags);
        }
        self.trace_phase();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window: WindowId, event: WindowEvent) {
        let redraw = matches!(event, WindowEvent::RedrawRequested);
        self.host.window_event(event_loop, window, event);
        if let Some(error) = &self.host.error {
            return self.fail(event_loop, error.to_string());
        }
        self.observe_exit();
        if redraw
            && !self.play_requested
            && self
                .host
                .native
                .as_ref()
                .is_some_and(|native| native.presented)
            && self.host.application.current_frame().is_some()
        {
            self.request_play(event_loop);
        }
        if redraw
            && self.exit_observed
            && self
                .host
                .native
                .as_ref()
                .is_some_and(|native| native.presented)
        {
            self.exit_presented = true;
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.advance(event_loop);
        // Every phase, including ordinary Open before a workspace exists, keeps its watchdog.
        // This never adds an animation timer or requests a redraw.
        let deadline = match event_loop.control_flow() {
            ControlFlow::WaitUntil(deadline) => deadline.min(self.deadline),
            _ => self.deadline,
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
    }
}

impl ExternalCadenceHost {
    fn advance(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.deadline {
            return self.fail(event_loop, "external cadence trajectory timed out");
        }
        let maintenance_before = self.host.application.next_supervised_play_deadline();
        let maintenance_due = maintenance_before.is_some_and(|deadline| now >= deadline);
        self.host.about_to_wait(event_loop);
        self.trace_phase();
        if let Some(error) = &self.host.error {
            return self.fail(event_loop, error.to_string());
        }
        let Some(frame) = self.host.application.current_frame() else {
            if self.host.application.state() == crate::EditorApplicationState::Recovery {
                self.fail(event_loop, "ordinary project Open entered Recovery");
            }
            return;
        };
        if self.play_requested
            && let Some(external) = frame.play().external()
        {
            if self.exit_requested {
                if external.state() != EditorExternalPlayState::Running {
                    self.exit_observed = true;
                }
            } else if external.state() == EditorExternalPlayState::Running {
                if !self.running_started {
                    self.running_started = true;
                    self.deadline = now + Duration::from_secs(30);
                }
                if frame.runtime_availability() != EditorRuntimeAvailability::ExternalSupervised {
                    return self.fail(event_loop, "READY did not publish ExternalSupervised");
                }
                let Some(native) = &self.host.native else {
                    return;
                };
                let settled = native.presented
                    && !native.ui_followup
                    && !native.context.io().want_text_input()
                    && !frame.project_browser().has_pending_work()
                    && !frame.image_import_work_pending()
                    && matches!(event_loop.control_flow(), ControlFlow::WaitUntil(_));
                if let Some((frame_index, output_sequence)) = self.quiet {
                    if native.frame_index != frame_index
                        || external.output_sequence() != output_sequence
                    {
                        return self.fail(
                            event_loop,
                            "quiet external maintenance caused a new native redraw or output",
                        );
                    }
                    if maintenance_due {
                        let Some(next_observation) =
                            self.host.application.next_supervised_play_deadline()
                        else {
                            return self.fail(
                                event_loop,
                                "quiet maintenance lost its process observation deadline",
                            );
                        };
                        if next_observation <= now
                            || maintenance_before.is_some_and(|before| next_observation <= before)
                            || !matches!(event_loop.control_flow(), ControlFlow::WaitUntil(deadline) if deadline == next_observation)
                            || self.host.repaint_deadline.is_some()
                        {
                            return self.fail(event_loop, "quiet maintenance did not advance its exact observation deadline without presentation");
                        }
                        self.maintenance_cycles += 1;
                        eprintln!(
                            "external cadence quiet maintenance={}",
                            self.maintenance_cycles
                        );
                        if self.maintenance_cycles == 3 {
                            if let Err(error) = fs::write(&self.exit_marker, b"exit") {
                                return self.fail(event_loop, error);
                            }
                            self.exit_requested = true;
                            self.deadline = now + Duration::from_secs(30);
                        }
                    }
                } else if settled {
                    self.quiet = Some((native.frame_index, external.output_sequence()));
                }
            } else if matches!(
                external.state(),
                EditorExternalPlayState::Idle
                    | EditorExternalPlayState::Faulted
                    | EditorExternalPlayState::CleanupIncomplete
            ) {
                return self.fail(
                    event_loop,
                    format!("real supervised Play failed: {:?}", frame.play().result()),
                );
            }
        } else if self.play_requested {
            return self.fail(event_loop, "accepted Play lost its external projection");
        }
    }
}
