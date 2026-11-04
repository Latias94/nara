#![cfg(target_os = "windows")]

mod canvas;
mod source_continuity;
mod work_costs;

use canvas::{install_canvas_button, observe_canvas_button};

use std::{path::Path, sync::Arc, time::Duration};

use dear_imgui_rs::{ConfigFlags, Context, MouseButton};
use nara::{
    ProductRecipe,
    app::{
        App, CoreStage, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
        PluginId, StartupStage,
    },
    ecs::{Commands, Query, Res, schedule::IntoScheduleConfigs},
    input::{ButtonInput, KeyCode},
    project_host::{EditorGameViewCandidateIdentity, TrustedCompiledEditorProjectIntent},
    render::{Camera2d, DisplayRect},
    render_wgpu::{
        WgpuDisplayOccurrence, WgpuImageCompositeReceipt, WgpuRenderInput, WgpuStockPolicy,
        WgpuSubmittedImageReceipt,
    },
    sprite::Sprite,
    sprite_render::__private::SpriteBatches,
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorPlayAction,
        EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection,
        EditorPlayRequestResult, EditorPlayState,
    },
    transform::Transform2d,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode as PhysicalKeyCode, PhysicalKey},
    platform::windows::EventLoopBuilderExtWindows,
    window::WindowId,
};

use super::{
    NativeImgui,
    canvas_tests::retire_application,
    lifecycle_tests::hidden_window,
    retirement::{finish_graphics_for_test, initialize_native_for_test},
};
use crate::{
    EditorApplication, EditorLaunchOptions,
    app::EditorApplicationDispatch,
    frontend::imgui::{UiProject, clear_controls, control_rect},
};

/// Read-only source facts retained only by the native test build. An assertion may consume them
/// only after matching their exact submitted image to an actual native composite receipt.
#[derive(Clone, Copy)]
pub(super) struct GameCandidateContent {
    sprite_instances: usize,
    center_x: Option<f32>,
    runtime_ui_present: bool,
}

impl GameCandidateContent {
    pub(super) fn captured(input: &WgpuRenderInput) -> Option<Self> {
        let capture = input.captured::<WgpuStockPolicy>()?;
        Some(Self::new(
            capture.sprite_batches(),
            !capture.ui_packets().is_empty(),
        ))
    }

