//! Opt-in round policy over the game's existing Input, clock and Scene transaction owners.
//!
//! This flow is not represented by the exact Resume protocol. Menu controls are frame actions;
//! ordinary gameplay commands remain owned and acknowledged by the original fixed transaction.

#[cfg(feature = "play-flow")]
mod hud;
#[cfg(feature = "play-flow")]
pub use hud::{PlayFlowHud, with_play_flow_hud};

use nara::{
    ProductRecipe, ProductRecipeError,
    advanced_prelude::{CurrentScene, StartupSceneActivationSet},
    app::{
        AppExitRequests, PluginCategory, PluginDeclaration, PluginId, RuntimeFrameStatus,
        RuntimeTimeSettings, StartupStage,
    },
    ecs::{World, schedule::IntoScheduleConfigs},
    identity::SceneInstanceId,
    input::{
        ActionBinding, ActionContext, ActionId, ActionMapBindings, ActionMapBuilder,
        ActionMapLimits, ActionMaps, ActionPhase, InputBinding, InputSeatId, KeyCode,
        SemanticActions,
    },
    prelude::{App, CoreStage, Plugin, PluginError, Resource},
};

use super::{ActiveLevel, GamePhase, Progress, Session, travel};
use crate::{
    levels::{LevelId, LevelKey},
    save::{Checkpoint, CheckpointLocation},
};

const ID: PluginId = PluginId::new("two-level-game.play-flow");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
    .requires_plugins(&[
        super::PLUGIN_ID,
        PluginId::new("two-level-game.input"),
        PluginId::new("nara.product.runtime-time"),
    ]);
const CONTEXT: &str = "two-level-game.menu";
const CONTROLS: [(&str, KeyCode); 4] = [
    ("confirm", KeyCode::Enter),
    ("pause", KeyCode::Escape),
    ("title", KeyCode::Character('t')),
    ("exit", KeyCode::Character('x')),
];

/// Player-facing policy, independent of Scene preparation and reconstruction status.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FlowPhase {
    #[default]
    Boot,
    Title,
    Starting,
    Playing,
    Paused,
    Resuming,
    Complete,
    Degraded,
    Exiting,
}

/// Observation only; callers operate through the installed menu action context.
#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
pub struct FlowSnapshot {
    pub phase: FlowPhase,
    pub round: u32,
    pub meadow_collected: bool,
    pub cavern_collected: bool,
    pub error: Option<String>,
}

/// Retains the admitted gameplay definition and this flow's exact installed menu derivation.
///
/// The original source or processed owner stays alive. Context enablement remains mutable in
/// the original PRIMARY action map; neither handle changes that state or grants input custody.
#[derive(Debug, Resource)]
pub struct FlowBindings {
    admitted: ActionMapBindings,
    installed: ActionMapBindings,
}

impl FlowBindings {
    #[must_use]
    pub fn admitted(&self) -> &ActionMapBindings {
        &self.admitted
    }

    #[must_use]
    pub fn installed(&self) -> &ActionMapBindings {
        &self.installed
    }
}

#[derive(Resource)]
struct Flow {
    phase: FlowPhase,
    round: u32,
    resume_scale: f32,
    drained: bool,
    starting: Option<Starting>,
    error: Option<String>,
    context: ActionContext,
    controls: [ActionId; 4],
}

struct Starting {
    previous: SceneInstanceId,
    return_to: FlowPhase,
}

/// Adds the source-game flow without changing the default product or checkpoint format.
pub fn with_play_flow(recipe: ProductRecipe) -> Result<ProductRecipe, ProductRecipeError> {
    recipe.add_plugin::<PlayFlowPlugin>()
}

#[derive(Default)]
struct PlayFlowPlugin;

impl Plugin for PlayFlowPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let mut time = app.world_mut()?.resource_mut::<RuntimeTimeSettings>();
        let scale = time.time_scale();
        if !scale.is_finite() || scale <= 0.0 {
            return Err(PluginError::SetupFailed {
                plugin: ID,
                message: "play flow requires an initially positive finite game time scale".into(),
            });
        }
        time.set_time_scale(0.0)
            .map_err(|error| PluginError::SetupFailed {
                plugin: ID,
                message: error.to_string(),
            })?;
        app.insert_resource(Flow {
            phase: FlowPhase::Boot,
            round: 0,
            resume_scale: scale,
            drained: false,
            starting: None,
            error: None,
            context: context(),
            controls: CONTROLS.map(|(name, _)| action(name)),
        })?
        .init_resource::<FlowSnapshot>()?;
        app.add_systems(
            StartupStage::Runtime,
            initialize.after(StartupSceneActivationSet),
        )?;
        app.add_systems(CoreStage::Update, advance)?;
        app.add_systems(
            CoreStage::Last,
            publish_while_host_paused.before(super::capture),
        )?;
        Ok(())
    }
}

