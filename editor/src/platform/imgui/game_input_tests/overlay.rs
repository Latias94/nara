use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nara::{
    ProductRecipe,
    app::{
        App, CoreStage, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
        PluginId,
    },
    ecs::{Query, schedule::IntoScheduleConfigs},
    project_host::TrustedCompiledEditorProjectIntent,
    render::{Camera2d, DisplayRect, ViewportRect},
    render_wgpu::{WgpuDeviceRequirements, WgpuDisplayOccurrence, WgpuImageCompositeReceipt},
    tooling::{EditorPlayAction, EditorPlayOperation, EditorPlayOperationResult, EditorPlayState},
    window::WindowId as NaraWindowId,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::WindowId,
};

use super::super::{
    EditorGamePresentedStamp, editor_window_packet, input::ImguiInput,
    lifecycle_tests::present_registered_images,
};
use super::{
    GameInputWorkflow, InputResponsePlugin, SubmittedGameObservation, center,
    run_game_workflow_with_intent,
};
use crate::{
    EditorApplication,
    frontend::imgui::{ImguiImageInteraction, UiProject, control_rect},
};

struct ClipGameCamera(Arc<AtomicBool>);

const CLIP_GAME_CAMERA: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("nara.test.native-game-overlay-clip-camera"),
    PluginCategory::Runtime,
)
.requires_plugins(&[nara::render::RENDER_PLUGIN_ID]);

