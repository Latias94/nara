//! Real retained-pump probes. Run each ignored test in a separate nextest process on Windows.

use super::*;
use nara_app::App;
use nara_window::{WindowPlugin, backend::WindowTargetPhase};
use std::sync::mpsc::{self, Sender};
use winit::platform::windows::EventLoopBuilderExtWindows;

mod maintenance;

#[derive(Debug)]
pub(super) struct NativeTestEvent {
    pub window: Sender<Arc<WinitWindow>>,
}

pub(super) fn handle_event(
    app: &mut WinitApp,
    event_loop: &ActiveEventLoop,
    event: NativeTestEvent,
) {
    if let Some(window) = app.platform_windows.get(&WindowId::PRIMARY) {
        let _ = event.window.send(window.clone());
    }
    // Exercise the actual Exit/reset path of the pinned Windows pump, not a mock pump result.
    event_loop.exit();
}

struct BoundedPlatformDriver {
    deadline: Instant,
}

impl __WinitPresentationDriver for BoundedPlatformDriver {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        now: Instant,
    ) -> Result<__WinitPresentationProgress, AppRunError> {
        let mut progress = NoPresentationDriver.maintain(runtime, handles, now)?;
        if now >= self.deadline {
            progress.exit = Some(AppExit::Requested);
        } else {
            progress.next_wake = Some(self.deadline);
        }
        Ok(progress)
    }

    fn retire(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        ids: &[WindowId],
        now: Instant,
    ) -> Result<bool, AppRunError> {
        NoPresentationDriver.retire(runtime, handles, ids, now)
    }
}

fn native_session(budget: Duration) -> WinitSession {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara retained native session",
            WindowResolution::new(320, 180),
        )),
    })
    .unwrap();
    let mut state = WinitApp::new(
        RuntimeInstance::start(app).unwrap(),
        Box::new(BoundedPlatformDriver {
            deadline: Instant::now() + Duration::from_secs(3),
        }),
        WinitControlFlow::Wait,
        budget,
    );
    state.event_proxy = Some(event_loop.create_proxy());
    WinitSession {
        app: Some(state),
        event_loop: Some(event_loop),
        executed: false,
    }
}

