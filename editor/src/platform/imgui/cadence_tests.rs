#![cfg(target_os = "windows")]

use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::{
    EditorLaunchOptions,
    frontend::imgui::{UiProject, clear_controls, control_rect},
};
use dear_imgui_rs::{ConfigFlags, MouseButton};
use nara::{
    ProductRecipe,
    app::{
        App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration,
        PluginError, PluginId, RuntimeFrameStatus, StartupStage,
    },
    ecs::{Commands, Res, schedule::IntoScheduleConfigs},
    project_host::TrustedCompiledEditorProjectIntent,
    render::Camera2d,
    tooling::{EditorAction, EditorPlayAction},
    transform::Transform2d,
};
use winit::platform::windows::EventLoopBuilderExtWindows;

static FIXED_TICKS: AtomicU64 = AtomicU64::new(0);
static LAST_RUNTIME_FRAME: Mutex<Option<(RuntimeFrameStatus, Duration)>> = Mutex::new(None);

#[derive(Debug, Default)]
struct CadenceObservationPlugin;

impl Plugin for CadenceObservationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.native-editor-play-cadence"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[
            nara::render::RENDER_PLUGIN_ID,
            nara::transform::TRANSFORM_PLUGIN_ID,
        ]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Scene, |mut commands: Commands| {
            commands.spawn((Transform2d::default(), Camera2d::default()));
        })?
        .add_systems(
            CoreStage::Last,
            |status: Res<RuntimeFrameStatus>, fixed: Res<FixedTime>| {
                *LAST_RUNTIME_FRAME.lock().unwrap() = Some((*status, fixed.timestep()));
            },
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            (|| {
                FIXED_TICKS.fetch_add(1, Ordering::Relaxed);
            })
            .in_set(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

#[test]
#[ignore = "requires a Windows desktop and actual Dear ImGui/WGPU Editor presentation"]
fn native_resume_redraw_rebases_paused_idle_before_the_owner_drive() {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut test = CadenceTestHost {
        project: &project.0,
        result: None,
    };
    event_loop.run_app(&mut test).unwrap();
    let observed = test
        .result
        .expect("the real native cadence trajectory must run")
        .unwrap();
    assert!(observed.resumed);
    assert!(
        observed.baseline_age < Duration::from_secs(1),
        "Resume redraw retained Paused wall time: baseline age={:?}; actual Runtime delta={:?}; fixed ticks {} -> {}",
        observed.baseline_age,
        observed.driven.real_delta,
        observed.fixed_before,
        observed.fixed_after,
    );
    assert!(
        observed.driven.frame > observed.resumed_frame.frame,
        "the next owner callback must actually drive the resumed Runtime",
    );
    assert!(
        (observed.owner_delta_lower..=observed.owner_delta_upper)
            .contains(&observed.driven.real_delta),
        "the actual Runtime delta must belong to the Running interval: {:?} outside {:?}..={:?}",
        observed.driven.real_delta,
        observed.owner_delta_lower,
        observed.owner_delta_upper,
    );
    assert!(!observed.driven.real_delta_clamped);
    assert!(!observed.driven.fixed_steps_capped);
    assert_eq!(observed.driven.fixed_discarded, Duration::ZERO);
    assert_eq!(
        observed.fixed_after - observed.fixed_before,
        u64::from(observed.driven.fixed_steps),
        "the observed simulation systems must match the actual frame's fixed steps",
    );
    assert_eq!(
        observed.driven.fixed_tick - observed.resumed_frame.fixed_tick,
        observed.fixed_after - observed.fixed_before,
    );
    let incoming = observed.resumed_frame.fixed_remainder
        + observed.resumed_frame.fixed_debt
        + observed.driven.virtual_delta;
    assert_eq!(
        incoming,
        observed.timestep * observed.driven.fixed_steps
            + observed.driven.fixed_remainder
            + observed.driven.fixed_debt,
        "the resume drive must preserve old remainder/debt and add only the new Running delta",
    );
}

struct CadenceObservation {
    resumed: bool,
    baseline_age: Duration,
    resumed_frame: RuntimeFrameStatus,
    driven: RuntimeFrameStatus,
    timestep: Duration,
    owner_delta_lower: Duration,
    owner_delta_upper: Duration,
    fixed_before: u64,
    fixed_after: u64,
}

struct CadenceTestHost<'a> {
    project: &'a std::path::Path,
    result: Option<Result<CadenceObservation, String>>,
}

impl ApplicationHandler for CadenceTestHost<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.result = Some(run_native_cadence(event_loop, self.project));
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn redraw(
    host: &mut NativeEditorHost,
    event_loop: &ActiveEventLoop,
    pointer: [f32; 2],
    button: Option<bool>,
) -> Result<(), String> {
    let native = host
        .native
        .as_mut()
        .ok_or("the native owner is unavailable")?;
    let viewport = native.context.main_viewport().id();
    let io = native.context.io_mut();
    io.clear_events_queue();
    io.add_focus_event(true);
    io.add_mouse_pos_event(pointer);
    io.add_mouse_viewport_event(viewport);
    if let Some(pressed) = button {
        io.add_mouse_button_event(MouseButton::Left, pressed);
    }
    let window = native.window.id();
    clear_controls();
    // Exercise the same RedrawRequested handling and action dispatch as the ordinary Host.
    // All input stays inside this hidden Context; no OS pointer or focus operation is used.
    host.window_event(event_loop, window, WindowEvent::RedrawRequested);
    if let Some(error) = &host.error {
        return Err(error.to_string());
    }
    Ok(())
}

fn dispatch(host: &mut NativeEditorHost, action: EditorPlayAction) -> Result<(), String> {
    let frame = host
        .application
        .current_frame()
        .ok_or("the workspace is unavailable")?;
    let dispatch = host
        .application
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Play(action)],
        ));
    host.application
        .drive_editor_frame(Duration::ZERO, dispatch.actions_observed(), true);
    Ok(())
}

