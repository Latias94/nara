use super::*;
use nara_app::{CoreStage, FixedTime, RealTime, RuntimeTimeSettings, VirtualTime};
use nara_ecs::{ResMut, Resource};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    sync::Weak,
};

mod maintenance_wake;

const POLL_WAIT: Duration = Duration::from_millis(10);
const MAINTENANCE_INTERVAL: Duration = Duration::from_millis(20);
const QUIET_INTERVAL: Duration = Duration::from_millis(120);
const PHASE_BUDGET: Duration = Duration::from_secs(3);
const RETIREMENT_BUDGET: Duration = Duration::from_secs(3);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
struct FrameCounts {
    tasks: u64,
    fixed: u64,
    last: u64,
}

#[derive(Default)]
struct DriverCounts {
    maintained: Cell<u64>,
    deadline_wakes: Cell<u64>,
    retired: Cell<u64>,
}

struct CadenceDriver {
    counts: Rc<DriverCounts>,
    next_wake: Instant,
    deadline: Instant,
}

impl WinitPresentationDriver for CadenceDriver {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        now: Instant,
    ) -> Result<WinitPresentationProgress, AppRunError> {
        self.counts.maintained.set(self.counts.maintained.get() + 1);
        let mut progress = NoPresentationDriver.maintain(runtime, handles, now)?;
        if now >= self.deadline {
            progress.exit = Some(AppExit::Requested);
        } else {
            if now >= self.next_wake {
                self.counts
                    .deadline_wakes
                    .set(self.counts.deadline_wakes.get() + 1);
                self.next_wake = now + MAINTENANCE_INTERVAL;
            }
            progress.next_wake = Some(self.next_wake.min(self.deadline));
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
        self.counts.retired.set(self.counts.retired.get() + 1);
        NoPresentationDriver.retire(runtime, handles, ids, now)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Observation {
    counts: FrameCounts,
    real: RealTime,
    virtual_time: VirtualTime,
    fixed_tick: u64,
    last_frame: Instant,
}

fn observation(session: &WinitSession) -> Observation {
    let world = session.runtime().world();
    Observation {
        counts: *world.resource::<FrameCounts>(),
        real: *world.resource::<RealTime>(),
        virtual_time: *world.resource::<VirtualTime>(),
        fixed_tick: world.resource::<FixedTime>().tick(),
        last_frame: session.app.as_ref().unwrap().last_frame,
    }
}

fn poll_live(session: &mut WinitSession) {
    assert_eq!(session.poll(POLL_WAIT), WinitRunProgress::Running);
    let counts = observation(session).counts;
    assert_eq!(counts.tasks, counts.last, "an App frame did not reach Last");
}

fn wait_for_full_frame(session: &mut WinitSession, before: Observation) -> Observation {
    let deadline = Instant::now() + PHASE_BUDGET;
    loop {
        let current = observation(session);
        if current.counts.tasks > before.counts.tasks {
            assert_eq!(
                current.counts.tasks - before.counts.tasks,
                current.counts.last - before.counts.last,
                "full App demand did not run TaskUpdate and Last together"
            );
            assert!(current.real.frame > before.real.frame);
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "full App demand timed out: before={before:?}, current={current:?}"
        );
        poll_live(session);
    }
}

fn wait_for_quiet(session: &mut WinitSession, driver: &DriverCounts) -> Observation {
    let deadline = Instant::now() + PHASE_BUDGET;
    let mut stable = observation(session);
    let mut quiet_since = Instant::now();
    let mut maintained = driver.maintained.get();
    let mut deadline_wakes = driver.deadline_wakes.get();
    loop {
        assert!(
            Instant::now() < deadline,
            "Wait did not reach a quiet native stretch: {stable:?}"
        );
        poll_live(session);
        let current = observation(session);
        if current != stable {
            stable = current;
            quiet_since = Instant::now();
            maintained = driver.maintained.get();
            deadline_wakes = driver.deadline_wakes.get();
        } else if quiet_since.elapsed() >= QUIET_INTERVAL
            && driver.maintained.get() >= maintained + 3
            && driver.deadline_wakes.get() >= deadline_wakes + 3
        {
            assert!(session.paused_frame_complete());
            return stable;
        }
    }
}

fn settle_native_window(session: &mut WinitSession) -> (WinitWindowId, Weak<WinitWindow>) {
    let deadline = Instant::now() + PHASE_BUDGET;
    let mut geometry = None;
    let mut stable_since = Instant::now();
    loop {
        assert!(Instant::now() < deadline, "native window did not settle");
        poll_live(session);
        if let Some(window) = session
            .app
            .as_ref()
            .unwrap()
            .platform_windows
            .get(&WindowId::PRIMARY)
        {
            assert_eq!(window.is_visible(), Some(true));
            let current = (window.id(), window.inner_size(), window.scale_factor());
            let resolution = configured_primary_window(session.runtime())
                .unwrap()
                .resolution;
            let synchronized = current.1.width > 0
                && current.1.height > 0
                && resolution.physical_width == current.1.width
                && resolution.physical_height == current.1.height
                && (resolution.scale_factor - current.2).abs() < 0.001;
            if geometry != Some(current) || !synchronized {
                geometry = Some(current);
                stable_since = Instant::now();
            } else if stable_since.elapsed() >= QUIET_INTERVAL
                && observation(session).counts.tasks >= 4
            {
                return (window.id(), Arc::downgrade(window));
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn apply_control(session: &mut WinitSession, control: RuntimeControl) {
    let ticket = match session.request_control(control) {
        RuntimeControlRequestResult::Accepted(ticket) => ticket,
        rejected => panic!("native Wait rejected {control:?}: {rejected:?}"),
    };
    assert!(!session.paused_frame_complete());
    let deadline = Instant::now() + PHASE_BUDGET;
    loop {
        match session.control_status(ticket) {
            Some(RuntimeControlStatus::Applied) => return,
            Some(RuntimeControlStatus::Pending) => {}
            status => panic!("native control {control:?} failed: {status:?}"),
        }
        assert!(Instant::now() < deadline, "{control:?} did not wake Wait");
        poll_live(session);
    }
}

#[test]
#[ignore = "requires an idle interactive Windows desktop; run in its own nextest process"]
fn original_native_session_switches_poll_wait_and_retires_without_losing_demand() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara original native cadence",
            WindowResolution::new(320, 180),
        )),
    })
    .unwrap();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
        .unwrap();
    app.init_resource::<FrameCounts>().unwrap();
    app.add_systems(CoreStage::TaskUpdate, |mut counts: ResMut<FrameCounts>| {
        counts.tasks += 1;
    })
    .unwrap();
    app.add_systems(CoreStage::FixedUpdate, |mut counts: ResMut<FrameCounts>| {
        counts.fixed += 1;
    })
    .unwrap();
    app.add_systems(CoreStage::Last, |mut counts: ResMut<FrameCounts>| {
        counts.last += 1;
    })
    .unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let runtime = RuntimeInstance::start(app).unwrap();
    let generation = runtime.generation();
    let driver = Rc::new(DriverCounts::default());
    let now = Instant::now();
    let mut session = WinitRunner::new(WinitControlFlow::Poll).start_with_presentation_on(
        runtime,
        Box::new(CadenceDriver {
            counts: driver.clone(),
            next_wake: now + MAINTENANCE_INTERVAL,
            deadline: now + Duration::from_secs(20),
        }),
        RETIREMENT_BUDGET,
        Some(event_loop),
    );
    let original_pump = session.event_loop.as_ref().map(std::ptr::from_ref);
    let mut original_window = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        assert!(original_pump.is_some());
        assert_eq!(session.control_flow(), WinitControlFlow::Poll);
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
        let (native_id, weak) = settle_native_window(&mut session);
        original_window = Some(weak);
        let polled = observation(&session);
        assert!(polled.counts.tasks >= 4);
        assert_eq!(polled.counts.fixed, 0);
        assert_eq!(polled.fixed_tick, 0);
        assert_eq!(polled.virtual_time.elapsed, Duration::ZERO);

        session.set_control_flow(WinitControlFlow::Wait).unwrap();
        assert_eq!(session.control_flow(), WinitControlFlow::Wait);
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
        let quiet = wait_for_quiet(&mut session, &driver);
        assert_eq!(quiet.fixed_tick, polled.fixed_tick);
        assert_eq!(quiet.virtual_time.elapsed, polled.virtual_time.elapsed);

        let redraw_at = Instant::now();
        session
            .app
            .as_ref()
            .unwrap()
            .platform_windows
            .get(&WindowId::PRIMARY)
            .unwrap()
            .request_redraw();
        let redrawn = wait_for_full_frame(&mut session, quiet);
        assert!(
            redrawn.real.elapsed - quiet.real.elapsed
                >= redraw_at.saturating_duration_since(quiet.last_frame),
            "native redraw lost elapsed time spent in maintenance-only wakes"
        );
        let quiet = wait_for_quiet(&mut session, &driver);
        assert_eq!(quiet.virtual_time.elapsed, Duration::ZERO);
        assert_eq!(quiet.counts.fixed, 0);

        let window = session
            .app
            .as_ref()
            .unwrap()
            .platform_windows
            .get(&WindowId::PRIMARY)
            .unwrap();
        assert_eq!(window.id(), native_id);
        let size = window.inner_size();
        let requested = PhysicalSize::new(size.width + 64, size.height + 48);
        let _ = window.request_inner_size(requested);
        let deadline = Instant::now() + PHASE_BUDGET;
        loop {
            assert!(
                Instant::now() < deadline,
                "original native resize timed out"
            );
            poll_live(&mut session);
            let window = session
                .app
                .as_ref()
                .unwrap()
                .platform_windows
                .get(&WindowId::PRIMARY)
                .unwrap();
            assert_eq!(window.id(), native_id);
            let resolution = configured_primary_window(session.runtime())
                .unwrap()
                .resolution;
            if window.inner_size() == requested
                && resolution.physical_width == requested.width
                && resolution.physical_height == requested.height
                && (resolution.scale_factor - window.scale_factor()).abs() < 0.001
                && observation(&session).counts.tasks > quiet.counts.tasks
            {
                break;
            }
        }
        wait_for_full_frame(&mut session, quiet);
        let quiet = wait_for_quiet(&mut session, &driver);
        assert_eq!(quiet.fixed_tick, 0);
        let resolution = configured_primary_window(session.runtime())
            .unwrap()
            .resolution;
        assert_eq!(resolution.physical_width, requested.width);
        assert_eq!(resolution.physical_height, requested.height);

        let scoped = session.with_driver_scope(|_| ()).unwrap();
        assert_eq!(scoped.post_operation_fault, None);
        assert!(!session.paused_frame_complete());
        wait_for_full_frame(&mut session, quiet);
        let quiet = wait_for_quiet(&mut session, &driver);
        assert_eq!(quiet.counts.fixed, 0);

        apply_control(&mut session, RuntimeControl::StepFixedTick);
        wait_for_full_frame(&mut session, quiet);
        let stepped = wait_for_quiet(&mut session, &driver);
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
        assert_eq!(stepped.counts.fixed, quiet.counts.fixed + 1);
        assert_eq!(stepped.fixed_tick, quiet.fixed_tick + 1);
        assert_eq!(
            stepped.virtual_time.elapsed - quiet.virtual_time.elapsed,
            session.runtime().world().resource::<FixedTime>().timestep()
        );

        apply_control(&mut session, RuntimeControl::Resume);
        assert_eq!(session.runtime().state(), RuntimeState::Running);
        let resumed = wait_for_full_frame(&mut session, stepped);
        apply_control(&mut session, RuntimeControl::Pause);
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
        wait_for_full_frame(&mut session, resumed);
        let quiet = wait_for_quiet(&mut session, &driver);

        session.set_control_flow(WinitControlFlow::Poll).unwrap();
        assert_eq!(session.control_flow(), WinitControlFlow::Poll);
        let mut continuous = quiet;
        for _ in 0..4 {
            std::thread::sleep(POLL_WAIT);
            continuous = wait_for_full_frame(&mut session, continuous);
            assert_eq!(session.runtime().state(), RuntimeState::Paused);
            assert_eq!(continuous.counts.fixed, quiet.counts.fixed);
            assert_eq!(continuous.fixed_tick, quiet.fixed_tick);
            assert_eq!(continuous.virtual_time.elapsed, quiet.virtual_time.elapsed);
        }
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(
            session.event_loop.as_ref().map(std::ptr::from_ref),
            original_pump
        );
        let state = session.app.as_ref().unwrap();
        assert_eq!(state.platform_windows.len(), 1);
        assert_eq!(
            state.nara_windows_by_winit.get(&native_id),
            Some(&WindowId::PRIMARY)
        );
        assert_eq!(state.platform_windows[&WindowId::PRIMARY].id(), native_id);
        assert!(Weak::ptr_eq(
            original_window.as_ref().unwrap(),
            &Arc::downgrade(&state.platform_windows[&WindowId::PRIMARY])
        ));
        let target = handles.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(target.phase, WindowTargetPhase::Active);
        assert_eq!(target.fault, None);
        assert!(target.provider_present);
        assert!(!target.surface_active);
        assert_eq!(driver.retired.get(), 0);

        session.request_close();
        assert!(!session.is_retired());
        for flow in [WinitControlFlow::Wait, WinitControlFlow::Poll] {
            assert!(session.set_control_flow(flow).is_err());
            assert_eq!(session.control_flow(), WinitControlFlow::Poll);
        }
    }));
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    if let Err(payload) = outcome {
        resume_unwind(payload);
    }

    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.runtime().state(), RuntimeState::Stopped);
    assert_eq!(
        session.event_loop.as_ref().map(std::ptr::from_ref),
        original_pump
    );
    for flow in [WinitControlFlow::Wait, WinitControlFlow::Poll] {
        assert!(session.set_control_flow(flow).is_err());
        assert_eq!(session.control_flow(), WinitControlFlow::Poll);
    }
    let state = session.app.as_ref().unwrap();
    assert!(state.platform_windows.is_empty());
    assert!(state.nara_windows_by_winit.is_empty());
    let target = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
    assert_eq!(target.fault, None);
    assert!(target.native_destroyed);
    assert!(!target.provider_present);
    assert!(!target.surface_active);
    assert!(original_window.unwrap().upgrade().is_none());
    assert!(driver.retired.get() > 0);
    let retired = session.into_retired().unwrap();
    assert_eq!(retired.result, Ok(AppExit::Requested));
    assert_eq!(retired.runtime.generation(), generation);
    assert_eq!(retired.runtime.state(), RuntimeState::Stopped);
    assert!(retired.event_loop.is_some());
    drop(retired);
}
