use std::error::Error;
use std::io;
use std::process::{Child, Command, ExitStatus};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use nara::{advanced_prelude::*, backend_prelude::*};

#[path = "support/runtime_retirement.rs"]
mod runtime_retirement;
use runtime_retirement::finish_runtime_after_winit;

fn main() -> Result<(), Box<dyn Error>> {
    let invocation = SmokeInvocation::from_args(std::env::args().skip(1))?;
    if invocation.child {
        return run_smoke_child(invocation.drop_backend_before_exit);
    }
    run_smoke_parent(invocation.drop_backend_before_exit)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct SmokeInvocation {
    child: bool,
    drop_backend_before_exit: bool,
}

impl SmokeInvocation {
    fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, io::Error> {
        let mut invocation = Self::default();
        for argument in args {
            match argument.as_str() {
                "--smoke-child" => invocation.child = true,
                "--drop-backend-before-exit" => invocation.drop_backend_before_exit = true,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "the surface retirement smoke received an unsupported argument",
                    ));
                }
            }
        }
        Ok(invocation)
    }
}

fn run_smoke_parent(drop_backend_before_exit: bool) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--smoke-child");
    if drop_backend_before_exit {
        command.arg("--drop-backend-before-exit");
    }
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return finish_smoke_child(status),
            Ok(None) => {}
            Err(poll_error) => {
                let reap_error = terminate_and_reap(&mut child).err();
                return Err(io::Error::other(format!(
                    "surface retirement smoke polling failed: {poll_error}; reap={reap_error:?}"
                ))
                .into());
            }
        }
        if Instant::now() >= deadline {
            let status = terminate_and_reap(&mut child)?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("surface retirement smoke exceeded its hard deadline: {status}"),
            )
            .into());
        }
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

fn terminate_and_reap(child: &mut Child) -> Result<ExitStatus, io::Error> {
    let kill_error = child.kill().err();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::park_timeout(Duration::from_millis(10));
            }
            Ok(None) => {
                return Err(io::Error::other(match kill_error {
                    Some(kill_error) => {
                        format!("kill={kill_error}; child remained live past the reap deadline")
                    }
                    None => "child remained live past the reap deadline".to_owned(),
                }));
            }
            Err(poll_error) => {
                return Err(io::Error::other(match kill_error {
                    Some(kill_error) => format!("kill={kill_error}; poll={poll_error}"),
                    None => format!("poll={poll_error}"),
                }));
            }
        }
    }
}

fn finish_smoke_child(status: ExitStatus) -> Result<(), Box<dyn Error>> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("surface retirement smoke child failed: {status}")).into())
    }
}

fn run_smoke_child(drop_backend_before_exit: bool) -> Result<(), Box<dyn Error>> {
    let resize_observed = Arc::new(AtomicBool::new(false));
    let backend_removed = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let mut app = App::new();
    app.insert_resource(SmokeState {
        drop_backend_before_exit,
        resize_requested: false,
        deadline: Instant::now() + Duration::from_secs(15),
        resize_observed: Arc::clone(&resize_observed),
        backend_removed: Arc::clone(&backend_removed),
        timed_out: Arc::clone(&timed_out),
    })?;
    app.add_plugins((
        MinimalPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(
                "nara surface retirement smoke",
                WindowResolution::new(320, 180),
            )),
        },
        WgpuBackendPlugins,
    ))?;
    app.add_systems(StartupStage::Scene, setup_scene)?
        .add_systems(CoreStage::Last, verify_resize_then_exit)?;
    let candidate = nara::app::RuntimeCandidate::admit(app.seal()?)?;
    let mut runtime = candidate.complete_startup()?.promote();

    assert!(runtime.world().contains_resource::<WgpuRenderBackend>());
    assert!(
        runtime
            .world()
            .contains_resource::<nara::window::backend::BackendWindowHandles>()
    );
    assert!(runtime.world().contains_resource::<ExtractedViews>());
    assert!(runtime.world().contains_resource::<RenderFrame>());
    assert!(runtime.world().contains_resource::<FrameStats>());
    assert!(runtime.world().contains_resource::<RenderBackendStatus>());
    assert!(runtime.world().contains_resource::<AppExitRequests>());

    let target_authority = runtime
        .world()
        .resource::<nara::window::backend::BackendWindowHandles>()
        .clone();
    let run_result = WinitRunner::default().run(&mut runtime);
    finish_runtime_after_winit(run_result, runtime)?;
    assert!(
        !timed_out.load(Ordering::SeqCst),
        "surface retirement smoke exceeded its deadline"
    );
    let target = target_authority
        .snapshot(WindowId::PRIMARY)
        .expect("the primary target should remain observable after shutdown");
    assert_eq!(
        target.phase,
        nara::window::backend::WindowTargetPhase::NativeDestroyed
    );
    assert_eq!(target.fault, None);
    assert!(
        resize_observed.load(Ordering::SeqCst),
        "the configured surface did not observe the resized window extent"
    );
    assert_eq!(
        backend_removed.load(Ordering::SeqCst),
        drop_backend_before_exit,
        "the backend-drop mode did not execute its distinct resource-removal path"
    );
    Ok(())
}

