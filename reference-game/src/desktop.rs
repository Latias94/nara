//! Winit and WGPU presentation for Night Relay.

use std::{
    error::Error,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use nara::{
    ecs::{bevy_ecs::error::Result as SystemResult, schedule::IntoScheduleConfigs},
    input::{ActionContext, ActionPhase, InputSeatId, InputSet},
    prelude::*,
    render_wgpu::desktop::DesktopPresentation,
    window::WindowId,
};

use crate::{GamePlugin, PilotInput, RunRules, TITLE, WINDOW_SIZE};

mod hud;
mod world;

const SMOKE_TIMEOUT: Duration = Duration::from_secs(15);

pub fn run(verify_first_present: bool) -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new(TITLE).with_window_size(WINDOW_SIZE.0, WINDOW_SIZE.1))?
        .register_schema::<RunRules>()?
        .add_plugins(GamePlugin::default())?
        .add_systems(Startup, (bind_controls, world::setup, hud::setup))?
        .add_systems(PreUpdate, read_controls.after(InputSet::ResolveActions))?
        .add_systems(Update, (world::update, world::fit_camera, hud::update))?;

    if verify_first_present {
        verify_present(&mut app)?;
    } else {
        app.run()?;
    }
    Ok(())
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
            } else if let Some(frame) = observation.latest()
                && frame.window == WindowId::PRIMARY
                && frame.direct_surface
                && frame.hud_glyph_primitives > 0
            {
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
