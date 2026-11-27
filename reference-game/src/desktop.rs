//! Winit and WGPU presentation for Night Relay.

#[cfg(feature = "source")]
use std::path::Path;
use std::{
    error::Error,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use nara::{
    app::{FrameExecutionStart, FrameIntent, FrameIntentSet},
    ecs::{Local, Single, bevy_ecs::error::Result as SystemResult, schedule::IntoScheduleConfigs},
    input::{ActionContext, ActionPhase, InputSeatId},
    prelude::*,
    render_wgpu::desktop::{DesktopPresentation, DesktopPresentedFrame},
    window::{WindowId, WindowSet},
};

use crate::{PilotInput, Round, RunRules, TITLE, WINDOW_SIZE, simulation::advance_round};

#[cfg(feature = "source")]
mod editor_play;
mod hud;
#[cfg(feature = "hot-reload")]
mod reload;
mod standalone;
mod world;

const SMOKE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Probe {
    FirstPresent,
    Pickup,
}

pub fn run(verify_first_present: bool) -> Result<(), Box<dyn Error>> {
    run_with_probe(verify_first_present.then_some(Probe::FirstPresent))
}

pub fn verify_gameplay() -> Result<(), Box<dyn Error>> {
    run_with_probe(Some(Probe::Pickup))
}

fn run_with_probe(probe: Option<Probe>) -> Result<(), Box<dyn Error>> {
    let mut app = desktop_app()?;
    let archive = std::env::current_exe()?.with_file_name("game.npak");
    #[cfg(feature = "source")]
    if archive.try_exists()? {
        standalone::install(&mut app, &archive)?;
    } else {
        install_source(&mut app, None)?;
    }
    #[cfg(not(feature = "source"))]
    standalone::install(&mut app, &archive)?;
    if let Some(probe) = probe {
        verify_present(&mut app, probe)?;
    } else {
        app.run()?;
    }
    Ok(())
}

#[cfg(feature = "source")]
pub fn run_editor_play(scene_path: &Path) -> Result<(), Box<dyn Error>> {
    let mut app = desktop_app()?;
    install_source(&mut app, Some(scene_path))?;
    editor_play::run(app)
}

#[cfg(feature = "source")]
fn install_source(app: &mut App, snapshot: Option<&Path>) -> Result<(), Box<dyn Error>> {
    use crate::{GamePlugin, arena::source};

    let project = source::SourceProject::new(app, &source::development_root()?)?;
    #[cfg(feature = "hot-reload")]
    let watcher = nara_asset::watch::AssetWatcher::new(project.watch_directory())?;
    let assets = project.assets()?;
    let content = project.load(&assets, snapshot)?;
    #[cfg(feature = "hot-reload")]
    let reload = reload::Reload::new(
        project,
        &assets,
        content.scene.clone(),
        snapshot.is_some(),
        watcher,
    );
    world::SceneVisuals::install(app.world_mut()?, content.images, content.materials)?;
    app.add_plugins(GamePlugin::from_scene(content.scene))?;
    #[cfg(feature = "hot-reload")]
    reload.install(app)?;
    Ok(())
}

fn desktop_app() -> Result<App, Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new(TITLE).with_window_size(WINDOW_SIZE.0, WINDOW_SIZE.1))?;
    app.add_systems(
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

fn verify_present(app: &mut App, probe: Probe) -> Result<(), Box<dyn Error>> {
    let require_pickup = probe == Probe::Pickup;
    if require_pickup {
        app.add_systems(
            FixedUpdate,
            fly_to_first_cell
                .in_set(FixedUpdateSet::Simulate)
                .before(advance_round),
        )?;
    }
    let deadline = Instant::now() + SMOKE_TIMEOUT;
    let presented = Arc::new(OnceLock::new());
    let receipt = Arc::clone(&presented);
    app.add_systems(
        Last,
        move |observation: Res<DesktopPresentation>,
              execution: Res<FrameExecutionStart>,
              round: Res<Round>,
              mut pickup_frame: Local<Option<u64>>,
              mut exit: ResMut<AppExitRequests>| {
            if round.collected() > 0 && pickup_frame.is_none() {
                *pickup_frame = Some(execution.frame());
            }
            if Instant::now() >= deadline {
                exit.request_exit();
            } else if let Some(frame) = observation.latest().filter(is_game_frame)
                && (!require_pickup || pickup_frame.is_some_and(|index| frame.frame_index >= index))
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
        if require_pickup {
            println!("Night Relay collected a power cell and presented the updated HUD");
        }
        return Ok(());
    }
    Err(if require_pickup {
        "the game closed or timed out before picking up a cell and presenting the HUD"
    } else {
        "the window closed or timed out before presenting the game and HUD"
    }
    .into())
}

fn fly_to_first_cell(
    mut waypoint: Local<usize>,
    mut input: ResMut<PilotInput>,
    round: Res<Round>,
    rules: Single<&RunRules>,
    time: Res<FixedTime>,
) {
    const ROUTE: [Vec2; 4] = [
        Vec2::new(-256.0, 0.0),
        Vec2::new(-64.0, 0.0),
        Vec2::new(-64.0, 96.0),
        Vec2::new(-32.0, 96.0),
    ];
    input.movement = Vec2::ZERO;
    if round.collected() != 0 {
        return;
    }
    while let Some(&target) = ROUTE.get(*waypoint) {
        let offset = target - round.position();
        if offset.length_squared() > 0.01 {
            input.movement = offset / (rules.move_speed * time.delta().as_secs_f32());
            return;
        }
        *waypoint += 1;
    }
}

fn is_game_frame(frame: &DesktopPresentedFrame) -> bool {
    frame.window == WindowId::PRIMARY && frame.direct_surface && frame.hud_glyph_primitives > 0
}
