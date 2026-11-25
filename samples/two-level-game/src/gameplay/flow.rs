//! Opt-in round policy over the game's existing Input, clock and Scene transaction owners.
//!
//! This flow is not represented by the exact Resume protocol. Menu controls are frame actions;
//! ordinary gameplay commands remain owned and acknowledged by the original fixed transaction.
//! Round Save inputs preceding the first Pause keep their place in the next fixed batch, even
//! if it arrives during Continue. Returning to Title discards the pending permissions. Host
//! exact steps admit Save input while leaving the Round menu in its current phase.

#[cfg(feature = "play-flow")]
mod hud;
#[cfg(feature = "play-flow")]
pub use hud::{PlayFlowHud, with_play_flow_hud};

#[cfg(feature = "gamepad")]
use nara::input::GamepadButton;
use nara::{
    ProductRecipe, ProductRecipeError,
    advanced_prelude::{CurrentScene, StartupSceneActivationSet},
    app::{
        AppExitRequests, PluginCategory, PluginDeclaration, PluginId, RuntimeFaultDetail,
        RuntimeFrameStatus, RuntimeTimeSettings, StartupStage,
    },
    ecs::{World, schedule::IntoScheduleConfigs},
    identity::SceneInstanceId,
    input::{
        ActionBinding, ActionContext, ActionContextDefinition, ActionId, ActionMapBindings,
        ActionMapBuilder, ActionMapDefinition, ActionMapError, ActionMapLimits, ActionMaps,
        ActionPhase, InputBinding, InputSeatId, InputSet, KeyCode, SemanticActions,
    },
    prelude::{App, CoreStage, Plugin, PluginError, Resource},
};

use super::{ActiveLevel, GamePhase, Progress, Session, input, travel};
use crate::{
    command::CommandSet,
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
#[cfg(feature = "gamepad")]
const GAMEPAD_CONTEXT: &str = "two-level-game.menu.gamepad";
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

/// Selects the controls derived from the admitted action definition at startup.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PlayFlowControls {
    #[default]
    Keyboard,
    /// Uses the portable button codes supplied by the optional Gilrs adapter.
    #[cfg(feature = "gamepad")]
    KeyboardAndGamepad,
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
    gamepad_menu: Option<GamepadMenu>,
    input: RoundInput,
    pending_saves: usize,
}

#[derive(Default)]
struct RoundInput {
    confirm: bool,
    pause: bool,
    title: bool,
    exit: bool,
    start: bool,
    paused_destination: Option<FlowPhase>,
}

impl RoundInput {
    fn pauses(&self) -> bool {
        self.pause || self.start
    }
}

#[derive(Clone)]
struct GamepadMenu {
    context: ActionContext,
    start: ActionId,
}