fn run_native_cadence(
    event_loop: &ActiveEventLoop,
    project: &std::path::Path,
) -> Result<CadenceObservation, String> {
    FIXED_TICKS.store(0, Ordering::Relaxed);
    *LAST_RUNTIME_FRAME.lock().unwrap() = None;
    let recipe = ProductRecipe::new()
        .add_plugin::<CadenceObservationPlugin>()
        .map_err(|error| error.to_string())?;
    let application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project).with_profile("desktop"),
        TrustedCompiledEditorProjectIntent::new()
            .with_profile("desktop")
            .with_recipe(recipe),
    )
    .map_err(|error| format!("trusted cadence Open rejected: {error:?}"))?;
    let window =
        super::lifecycle_tests::hidden_window(event_loop, "Nara paused idle cadence proof")
            .map_err(|error| error.to_string())?;
    let mut host = NativeEditorHost {
        application,
        supervised_play_wake: SupervisedPlayWake::default(),
        native: None,
        window: Some(window),
        retained: None,
        gpu_recovery: EditorGpuRecovery::Ready,
        error: None,
        last_drive: None,
        repaint_deadline: None,
    };
    let result = (|| {
        host.resumed(event_loop);
        let native = host
            .native
            .as_mut()
            .ok_or("the native owner failed to initialize")?;
        native.layout_loaded = true;
        let mut flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
        flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
        native.context.io_mut().set_config_flags(flags);
        let frame = host
            .application
            .current_frame()
            .ok_or("the workspace is unavailable")?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the test Scene is unavailable")?;
        dispatch(
            &mut host,
            EditorPlayAction::Play {
                document: scene.document,
                base_revision: scene.revision,
            },
        )?;
        for _ in 0..32 {
            redraw(&mut host, event_loop, [-1000.0; 2], None)?;
            host.about_to_wait(event_loop);
            if host
                .application
                .current_frame()
                .is_some_and(|frame| frame.play().state() == EditorPlayState::Running)
            {
                break;
            }
        }
        dispatch(&mut host, EditorPlayAction::Pause)?;
        redraw(&mut host, event_loop, [-1000.0; 2], None)?;
        host.about_to_wait(event_loop);
        if !host
            .application
            .current_frame()
            .is_some_and(|frame| frame.play().state() == EditorPlayState::Paused)
        {
            return Err("the real Runtime did not reach Paused".into());
        }
        let button =
            control_rect("embedded:Resume").ok_or("the actual Resume control was not painted")?;
        let pointer = [
            (button.min[0] + button.max[0]) * 0.5,
            (button.min[1] + button.max[1]) * 0.5,
        ];
        redraw(&mut host, event_loop, pointer, None)?;
        redraw(&mut host, event_loop, pointer, Some(true))?;
        // Model a long idle without a sleep or an unrelated owner drive before WM_PAINT.
        host.last_drive = Some(Instant::now() - Duration::from_secs(60));
        redraw(&mut host, event_loop, pointer, Some(false))?;
        let resumed = host
            .application
            .current_frame()
            .is_some_and(|frame| frame.play().state() == EditorPlayState::Running);
        let baseline = host
            .last_drive
            .ok_or("the resumed Host did not establish a Running baseline")?;
        let baseline_age = Instant::now().saturating_duration_since(baseline);
        let (resumed_frame, timestep) = LAST_RUNTIME_FRAME
            .lock()
            .unwrap()
            .ok_or("the Resume action did not reach an actual Runtime frame")?;
        let fixed_before = FIXED_TICKS.load(Ordering::Relaxed);
        let owner_before = Instant::now();
        host.about_to_wait(event_loop);
        let owner_after = Instant::now();
        let (driven, _) = LAST_RUNTIME_FRAME
            .lock()
            .unwrap()
            .ok_or("the owner drive did not publish an actual Runtime frame")?;
        let fixed_after = FIXED_TICKS.load(Ordering::Relaxed);
        Ok(CadenceObservation {
            resumed,
            baseline_age,
            resumed_frame,
            driven,
            timestep,
            owner_delta_lower: owner_before.saturating_duration_since(baseline),
            owner_delta_upper: owner_after.saturating_duration_since(baseline),
            fixed_before,
            fixed_after,
        })
    })();
    let graphics = host
        .native
        .as_mut()
        .map_or(Ok(()), NativeImgui::shutdown_graphics)
        .map_err(|error| error.to_string());
    host.native.take();
    host.window.take();
    let retirement = super::canvas_tests::retire_application(&mut host.application);
    match (result, graphics, retirement) {
        (Ok(observed), Ok(()), Ok(())) => Ok(observed),
        (result, graphics, retirement) => Err([result.err(), graphics.err(), retirement.err()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ")),
    }
}
