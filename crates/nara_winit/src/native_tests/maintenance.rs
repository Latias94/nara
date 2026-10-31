//! Maintenance-only wakes observed on the original Windows pump and actual native window.

use super::*;
use nara_app::{CoreStage, RealTime};
use nara_ecs::{ResMut, Resource};
use std::{cell::Cell, rc::Rc};
use winit::event::StartCause;

const MAINTENANCE_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
struct FrameCounts {
    gameplay: u64,
    last: u64,
}

fn count_gameplay(mut counts: ResMut<FrameCounts>) {
    counts.gameplay += 1;
}

fn count_last(mut counts: ResMut<FrameCounts>) {
    counts.last += 1;
}

fn frame_counts(runtime: &RuntimeInstance) -> (u64, FrameCounts) {
    (
        runtime.world().resource::<RealTime>().frame,
        *runtime.world().resource::<FrameCounts>(),
    )
}

struct MaintenanceDriver {
    exit_requested: Rc<Cell<bool>>,
    calls: Rc<Cell<usize>>,
    deadline: Instant,
}

impl __WinitPresentationDriver for MaintenanceDriver {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        now: Instant,
    ) -> Result<__WinitPresentationProgress, AppRunError> {
        self.calls.set(self.calls.get() + 1);
        let mut progress = NoPresentationDriver.maintain(runtime, handles, now)?;
        if self.exit_requested.get() || now >= self.deadline {
            progress.exit = Some(AppExit::Requested);
        } else {
            progress.next_wake = Some((now + MAINTENANCE_INTERVAL).min(self.deadline));
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

struct MaintenanceProbe {
    state: WinitApp,
    start_cause: StartCause,
    window_demand: bool,
    quiet_wakes: usize,
    redraw_origin: Option<(Instant, Instant)>,
    redraw_observed: bool,
    redraw_advanced: bool,
    exit_from_maintenance: Option<StartCause>,
    exit_requested: Rc<Cell<bool>>,
    maintenance_calls: Rc<Cell<usize>>,
    failure: Option<&'static str>,
}

impl ApplicationHandler<WinitUserEvent> for MaintenanceProbe {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.start_cause = cause;
        self.window_demand = false;
        self.state.new_events(event_loop, cause);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.state.resumed(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WinitWindowId,
        event: WinitWindowEvent,
    ) {
        self.window_demand |= window_event_requests_frame(&event);
        if self.redraw_origin.is_some() && matches!(&event, WinitWindowEvent::RedrawRequested) {
            self.redraw_observed = true;
        }
        self.state.window_event(event_loop, window_id, event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: WinitUserEvent) {
        self.state.user_event(event_loop, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let before = frame_counts(&self.state.runtime);
        let last_frame = self.state.last_frame;
        let calls_before = self.maintenance_calls.get();
        let maintenance_only = !self.window_demand
            && !self.state.frame_requested
            && !self.state.shutdown.is_started()
            && before.0 > 0;
        self.state.about_to_wait(event_loop);
        let after = frame_counts(&self.state.runtime);

        // A new pump call can resume with WaitCancelled before the requested timer expires.
        // That is still a maintenance-only boundary when no native frame demand was delivered.
        // Check every such boundary, including the one that consumes the driver's exit request.
        if maintenance_only {
            if before != after {
                self.failure
                    .get_or_insert("maintenance wake advanced Runtime, gameplay or Last");
            }
            if self.state.last_frame != last_frame {
                self.failure
                    .get_or_insert("maintenance wake consumed the frame delta clock");
            }
            if self.maintenance_calls.get() <= calls_before {
                self.failure
                    .get_or_insert("maintenance wake did not service the local driver");
            }
            if self.state.shutdown.is_started() {
                self.exit_from_maintenance = Some(self.start_cause);
            }
        }

        // Keep the timer evidence separate: immediate pump re-entry must not count toward the
        // six actual deadline wakes required by this native test.
        if maintenance_only && matches!(self.start_cause, StartCause::ResumeTimeReached { .. }) {
            self.quiet_wakes += 1;
            if self.quiet_wakes >= 3
                && self.redraw_origin.is_none()
                && let Some(window) = self.state.platform_windows.get(&WindowId::PRIMARY)
            {
                self.redraw_origin = Some((last_frame, Instant::now()));
                // This is one actual native redraw request. The preceding idle interval has no
                // synthetic redraws, and the production handler must honor this real demand.
                window.request_redraw();
            }
            if self.quiet_wakes >= 6 && self.redraw_advanced {
                self.exit_requested.set(true);
            }
        }

        if after.0 > before.0
            && !self.redraw_advanced
            && self.redraw_observed
            && let Some((previous_frame, requested_at)) = self.redraw_origin
        {
            let delta = self.state.runtime.world().resource::<RealTime>().delta;
            if delta < requested_at.saturating_duration_since(previous_frame) {
                self.failure
                    .get_or_insert("real redraw lost time spent in maintenance-only wakes");
            }
            if after.1.gameplay != before.1.gameplay + 1 || after.1.last != before.1.last + 1 {
                self.failure
                    .get_or_insert("real redraw did not advance gameplay and Last exactly once");
            }
            self.redraw_advanced = true;
        }
        if self.failure.is_some() {
            self.exit_requested.set(true);
        }
    }
}

#[test]
#[ignore = "requires an idle interactive Windows desktop; run in its own nextest process"]
fn wait_maintenance_wakes_do_not_advance_gameplay_or_last_on_the_retained_pump() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let mut event_loop = builder.build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara maintenance-only pump",
            WindowResolution::new(320, 180),
        )),
    })
    .unwrap();
    app.init_resource::<FrameCounts>().unwrap();
    app.add_systems(CoreStage::Update, count_gameplay).unwrap();
    app.add_systems(CoreStage::Last, count_last).unwrap();
    let exit_requested = Rc::new(Cell::new(false));
    let maintenance_calls = Rc::new(Cell::new(0));
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut state = WinitApp::new(
        RuntimeInstance::start(app).unwrap(),
        Box::new(MaintenanceDriver {
            exit_requested: exit_requested.clone(),
            calls: maintenance_calls.clone(),
            deadline,
        }),
        WinitControlFlow::Wait,
        Duration::from_millis(500),
    );
    state.event_proxy = Some(event_loop.create_proxy());
    let handles = state.backend_windows.clone();
    let initial_frame = frame_counts(&state.runtime).0;
    let mut probe = MaintenanceProbe {
        state,
        start_cause: StartCause::Init,
        window_demand: false,
        quiet_wakes: 0,
        redraw_origin: None,
        redraw_observed: false,
        redraw_advanced: false,
        exit_from_maintenance: None,
        exit_requested,
        maintenance_calls,
        failure: None,
    };
    loop {
        if let PumpStatus::Exit(_) =
            event_loop.pump_app_events(Some(Duration::from_millis(40)), &mut probe)
        {
            break;
        }
        if Instant::now() >= deadline + Duration::from_secs(1) {
            probe
                .failure
                .get_or_insert("native maintenance probe exceeded its finite deadline");
            probe.state.begin_shutdown_transition(Instant::now());
        }
        if probe.state.shutdown == WinitShutdownState::Complete
            || probe.state.retirement_drive_incomplete(Instant::now())
        {
            break;
        }
    }
    let mut event_loop = WinitEventLoop::from_native(event_loop);
    event_loop.resumed = probe.state.native_resumed;
    let mut session = WinitSession {
        app: Some(probe.state),
        event_loop: Some(event_loop),
        executed: true,
    };
    session.execute();
    assert!(session.is_retired());
    assert_eq!(probe.failure, None);
    assert!(
        frame_counts(session.runtime()).0 > initial_frame,
        "the initial real frame never ran"
    );
    assert!(
        probe.quiet_wakes >= 6,
        "too few actual timer wakes without window demand"
    );
    assert!(
        probe.redraw_observed,
        "the explicit redraw was not dispatched by the native pump"
    );
    assert!(
        probe.redraw_advanced,
        "the explicit native redraw never advanced a real frame"
    );
    assert!(
        probe.exit_from_maintenance.is_some(),
        "driver exit was not observed at a maintenance-only boundary"
    );
    eprintln!(
        "maintenance-only exit: cause={:?}, timer_wakes={}, counts={:?}",
        probe.exit_from_maintenance,
        probe.quiet_wakes,
        frame_counts(session.runtime())
    );
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}