impl GamepadMenu {
    fn for_controls(controls: PlayFlowControls) -> Option<Self> {
        match controls {
            PlayFlowControls::Keyboard => None,
            #[cfg(feature = "gamepad")]
            PlayFlowControls::KeyboardAndGamepad => Some(Self {
                context: ActionContext::new(GAMEPAD_CONTEXT).expect("static gamepad menu context"),
                start: action("start"),
            }),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum ControlError {
    #[error("PRIMARY input map is absent")]
    MissingMap,
    #[error("round flow requires the gameplay input context")]
    MissingGameplayContext,
    #[error("menu keys Enter, Escape, T and X must be unbound in the source map")]
    ReservedKeys,
    #[cfg(feature = "gamepad")]
    #[error("controller round reserves Start for its menu and can replace only gameplay Retry")]
    ReservedStart,
    #[error("round input map could not be derived: {0}")]
    Definition(#[from] ActionMapError),
}

impl ControlError {
    fn fault_detail(&self) -> RuntimeFaultDetail {
        let (code, summary) = match self {
            Self::MissingMap => (
                "two-level-game.round-input-missing",
                "The Round requires a PRIMARY input map",
            ),
            Self::MissingGameplayContext => (
                "two-level-game.round-gameplay-context-missing",
                "The Round requires the gameplay input context",
            ),
            Self::ReservedKeys => (
                "two-level-game.round-menu-key-conflict",
                "Menu keys Enter, Escape, T and X must be unbound in the source map",
            ),
            #[cfg(feature = "gamepad")]
            Self::ReservedStart => (
                "two-level-game.round-start-conflict",
                "Controller round reserves Start for its menu and can replace only gameplay Retry",
            ),
            Self::Definition(_) => (
                "two-level-game.round-input-definition-invalid",
                "The derived Round input map could not be admitted",
            ),
        };
        RuntimeFaultDetail::new(code, summary, "two-level-game.play-flow")
            .expect("static Round input diagnostic")
    }
}

struct Starting {
    previous: SceneInstanceId,
    return_to: FlowPhase,
}

/// Adds the source-game flow without changing the default product or checkpoint format.
pub fn with_play_flow(
    recipe: ProductRecipe,
    controls: PlayFlowControls,
) -> Result<ProductRecipe, ProductRecipeError> {
    match controls {
        PlayFlowControls::Keyboard => recipe.add_plugin::<PlayFlowPlugin>(),
        #[cfg(feature = "gamepad")]
        PlayFlowControls::KeyboardAndGamepad => recipe
            .add_configured_plugin(b"two-level-game.play-flow.gamepad/v1\0", move || {
                PlayFlowPlugin { controls }
            }),
    }
}

#[derive(Default)]
struct PlayFlowPlugin {
    controls: PlayFlowControls,
}

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
            gamepad_menu: GamepadMenu::for_controls(self.controls),
            input: RoundInput::default(),
            pending_saves: 0,
        })?
        .init_resource::<FlowSnapshot>()?;
        app.add_systems(
            StartupStage::Runtime,
            initialize.after(StartupSceneActivationSet),
        )?;
        app.add_systems(
            CoreStage::PreUpdate,
            read_input
                .in_set(CommandSet::FrameInput)
                .after(InputSet::ResolveActions),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            read_input.in_set(CommandSet::FrameInput),
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
    install_controls(world).map_err(|error| error.fault_detail().into_bevy_error())?;
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

fn install_controls(world: &mut World) -> Result<(), ControlError> {
    let gamepad_menu = world.resource::<Flow>().gamepad_menu.clone();
    let mut maps = world.resource_mut::<ActionMaps>();
    let map = maps
        .get_mut(InputSeatId::PRIMARY)
        .ok_or(ControlError::MissingMap)?;
    let bindings = compile_controls(map.definition(), gamepad_menu.as_ref(), |entry| {
        map.is_context_enabled(&entry.context)
    })?;
    let retained = FlowBindings {
        admitted: map.bindings_handle(),
        installed: bindings.clone(),
    };
    map.replace_bindings(bindings);
    map.disable_context(&ActionContext::gameplay())?;
    drop(maps);
    world.insert_resource(retained);
    Ok(())
}

#[cfg(feature = "play-flow")]
pub(crate) fn validate_input(
    definition: &ActionMapDefinition,
    controls: PlayFlowControls,
) -> Result<(), String> {
    let menu = GamepadMenu::for_controls(controls);
    compile_controls(definition, menu.as_ref(), |entry| entry.enabled)
        .map(drop)
        .map_err(|error| error.to_string())
}

fn compile_controls(
    definition: &ActionMapDefinition,
    #[cfg_attr(
        not(feature = "gamepad"),
        allow(unused_variables, reason = "no gamepad menu can be selected")
    )]
    gamepad_menu: Option<&GamepadMenu>,
    enabled: impl Fn(&ActionContextDefinition) -> bool,
) -> Result<ActionMapBindings, ControlError> {
    let gameplay = ActionContext::gameplay();
    if !definition
        .contexts()
        .iter()
        .any(|entry| entry.context == gameplay)
    {
        return Err(ControlError::MissingGameplayContext);
    }
    if CONTROLS.iter().any(|(_, key)| {
        definition
            .bindings()
            .iter()
            .any(|binding| binding.input == InputBinding::Key(*key))
    }) {
        return Err(ControlError::ReservedKeys);
    }
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    for entry in definition.contexts() {
        builder.declare_context(entry.context.clone(), enabled(entry))?;
    }
    for binding in definition.bindings() {
        #[cfg(feature = "gamepad")]
        if gamepad_menu.is_some() && superseded_gamepad_start(binding, &gameplay)? {
            continue;
        }
        builder.bind(binding.clone())?;
    }
    builder.declare_context(context(), true)?;
    for (name, key) in CONTROLS {
        builder.bind(ActionBinding::key(action(name), key).with_context(context()))?;
    }
    #[cfg(feature = "gamepad")]
    if let Some(menu) = gamepad_menu {
        builder.declare_context(menu.context.clone(), false)?;
        for (action, button, context) in [
            (menu.start.clone(), GamepadButton::Start, context()),
            (action("title"), GamepadButton::East, menu.context.clone()),
            (action("exit"), GamepadButton::Select, menu.context.clone()),
        ] {
            builder
                .bind(ActionBinding::gamepad_button(action, button.code()).with_context(context))?;
        }
    }
    Ok(builder.compile()?.into_bindings())
}

#[cfg(feature = "gamepad")]
fn superseded_gamepad_start(
    binding: &ActionBinding,
    gameplay: &ActionContext,
) -> Result<bool, ControlError> {
    if binding.input != InputBinding::GamepadButton(GamepadButton::Start.code()) {
        return Ok(false);
    }
    if &binding.context == gameplay
        && binding.action.as_str() == "two-level-game.r"
        && binding.transform == nara::input::ActionTransform::Button
    {
        return Ok(true);
    }
    Err(ControlError::ReservedStart)
}

pub(super) fn simulation_enabled(world: &World) -> bool {
    world
        .get_resource::<Flow>()
        .is_none_or(|flow| flow.phase == FlowPhase::Playing)
}

pub(super) fn fixed_input(world: &mut World) -> (bool, usize) {
    let Some(mut flow) = world.get_resource_mut::<Flow>() else {
        return (false, 0);
    };
    if flow.phase == FlowPhase::Resuming {
        flow.drained = true;
    }
    (
        flow.phase != FlowPhase::Playing,
        std::mem::take(&mut flow.pending_saves),
    )
}

fn enter(world: &mut World, phase: FlowPhase) {
    let enabled = phase == FlowPhase::Playing;
    let gamepad_context = world
        .resource::<Flow>()
        .gamepad_menu
        .as_ref()
        .map(|menu| menu.context.clone());
    let result = world
        .resource_mut::<ActionMaps>()
        .get_mut(InputSeatId::PRIMARY)
        .ok_or_else(|| "PRIMARY map is absent".to_owned())
        .and_then(|map| {
            map.set_context_enabled(&ActionContext::gameplay(), enabled)
                .map_err(|error| error.to_string())?;
            if let Some(context) = gamepad_context {
                let menu_open = matches!(
                    phase,
                    FlowPhase::Title
                        | FlowPhase::Paused
                        | FlowPhase::Complete
                        | FlowPhase::Degraded
                );
                map.set_context_enabled(&context, menu_open)
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
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
    if !matches!(
        phase,
        FlowPhase::Playing | FlowPhase::Paused | FlowPhase::Resuming
    ) {
        flow.pending_saves = 0;
    }
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
    match travel::begin(world, checkpoint, travel::TravelKind::Travel) {
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

fn read_input(world: &mut World) {
    let host_paused = world.resource::<RuntimeTimeSettings>().paused();
    let flow = world.resource::<Flow>();
    let [confirm, pause, title, exit] = &flow.controls;
    let gameplay = ActionContext::gameplay();
    let mut menu_input = RoundInput::default();
    let mut saves = 0;
    for outcome in world.resource::<SemanticActions>().as_slice() {
        if outcome.seat != InputSeatId::PRIMARY || outcome.phase != ActionPhase::Started {
            continue;
        }
        if outcome.context == gameplay {
            if outcome.action.as_str() == input::SAVE_ACTION
                && flow.phase == FlowPhase::Playing
                && !menu_input.pauses()
            {
                saves += 1;
            }
        } else if !host_paused
            && (outcome.context == flow.context
                || flow
                    .gamepad_menu
                    .as_ref()
                    .is_some_and(|menu| outcome.context == menu.context))
        {
            let action = &outcome.action;
            let start = flow
                .gamepad_menu
                .as_ref()
                .is_some_and(|menu| action == &menu.start);
            if flow.phase == FlowPhase::Paused && menu_input.paused_destination.is_none() {
                menu_input.paused_destination = if action == title {
                    Some(FlowPhase::Title)
                } else if action == pause || action == confirm || start {
                    Some(FlowPhase::Resuming)
                } else {
                    None
                };
            }
            menu_input.confirm |= action == confirm;
            menu_input.pause |= action == pause;
            menu_input.title |= action == title;
            menu_input.exit |= action == exit;
            menu_input.start |= start;
        }
    }
    let mut flow = world.resource_mut::<Flow>();
    flow.input = menu_input;
    flow.pending_saves += saves;
}

fn advance(world: &mut World) {
    if world.resource::<RuntimeFrameStatus>().simulation_paused {
        return;
    }
    let (phase, input) = {
        let mut flow = world.resource_mut::<Flow>();
        (flow.phase, std::mem::take(&mut flow.input))
    };
    if input.exit {
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
    } else if phase == FlowPhase::Playing && input.pauses() {
        enter(world, FlowPhase::Paused);
    } else if phase == FlowPhase::Paused {
        if let Some(destination) = input.paused_destination {
            enter(world, destination);
        }
    } else if input.title && matches!(phase, FlowPhase::Complete | FlowPhase::Degraded) {
        enter(world, FlowPhase::Title);
    } else if (input.confirm || input.start)
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