    fn new(sprites: Option<&SpriteBatches>, runtime_ui_present: bool) -> Self {
        let center_x = sprites.and_then(|sprites| {
            let mut centers = sprites
                .as_slice()
                .iter()
                .filter(|batch| batch.sort_key == 0)
                .flat_map(|batch| batch.instances.iter().map(|instance| instance.center.x));
            let center = centers.next()?;
            centers.next().is_none().then_some(center)
        });
        Self {
            sprite_instances: sprites.map_or(0, SpriteBatches::total_instances),
            center_x,
            runtime_ui_present,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SubmittedGameObservation {
    pub(super) image: WgpuSubmittedImageReceipt,
    pub(super) occurrence: WgpuDisplayOccurrence,
    pub(super) identity: EditorGameViewCandidateIdentity,
    pub(super) content: GameCandidateContent,
}

#[derive(Clone, Copy)]
struct GameObservation {
    submitted: SubmittedGameObservation,
    receipt: WgpuImageCompositeReceipt,
    scene: WgpuSubmittedImageReceipt,
    center_x: f32,
}

impl GameObservation {
    fn runtime_frame(self) -> u64 {
        self.submitted.identity.frame_index()
    }
}

#[derive(Debug, Default)]
struct InputResponsePlugin;

const INPUT_RESPONSE_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("nara.test.native-game-input-response"),
    PluginCategory::Runtime,
)
.requires_plugins(&[
    nara::render::RENDER_PLUGIN_ID,
    nara::input::INPUT_PLUGIN_ID,
    nara::transform::TRANSFORM_PLUGIN_ID,
]);

impl Plugin for InputResponsePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &INPUT_RESPONSE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Scene, |mut commands: Commands| {
            commands.spawn((Transform2d::default(), Camera2d::default()));
        })?
        .add_systems(StartupStage::Scene, install_canvas_button)?
        .add_systems(CoreStage::Last, observe_canvas_button)?
        .add_systems(
            CoreStage::FixedUpdate,
            move_sprite.in_set(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

fn move_sprite(keys: Res<ButtonInput<KeyCode>>, mut sprites: Query<(&Sprite, &mut Transform2d)>) {
    if keys.pressed(KeyCode::Character('d')) {
        for (sprite, mut transform) in &mut sprites {
            if sprite.sort_key == 0 {
                transform.translation.x += 2.0;
            }
        }
    }
}

pub(super) fn game_view_intent() -> Result<TrustedCompiledEditorProjectIntent, String> {
    let recipe = ProductRecipe::new()
        .add_plugin::<InputResponsePlugin>()
        .map_err(|error| error.to_string())?;
    Ok(TrustedCompiledEditorProjectIntent::new()
        .with_profile("desktop")
        .with_recipe(recipe))
}

struct InputObservation {
    initial: GameObservation,
    moved: GameObservation,
    reopened: GameObservation,
    moved_after_reopen: GameObservation,
    paused: GameObservation,
    resized: GameObservation,
    stepped: GameObservation,
    released_step: GameObservation,
    closed_target_retired: bool,
    paused_recaptured: bool,
    text_released_capture: bool,
    text_state_diagnostics: String,
    stopped: bool,
    rejected_control_released_live_route: bool,
    control_release_diagnostics: String,
    presents: u64,
}

#[test]
#[ignore = "requires a Windows desktop and actual WGPU sprite/UI presentation"]
fn native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime() {
    let project = UiProject::new();
    qualify_game_input(&project.0, false);
}

#[test]
#[ignore = "requires a Windows desktop and actual fixed-world WGPU sprite/UI presentation"]
fn native_fixed_world_canvas_keeps_ui_input_resize_and_step_coherent() {
    let project = UiProject::new();
    let path = project.0.join("nara.toml");
    let manifest = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        format!("{manifest}\n[render.world_canvas]\nwidth = 320\nheight = 180\n"),
    )
    .unwrap();
    qualify_game_input(&project.0, true);
}

fn qualify_game_input(project: &Path, canvas_buttons: bool) {
    canvas::reset_probe();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut host = GameInputTestHost {
        project,
        canvas_buttons,
        result: None,
    };
    event_loop.run_app(&mut host).unwrap();
    // The fixture completes real renderer, platform, native-window and Session retirement before
    // returning any behavior facts, including the failure path.
    let observed = host
        .result
        .expect("the native input workflow must run")
        .unwrap();
    assert!(observed.presents > 0);
    assert!(observed.closed_target_retired);
    assert!(observed.paused_recaptured);
    assert!(
        observed.text_released_capture,
        "{}",
        observed.text_state_diagnostics
    );
    assert!(observed.stopped);
    assert!(
        observed.rejected_control_released_live_route,
        "{}",
        observed.control_release_diagnostics
    );
    assert!(observed.moved.center_x > observed.initial.center_x + f32::EPSILON);
    assert!(observed.moved.runtime_frame() > observed.initial.runtime_frame());
    assert_ne!(
        observed.reopened.submitted.image.producer_id(),
        observed.moved.submitted.image.producer_id()
    );
    assert_eq!(
        observed.reopened.scene.producer_id(),
        observed.initial.scene.producer_id()
    );
    assert!(observed.reopened.runtime_frame() > observed.moved.runtime_frame());
    assert!(observed.moved_after_reopen.center_x > observed.reopened.center_x + f32::EPSILON);
    assert!(observed.paused.runtime_frame() > observed.moved.runtime_frame());
    assert!(
        observed
            .resized
            .submitted
            .identity
            .start()
            .geometry_generation()
            > observed
                .paused
                .submitted
                .identity
                .start()
                .geometry_generation()
    );
    assert!(
        observed.resized.submitted.image.target_generation()
            > observed.paused.submitted.image.target_generation()
    );
    assert!(
        observed.resized.receipt.presentation_generation()
            > observed.paused.receipt.presentation_generation()
    );
    assert_ne!(
        observed.resized.submitted.image.target().extent(),
        observed.paused.submitted.image.target().extent()
    );
    assert_eq!(
        observed.resized.scene.producer_id(),
        observed.paused.scene.producer_id()
    );
    assert_eq!(
        observed.resized.scene.target(),
        observed.paused.scene.target()
    );
    assert_eq!(
        observed.resized.scene.target_generation(),
        observed.paused.scene.target_generation()
    );
    assert!(observed.stepped.runtime_frame() > observed.resized.runtime_frame());
    assert!(
        observed.stepped.center_x > observed.resized.center_x + f32::EPSILON,
        "Step did not move the presented sprite: frame/geometry/center {:?} -> {:?}",
        (
            observed.resized.runtime_frame(),
            observed
                .resized
                .submitted
                .identity
                .start()
                .geometry_generation(),
            observed.resized.center_x,
        ),
        (
            observed.stepped.runtime_frame(),
            observed
                .stepped
                .submitted
                .identity
                .start()
                .geometry_generation(),
            observed.stepped.center_x,
        ),
    );
    assert!(observed.released_step.runtime_frame() > observed.stepped.runtime_frame());
    assert_eq!(
        observed.released_step.center_x, observed.stepped.center_x,
        "the rejected control must release Runtime-held D before another Paused Step"
    );
    for frame in [
        observed.initial,
        observed.moved,
        observed.reopened,
        observed.moved_after_reopen,
        observed.paused,
        observed.resized,
        observed.stepped,
        observed.released_step,
    ] {
        assert!(frame.submitted.content.sprite_instances > 0);
        assert!(frame.submitted.content.runtime_ui_present);
        assert!(frame.center_x.is_finite());
        assert_ne!(
            frame.scene.producer_id(),
            frame.submitted.image.producer_id()
        );
        assert_eq!(frame.receipt.image(), frame.submitted.image);
        assert_eq!(frame.receipt.occurrence(), frame.submitted.occurrence);
    }
}

struct GameInputTestHost<'a> {
    project: &'a Path,
    canvas_buttons: bool,
    result: Option<Result<InputObservation, String>>,
}

impl ApplicationHandler for GameInputTestHost<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_input_workflow(
                event_loop,
                self.project,
                self.canvas_buttons,
            ));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_input_workflow(
    event_loop: &ActiveEventLoop,
    project: &Path,
    canvas_buttons: bool,
) -> Result<InputObservation, String> {
    run_game_workflow(event_loop, project, canvas_buttons, |workflow| {
        workflow.run()
    })
}

