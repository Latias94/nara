use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_engine::{
    app::{App, CoreStage, FixedTime, RuntimeInstance},
    core::ByteLimit,
    gameplay::{
        GameplayCommandBatch, GameplayCommandDraft, GameplayCommandEnvelope,
        GameplayCommandIngressSource, GameplayCommandPayload, GameplayCommandPlugin,
        GameplayCommandQueue, GameplayCommandQueueSettings, GameplayCommandSet,
        GameplayCommandSource, GameplayCommandSourceId, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTarget, GameplayCommandTick,
        GameplayCommandTypeId, GameplayCommandValue, LocalGameplayCommands,
        add_local_gameplay_command_systems,
    },
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapBuilder,
        ActionMapLimits, ActionMaps, ActionPhase, ActionValue, AxisDeadZone, AxisDirection,
        AxisValue, InputGeometryGeneration, InputLimits, InputPresentationEpoch,
        InputPresentedRoute, InputRouteGeneration, InputSeatId, InputSet, InputSlotGeneration,
        InputSurfaceId, InputTimeline, InputUiDisposition, InputWindowId, KeyCode,
        SemanticActionCause, SemanticActions,
    },
};
use serde::{Deserialize, Serialize};

pub(super) const MOVE: &str = "consumer.move";
pub(super) const FIRE: &str = "consumer.fire";
pub(super) const MOVE_COMMAND: &str = "consumer.move-axis.v1";
pub(super) const FIRE_COMMAND: &str = "consumer.fire-button.v1";
pub(super) const AI: &str = "consumer.autopilot";
pub(super) const EXTERNAL: &str = "consumer.script";
pub(super) const REPLAY: &str = "consumer.recorded-trajectory";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Seat {
    Keyboard,
    Controller,
}

impl Seat {
    pub(super) const ALL: [Self; 2] = [Self::Keyboard, Self::Controller];

    pub(super) fn id(self) -> InputSeatId {
        match self {
            Self::Keyboard => InputSeatId::PRIMARY,
            Self::Controller => InputSeatId::new(7),
        }
    }

    pub(super) fn player(self) -> Player {
        match self {
            Self::Keyboard => Player::Copper,
            Self::Controller => Player::Indigo,
        }
    }