#[derive(Debug, Resource)]
struct SmokeState {
    drop_backend_before_exit: bool,
    resize_requested: bool,
    deadline: Instant,
    resize_observed: Arc<AtomicBool>,
    backend_removed: Arc<AtomicBool>,
    timed_out: Arc<AtomicBool>,
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Name::new("camera"),
        Transform2d::default(),
        Camera2d {
            clear_color: Some(Color::rgb(0.08, 0.1, 0.13)),
            ..Camera2d::default()
        },
    ));
}

fn verify_resize_then_exit(
    mut commands: Commands,
    frame: Res<RenderFrame>,
    backend: Res<WgpuRenderBackend>,
    mut windows: Query<&mut Window>,
    mut state: ResMut<SmokeState>,
    mut exit: ResMut<AppExitRequests>,
) {
    if enforce_smoke_deadline(Instant::now(), state.deadline, &state.timed_out, &mut exit) {
        eprintln!(
            "surface smoke timeout: frame={:?}, backend_state={:?}, backend_error={:?}, configured_extent={:?}, resize_requested={}",
            frame.state,
            backend.state(),
            backend.last_error(),
            backend.configured_surface_extent(WindowId::PRIMARY),
            state.resize_requested,
        );
        return;
    }

    if frame.state != RenderFrameState::Submitted {
        return;
    }
    let transaction = backend.frame_transaction_stats();
    assert_eq!(transaction.frame_index(), Some(frame.index));
    assert_eq!(transaction.packet_admissions(), 1);
    assert_eq!(transaction.packet_rejections(), 0);
    assert_eq!(transaction.surface_acquire_attempts(), 1);
    assert_eq!(transaction.surface_acquires(), 1);
    assert_eq!(transaction.queue_submissions(), 1);
    assert_eq!(transaction.presents(), 1);

    let resized = Extent2d::new(400, 240).expect("smoke extent is non-zero");
    if !state.resize_requested {
        if let Some(mut window) = windows.iter_mut().next() {
            window.resolution.physical_width = resized.width;
            window.resolution.physical_height = resized.height;
            state.resize_requested = true;
        }
        return;
    }

    if backend.configured_surface_extent(WindowId::PRIMARY) == Some(resized) {
        state.resize_observed.store(true, Ordering::SeqCst);
        if state.drop_backend_before_exit {
            let backend_removed = Arc::clone(&state.backend_removed);
            commands.queue(move |world: &mut World| {
                backend_removed.store(
                    world.remove_resource::<WgpuRenderBackend>().is_some(),
                    Ordering::SeqCst,
                );
            });
        }
        exit.request_exit();
    }
}

fn enforce_smoke_deadline(
    now: Instant,
    deadline: Instant,
    timed_out: &AtomicBool,
    exit: &mut AppExitRequests,
) -> bool {
    if now < deadline {
        return false;
    }
    timed_out.store(true, Ordering::SeqCst);
    exit.request_exit();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_before_first_submission_requests_failure_exit() {
        let now = Instant::now();
        let timed_out = AtomicBool::new(false);
        let mut exit = AppExitRequests::default();

        assert!(enforce_smoke_deadline(now, now, &timed_out, &mut exit));
        assert!(timed_out.load(Ordering::SeqCst));
        assert_eq!(exit.requested(), Some(AppExit::Requested));
    }

    #[test]
    fn active_deadline_does_not_request_exit() {
        let now = Instant::now();
        let timed_out = AtomicBool::new(false);
        let mut exit = AppExitRequests::default();

        assert!(!enforce_smoke_deadline(
            now,
            now + Duration::from_secs(1),
            &timed_out,
            &mut exit
        ));
        assert!(!timed_out.load(Ordering::SeqCst));
        assert_eq!(exit.requested(), None);
    }

    #[test]
    fn invocation_routes_parent_and_child_modes_without_recursion() {
        assert_eq!(
            SmokeInvocation::from_args(Vec::<String>::new()).unwrap(),
            SmokeInvocation::default()
        );
        assert_eq!(
            SmokeInvocation::from_args([
                "--smoke-child".to_owned(),
                "--drop-backend-before-exit".to_owned(),
            ])
            .unwrap(),
            SmokeInvocation {
                child: true,
                drop_backend_before_exit: true,
            }
        );
        assert!(SmokeInvocation::from_args(["--unknown".to_owned()]).is_err());
    }
}