fn exit_with_retained_window(session: &mut WinitSession) -> Arc<WinitWindow> {
    let (send, receive) = mpsc::channel();
    session
        .event_loop
        .as_ref()
        .unwrap()
        .create_proxy()
        .send_event(WinitUserEvent::NativeTest(NativeTestEvent { window: send }))
        .unwrap();
    assert_eq!(session.execute(), WinitRunProgress::Incomplete);
    receive.recv_timeout(Duration::from_secs(1)).unwrap()
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn early_exit_resumes_the_original_pump_without_recreating_the_window() {
    let mut session = native_session(Duration::from_millis(500));
    let pump = std::ptr::from_ref(session.event_loop.as_ref().unwrap());
    let generation = session.runtime().generation();
    let handles = session.app.as_ref().unwrap().backend_windows.clone();
    let window = exit_with_retained_window(&mut session);
    let native_id = window.id();
    assert_eq!(
        session
            .app
            .as_ref()
            .unwrap()
            .nara_windows_by_winit
            .get(&native_id),
        Some(&WindowId::PRIMARY)
    );
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::ProviderReleased
    );
    drop(window);

    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        std::ptr::from_ref(session.event_loop.as_ref().unwrap()),
        pump
    );
    assert_eq!(session.runtime().generation(), generation);
    let app = session.app.as_ref().unwrap();
    assert_eq!(app.owned_window_ids, BTreeSet::from([WindowId::PRIMARY]));
    assert!(app.nara_windows_by_winit.is_empty());
    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(snapshot.phase, WindowTargetPhase::NativeDestroyed);
    assert_eq!(snapshot.fault, None);
    let retired = session.into_retired().ok().unwrap();
    assert!(
        retired
            .result
            .unwrap_err()
            .to_string()
            .contains("event pump exited before controlled retirement")
    );
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn native_budget_expiry_returns_the_same_owner_then_retry_observes_destroyed() {
    let mut session = native_session(Duration::from_millis(20));
    let generation = session.runtime().generation();
    let handles = session.app.as_ref().unwrap().backend_windows.clone();
    // This explicit extra platform owner delays the last Window Drop, without inventing a GPU
    // surface or using missing render resources as a retirement acknowledgement.
    let window = exit_with_retained_window(&mut session);
    let native_id = window.id();
    let start = Instant::now();
    assert_eq!(session.execute(), WinitRunProgress::Incomplete);
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(session.runtime().state(), RuntimeState::Stopped);
    assert!(!session.is_retired());
    session = session.into_retired().err().unwrap();
    // Moving the incomplete aggregate is allowed; the original native objects move with it.
    let pump = std::ptr::from_ref(session.event_loop.as_ref().unwrap());
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(
        session
            .app
            .as_ref()
            .unwrap()
            .nara_windows_by_winit
            .get(&native_id),
        Some(&WindowId::PRIMARY)
    );
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::ProviderReleased
    );
    assert_eq!(
        session.app.as_ref().unwrap().native_retirement_failure,
        Some(AppRunError::runner(
            "native session retirement budget expired"
        ))
    );
    drop(window);

    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        std::ptr::from_ref(session.event_loop.as_ref().unwrap()),
        pump
    );
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    assert!(
        session
            .app
            .as_ref()
            .unwrap()
            .nara_windows_by_winit
            .is_empty()
    );
    assert!(session.into_retired().ok().unwrap().result.is_err());
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn event_loop_construction_failure_still_retires_the_supplied_runtime() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let original_pump = builder.build().unwrap();
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let runtime = RuntimeInstance::start(app).unwrap();
    let generation = runtime.generation();
    let session = WinitRunner::default().__run_with_presentation(
        runtime,
        Box::new(NoPresentationDriver),
        Duration::from_millis(500),
    );
    assert!(session.event_loop.is_none());
    assert!(session.is_retired());
    assert_eq!(session.runtime().generation(), generation);
    let retired = session.into_retired().ok().unwrap();
    assert_eq!(retired.runtime.state(), RuntimeState::Stopped);
    assert!(
        retired
            .result
            .unwrap_err()
            .to_string()
            .contains("failed to create winit event loop")
    );
    drop(original_pump);
}

#[test]
#[ignore = "spawns an aborting native child; requires an interactive Windows desktop"]
fn unfinished_native_session_drop_is_fail_stop() {
    use std::process::{Command, Stdio};

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_tests::unfinished_native_session_drop_child",
            "--ignored",
            "--nocapture",
        ])
        .env("NARA_WINIT_DROP_CHILD", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let timed_out = loop {
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            break true;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !timed_out,
        "unfinished native session Drop exceeded its finite budget"
    );
    assert!(
        !output.status.success(),
        "unfinished session Drop released pending native owners"
    );
    assert!(
        stderr.contains("NARA_WINIT_UNFINISHED_DROP"),
        "child failed before the Drop probe: {stderr}"
    );
    assert!(!stderr.contains("NARA_WINIT_DROP_RETURNED"));
}

#[test]
#[ignore = "internal subprocess fixture; invoked only by unfinished_native_session_drop_is_fail_stop"]
fn unfinished_native_session_drop_child() {
    if std::env::var_os("NARA_WINIT_DROP_CHILD").is_none() {
        return;
    }
    let mut session = native_session(Duration::from_millis(20));
    let retained_window = exit_with_retained_window(&mut session);
    use std::io::Write;
    let mut stderr = std::io::stderr();
    writeln!(stderr, "NARA_WINIT_UNFINISHED_DROP").unwrap();
    stderr.flush().unwrap();
    drop(session);
    writeln!(stderr, "NARA_WINIT_DROP_RETURNED").unwrap();
    std::hint::black_box(retained_window);
}
