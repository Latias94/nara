#![cfg(target_os = "windows")]

use std::{path::Path, sync::Arc, time::Duration};

use dear_imgui_rs::{ConfigFlags, Context, Key, MouseButton};
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
    render_wgpu::__private::{WgpuEditorCompositeReceipt, WgpuSubmittedImageReceipt},
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

use super::{NativeImgui, canvas_tests::retire_application, lifecycle_tests::hidden_window};
use crate::{
    EditorApplication, EditorLaunchOptions,
    app::EditorApplicationDispatch,
    frontend::imgui::{UiProject, clear_controls, control_clip, control_rect, control_visible},
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
    pub(super) fn new(sprites: Option<&SpriteBatches>, runtime_ui_present: bool) -> Self {
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
    pub(super) identity: EditorGameViewCandidateIdentity,
    pub(super) content: GameCandidateContent,
}

#[derive(Clone, Copy)]
struct GameObservation {
    submitted: SubmittedGameObservation,
    receipt: WgpuEditorCompositeReceipt,
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
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut host = GameInputTestHost {
        project: &project.0,
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
        observed.reopened.submitted.image.view_id(),
        observed.moved.submitted.image.view_id()
    );
    assert_eq!(
        observed.reopened.scene.view_id(),
        observed.initial.scene.view_id()
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
        observed.resized.scene.view_id(),
        observed.paused.scene.view_id()
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
        assert_ne!(frame.scene.view_id(), frame.submitted.image.view_id());
        assert_eq!(frame.receipt.image(), frame.submitted.image);
    }
}

struct GameInputTestHost<'a> {
    project: &'a Path,
    result: Option<Result<InputObservation, String>>,
}

impl ApplicationHandler for GameInputTestHost<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_input_workflow(event_loop, self.project));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_input_workflow(
    event_loop: &ActiveEventLoop,
    project: &Path,
) -> Result<InputObservation, String> {
    let intent = game_view_intent()?;
    let window = hidden_window(event_loop, "Nara native Game input workflow")
        .map_err(|error| error.to_string())?;
    let mut native =
        NativeImgui::new(event_loop, Arc::clone(&window)).map_err(|error| error.to_string())?;
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
            let cleanup = native.shutdown_graphics();
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
    let result = GameInputWorkflow {
        native: &mut native,
        application: &mut application,
        event_loop,
        pointer: [-1000.0, -1000.0],
        presents: 0,
        stage: "initial workspace",
        interaction: String::new(),
    }
    .run();
    let graphics = native
        .shutdown_graphics()
        .map_err(|error| error.to_string());
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
    interaction: String,
}

impl GameInputWorkflow<'_> {
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

    fn reveal(&mut self, label: &str) -> Result<(), String> {
        for _ in 0..32 {
            let rect =
                control_rect(label).ok_or_else(|| format!("widget {label:?} was not painted"))?;
            let clip = control_clip(label)
                .ok_or_else(|| format!("widget {label:?} has no actual draw clip"))?;
            let point = center(rect);
            if clip.contains(point) && control_visible(label) {
                return Ok(());
            }
            if point[0] < clip.min[0] || point[0] >= clip.max[0] {
                return Err(format!("widget {label:?} is horizontally clipped"));
            }
            if point[1] >= clip.min[1] && point[1] < clip.max[1] {
                self.paint(|_| {})?;
                continue;
            }
            let direction = if point[1] < clip.min[1] { 3.0 } else { -3.0 };
            self.pointer = center(clip);
            // Match the existing native Canvas driver: hover first so event trickling cannot
            // defer and then discard the wheel when the next local input queue is installed.
            self.paint(|_| {})?;
            self.paint(|context| context.io_mut().add_mouse_wheel_event([0.0, direction]))?;
            self.paint(|_| {})?;
            self.paint(|_| {})?;
            let after = control_rect(label)
                .ok_or_else(|| format!("scrolling stopped painting widget {label:?}"))?;
            if after.min[1] == rect.min[1] {
                return Err(format!(
                    "scrolling reached an edge without revealing widget {label:?}"
                ));
            }
        }
        Err(format!(
            "widget {label:?} stayed clipped after 32 scroll steps"
        ))
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
            .find(|receipt| receipt.image() == submitted.image)
            .ok_or("the exact Runtime sprite/UI image was not actually composited")?;
        let scene = primary
            .composites()
            .iter()
            .copied()
            .find(|receipt| Some(receipt.image()) == self.native.scene.submitted)
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
            && !self.native.gpu.viewport_target_matches(
                moved.submitted.image.view_id(),
                moved.submitted.image.source_generation(),
                moved.submitted.image.target(),
            )
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
        self.click("entity:image-source")?;
        self.click("embedded:Game image")?;
        let paused_recaptured = self.native.input.is_captured()
            && self
                .application
                .current_frame()
                .is_some_and(|frame| frame.play().state() == EditorPlayState::Paused);
        self.stage = "Inspector text releases Game capture";
        self.reveal("field:nara.sprite.Sprite:size.x")?;
        self.click("field:nara.sprite.Sprite:size.x")?;
        let captured = self.native.input.is_captured();
        let owner = self.native.input.text_owner_item();
        let (active, text_state) = self.native.context.binding().with_bound_context(|| {
            // SAFETY: this live test Context is bound on its UI thread after presentation.
            // Copy only identities; no Dear ImGui pointer escapes the scope.
            unsafe {
                let active = dear_imgui_rs::sys::igGetActiveID();
                let state = dear_imgui_rs::sys::igGetInputTextState(active);
                (active, (!state.is_null()).then(|| (*state).ID))
            }
        });
        let text_active = active != 0 && Some(active) == owner && text_state == Some(active);
        let text_released_capture = !captured && text_active;
        let text_state_diagnostics = format!(
            "Inspector text activation: captured={captured}, actual_text_active={text_active}, IO want_text_input={}, active={active}, text_state={text_state:?}, routed_text_owner={owner:?}, rect={:?}, clip={:?}, visible={}",
            self.native.context.io().want_text_input(),
            control_rect("field:nara.sprite.Sprite:size.x"),
            control_clip("field:nara.sprite.Sprite:size.x"),
            control_visible("field:nara.sprite.Sprite:size.x"),
        );
        self.paint(|context| context.io_mut().add_input_characters_utf8("99"))?;
        self.paint(|context| context.io_mut().add_key_event(Key::Escape, true))?;
        self.paint(|context| context.io_mut().add_key_event(Key::Escape, false))?;
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