fn context() -> ActionContext {
    ActionContext::new(CONTEXT).expect("static menu context")
}

fn action(name: &str) -> ActionId {
    ActionId::new(format!("two-level-game.menu.{name}")).expect("static menu action")
}

fn initialize(world: &mut World) -> Result<(), nara::ecs::bevy_ecs::error::BevyError> {
    install_controls(world).map_err(std::io::Error::other)?;
    if world.resource::<Session>().initialized {
        enter(world, FlowPhase::Title);
    } else {
        let error = world.resource::<Session>().error.clone();
        world.resource_mut::<Flow>().error = error;
        enter(world, FlowPhase::Degraded);
    }
    publish(world);
    Ok(())
}

fn install_controls(world: &mut World) -> Result<(), String> {
    let mut maps = world.resource_mut::<ActionMaps>();
    let map = maps
        .get_mut(InputSeatId::PRIMARY)
        .ok_or("PRIMARY map is absent")?;
    if CONTROLS.iter().any(|(_, key)| {
        map.bindings()
            .iter()
            .any(|binding| binding.input == InputBinding::Key(*key))
    }) {
        return Err("menu keys Enter, Escape, T and X must be unbound in the source map".into());
    }
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    for entry in map.definition().contexts() {
        builder
            .declare_context(
                entry.context.clone(),
                map.is_context_enabled(&entry.context),
            )
            .map_err(|error| error.to_string())?;
    }
    for binding in map.bindings() {
        builder
            .bind(binding.clone())
            .map_err(|error| error.to_string())?;
    }
    builder
        .declare_context(context(), true)
        .map_err(|error| error.to_string())?;
    for (name, key) in CONTROLS {
        builder
            .bind(ActionBinding::key(action(name), key).with_context(context()))
            .map_err(|error| error.to_string())?;
    }
    let bindings = builder
        .compile()
        .map_err(|error| error.to_string())?
        .into_bindings();
    let retained = FlowBindings {
        admitted: map.bindings_handle(),
        installed: bindings.clone(),
    };
    map.replace_bindings(bindings);
    map.disable_context(&ActionContext::gameplay())
        .map_err(|error| error.to_string())?;
    drop(maps);
    world.insert_resource(retained);
    Ok(())
}

pub(super) fn simulation_enabled(world: &World) -> bool {
    world
        .get_resource::<Flow>()
        .is_none_or(|flow| flow.phase == FlowPhase::Playing)
}

pub(super) fn drain_or_block_gameplay(world: &mut World) -> bool {
    let Some(mut flow) = world.get_resource_mut::<Flow>() else {
        return false;
    };
    if flow.phase == FlowPhase::Resuming {
        flow.drained = true;
    }
    flow.phase != FlowPhase::Playing
}

fn enter(world: &mut World, phase: FlowPhase) {
    let enabled = phase == FlowPhase::Playing;
    let result = world
        .resource_mut::<ActionMaps>()
        .get_mut(InputSeatId::PRIMARY)
        .ok_or_else(|| "PRIMARY map is absent".to_owned())
        .and_then(|map| {
            map.set_context_enabled(&ActionContext::gameplay(), enabled)
                .map_err(|error| error.to_string())
        });
    let phase = if let Err(error) = result {
        world.resource_mut::<Flow>().error = Some(error);
        FlowPhase::Degraded
    } else {
        phase
    };
    if phase != FlowPhase::Playing && world.resource::<Session>().held != [false; 4] {
        world.resource_mut::<Session>().held = [false; 4];
        super::project_animation(world);
    }
    let scale = if matches!(phase, FlowPhase::Playing | FlowPhase::Resuming) {
        world.resource::<Flow>().resume_scale
    } else {
        let scale = world.resource::<RuntimeTimeSettings>().time_scale();
        if scale > 0.0 {
            world.resource_mut::<Flow>().resume_scale = scale;
        }
        0.0
    };
    world
        .resource_mut::<RuntimeTimeSettings>()
        .set_time_scale(scale)
        .expect("flow selects zero or a previously admitted positive time scale");
    let mut flow = world.resource_mut::<Flow>();
    flow.phase = phase;
    flow.drained = false;
}

fn new_round(world: &mut World) {
    let previous = CurrentScene::get(world).map(|current| current.receipt().instance_id());
    let Some(previous) = previous else {
        world.resource_mut::<Flow>().error = Some("current Scene is unavailable".into());
        enter(world, FlowPhase::Degraded);
        return;
    };
    let checkpoint = Checkpoint::new(CheckpointLocation {
        level: LevelKey::main(LevelId::Meadow),
        anchor: nara::identity::SceneEntityId::new("checkpoint").expect("static anchor"),
    });
    match travel::begin(world, checkpoint, false) {
        Ok(()) => {
            let mut flow = world.resource_mut::<Flow>();
            let return_to = flow.phase;
            flow.starting = Some(Starting {
                previous,
                return_to,
            });
            flow.error = None;
            enter(world, FlowPhase::Starting);
        }
        Err(error) => world.resource_mut::<Flow>().error = Some(error),
    }
}