    pub(super) fn from_id(id: InputSeatId) -> Self {
        Self::ALL.into_iter().find(|seat| seat.id() == id).unwrap()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Window {
    Play,
    Guest,
}

impl Window {
    pub(super) fn id(self) -> InputWindowId {
        InputWindowId::new(match self {
            Self::Play => 11,
            Self::Guest => 22,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Player {
    #[serde(rename = "player.copper")]
    Copper,
    #[serde(rename = "player.indigo")]
    Indigo,
}

impl Player {
    pub(super) const ALL: [Self; 2] = [Self::Copper, Self::Indigo];

    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Copper => "player.copper",
            Self::Indigo => "player.indigo",
        }
    }

    pub(super) fn index(self) -> usize {
        match self {
            Self::Copper => 0,
            Self::Indigo => 1,
        }
    }

    pub(super) fn from_target(target: Option<&GameplayCommandTarget>) -> Option<Self> {
        let Some(GameplayCommandTarget::Named(id)) = target else {
            return None;
        };
        Self::ALL
            .into_iter()
            .find(|player| player.id() == id.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Bindings {
    OriginalV1,
    ReboundV1,
}

impl Bindings {
    pub(super) fn positive(self) -> KeyCode {
        match self {
            Self::OriginalV1 => KeyCode::Character('d'),
            Self::ReboundV1 => KeyCode::ArrowRight,
        }
    }

    pub(super) fn negative(self) -> KeyCode {
        match self {
            Self::OriginalV1 => KeyCode::Character('a'),
            Self::ReboundV1 => KeyCode::ArrowLeft,
        }
    }

    pub(super) fn fire(self) -> KeyCode {
        match self {
            Self::OriginalV1 => KeyCode::Space,
            Self::ReboundV1 => KeyCode::Enter,
        }
    }

    pub(super) fn compile(self) -> ActionMapBindings {
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        for binding in [
            ActionBinding::key_axis(action(MOVE), self.positive(), AxisDirection::Positive),
            ActionBinding::key_axis(action(MOVE), self.negative(), AxisDirection::Negative),
            ActionBinding::key(action(FIRE), self.fire()),
            ActionBinding::gamepad_axis(
                action(MOVE),
                if self == Self::OriginalV1 { 0 } else { 5 },
                AxisDeadZone::new(if self == Self::OriginalV1 { 0.25 } else { 0.5 }).unwrap(),
                self == Self::ReboundV1,
            ),
            ActionBinding::gamepad_button(
                action(FIRE),
                if self == Self::OriginalV1 { 0 } else { 5 },
            ),
        ] {
            builder.bind(binding).unwrap();
        }
        builder.compile().unwrap().into_bindings()
    }
}

pub(super) fn action(id: &str) -> ActionId {
    ActionId::new(id).unwrap()
}

pub(super) fn map(bindings: ActionMapBindings) -> ActionMap {
    ActionMap::new(
        bindings,
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerState {
    pub player: Player,
    pub position: f64,
    pub held_axis: f32,
    pub firing: bool,
    pub shots: u64,
    pub commands: u64,
}

#[derive(Clone, Debug, PartialEq, Resource, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GameState {
    pub tick: u64,
    pub players: [PlayerState; 2],
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            tick: 0,
            players: Player::ALL.map(|player| PlayerState {
                player,
                position: 0.0,
                held_axis: 0.0,
                firing: false,
                shots: 0,
                commands: 0,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum Intent {
    Axis(AxisValue),
    Button(bool),
}

pub(super) fn command(player: Player, seat: Option<Seat>, intent: Intent) -> GameplayCommandDraft {
    let (kind, value) = match intent {
        Intent::Axis(value) => (
            MOVE_COMMAND,
            GameplayCommandValue::F64(f64::from(value.get())),
        ),
        Intent::Button(value) => (FIRE_COMMAND, GameplayCommandValue::Bool(value)),
    };
    let mut payload = GameplayCommandPayload::new();
    payload.insert("value", value).unwrap();
    payload
        .insert(
            "seat",
            GameplayCommandValue::I64(seat.map_or(-1, |seat| i64::from(seat.id().get()))),
        )
        .unwrap();
    GameplayCommandDraft::new(GameplayCommandTypeId::new(kind).unwrap())
        .with_target(GameplayCommandTarget::named(player.id()).unwrap())
        .with_payload(payload)
}

pub(super) fn submission(
    tick: u64,
    source: GameplayCommandIngressSource,
    sequence: u64,
    draft: GameplayCommandDraft,
) -> GameplayCommandSubmission {
    GameplayCommandSubmission::new(
        GameplayCommandTick::new(tick).unwrap(),
        source,
        GameplayCommandSourceSequence::new(sequence).unwrap(),
        draft,
    )
}

fn local_adapter(actions: Res<SemanticActions>, mut commands: ResMut<LocalGameplayCommands>) {
    for outcome in actions.as_slice() {
        assert_eq!(outcome.context, ActionContext::gameplay());
        let seat = Seat::from_id(outcome.seat);
        let intent = match (outcome.action.as_str(), outcome.value) {
            (MOVE, ActionValue::Axis(value)) => Intent::Axis(value),
            (FIRE, ActionValue::Button(value)) => Intent::Button(value),
            other => panic!("unexpected game action {other:?}"),
        };
        commands
            .push(command(seat.player(), Some(seat), intent))
            .unwrap();
    }
}

#[derive(Default, Resource)]
struct Autopilot {
    submitted: bool,
}

fn autopilot(
    actions: Res<SemanticActions>,
    mut autopilot: ResMut<Autopilot>,
    mut queue: ResMut<GameplayCommandQueue>,
) {
    if !autopilot.submitted
        && actions.as_slice().iter().any(|outcome| {
            outcome.seat == Seat::Keyboard.id()
                && outcome.action.as_str() == MOVE
                && outcome.phase == ActionPhase::Started
        })
    {
        let tick = queue.stats().closed_through_tick + 1;
        queue
            .submit(submission(
                tick,
                GameplayCommandIngressSource::ai(AI).unwrap(),
                1,
                command(
                    Player::Copper,
                    None,
                    Intent::Axis(AxisValue::new(-0.5).unwrap()),
                ),
            ))
            .unwrap();
        autopilot.submitted = true;
    }
}

fn simulate(batch: Res<GameplayCommandBatch>, mut game: ResMut<GameState>) {
    assert_eq!(batch.active_tick().unwrap().get(), game.tick + 1);
    for envelope in batch.commands() {
        let player = Player::from_target(envelope.target()).unwrap();
        let state = &mut game.players[player.index()];
        match (
            envelope.command_type().as_str(),
            envelope.payload().get("value"),
        ) {
            (MOVE_COMMAND, Some(GameplayCommandValue::F64(value))) => {
                state.held_axis = AxisValue::new(*value as f32).unwrap().get();
            }
            (FIRE_COMMAND, Some(GameplayCommandValue::Bool(value))) => state.firing = *value,
            other => panic!("unexpected gameplay command {other:?}"),
        }
        state.commands += 1;
    }
    for state in &mut game.players {
        let speed = match state.player {
            Player::Copper => 2.0,
            Player::Indigo => -3.0,
        };
        state.position += f64::from(state.held_axis) * speed;
        state.shots += u64::from(state.firing);
    }
    game.tick += 1;
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CapturedTick {
    pub commands: Vec<GameplayCommandEnvelope>,
    pub state: GameState,
}

#[derive(Default, Resource)]
pub(super) struct CapturedTicks(pub Vec<CapturedTick>);

fn capture(
    batch: Res<GameplayCommandBatch>,
    game: Res<GameState>,
    queue: Res<GameplayCommandQueue>,
    mut captured: ResMut<CapturedTicks>,
) {
    let tick = batch.active_tick().unwrap().get();
    let stats = queue.stats();
    assert_eq!(tick, game.tick);
    assert_eq!(stats.closed_through_tick, tick);
    assert_eq!(
        stats.acknowledged_through_tick + 1,
        tick,
        "capture must precede Ack"
    );
    assert_eq!(stats.active_commands, batch.len());
    captured.0.push(CapturedTick {
        commands: batch.commands().to_vec(),
        state: (*game).clone(),
    });
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Edge {
    pub seat: Seat,
    pub action: String,
    pub phase: ActionPhase,
    pub value: ActionValue,
    pub cause: SemanticActionCause,
}

#[derive(Default, Resource)]
pub(super) struct ObservedActions {
    pub passes: usize,
    pub edges: Vec<Edge>,
}

pub(super) fn held(actions: &SemanticActions, seat: Seat) -> (f32, bool) {
    let context = ActionContext::gameplay();
    let axis = match actions.value(seat.id(), &action(MOVE), &context) {
        Some(ActionValue::Axis(value)) => value.get(),
        None => 0.0,
        other => panic!("movement must remain an axis: {other:?}"),
    };
    let firing = match actions.value(seat.id(), &action(FIRE), &context) {
        Some(ActionValue::Button(value)) => value,
        None => false,
        other => panic!("fire must remain a button: {other:?}"),
    };
    assert_eq!(
        actions.pressed(seat.id(), &action(MOVE), &context),
        axis != 0.0
    );
    assert_eq!(actions.pressed(seat.id(), &action(FIRE), &context), firing);
    (axis, firing)
}

fn observe_actions(actions: Res<SemanticActions>, mut observed: ResMut<ObservedActions>) {
    observed.passes += 1;
    for (index, outcome) in actions.as_slice().iter().enumerate() {
        let seat = Seat::from_id(outcome.seat);
        assert_eq!(outcome.context, ActionContext::gameplay());
        assert_eq!(
            outcome.value.is_active(),
            outcome.phase != ActionPhase::Released
        );
        if outcome.cause == SemanticActionCause::PhysicalInput {
            let provenance = outcome.provenance.unwrap();
            assert!(provenance.sequence() > 0);
            assert_eq!(provenance.route().seat(), outcome.seat);
        } else {
            assert_eq!(outcome.phase, ActionPhase::Released);
            assert!(outcome.provenance.is_none());
        }
        if !actions.as_slice()[index + 1..].iter().any(|later| {
            later.seat == outcome.seat
                && later.action == outcome.action
                && later.context == outcome.context
        }) {
            let query = held(&actions, seat);
            match outcome.value {
                ActionValue::Axis(value) => assert_eq!(query.0, value.get()),
                ActionValue::Button(value) => assert_eq!(query.1, value),
            }
        }
        observed.edges.push(Edge {
            seat,
            action: outcome.action.as_str().to_owned(),
            phase: outcome.phase,
            value: outcome.value,
            cause: outcome.cause,
        });
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ConfigurationChange {
    Present { window: Window, seat: Seat },
    Rebind { seat: Seat, bindings: Bindings },
    Context { seat: Seat, enabled: bool },
    Remove { seat: Seat },
    Restore { seat: Seat },
    Capture { seat: Option<Seat> },
}

#[derive(Resource)]
struct Configuration {
    pending: Arc<Mutex<Vec<ConfigurationChange>>>,
    removed: BTreeMap<InputSeatId, ActionMap>,
    capture: Option<Seat>,
    consumed: usize,
}

fn configure(
    mut configuration: ResMut<Configuration>,
    mut maps: ResMut<ActionMaps>,
    mut timeline: ResMut<InputTimeline>,
) {
    let changes = std::mem::take(&mut *configuration.pending.lock().unwrap());
    for change in changes {
        match change {
            ConfigurationChange::Present { window, seat } => {
                timeline
                    .publish_presented_route(InputPresentedRoute::new(
                        window.id(),
                        InputSurfaceId::new(window.id().get() + 100),
                        seat.id(),
                        InputPresentationEpoch::new(1).unwrap(),
                        InputSlotGeneration::new(1).unwrap(),
                        InputRouteGeneration::new(1).unwrap(),
                        InputGeometryGeneration::new(1).unwrap(),
                    ))
                    .unwrap();
            }
            ConfigurationChange::Rebind { seat, bindings } => {
                maps.get_mut(seat.id())
                    .unwrap()
                    .replace_bindings(bindings.compile());
            }
            ConfigurationChange::Context { seat, enabled } => {
                assert!(
                    maps.get_mut(seat.id())
                        .unwrap()
                        .set_context_enabled(&ActionContext::gameplay(), enabled)
                        .unwrap()
                );
            }
            ConfigurationChange::Remove { seat } => {
                let removed = maps.remove(seat.id()).unwrap();
                assert!(configuration.removed.insert(seat.id(), removed).is_none());
            }
            ConfigurationChange::Restore { seat } => {
                let original = configuration.removed.remove(&seat.id()).unwrap();
                assert!(maps.insert(seat.id(), original).unwrap().is_none());
            }
            ConfigurationChange::Capture { seat } => configuration.capture = seat,
        }
    }
}

fn route_ui(mut configuration: ResMut<Configuration>, mut timeline: ResMut<InputTimeline>) {
    let capture = configuration.capture;
    configuration.consumed += timeline.route_pending_ui_input(|entry| {
        capture
            .filter(|seat| seat.id() == entry.route().seat())
            .filter(|_| entry.action_input().is_some())
            .map(|_| InputUiDisposition::Consumed)
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
pub(super) struct Producers {
    pub local_adapter: bool,
    pub ai: bool,
    pub external_driver: bool,
}

impl Producers {
    pub(super) const LIVE: Self = Self {
        local_adapter: true,
        ai: true,
        external_driver: true,
    };
    pub(super) const RECORDED: Self = Self {
        local_adapter: false,
        ai: false,
        external_driver: false,
    };
}

pub(super) struct Started {
    pub runtime: RuntimeInstance,
    pub configuration: Arc<Mutex<Vec<ConfigurationChange>>>,
}

pub(super) fn start(bindings: Bindings, replay: bool, local_probe: bool) -> Started {
    let shared = bindings.compile();
    let primary = map(shared.clone());
    let secondary = map(shared);
    assert!(
        primary
            .bindings_handle()
            .shares_owner_with(&secondary.bindings_handle())
    );
    let mut maps = ActionMaps::new(primary);
    assert!(
        maps.insert(Seat::Controller.id(), secondary)
            .unwrap()
            .is_none()
    );
    let configuration = Arc::new(Mutex::new(Vec::new()));
    let mut app = App::new();
    app.insert_resource(
        InputTimeline::with_limits(
            InputLimits::new(256, 4, 4, 32)
                .unwrap()
                .with_gamepads(8)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    app.insert_resource(maps).unwrap();
    app.add_plugin(nara_engine::input::InputPlugin).unwrap();
    if replay {
        let queue = GameplayCommandQueue::for_replay(
            GameplayCommandQueueSettings::default(),
            GameplayCommandSourceId::new(REPLAY).unwrap(),
        );
        assert_eq!(queue.stats().retained_commands, 0);
        assert_eq!(queue.replay_source().unwrap().as_str(), REPLAY);
        app.insert_resource(queue).unwrap();
    }
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    let producers = if replay {
        Producers::RECORDED
    } else {
        Producers::LIVE
    };
    app.insert_resource(producers).unwrap();
    app.insert_resource(Configuration {
        pending: Arc::clone(&configuration),
        removed: BTreeMap::new(),
        capture: None,
        consumed: 0,
    })
    .unwrap();
    app.init_resource::<GameState>().unwrap();
    app.init_resource::<CapturedTicks>().unwrap();
    app.init_resource::<ObservedActions>().unwrap();
    app.add_systems(
        CoreStage::First,
        configure.before(InputSet::ProjectTimeline),
    )
    .unwrap();
    app.add_systems(CoreStage::First, route_ui.in_set(InputSet::RouteUi))
        .unwrap();
    add_local_gameplay_command_systems(&mut app, observe_actions).unwrap();
    if producers.local_adapter {
        add_local_gameplay_command_systems(&mut app, local_adapter).unwrap();
    }
    if producers.ai {
        app.init_resource::<Autopilot>().unwrap();
        add_local_gameplay_command_systems(&mut app, autopilot).unwrap();
    }
    if local_probe {
        assert!(replay);
        app.world_mut()
            .unwrap()
            .resource_mut::<LocalGameplayCommands>()
            .push(command(
                Player::Copper,
                Some(Seat::Keyboard),
                Intent::Axis(AxisValue::new(1.0).unwrap()),
            ))
            .unwrap();
    }
    app.add_systems(
        CoreStage::FixedUpdate,
        simulate.in_set(GameplayCommandSet::Consume),
    )
    .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        capture.in_set(GameplayCommandSet::Capture),
    )
    .unwrap();
    Started {
        runtime: RuntimeInstance::start(app).unwrap(),
        configuration,
    }
}

pub(super) fn state(runtime: &RuntimeInstance) -> GameState {
    runtime.world().resource::<GameState>().clone()
}

pub(super) fn consumed(runtime: &RuntimeInstance) -> usize {
    runtime.world().resource::<Configuration>().consumed
}

pub(super) fn fixed_nanos(ticks: u32) -> u64 {
    u64::try_from((FixedTime::DEFAULT_TIMESTEP * ticks).as_nanos()).unwrap()
}

pub(super) fn continue_simulation(runtime: &mut RuntimeInstance, ticks: u32) {
    let before = state(runtime);
    let outcome = runtime
        .drive(Duration::from_nanos(fixed_nanos(ticks)))
        .unwrap();
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, ticks);
    let after = state(runtime);
    assert_eq!(after.tick, before.tick + u64::from(ticks));
    for (old, new) in before.players.iter().zip(&after.players) {
        let speed = if old.player == Player::Copper {
            2.0
        } else {
            -3.0
        };
        assert_eq!(
            new.position,
            old.position + f64::from(old.held_axis) * speed * f64::from(ticks)
        );
        assert_eq!(new.held_axis, old.held_axis);
        assert_eq!(
            new.shots,
            old.shots + u64::from(old.firing) * u64::from(ticks)
        );
        assert_eq!(new.commands, old.commands);
    }
}

pub(super) fn original_source_is_known(source: &GameplayCommandSource) -> bool {
    match source {
        GameplayCommandSource::LocalAction => true,
        GameplayCommandSource::Ai { agent } => agent.as_str() == AI,
        GameplayCommandSource::External { producer } => producer.as_str() == EXTERNAL,
        _ => false,
    }
}