fn run_game_workflow<T>(
    event_loop: &ActiveEventLoop,
    project: &Path,
    canvas_buttons: bool,
    run: impl FnOnce(GameInputWorkflow<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let intent = game_view_intent()?;
    let window = hidden_window(event_loop, "Nara native Game input workflow")
        .map_err(|error| error.to_string())?;
    let mut native = initialize_native_for_test(event_loop, Arc::clone(&window))?;
    native.layout_loaded = true;
    let mut flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
    flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
    native.context.io_mut().set_config_flags(flags);
    native.frontend.place_test_game(false);
    let opened = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project).with_profile("desktop"),
        intent,
    );
    let mut application = match opened {
        Ok(application) => application,
        Err(error) => {
            let cleanup = finish_graphics_for_test(&mut native);
            return Err(format!(
                "trusted native Game Open rejected: {error:?}; diagnostics={:?}; native cleanup: {cleanup:?}",
                error.diagnostics()
            ));
        }
    };
    application.native_shell_ready();
    let game_panel = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.game-view")
        .expect("the built-in Game panel is registered")
        .id();
    // Keep this input/resize fixture out of the default dock recipe before its first frame.
    // Undocking a builder-created window later retains its docked size, which can leave only
    // one pixel below the ordinary Play controls. The real floating window starts at the
    // existing usable default rectangle and remains freely resizable by its actual grip.
    application
        .panel_workspace_mut()
        .layout_mut()
        .place(game_panel, crate::workspace::EditorPanelPlacement::Floating);
    let result = run(GameInputWorkflow {
        native: &mut native,
        application: &mut application,
        event_loop,
        pointer: [-1000.0, -1000.0],
        presents: 0,
        stage: "initial workspace",
        canvas_buttons,
        interaction: String::new(),
    });
    let graphics = finish_graphics_for_test(&mut native);
    drop(native);
    drop(window);
    let retirement = retire_application(&mut application);
    match (result, graphics, retirement) {
        (Ok(observed), Ok(()), Ok(())) => Ok(observed),
        (result, graphics, retirement) => Err([
            result.err(),
            graphics
                .err()
                .map(|error| format!("graphics cleanup: {error}")),
            retirement
                .err()
                .map(|error| format!("Session cleanup: {error}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; ")),
    }
}

struct GameInputWorkflow<'a> {
    native: &'a mut NativeImgui,
    application: &'a mut EditorApplication,
    event_loop: &'a ActiveEventLoop,
    pointer: [f32; 2],
    presents: u64,
    stage: &'static str,
    canvas_buttons: bool,
    interaction: String,
}

