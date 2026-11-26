//! Winit and WGPU presentation for Night Relay.

use std::{
    error::Error,
    path::Path,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use nara::{
    app::{FrameIntent, FrameIntentSet},
    ecs::{bevy_ecs::error::Result as SystemResult, schedule::IntoScheduleConfigs},
    input::{ActionContext, ActionPhase, InputSeatId},
    prelude::*,
    render_wgpu::desktop::{DesktopPresentation, DesktopPresentedFrame},
    window::{WindowId, WindowSet},
};

use crate::{GamePlugin, PilotInput, TITLE, WINDOW_SIZE, arena::source};

mod editor_play;
mod hud;
mod world;

const SMOKE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Resource)]
struct SceneTextures(source::TextureHandles);

pub fn run(verify_first_present: bool) -> Result<(), Box<dyn Error>> {
    let mut app = desktop_app(None)?;
    if verify_first_present {
        verify_present(&mut app)?;
    } else {
        app.run()?;
    }
    Ok(())
}

pub fn run_editor_play(scene_path: &Path) -> Result<(), Box<dyn Error>> {
    editor_play::run(desktop_app(Some(scene_path))?)
}

fn desktop_app(snapshot: Option<&Path>) -> Result<App, Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new(TITLE).with_window_size(WINDOW_SIZE.0, WINDOW_SIZE.1))?;
    let (scene, textures) = source::load(&mut app, source::development_root(), snapshot)?;
    app.insert_resource(SceneTextures(textures))?
        .add_plugins(GamePlugin::from_scene(scene))?
        .add_systems(
            Startup,
            (
                bind_controls,
                world::setup,
                (hud::setup, hud::update).chain(),
            ),
        )?
        .add_systems(FrameIntent, read_controls.in_set(FrameIntentSet::Collect))?
        .add_systems(First, world::fit_camera.after(WindowSet::ApplyEvents))?
        .add_systems(
            FixedUpdate,
            (world::update_appearance, hud::update).in_set(FixedUpdateSet::Finalize),
        )?;
    Ok(app)
}

fn bind_controls(world: &mut World) -> SystemResult {
    use AxisDirection::{Negative, Positive};
    use KeyCode::{
        ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Enter, Escape, KeyA, KeyD, KeyR, KeyS, KeyW,
    };

    let horizontal = ActionId::new("move.x")?;
    let vertical = ActionId::new("move.y")?;
    let restart = ActionId::new("restart")?;
    let bindings = [
        ActionBinding::key_axis(horizontal.clone(), KeyA, Negative),
        ActionBinding::key_axis(horizontal.clone(), ArrowLeft, Negative),
        ActionBinding::key_axis(horizontal.clone(), KeyD, Positive),
        ActionBinding::key_axis(horizontal, ArrowRight, Positive),
        ActionBinding::key_axis(vertical.clone(), KeyS, Negative),
        ActionBinding::key_axis(vertical.clone(), ArrowDown, Negative),
        ActionBinding::key_axis(vertical.clone(), KeyW, Positive),
        ActionBinding::key_axis(vertical, ArrowUp, Positive),
        ActionBinding::key(restart.clone(), KeyR),
        ActionBinding::key(restart, Enter),
        ActionBinding::key(ActionId::new("quit")?, Escape),
    ];
    world.insert_resource(ActionMaps::new(ActionMap::from_bindings(bindings)?));
    Ok(())
}

fn read_controls(
    actions: Res<ActionState>,
    mut input: ResMut<PilotInput>,
    mut exit: ResMut<AppExitRequests>,
) {
    let actions = actions.for_seat(InputSeatId::PRIMARY);
    input.movement = actions.axis_2d("move");
    let gameplay = ActionContext::gameplay();
    for edge in actions.edges() {
        if edge.phase != ActionPhase::Started || edge.context != gameplay {
            continue;
        }
        match edge.action.as_str() {
            "restart" => input.restart = true,
            "quit" => exit.request_exit(),
            _ => {}
        }
    }
}

fn verify_present(app: &mut App) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + SMOKE_TIMEOUT;
    let presented = Arc::new(OnceLock::new());
    let receipt = Arc::clone(&presented);
    app.add_systems(
        Last,
        move |observation: Res<DesktopPresentation>, mut exit: ResMut<AppExitRequests>| {
            if Instant::now() >= deadline {
                exit.request_exit();
            } else if let Some(frame) = observation.latest().filter(is_game_frame) {
                let _ = receipt.set((frame.extent, frame.hud_glyph_primitives));
                exit.request_success();
            }
        },
    )?;
    let exit = app.run()?;
    if let (AppExit::Success, Some((extent, glyphs))) = (exit, presented.get()) {
        println!(
            "Night Relay presented {}x{} with {glyphs} HUD glyphs",
            extent.width, extent.height
        );
        return Ok(());
    }
    Err("the window closed or timed out before presenting the game and HUD".into())
}

fn is_game_frame(frame: &DesktopPresentedFrame) -> bool {
    frame.window == WindowId::PRIMARY && frame.direct_surface && frame.hud_glyph_primitives > 0
}