fn finish_start(world: &mut World) {
    if !world.resource::<travel::Travel>().is_idle() {
        return;
    }
    let starting = world
        .resource_mut::<Flow>()
        .starting
        .take()
        .expect("Starting owns one attempt");
    let session = world.resource::<Session>();
    let changed = CurrentScene::get(world)
        .is_some_and(|current| current.receipt().instance_id() != starting.previous);
    let successful = changed
        && session.phase == GamePhase::Running
        && session.error.is_none()
        && world.resource::<ActiveLevel>().key() == LevelKey::main(LevelId::Meadow);
    if successful {
        let mut session = world.resource_mut::<Session>();
        session.initialized = true;
        session.clock.ticks = 0;
        session.clock.elapsed_nanos = 0;
        session.travel_count = 0;
        session.restarts = 0;
        session.saves = 0;
        session.restores = 0;
        let mut flow = world.resource_mut::<Flow>();
        flow.round = flow.round.saturating_add(1);
        flow.error = None;
        enter(world, FlowPhase::Resuming);
    } else {
        let error = session
            .error
            .clone()
            .unwrap_or_else(|| "new round did not commit and reconstruct its Scene".into());
        world.resource_mut::<Flow>().error = Some(error);
        enter(
            world,
            if changed {
                FlowPhase::Degraded
            } else {
                starting.return_to
            },
        );
    }
}

fn goals(world: &World) -> [bool; 2] {
    let progress = world.resource::<Progress>().checkpoint();
    [LevelId::Meadow, LevelId::Cavern].map(|level| {
        progress
            .collected
            .iter()
            .any(|pickup| pickup.level == LevelKey::main(level))
    })
}

fn advance(world: &mut World) {
    if world.resource::<RuntimeFrameStatus>().simulation_paused {
        return;
    }
    let flow = world.resource::<Flow>();
    let pressed = |id: &ActionId| {
        world
            .resource::<SemanticActions>()
            .as_slice()
            .iter()
            .any(|outcome| {
                outcome.seat == InputSeatId::PRIMARY
                    && outcome.context == flow.context
                    && &outcome.action == id
                    && outcome.phase == ActionPhase::Started
            })
    };
    let [confirm, pause, title, exit] = flow.controls.each_ref().map(pressed);
    let phase = flow.phase;
    if exit {
        enter(world, FlowPhase::Exiting);
        world.resource_mut::<AppExitRequests>().request_exit();
    } else if phase == FlowPhase::Starting {
        finish_start(world);
    } else if world.resource::<Session>().phase == GamePhase::Degraded
        && matches!(
            phase,
            FlowPhase::Playing | FlowPhase::Paused | FlowPhase::Resuming | FlowPhase::Complete
        )
    {
        let error = world.resource::<Session>().error.clone();
        world.resource_mut::<Flow>().error = error;
        enter(world, FlowPhase::Degraded);
    } else if phase == FlowPhase::Playing && goals(world) == [true, true] {
        enter(world, FlowPhase::Complete);
    } else if phase == FlowPhase::Resuming {
        if world.resource::<Flow>().drained {
            enter(world, FlowPhase::Playing);
        }
    } else if pause && phase == FlowPhase::Playing {
        enter(world, FlowPhase::Paused);
    } else if (pause || confirm) && phase == FlowPhase::Paused {
        enter(world, FlowPhase::Resuming);
    } else if title
        && matches!(
            phase,
            FlowPhase::Paused | FlowPhase::Complete | FlowPhase::Degraded
        )
    {
        enter(world, FlowPhase::Title);
    } else if confirm
        && matches!(
            phase,
            FlowPhase::Title | FlowPhase::Complete | FlowPhase::Degraded
        )
    {
        new_round(world);
    }
    publish(world);
}

fn publish_while_host_paused(world: &mut World) {
    if world.resource::<RuntimeFrameStatus>().simulation_paused {
        publish(world);
    }
}

fn publish(world: &mut World) {
    let flow = world.resource::<Flow>();
    let [meadow_collected, cavern_collected] = goals(world);
    let snapshot = FlowSnapshot {
        phase: flow.phase,
        round: flow.round,
        meadow_collected,
        cavern_collected,
        error: flow.error.clone(),
    };
    let previous = world.resource::<FlowSnapshot>();
    if previous.phase != snapshot.phase
        || previous.round != snapshot.round
        || previous.error != snapshot.error
    {
        println!(
            "two_level_flow: phase={:?} round={} meadow={} cavern={} error={:?}",
            snapshot.phase, snapshot.round, meadow_collected, cavern_collected, snapshot.error
        );
    }
    *world.resource_mut::<FlowSnapshot>() = snapshot;
}