impl GameInputWorkflow<'_> {
    fn verify_display_occurrences(&mut self) -> Result<(), String> {
        self.application
            .drive_editor_frame(Duration::from_millis(20), false, true);
        self.native.frame_index += 1;
        let window = super::editor_window_packet(&self.native.window);
        self.native
            .gpu
            .begin_window_frame(window, self.native.frame_index)
            .map_err(|error| error.to_string())?;
        let mut pending = self
            .native
            .submit_game(self.application, window)
            .map_err(|error| error.to_string())?
            .ok_or("the occurrence fixture has no Game candidate")?;
        let designated = pending.display.clone();
        let mirror = self
            .native
            .gpu
            .display_image(designated.output().clone())
            .map_err(|error| error.to_string())?;
        let mirror_texture = self
            .native
            .renderer
            .register(&mirror)
            .map_err(|error| error.to_string())?;
        let designated_texture = self
            .native
            .game
            .texture
            .ok_or("the Game output has no registered display")?;
        // Actually present M while D is absent, before the later window paint containing D.
        let mirror_only = super::lifecycle_tests::present_registered_images(
            self.native,
            self.event_loop,
            &[(mirror_texture, mirror.clone())],
        )?;
        let mirror_only_receipt = mirror_only
            .composites()
            .iter()
            .copied()
            .find(|receipt| receipt.occurrence() == mirror.occurrence())
            .ok_or("the first passive mirror did not reach native presentation")?;
        if pending
            .composite(mirror_only.composites().iter().copied())
            .is_some()
        {
            return Err(
                "a mirror-only native paint completed the absent designated display".to_owned(),
            );
        }
        let ui = pending
            .ui
            .take()
            .ok_or("the mirror-first fixture has no original UI permit")?;
        pending.ui = Some(match ui.presented(mirror_only_receipt) {
            Err(ui) => ui,
            Ok(_) => {
                return Err("mirror-first presentation consumed the pending UI permit".to_owned());
            }
        });
        let outcome = super::lifecycle_tests::present_registered_images(
            self.native,
            self.event_loop,
            &[
                (mirror_texture, mirror.clone()),
                (designated_texture, designated.clone()),
            ],
        )?;
        self.native
            .renderer
            .unregister(mirror_texture)
            .map_err(|error| error.to_string())?;
        let mirror_receipt = outcome
            .composites()
            .iter()
            .copied()
            .find(|receipt| receipt.occurrence() == mirror.occurrence())
            .ok_or("the passive Game mirror was not composited")?;
        let designated_receipt = pending
            .composite(outcome.composites().iter().copied())
            .ok_or("the designated Game display was not composited")?;
        if mirror_receipt.image() != designated_receipt.image()
            || mirror_receipt.occurrence() == designated_receipt.occurrence()
            || pending.composite([mirror_receipt]).is_some()
            || pending.composite([mirror_receipt, designated_receipt]) != Some(designated_receipt)
        {
            return Err(
                "Game completion did not select its exact occurrence among identical pixels"
                    .to_owned(),
            );
        }
        let ui = pending
            .ui
            .take()
            .ok_or("the Game fixture has no Runtime UI permit")?;
        let ui = match ui.presented(mirror_receipt) {
            Err(ui) => ui,
            Ok(_) => {
                return Err("a passive mirror consumed the designated Runtime UI permit".to_owned());
            }
        };
        if ui.display() != &designated
            || ui.output() != designated.output()
            || ui.image() != designated.receipt()
        {
            return Err("UI permit rejection changed its original output or display".to_owned());
        }
        pending.ui = Some(ui);

        let mut input = super::input::ImguiInput::default();
        let native_window = self.native.window.id();
        let viewport = self.native.context.main_viewport().id();
        let scale = self.native.context.io().display_framebuffer_scale()[0];
        input.handle_event(
            native_window,
            &WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: PhysicalPosition::new(32.0 * f64::from(scale), 32.0 * f64::from(scale)),
            },
            &self.native.context,
        );
        let mut interaction = crate::frontend::imgui::ImguiImageInteraction {
            occurrence: Some(mirror.occurrence()),
            rect: DisplayRect {
                min: [0.0, 0.0],
                max: [64.0, 64.0],
            },
            viewport,
            viewport_origin: [0.0, 0.0],
            pixels_per_point: scale,
            owns_pointer: true,
            capture_requested: true,
            window: dear_imgui_rs::Id::from(1),
        };
        let stamp = super::EditorGamePresentedStamp::new(pending.identity, designated_receipt);
        input.observe(
            Some(interaction),
            None,
            Some(stamp),
            native_window,
            viewport,
            &[],
            true,
            false,
            self.application.active_game_start_request(),
        );
        if input.is_captured() {
            return Err(
                "a mirror interaction captured input using another occurrence's receipt".to_owned(),
            );
        }
        interaction.occurrence = Some(designated.occurrence());
        input.observe(
            Some(interaction),
            None,
            Some(stamp),
            native_window,
            viewport,
            &[],
            true,
            false,
            self.application.active_game_start_request(),
        );
        if !input.is_captured() {
            return Err("the exact designated interaction did not capture input".to_owned());
        }
        let completion = pending.finish_after_error(
            [mirror_receipt, designated_receipt],
            &super::imgui_error("primary paint failed after secondary Game presentation"),
        );
        self.native
            .return_game_completion(self.application, completion)
            .map_err(|error| error.to_string())
    }

    fn frame_error(&self, operation: &str, error: impl std::fmt::Display) -> String {
        format!(
            "stage={}; interaction={}; operation={operation}; host frame={}; error={error}; active geometry={:?}; candidate geometry={:?}; captured={}",
            self.stage,
            self.interaction,
            self.native.frame_index,
            self.application.active_game_start_request().map(|start| (
                start.geometry_generation(),
                start.logical_window().resolution.physical_width,
                start.logical_window().resolution.physical_height
            )),
            self.native
                .game_candidate
                .map(|candidate| candidate.start().geometry_generation()),
            self.native.input.is_captured(),
        )
    }

    fn paint(&mut self, input: impl FnOnce(&mut Context)) -> Result<(), String> {
        if self
            .application
            .current_frame()
            .is_some_and(|frame| frame.play().state() == EditorPlayState::Running)
        {
            self.native.input.mark_presented_stale();
        }
        self.application
            .drive_editor_frame(Duration::from_millis(20), false, true);
        let origin = self.native.context.main_viewport().pos();
        let scale = f64::from(self.native.context.io().display_framebuffer_scale()[0]);
        let event = WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                f64::from(self.pointer[0] - origin[0]) * scale,
                f64::from(self.pointer[1] - origin[1]) * scale,
            ),
        };
        self.native
            .input
            .handle_event(self.native.window.id(), &event, &self.native.context);
        self.native
            .input
            .flush(self.application)
            .map_err(|error| self.frame_error("cursor input flush before redraw", error))?;
        clear_controls();
        let pointer = self.pointer;
        let exited = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event(pointer);
                io.add_mouse_viewport_event(viewport);
                input(context);
            })
            .map_err(|error| self.frame_error("native redraw and admitted UI output", error))?;
        if exited {
            return Err("the Game input workflow unexpectedly requested exit".into());
        }
        self.presents += self.native.gpu.frame_transaction_stats().presents();
        Ok(())
    }

    fn settle(&mut self) -> Result<(), String> {
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        Ok(())
    }

    fn click_point(&mut self, point: [f32; 2]) -> Result<(), String> {
        self.pointer = point;
        self.paint(|_| {})?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true)
        })?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false)
        })?;
        self.settle()
    }

    fn click(&mut self, label: &str) -> Result<(), String> {
        self.interaction = format!("click {label}");
        let rect = control_rect(label)
            .ok_or_else(|| format!("the actual Game widget {label:?} was not painted"))?;
        self.click_point(center(rect))
    }

    fn key(&mut self, state: ElementState) -> Result<(), String> {
        self.interaction = format!("D {state:?}");
        self.native.input.handle_key(
            self.native.window.id(),
            PhysicalKey::Code(PhysicalKeyCode::KeyD),
            state,
            false,
        );
        self.native
            .input
            .flush(self.application)
            .map_err(|error| self.frame_error("physical key input flush", error))
    }

    fn action(&mut self, action: EditorPlayAction) -> Result<EditorApplicationDispatch, String> {
        self.interaction = format!("owner action {action:?}");
        let frame = self
            .application
            .current_frame()
            .ok_or("the embedded Game lost its workspace")?;
        let dispatch = self
            .application
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Play(action)],
            ));
        if dispatch.invalidates_game_input() {
            self.native
                .input
                .invalidate_after_control(self.application.active_game_start_request());
            self.native.game_candidate = None;
        }
        self.application.drive_editor_frame(
            Duration::ZERO,
            dispatch.actions_observed(),
            dispatch.requires_runtime_drive() || dispatch.requests_step(),
        );
        Ok(dispatch)
    }

    fn observe(&self) -> Result<GameObservation, String> {
        let submitted = self
            .native
            .submitted_game_observation
            .ok_or("the Runtime has no submitted sprite/UI observation")?;
        let (primary, secondary) = self
            .native
            .lifecycle_frame
            .as_ref()
            .ok_or("the native frame did not present")?;
        let receipt = primary
            .composites()
            .iter()
            .chain(secondary.iter().flat_map(|outcome| outcome.composites()))
            .copied()
            .find(|receipt| {
                receipt.image() == submitted.image && receipt.occurrence() == submitted.occurrence
            })
            .ok_or("the exact Runtime sprite/UI image was not actually composited")?;
        let (_, _, scene_display, _, _) = self
            .native
            .owned_scene_snapshot_for_test()
            .ok_or("the concurrent Scene has no confirmed Nara display")?;
        let scene = primary
            .composites()
            .iter()
            .copied()
            .find(|receipt| {
                Some(receipt.image()) == self.native.scene.submitted
                    && receipt.occurrence() == scene_display.occurrence()
            })
            .ok_or("Game omitted the concurrent independent Scene composite")?
            .image();
        let center_x = submitted
            .content
            .center_x
            .ok_or("the submitted image did not contain exactly one tracked sprite")?;
        Ok(GameObservation {
            submitted,
            receipt,
            scene,
            center_x,
        })
    }

    fn run(mut self) -> Result<InputObservation, String> {
        self.settle()?;
        self.click("embedded:Play")?;
        self.stage = "independent Game display occurrences";
        self.verify_display_occurrences()?;
        self.settle()?;
        let initial = self.observe()?;
        let game_image = control_rect("embedded:Game image")
            .ok_or("the actual Game image geometry is missing")?;
        if game_image.max[0] - game_image.min[0] < 32.0
            || game_image.max[1] - game_image.min[1] < 32.0
        {
            return Err(format!(
                "the native input fixture did not expose a clickable Game image: image={game_image:?}, window={:?}",
                control_rect("embedded:Game window")
            ));
        }
        self.stage = "initial Game capture and movement";
        self.click("embedded:Game image")?;
        if !self.native.input.is_captured() {
            return Err("clicking the presented Game image did not capture input".into());
        }
        let canvas_buttons = self.canvas_buttons;
        if canvas_buttons {
            self.stage = "Runtime UI button before Game resize";
            self.click_runtime_canvas_button()?;
        }
        self.key(ElementState::Pressed)?;
        self.settle()?;
        let moved = self.observe()?;
        self.key(ElementState::Released)?;
        let game_panel = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "nara.game-view")
            .ok_or("the Game panel is missing")?
            .id();
        self.stage = "close Game while Runtime continues";
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .close(game_panel);
        self.settle()?;
        let closed_target_retired = self.native.game.view.is_none()
            && self.native.game.texture.is_none()
            && !self.native.input.is_captured()
            && self.native.game.identity.is_none()
            && self.native.game.display.is_none()
            && self
                .application
                .current_frame()
                .is_some_and(|frame| frame.play().state() == EditorPlayState::Running);
        self.stage = "reopen Game and movement";
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .reopen(game_panel);
        self.settle()?;
        let reopened = self.observe()?;
        self.click("embedded:Game image")?;
        self.key(ElementState::Pressed)?;
        self.settle()?;
        let moved_after_reopen = self.observe()?;
        self.key(ElementState::Released)?;
        self.stage = "Pause and refocus existing Game image";
        self.click("embedded:Pause")?;
        let paused = self.observe()?;
        self.presents += super::authoring_workflow_tests::hierarchy_click(
            self.native,
            self.application,
            self.event_loop,
            &mut self.pointer,
            "Select image-source",
        )?;
        self.click("embedded:Game image")?;
        let paused_recaptured = self.native.input.is_captured()
            && self
                .application
                .current_frame()
                .is_some_and(|frame| frame.play().state() == EditorPlayState::Paused);
        self.stage = "Inspector text releases Game capture";
        let (text_released_capture, text_state_diagnostics, presents) =
            super::authoring_workflow_tests::game_inspector_text_handoff(
                self.native,
                self.application,
                self.event_loop,
                &mut self.pointer,
            )?;
        self.presents += presents;
        self.stage = "resize Game through actual window grip";
        self.interaction = "resize grip press, move +64/+48, release".into();
        let window = control_rect("embedded:Game window")
            .ok_or("the actual Game window geometry is missing")?;
        let grip = [window.max[0] - 4.0, window.max[1] - 4.0];
        self.pointer = grip;
        self.paint(|_| {})?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true)
        })?;
        self.pointer = [grip[0] + 64.0, grip[1] + 48.0];
        self.paint(|_| {})?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false)
        })?;
        self.settle()?;
        let resized = self.observe()?;
        if canvas_buttons {
            self.stage = "Runtime UI button after actual Game resize";
            self.action(EditorPlayAction::Resume)?;
            self.settle()?;
            self.click("embedded:Game image")?;
            self.click_runtime_canvas_button()?;
            self.action(EditorPlayAction::Pause)?;
            self.settle()?;
        }
        self.stage = "recapture resized Paused Game and fixed Step";
        self.click("embedded:Game image")?;
        self.key(ElementState::Pressed)?;
        self.action(EditorPlayAction::StepFixedTick)?;
        self.settle()?;
        let stepped = self.observe()?;
        if !self.application.current_frame().is_some_and(|frame| {
            frame.play().state() == EditorPlayState::Paused
                && matches!(
                    frame.play().result(),
                    Some(EditorPlayOperationResult::Applied {
                        operation: EditorPlayOperation::StepFixedTick,
                        ..
                    })
                )
        }) {
            return Err("the real fixed Step did not complete and retain Paused".into());
        }
        self.stage = "rejected control releases the still-live Runtime route";
        let retained = self.application.active_game_start_request();
        let captured_before = self.native.input.is_captured();
        let dispatch = self.action(EditorPlayAction::Cancel)?;
        let cancel_result = dispatch.feedback().and_then(Result::ok).and_then(|report| {
            report.outcomes().iter().find_map(|outcome| match outcome {
                EditorActionOutcome::Play(result) => Some(*result),
                _ => None,
            })
        });
        self.native
            .input
            .flush(self.application)
            .map_err(|error| self.frame_error("release after rejected Cancel", error))?;
        let current = self.application.active_game_start_request();
        let captured_after = self.native.input.is_captured();
        let rejected_control_released_live_route = retained.is_some()
            && current == retained
            && captured_before
            && !captured_after
            && cancel_result
                == Some(EditorPlayRequestResult::Rejected(
                    EditorPlayRejection::InvalidState,
                ));
        let control_release_diagnostics = format!(
            "Cancel: start_before={:?}, start_after={:?}, result={cancel_result:?}, captured_before={captured_before}, captured_after={captured_after}",
            retained.map(|start| (start.runtime_generation(), start.geometry_generation())),
            current.map(|start| (start.runtime_generation(), start.geometry_generation())),
        );
        // Do not release or re-press D here: a real Step must observe the release already consumed
        // by the still-live Runtime, rather than merely proving the router forgot its capture.
        self.stage = "Paused Step after rejected control releases Runtime-held D";
        self.action(EditorPlayAction::StepFixedTick)?;
        self.settle()?;
        let released_step = self.observe()?;
        self.key(ElementState::Released)?;
        self.stage = "Stop and physical Game retirement";
        self.click("embedded:Game image")?;
        if !self.native.input.is_captured() {
            return Err(
                "the Stop retirement proof must begin with a genuinely captured Game route".into(),
            );
        }
        self.action(EditorPlayAction::Stop)?;
        self.settle()?;
        let stopped = self.native.game.view.is_none()
            && self.native.game.submitted.is_none()
            && self.application.active_game_start_request().is_none()
            && self.application.pending_game_start_request().is_none()
            && self
                .application
                .current_frame()
                .is_some_and(|frame| frame.play().state() == EditorPlayState::Empty);
        Ok(InputObservation {
            initial,
            moved,
            reopened,
            moved_after_reopen,
            paused,
            resized,
            stepped,
            released_step,
            closed_target_retired,
            paused_recaptured,
            text_released_capture,
            text_state_diagnostics,
            stopped,
            rejected_control_released_live_route,
            control_release_diagnostics,
            presents: self.presents,
        })
    }
}

fn center(rect: DisplayRect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