impl Plugin for ClipGameCamera {
    fn declaration() -> &'static PluginDeclaration {
        &CLIP_GAME_CAMERA
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let clip = Arc::clone(&self.0);
        app.add_systems(
            CoreStage::FixedUpdate,
            (move |mut cameras: Query<&mut Camera2d>| {
                if clip.load(Ordering::Relaxed) {
                    for mut camera in &mut cameras {
                        camera.viewport = ViewportRect::new(u32::MAX - 1, 0, 1, 1);
                    }
                }
            })
            .in_set(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

#[test]
#[ignore = "requires a Windows desktop and actual WGPU Game overlay presentation"]
fn native_game_overlay_rolls_back_uncomposited_upload_and_presents_the_step() {
    let project = UiProject::new();
    let path = project.0.join("nara.toml");
    let manifest = std::fs::read_to_string(&path).unwrap();
    assert!(manifest.contains("resizable = true"));
    std::fs::write(
        path,
        manifest.replace("resizable = true", "resizable = false"),
    )
    .unwrap();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut host = OverlayTestHost {
        project: &project,
        result: None,
    };
    event_loop.run_app(&mut host).unwrap();
    host.result
        .expect("the native Game overlay workflow must run")
        .unwrap();
}

struct OverlayTestHost<'a> {
    project: &'a UiProject,
    result: Option<Result<(), String>>,
}

impl ApplicationHandler for OverlayTestHost<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_overlay_workflow(event_loop, self.project));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_overlay_workflow(event_loop: &ActiveEventLoop, project: &UiProject) -> Result<(), String> {
    let clip = Arc::new(AtomicBool::new(false));
    let recipe = ProductRecipe::new()
        .add_plugin::<InputResponsePlugin>()
        .and_then(|recipe| {
            let clip = Arc::clone(&clip);
            recipe.add_configured_plugin((), move |_| ClipGameCamera(Arc::clone(&clip)))
        })
        .map_err(|error| error.to_string())?;
    let intent = TrustedCompiledEditorProjectIntent::new()
        .with_profile("desktop")
        .with_recipe(recipe);
    run_game_workflow_with_intent(
        event_loop,
        &project.0,
        false,
        &WgpuDeviceRequirements::default(),
        intent,
        |workflow| workflow.verify_overlay_rollback(&clip),
    )
}

impl GameInputWorkflow<'_> {
    fn verify_overlay_rollback(mut self, clip: &AtomicBool) -> Result<(), String> {
        self.settle()?;
        self.click("embedded:Play")?;
        let first = self.observe()?.submitted;
        if first.content.world_views != 1 || !first.content.runtime_ui_present {
            return Err("Play did not composite the initial world and HUD".into());
        }
        self.click("embedded:Pause")?;
        self.verify_paused_game_handoff()?;
        self.verify_idle_game_displacement()?;
        self.action(EditorPlayAction::Resume)?;
        self.settle()?;
        self.click("embedded:Pause")?;
        let incumbent = self.observe()?;
        let previous = incumbent.submitted;
        let previous_texture = self
            .native
            .game
            .texture
            .ok_or("missing incumbent Game texture")?;
        if self.native.game_composite_receipt() != Some(incumbent.receipt)
            || self.native.game_candidate != Some(previous.identity)
            || previous.content.world_views != 1
        {
            return Err("Pause did not retain the presented world image".into());
        }

        self.stage = "uncomposited HUD upload during fixed Step";
        clip.store(true, Ordering::Relaxed);
        self.action(EditorPlayAction::StepFixedTick)?;
        self.application
            .drive_editor_frame(Duration::ZERO, false, true);
        if !step_is_pending(self.application) {
            return Err("the fixed Step did not wait for its original Game presentation".into());
        }
        let rejected = self.upload_without_composite()?;
        if !later_on_same_target(previous, rejected)
            || self.native.game.texture != Some(previous_texture)
            || self.native.game.submitted != Some(previous.image)
            || self
                .native
                .game
                .display
                .as_ref()
                .map(|display| display.occurrence())
                != Some(previous.occurrence)
            || self.native.game_candidate != Some(previous.identity)
            || self.native.game_composite_receipt() != Some(incumbent.receipt)
            || !self.application.game_step_requires_runtime_drive()
            || !step_is_pending(self.application)
        {
            return Err(
                "missing HUD composite did not restore the exact incumbent and defer Step".into(),
            );
        }

        self.stage = "repaint the restored Game image without driving Runtime";
        self.paint_current(|_| {})?;
        self.verify_game_paint(previous, &[rejected.occurrence])?;
        if self
            .native
            .submitted_game_observation
            .map(|submitted| submitted.identity)
            != Some(rejected.identity)
            || self.native.game.texture != Some(previous_texture)
            || !self.application.game_step_requires_runtime_drive()
        {
            return Err("the incumbent repaint changed the capture or settled Step".into());
        }

        self.stage = "present the retry of the original fixed Step";
        self.paint(|_| {})?;
        let accepted = self
            .native
            .submitted_game_observation
            .ok_or("missing retried HUD submission")?;
        let receipt =
            self.verify_game_paint(accepted, &[previous.occurrence, rejected.occurrence])?;
        if !later_on_same_target(rejected, accepted)
            || accepted.content.world_views != 0
            || !accepted.content.runtime_ui_present
            || self.application.active_game_start_request() != Some(previous.identity.start())
        {
            return Err("the Step retry did not present a new Stock HUD on the same target".into());
        }
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        let applied = EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::StepFixedTick,
            generation: Some(accepted.identity.runtime_generation().get()),
        };
        if !self.application.current_frame().is_some_and(|frame| {
            frame.play().state() == EditorPlayState::Paused
                && frame.play().result() == Some(applied)
        }) || self.application.game_step_requires_runtime_drive()
        {
            return Err(
                "the accepted HUD presentation did not settle the original fixed Step".into(),
            );
        }
        self.verify_overlay_interaction(
            accepted,
            receipt,
            [previous.occurrence, rejected.occurrence],
        )
    }

    fn verify_paused_game_handoff(&mut self) -> Result<(), String> {
        self.stage = "move the same paused Game image to a secondary native window and back";
        let incumbent = self.observe()?;
        let identity = incumbent.submitted.identity;
        let original = incumbent.receipt;
        let previous_texture = self.native.game.texture;
        if identity.start().resizable() {
            return Err("the window handoff requires the same fixed logical Game target".into());
        }
        let flags = self.native.context.io().config_flags();
        self.native
            .context
            .io_mut()
            .set_config_flags(flags | dear_imgui_rs::ConfigFlags::VIEWPORTS_ENABLE);
        for detached in [true, false] {
            self.native.frontend.place_test_game(detached);
            let mut switched = false;
            for _ in 0..8 {
                self.paint_current(|_| {})?;
                if let Some(receipt) = self.native.game_composite_receipt()
                    && (receipt.window().window_id() != original.window().window_id()) == detached
                {
                    let (primary, _) = self.native.lifecycle_frame.as_ref().unwrap();
                    if receipt.occurrence() != original.occurrence()
                        || receipt.image() != original.image()
                        || self.native.game_candidate != Some(identity)
                        || self.native.game.texture != previous_texture
                        || self
                            .native
                            .presented_game_image
                            .as_ref()
                            .is_none_or(|image| image.world_revoker.is_none())
                        || detached
                            && (primary.window().is_none()
                                || primary
                                    .composites()
                                    .iter()
                                    .any(|sample| sample.occurrence() == original.occurrence()))
                    {
                        return Err("the native handoff changed the incumbent or never displaced the primary Game draw".into());
                    }
                    switched = true;
                    break;
                }
            }
            if !switched {
                return Err(format!(
                    "the exact paused Game image did not move with detached={detached}"
                ));
            }
        }
        self.native.context.io_mut().set_config_flags(flags);
        Ok(())
    }

    fn verify_idle_game_displacement(&mut self) -> Result<(), String> {
        self.stage = "settle a displaced window without a Game candidate";
        let incumbent = self.observe()?;
        if self
            .native
            .presented_game_image
            .as_ref()
            .is_none_or(|image| image.world_revoker.is_none())
            || self
                .application
                .take_game_view_candidate()
                .map_err(|error| self.frame_error("read the paused Game capture", error))?
                .is_some()
        {
            return Err(
                "the paused Game must have an accepted world guard and no candidate".into(),
            );
        }
        self.native.frame_index += 1;
        let frame_index = self.native.frame_index;
        let window = editor_window_packet(&self.native.window);
        self.native
            .gpu
            .begin_window_frame(window, frame_index)
            .map_err(|error| self.frame_error("begin displaced window frame", error))?;
        let mut other = window;
        other.id = NaraWindowId::new(window.id.get().checked_add(1).unwrap());
        other.resolution.physical_width = 0;
        let skipped = self
            .native
            .gpu
            .present_window(
                other,
                frame_index,
                &[],
                |_| unreachable!(),
                || unreachable!(),
            )
            .map_err(|error| self.frame_error("observe other unavailable window", error))?;
        if skipped.unavailable_window() != Some(other.id) {
            return Err("the zero-size result did not identify the other window".into());
        }
        self.native
            .settle_game_image(
                self.application,
                None,
                std::iter::once(&skipped),
                None,
                None,
                None,
            )
            .map_err(|error| self.frame_error("settle other unavailable window", error))?;
        if self.native.game_composite_receipt() != Some(incumbent.receipt)
            || self.native.game_candidate != Some(incumbent.submitted.identity)
        {
            return Err("the other window displaced the Game image".into());
        }
        let clear = present_registered_images(self.native, self.event_loop, &[])
            .map_err(|error| self.frame_error("clear the original native window", error))?;
        if clear
            .window()
            .is_none_or(|receipt| receipt.window_id() != window.id)
            || !clear.composites().is_empty()
        {
            return Err(
                "the original window did not present without the incumbent Game image".into(),
            );
        }
        self.native
            .settle_game_image(
                self.application,
                None,
                std::iter::once(&clear),
                None,
                None,
                None,
            )
            .map_err(|error| self.frame_error("settle the displaced Game image", error))?;
        if self.native.game_composite_receipt().is_some()
            || self.native.game_candidate != Some(incumbent.submitted.identity)
            || self
                .native
                .game
                .display
                .as_ref()
                .map(|display| display.occurrence())
                != Some(incumbent.submitted.occurrence)
        {
            return Err(
                "the original native presentation did not revoke only the displaced Game receipt"
                    .into(),
            );
        }
        self.paint_current(|_| {})?;
        self.verify_game_paint(incumbent.submitted, &[])?;
        if self
            .native
            .presented_game_image
            .as_ref()
            .is_none_or(|image| image.world_revoker.is_some())
        {
            return Err("repainting the retained image created a new world guard without a Runtime candidate".into());
        }
        Ok(())
    }

    fn upload_without_composite(&mut self) -> Result<SubmittedGameObservation, String> {
        self.native.frame_index += 1;
        let window = editor_window_packet(&self.native.window);
        self.native
            .gpu
            .begin_window_frame(window, self.native.frame_index)
            .map_err(|error| self.frame_error("begin HUD upload frame", error))?;
        let pending = self
            .native
            .submit_game(self.application, window)
            .map_err(|error| self.frame_error("upload pending HUD", error))?
            .ok_or("the fixed Step did not upload a Game candidate")?;
        let uploaded = self.native.submitted_game_observation.filter(|submitted| {
            submitted.identity == pending.identity
                && submitted.image == pending.display.receipt()
                && submitted.occurrence == pending.display.occurrence()
                && self.native.game.submitted == Some(submitted.image)
                && self.native.game.matches_display(&pending.display)
        });
        let original_ui = pending.ui.as_ref().is_some_and(|ui| {
            ui.display() == &pending.display && ui.image() == pending.display.receipt()
        });
        let overlay = pending.overlay.is_some();
        self.native
            .settle_game_image(
                self.application,
                Some(pending),
                std::iter::empty(),
                None,
                None,
                None,
            )
            .map_err(|error| self.frame_error("settle uncomposited HUD", error))?;
        if !overlay || !original_ui {
            return Err("the uncomposited upload did not carry the original HUD submission".into());
        }
        uploaded.ok_or_else(|| "the pending HUD did not select its real WGPU display".into())
    }

    fn verify_game_paint(
        &self,
        submitted: SubmittedGameObservation,
        absent: &[WgpuDisplayOccurrence],
    ) -> Result<WgpuImageCompositeReceipt, String> {
        let (primary, _) = self
            .native
            .lifecycle_frame
            .as_ref()
            .ok_or("missing native paint")?;
        let receipts = primary.composites();
        if receipts
            .iter()
            .any(|receipt| absent.contains(&receipt.occurrence()))
        {
            return Err(format!(
                "{} painted a superseded Game occurrence",
                self.stage
            ));
        }
        let receipt = receipts
            .iter()
            .copied()
            .find(|receipt| {
                receipt.image() == submitted.image && receipt.occurrence() == submitted.occurrence
            })
            .ok_or_else(|| format!("{} did not composite the expected Game display", self.stage))?;
        if self.native.game_candidate != Some(submitted.identity)
            || self.native.game_composite_receipt() != Some(receipt)
        {
            return Err(format!(
                "{} did not join the receipt to its candidate",
                self.stage
            ));
        }
        Ok(receipt)
    }

    fn verify_overlay_interaction(
        &mut self,
        accepted: SubmittedGameObservation,
        receipt: WgpuImageCompositeReceipt,
        stale: [WgpuDisplayOccurrence; 2],
    ) -> Result<(), String> {
        let image = control_rect("embedded:Game image").ok_or("missing presented HUD geometry")?;
        let viewport = self.native.context.main_viewport();
        let origin = viewport.pos();
        let viewport = viewport.id();
        let scale = self.native.context.io().display_framebuffer_scale()[0];
        let local = |point: [f32; 2]| [point[0] - origin[0], point[1] - origin[1]];
        let rect = DisplayRect {
            min: local(image.min),
            max: local(image.max),
        };
        let [x, y] = center(rect);
        let window = self.native.window.id();
        let mut input = ImguiInput::default();
        input.handle_event(
            window,
            &WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: PhysicalPosition::new(f64::from(x * scale), f64::from(y * scale)),
            },
            &self.native.context,
        );
        let stamp = EditorGamePresentedStamp::new(accepted.identity, receipt);
        for occurrence in stale.into_iter().chain([accepted.occurrence]) {
            input.observe(
                Some(ImguiImageInteraction {
                    occurrence: Some(occurrence),
                    rect,
                    viewport,
                    viewport_origin: origin,
                    pixels_per_point: scale,
                    owns_pointer: true,
                    capture_requested: true,
                    window: dear_imgui_rs::Id::from(1),
                }),
                None,
                Some(stamp),
                window,
                viewport,
                &[],
                true,
                false,
                self.application.active_game_start_request(),
            );
            if input.is_captured() != (occurrence == accepted.occurrence) {
                return Err(format!(
                    "HUD input used the wrong Game occurrence: {occurrence:?}"
                ));
            }
        }
        Ok(())
    }
}

fn step_is_pending(application: &EditorApplication) -> bool {
    application.current_frame().is_some_and(|frame| {
        frame.play().state() == EditorPlayState::Stepping
            && frame.play().result()
                == Some(EditorPlayOperationResult::Pending {
                    operation: EditorPlayOperation::StepFixedTick,
                })
    })
}

fn later_on_same_target(
    previous: SubmittedGameObservation,
    next: SubmittedGameObservation,
) -> bool {
    next.identity.start() == previous.identity.start()
        && next.identity.frame_index() > previous.identity.frame_index()
        && next.occurrence != previous.occurrence
        && next.image != previous.image
        && next.image.producer_id() == previous.image.producer_id()
        && next.image.target() == previous.image.target()
        && next.image.target_generation() == previous.image.target_generation()
}
