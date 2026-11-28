//! Winit and WGPU presentation for Night Relay.

use std::{error::Error, path::Path};

use nara::{
    app::{FrameIntent, FrameIntentSet},
    ecs::{
        bevy_ecs::{error::Result as SystemResult, schedule::common_conditions::resource_changed},
        schedule::IntoScheduleConfigs,
    },
    input::{ActionContext, ActionPhase, InputSeatId},
    prelude::*,
    render::{animation::SpriteAnimationUpdateSet, sprite::SpriteSourceHydrationSet},
    render_wgpu::desktop::DesktopPresentedFrame,
    save::{SaveError, SaveStore},
    scene::spatial::propagate_spatial,
    window::WindowId,
};

use crate::{
    GamePlugin, PilotInput, TITLE, WINDOW_SIZE,
    campaign::{Campaign, SaveSnapshot, Sector, resolve_switch},
    scene_roles::register_scene_roles,
    simulation::ArenaInstance,
};

#[cfg(feature = "source")]
mod editor_play;
mod hud;
mod probe;
#[cfg(feature = "hot-reload")]
mod reload;
mod world;

pub fn run(verify_first_present: bool) -> Result<(), Box<dyn Error>> {
    run_with_probe(verify_first_present.then_some(probe::Smoke::FirstPresent))
}

pub fn verify_gameplay() -> Result<(), Box<dyn Error>> {
    run_with_probe(Some(probe::Smoke::Pickup))
}

pub fn verify_r1(save_dir: &Path) -> Result<(), Box<dyn Error>> {
    verify_campaign(save_dir, probe::CampaignRun::Arrival)
}

pub fn verify_r2(save_dir: &Path) -> Result<(), Box<dyn Error>> {
    verify_campaign(save_dir, probe::CampaignRun::BothSectors)
}

fn verify_campaign(save_dir: &Path, run: probe::CampaignRun) -> Result<(), Box<dyn Error>> {
    let store = SaveStore::in_dir(save_dir);
    match store.load::<SaveSnapshot>(run.save_slot()) {
        Err(SaveError::NotFound(_)) => {}
        Ok(_) => return Err("use a fresh save directory for campaign verification".into()),
        Err(error) => return Err(error.into()),
    }
    let mut app = desktop_app()?;
    app.insert_resource(Campaign::with_save(store, run.save_slot()))?;
    install_archive(
        &mut app,
        &std::env::current_exe()?.with_file_name("game.npak"),
        Sector::One,
    )?;
    probe::verify_campaign(&mut app, save_dir, run)
}

fn run_with_probe(probe: Option<probe::Smoke>) -> Result<(), Box<dyn Error>> {
    let mut app = desktop_app()?;
    let campaign = if probe.is_some() {
        Campaign::default()
    } else {
        Campaign::persistent()
    };
    let sector = campaign.sector;
    app.insert_resource(campaign)?;
    let archive = std::env::current_exe()?.with_file_name("game.npak");
    #[cfg(feature = "source")]
    let use_source = !archive.try_exists()?;
    #[cfg(not(feature = "source"))]
    let use_source = false;
    if use_source {
        #[cfg(feature = "source")]
        install_source(&mut app, None, sector)?;
    } else {
        install_archive(&mut app, &archive, sector)?;
    }
    if let Some(probe) = probe {
        probe::verify_present(&mut app, probe)?;
    } else {
        app.run()?;
    }
    Ok(())
}

fn install_archive(app: &mut App, path: &Path, sector: Sector) -> Result<(), Box<dyn Error>> {
    let source = RuntimeAssetSource::from_npak(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    app.set_asset_source(source)?;
    switch_scene_sync(app.world_mut()?, sector.asset()?)?;
    app.add_plugins(GamePlugin)?;
    Ok(())
}

#[cfg(feature = "source")]
pub fn run_editor_play(scene_path: &Path) -> Result<(), Box<dyn Error>> {
    let mut app = desktop_app()?;
    app.insert_resource(Campaign::default())?;
    install_source(&mut app, Some(scene_path), Sector::One)?;
    editor_play::run(app)
}

#[cfg(feature = "source")]
fn install_source(
    app: &mut App,
    snapshot: Option<&Path>,
    mut sector: Sector,
) -> Result<(), Box<dyn Error>> {
    use crate::{arena::source::SourceProject, development_root};

    let root = development_root()?;
    #[cfg(feature = "hot-reload")]
    let watcher = nara_asset::watch::AssetWatcher::new(root.join("assets"))?;
    let assets = RuntimeAssetSource::from_project_dir(&root, "assets")?;
    let project = SourceProject::new(app)?;
    let document = snapshot
        .map(|path| project.read_snapshot(&assets, path))
        .transpose()?;
    if let Some(document) = &document
        && project
            .read_scene(&assets, Sector::Two.asset()?)
            .is_ok_and(|saved| saved.id == document.id)
    {
        sector = Sector::Two;
        app.world_mut()?.resource_mut::<Campaign>().sector = sector;
    }
    let id = sector.asset()?;
    app.set_asset_source(assets.clone())?;
    let prepared = document
        .as_ref()
        .map(|scene| project.cook(scene))
        .transpose()?;
    if let Some(scene) = &prepared {
        switch_scene_asset_sync(app.world_mut()?, id, scene)?;
    } else {
        switch_scene_sync(app.world_mut()?, id)?;
    }
    app.add_plugins(GamePlugin)?;
    #[cfg(feature = "hot-reload")]
    {
        use crate::arena::source::SceneLayers;

        let layers = SceneLayers::capture(&project, &assets, id, document);
        reload::Reload::new(root, project, layers, id, prepared, watcher).install(app)?;
    }
    Ok(())
}

fn desktop_app() -> Result<App, Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new(TITLE).with_window_size(WINDOW_SIZE.0, WINDOW_SIZE.1))?;
    register_scene_roles(&mut app)?;
    app.insert_resource(world::post_process())?
        .add_systems(Startup, (bind_controls, (hud::setup, hud::update).chain()))?
        .add_systems(FrameIntent, read_controls.in_set(FrameIntentSet::Collect))?
        .add_systems(Update, world::update_fade)?
        .add_systems(
            CoreStage::TaskUpdate,
            (
                world::dress_arena
                    .after(resolve_switch)
                    .after(SpriteSourceHydrationSet),
                (propagate_spatial, hud::update)
                    .after(world::dress_arena)
                    .run_if(resource_changed::<ArenaInstance>),
            ),
        )?
        .add_systems(
            FixedUpdate,
            (
                world::update_appearance
                    .before(SpatialSet::AfterFixed)
                    .before(SpriteAnimationUpdateSet),
                hud::update,
            )
                .in_set(FixedUpdateSet::Finalize),
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

fn is_game_frame(frame: &DesktopPresentedFrame) -> bool {
    frame.window == WindowId::PRIMARY && frame.direct_surface && frame.hud_glyph_primitives > 0
}
