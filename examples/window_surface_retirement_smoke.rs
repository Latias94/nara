use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use nara::{advanced_prelude::*, backend_prelude::*};

fn main() -> Result<(), AppRunError> {
    let drop_backend_before_exit =
        std::env::args().any(|argument| argument == "--drop-backend-before-exit");
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
    })?
    .add_plugins(MinimalPlugins)?
    .add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara surface retirement smoke",
            WindowResolution::new(320, 180),
        )),
    })?
    .add_plugin(WinitPlugin::default())?
    .add_plugin(WgpuRenderPlugin)?
    .add_startup_systems(StartupStage::Scene, setup_scene)?
    .add_systems(CoreStage::Last, verify_resize_then_exit)?;

    app.finish_plugins()?;
    assert!(app.world().contains_resource::<WgpuRenderBackend>());
    assert!(
        app.world()
            .contains_resource::<nara::window::backend::BackendWindowHandles>()
    );
    assert!(app.world().contains_resource::<ExtractedViews>());
    assert!(app.world().contains_resource::<RenderFrame>());
    assert!(app.world().contains_resource::<FrameStats>());
    assert!(app.world().contains_resource::<RenderBackendStatus>());
    assert!(app.world().contains_resource::<AppExitRequests>());

    let target_authority = app
        .world()
        .resource::<nara::window::backend::BackendWindowHandles>()
        .clone();
    app.run()?;
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
        return;
    }

    if frame.state != RenderFrameState::Submitted {
        return;
    }

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
}
