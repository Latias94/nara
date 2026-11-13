use std::{collections::BTreeMap, time::Duration};

use nara_engine::{
    app::{
        RuntimeControl, RuntimeControlRequestResult, RuntimeDriverScope, RuntimeInstance,
        RuntimeState,
    },
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandKey,
        GameplayCommandQueue, GameplayCommandRejection, GameplayCommandSource,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
        GameplayCommandValue, submit_gameplay_driver_command,
    },
    input::{
        ActionContext, ActionMaps, ActionValue, AxisValue, ButtonDriverInput,
        ButtonInputDriverError, ButtonTransitionPhase, GamepadConnection, InputIngressError,
        InputTimeline, InputWindowEvent, KeyCode, SemanticActions, apply_gamepad_axis_input,
        apply_gamepad_button_input, apply_keyboard_driver_input_at,
        apply_window_input_driver_event, connect_gamepad_input, connect_input_endpoint,
        connect_synthetic_gamepad_input, disconnect_gamepad_input,
    },
};
use serde::{Deserialize, Serialize};

use super::runtime::{
    Bindings, CapturedTick, CapturedTicks, ConfigurationChange, EXTERNAL, FIRE_COMMAND, GameState,
    Intent, MOVE, MOVE_COMMAND, ObservedActions, Player, Producers, REPLAY, Seat, Started, Window,
    action, command, consumed, held, original_source_is_known, start, state, submission,
};

const FORMAT: u32 = 1;
const MAX_ENCODED_BYTES: usize = 256 * 1024;
const MAX_OPERATIONS: usize = 512;
const MAX_TICKS: usize = 128;
const MAX_COMMANDS: usize = 256;
const BACKEND_LABEL: u16 = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ImportError {
    Encoding,
    Version,
    PhysicalPolicy,
    Vocabulary,
    Identity,
    ValueKind,
    Ordering,
    Budget,
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ImportError> {
    if bytes.len() > MAX_ENCODED_BYTES {
        return Err(ImportError::Budget);
    }
    serde_json::from_slice(bytes).map_err(|_| ImportError::Encoding)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Normalization {
    FiniteSignedScalarV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DeviceOrigin {
    Platform,
    Synthetic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Admission {
    Accept,
    Expired,
    Foreign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Control {
    Pause,
    ExactStep,
    Resume,
}

impl Control {
    fn runtime(self) -> RuntimeControl {
        match self {
            Self::Pause => RuntimeControl::Pause,
            Self::ExactStep => RuntimeControl::StepFixedTick,
            Self::Resume => RuntimeControl::Resume,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum RawOperation {
    Endpoint {
        window: Window,
        seat: Seat,
    },
    Connect {
        device: u8,
        window: Window,
        seat: Seat,
        origin: DeviceOrigin,
    },
    Disconnect {
        device: u8,
        admission: Admission,
    },
    Key {
        window: Window,
        seat: Seat,
        key: KeyCode,
        pressed: bool,
    },
    Button {
        device: u8,
        button: u16,
        pressed: bool,
        admission: Admission,
    },
    Axis {
        device: u8,
        axis: u16,
        value: AxisValue,
        admission: Admission,
    },
    ForeignAxis {
        value: AxisValue,
    },
    Focus {
        window: Window,
        seat: Seat,
        focused: bool,
    },
    Configure {
        configuration: ConfigurationChange,
    },
    External {
        tick: u64,
        sequence: u64,
        player: Player,
        intent: Intent,
    },
    Control {
        control: Control,
    },
    Frame {
        real_delta_nanos: u64,
        steps: u32,
        checkpoint: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawRecording {
    pub format_version: u32,
    pub normalization: Normalization,
    pub bindings: Bindings,
    pub fixed_timestep_nanos: u64,
    pub operations: Vec<RawOperation>,
}

impl RawRecording {
    pub(super) fn new(operations: Vec<RawOperation>) -> Self {
        Self {
            format_version: FORMAT,
            normalization: Normalization::FiniteSignedScalarV1,
            bindings: Bindings::OriginalV1,
            fixed_timestep_nanos: super::runtime::fixed_nanos(1),
            operations,
        }
    }

    pub(super) fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    pub(super) fn load(bytes: &[u8], requested_bindings: Bindings) -> Result<Self, ImportError> {
        let recording: Self = decode(bytes)?;
        if recording.format_version != FORMAT {
            return Err(ImportError::Version);
        }
        if recording.bindings != requested_bindings
            || recording.fixed_timestep_nanos != super::runtime::fixed_nanos(1)
        {
            return Err(ImportError::PhysicalPolicy);
        }
        if recording.operations.len() > MAX_OPERATIONS {
            return Err(ImportError::Budget);
        }
        let mut devices = BTreeMap::new();
        let mut paused = false;
        let mut pending_control = None;
        let mut ticks = 0_u64;
        for operation in &recording.operations {
            match operation {
                RawOperation::Connect { device, .. }
                    if *device >= 8 || devices.insert(*device, true).is_some() =>
                {
                    return Err(ImportError::Identity);
                }
                RawOperation::Disconnect { device, admission } => {
                    let connected = devices.get_mut(device).ok_or(ImportError::Identity)?;
                    match admission {
                        Admission::Accept if *connected => *connected = false,
                        Admission::Expired if !*connected => {}
                        _ => return Err(ImportError::Identity),
                    }
                }
                RawOperation::Axis {
                    device, admission, ..
                }
                | RawOperation::Button {
                    device, admission, ..
                } => {
                    let connected = *devices.get(device).ok_or(ImportError::Identity)?;
                    if !matches!(
                        (connected, *admission),
                        (true, Admission::Accept) | (false, Admission::Expired)
                    ) {
                        return Err(ImportError::Identity);
                    }
                }
                RawOperation::Control { control }
                    if pending_control.replace(*control).is_some()
                        || !matches!(
                            (paused, *control),
                            (false, Control::Pause) | (true, Control::ExactStep | Control::Resume)
                        ) =>
                {
                    return Err(ImportError::Ordering);
                }
                RawOperation::Frame {
                    real_delta_nanos,
                    steps,
                    ..
                } => {
                    if *real_delta_nanos > super::runtime::fixed_nanos(4) || *steps > 4 {
                        return Err(ImportError::Budget);
                    }
                    match pending_control.take() {
                        Some(Control::Pause) => paused = true,
                        Some(Control::Resume) => paused = false,
                        Some(Control::ExactStep) if *steps == 1 => {}
                        Some(Control::ExactStep) => return Err(ImportError::Ordering),
                        None if paused && *steps != 0 => return Err(ImportError::Ordering),
                        None => {}
                    }
                    ticks += u64::from(*steps);
                }
                RawOperation::External { tick, sequence, .. }
                    if *tick == 0
                        || *sequence == 0
                        || *tick <= ticks
                        || *tick > MAX_TICKS as u64 =>
                {
                    return Err(ImportError::Ordering);
                }
                _ => {}
            }
        }
        if pending_control.is_some() || paused || ticks > MAX_TICKS as u64 {
            return Err(ImportError::Ordering);
        }
        Ok(recording)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FrameEvidence {
    pub checkpoint: String,
    pub game: GameState,
    pub semantic: [(f32, bool); 2],
    pub selected_axes: [Option<ActionValue>; 2],
    pub passes: usize,
    pub edges: usize,
    pub consumed: usize,
    pub pending_commands: usize,
    pub paused: bool,
}

pub(super) struct Harness {
    pub started: Started,
    pub connections: BTreeMap<u8, GamepadConnection>,
    pub frames: Vec<FrameEvidence>,
    foreign: GamepadConnection,
}

pub(super) fn driver<T>(
    runtime: &mut RuntimeInstance,
    operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> Result<T, ButtonInputDriverError>,
) -> Result<T, ButtonInputDriverError> {
    let outcome = runtime.with_driver_scope(operation).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output
}

impl Harness {
    pub(super) fn new(bindings: Bindings, replay: bool, local_probe: bool) -> Self {
        let mut foreign_owner = InputTimeline::default();
        foreign_owner
            .connect_endpoint(Window::Guest.id(), Seat::Controller.id())
            .unwrap();
        let foreign = foreign_owner
            .connect_synthetic_gamepad(Window::Guest.id(), Seat::Controller.id(), BACKEND_LABEL)
            .unwrap();
        Self {
            started: start(bindings, replay, local_probe),
            connections: BTreeMap::new(),
            frames: Vec::new(),
            foreign,
        }
    }

    pub(super) fn runtime(&self) -> &RuntimeInstance {
        &self.started.runtime
    }

    pub(super) fn frame(&self, name: &str) -> &FrameEvidence {
        self.frames
            .iter()
            .find(|frame| frame.checkpoint == name)
            .unwrap()
    }

    pub(super) fn capture(&self) -> &[CapturedTick] {
        &self.runtime().world().resource::<CapturedTicks>().0
    }

    pub(super) fn play(&mut self, recording: &RawRecording) {
        assert_eq!(
            *self.runtime().world().resource::<Producers>(),
            Producers::LIVE
        );
        for operation in &recording.operations {
            self.apply(operation);
        }
    }

    fn connection_operation(
        &mut self,
        device: Option<u8>,
        expected: Admission,
        operation: impl FnOnce(
            &mut RuntimeDriverScope<'_>,
            &GamepadConnection,
        ) -> Result<(), ButtonInputDriverError>,
    ) {
        let connection = device
            .map_or(&self.foreign, |device| {
                self.connections.get(&device).unwrap()
            })
            .clone();
        let timeline = self.runtime().world().resource::<InputTimeline>();
        let before = (
            timeline.entries().copied().collect::<Vec<_>>(),
            timeline.cursors(),
            state(self.runtime()),
        );
        let result = driver(&mut self.started.runtime, |scope| {
            operation(scope, &connection)
        });
        match expected {
            Admission::Accept => result.unwrap(),
            Admission::Expired => assert_eq!(
                result,
                Err(ButtonInputDriverError::Timeline(
                    InputIngressError::GamepadConnectionExpired {
                        connection_generation: connection.connection_generation()
                    },
                ))
            ),
            Admission::Foreign => assert_eq!(
                result,
                Err(ButtonInputDriverError::Timeline(
                    InputIngressError::ForeignGamepadConnection,
                ))
            ),
        }
        if expected != Admission::Accept {
            let timeline = self.runtime().world().resource::<InputTimeline>();
            assert_eq!(
                (
                    timeline.entries().copied().collect::<Vec<_>>(),
                    timeline.cursors(),
                    state(self.runtime())
                ),
                before
            );
        }
    }

    pub(super) fn apply(&mut self, operation: &RawOperation) {
        match operation {
            RawOperation::Endpoint { window, seat } => {
                driver(&mut self.started.runtime, |scope| {
                    connect_input_endpoint(scope, window.id(), seat.id())
                })
                .unwrap();
            }
            RawOperation::Connect {
                device,
                window,
                seat,
                origin,
            } => {
                let connection = driver(&mut self.started.runtime, |scope| match origin {
                    DeviceOrigin::Platform => {
                        connect_gamepad_input(scope, window.id(), seat.id(), BACKEND_LABEL)
                    }
                    DeviceOrigin::Synthetic => connect_synthetic_gamepad_input(
                        scope,
                        window.id(),
                        seat.id(),
                        BACKEND_LABEL,
                    ),
                })
                .unwrap();
                for old in self.connections.values() {
                    assert_ne!(*old, connection);
                    assert!(old.connection_generation() < connection.connection_generation());
                }
                assert!(self.connections.insert(*device, connection).is_none());
            }
            RawOperation::Disconnect { device, admission } => {
                self.connection_operation(Some(*device), *admission, |scope, connection| {
                    disconnect_gamepad_input(scope, connection).map(|_| ())
                })
            }
            RawOperation::Axis {
                device,
                axis,
                value,
                admission,
            } => self.connection_operation(Some(*device), *admission, |scope, connection| {
                apply_gamepad_axis_input(scope, connection, *axis, *value).map(|_| ())
            }),
            RawOperation::Button {
                device,
                button,
                pressed,
                admission,
            } => self.connection_operation(Some(*device), *admission, |scope, connection| {
                apply_gamepad_button_input(
                    scope,
                    connection,
                    *button,
                    if *pressed {
                        ButtonTransitionPhase::Pressed
                    } else {
                        ButtonTransitionPhase::Released
                    },
                )
                .map(|_| ())
            }),
            RawOperation::ForeignAxis { value } => {
                self.connection_operation(None, Admission::Foreign, |scope, connection| {
                    apply_gamepad_axis_input(scope, connection, 0, *value).map(|_| ())
                })
            }
            RawOperation::Key {
                window,
                seat,
                key,
                pressed,
            } => {
                driver(&mut self.started.runtime, |scope| {
                    apply_keyboard_driver_input_at(
                        scope,
                        window.id(),
                        seat.id(),
                        if *pressed {
                            ButtonDriverInput::Press(*key)
                        } else {
                            ButtonDriverInput::Release(*key)
                        },
                    )
                })
                .unwrap();
            }
            RawOperation::Focus {
                window,
                seat,
                focused,
            } => {
                driver(&mut self.started.runtime, |scope| {
                    apply_window_input_driver_event(
                        scope,
                        window.id(),
                        seat.id(),
                        InputWindowEvent::Focused(*focused),
                    )
                })
                .unwrap();
            }
            RawOperation::Configure { configuration } => {
                self.started
                    .configuration
                    .lock()
                    .unwrap()
                    .push(configuration.clone());
            }
            RawOperation::External {
                tick,
                sequence,
                player,
                intent,
            } => {
                assert!(
                    self.runtime()
                        .world()
                        .resource::<Producers>()
                        .external_driver
                );
                self.submit(submission(
                    *tick,
                    GameplayCommandIngressSource::external(EXTERNAL).unwrap(),
                    *sequence,
                    command(*player, None, *intent),
                ))
                .unwrap();
            }
            RawOperation::Control { control } => self.control(*control),
            RawOperation::Frame {
                real_delta_nanos,
                steps,
                checkpoint,
            } => {
                self.drive(*real_delta_nanos, *steps, checkpoint);
            }
        }
    }

    pub(super) fn control(&mut self, control: Control) {
        assert!(matches!(
            self.started.runtime.request_control(control.runtime()),
            RuntimeControlRequestResult::Accepted(_)
        ));
    }

    pub(super) fn drive(&mut self, nanos: u64, steps: u32, checkpoint: &str) {
        let before = state(self.runtime());
        let outcome = self
            .started
            .runtime
            .drive(Duration::from_nanos(nanos))
            .unwrap();
        assert_eq!(
            outcome.frame().unwrap().status.fixed_steps,
            steps,
            "{checkpoint}"
        );
        assert_eq!(
            state(self.runtime()).tick,
            before.tick + u64::from(steps),
            "{checkpoint}"
        );
        if steps == 0 {
            assert_eq!(
                state(self.runtime()),
                before,
                "a zero-tick frame cannot execute gameplay: {checkpoint}"
            );
        }
        let actions = self.runtime().world().resource::<SemanticActions>();
        let observed = self.runtime().world().resource::<ObservedActions>();
        let queue = self.runtime().world().resource::<GameplayCommandQueue>();
        let stats = queue.stats();
        assert_eq!(stats.acknowledged_through_tick, state(self.runtime()).tick);
        assert_eq!(stats.active_commands, 0);
        assert_eq!(stats.quarantined_commands, 0);
        assert_eq!(stats.lifecycle_faults, 0);
        assert!(
            self.runtime()
                .world()
                .resource::<InputTimeline>()
                .retained_len()
                <= 256
        );
        let evidence = FrameEvidence {
            checkpoint: checkpoint.to_owned(),
            game: state(self.runtime()),
            semantic: Seat::ALL.map(|seat| held(actions, seat)),
            selected_axes: Seat::ALL
                .map(|seat| actions.value(seat.id(), &action(MOVE), &ActionContext::gameplay())),
            passes: observed.passes,
            edges: observed.edges.len(),
            consumed: consumed(self.runtime()),
            pending_commands: stats.pending_commands,
            paused: self.runtime().state() == RuntimeState::Paused,
        };
        self.frames.push(evidence);
    }

    pub(super) fn submit(
        &mut self,
        submission: GameplayCommandSubmission,
    ) -> Result<GameplayCommandKey, GameplayCommandRejection> {
        let outcome = self
            .started
            .runtime
            .with_driver_scope(|scope| submit_gameplay_driver_command(scope, submission).unwrap())
            .unwrap();
        assert!(outcome.post_operation_fault.is_none());
        outcome.output
    }

    pub(super) fn import_semantic(
        &mut self,
        bytes: &[u8],
    ) -> Result<SemanticRecording, ImportError> {
        let recording = SemanticRecording::load(bytes)?;
        assert_eq!(
            *self.runtime().world().resource::<Producers>(),
            Producers::RECORDED
        );
        assert_eq!(
            self.runtime()
                .world()
                .resource::<GameplayCommandQueue>()
                .stats()
                .retained_commands,
            0
        );
        for tick in &recording.ticks {
            for entry in &tick.commands {
                self.submit(submission(
                    tick.state.tick,
                    GameplayCommandIngressSource::replay(REPLAY).unwrap(),
                    entry.ordinal,
                    entry.command.clone(),
                ))
                .unwrap();
            }
        }
        Ok(recording)
    }

    pub(super) fn play_controls(&mut self, raw: &RawRecording) {
        for operation in &raw.operations {
            match operation {
                RawOperation::Control { control } => self.control(*control),
                RawOperation::Frame {
                    real_delta_nanos,
                    steps,
                    checkpoint,
                } => self.drive(*real_delta_nanos, *steps, checkpoint),
                _ => {}
            }
        }
    }

    pub(super) fn finish(self) {
        self.started.runtime.finish().unwrap();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ValueKind {
    Axis,
    Button,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VocabularyEntry {
    pub command_type: String,
    pub value_kind: ValueKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Vocabulary {
    pub id: String,
    pub revision: u32,
    pub entries: Vec<VocabularyEntry>,
}

impl Vocabulary {
    fn current() -> Self {
        Self {
            id: "consumer.two-player".to_owned(),
            revision: 1,
            entries: vec![
                VocabularyEntry {
                    command_type: MOVE_COMMAND.to_owned(),
                    value_kind: ValueKind::Axis,
                },
                VocabularyEntry {
                    command_type: FIRE_COMMAND.to_owned(),
                    value_kind: ValueKind::Button,
                },
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordedCommand {
    pub ordinal: u64,
    pub original_source: GameplayCommandSource,
    pub original_sequence: GameplayCommandSourceSequence,
    pub command: GameplayCommandDraft,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordedTick {
    pub commands: Vec<RecordedCommand>,
    pub state: GameState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticRecording {
    pub format_version: u32,
    pub vocabulary: Vocabulary,
    pub players: [Player; 2],
    pub fixed_timestep_nanos: u64,
    pub ticks: Vec<RecordedTick>,
}

impl SemanticRecording {
    pub(super) fn capture(captured: &[CapturedTick]) -> Self {
        let mut ordinal = 0;
        let ticks = captured
            .iter()
            .map(|tick| RecordedTick {
                state: tick.state.clone(),
                commands: tick
                    .commands
                    .iter()
                    .map(|envelope| {
                        assert_eq!(envelope.tick().get(), tick.state.tick);
                        ordinal += 1;
                        RecordedCommand {
                            ordinal,
                            original_source: envelope.source().clone(),
                            original_sequence: envelope.source_sequence(),
                            command: envelope.command().clone(),
                        }
                    })
                    .collect(),
            })
            .collect();
        Self {
            format_version: FORMAT,
            vocabulary: Vocabulary::current(),
            players: Player::ALL,
            fixed_timestep_nanos: super::runtime::fixed_nanos(1),
            ticks,
        }
    }

    pub(super) fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    pub(super) fn load(bytes: &[u8]) -> Result<Self, ImportError> {
        let recording: Self = decode(bytes)?;
        if recording.format_version != FORMAT {
            return Err(ImportError::Version);
        }
        if recording.vocabulary != Vocabulary::current() {
            return Err(ImportError::Vocabulary);
        }
        if recording.players != Player::ALL {
            return Err(ImportError::Identity);
        }
        if recording.fixed_timestep_nanos != super::runtime::fixed_nanos(1) {
            return Err(ImportError::PhysicalPolicy);
        }
        if recording.ticks.is_empty() || recording.ticks.len() > MAX_TICKS {
            return Err(ImportError::Budget);
        }
        let mut ordinal = 0_u64;
        for (index, tick) in recording.ticks.iter().enumerate() {
            if tick.state.tick != index as u64 + 1 {
                return Err(ImportError::Ordering);
            }
            for (player, expected) in tick.state.players.iter().zip(Player::ALL) {
                if player.player != expected
                    || !player.position.is_finite()
                    || AxisValue::new(player.held_axis).is_err()
                {
                    return Err(ImportError::Identity);
                }
            }
            let mut previous_key = None;
            for entry in &tick.commands {
                ordinal += 1;
                if ordinal > MAX_COMMANDS as u64 {
                    return Err(ImportError::Budget);
                }
                if entry.ordinal != ordinal {
                    return Err(ImportError::Ordering);
                }
                if !original_source_is_known(&entry.original_source) {
                    return Err(ImportError::Identity);
                }
                let key = GameplayCommandKey::new(
                    GameplayCommandTick::new(tick.state.tick).unwrap(),
                    entry.original_source.clone(),
                    entry.original_sequence,
                );
                if previous_key
                    .as_ref()
                    .is_some_and(|previous| previous >= &key)
                {
                    return Err(ImportError::Ordering);
                }
                previous_key = Some(key);
                validate_command(entry)?;
            }
        }
        Ok(recording)
    }
}

fn validate_command(entry: &RecordedCommand) -> Result<(), ImportError> {
    let player = Player::from_target(entry.command.target()).ok_or(ImportError::Identity)?;
    if entry.command.payload().len() != 2 {
        return Err(ImportError::ValueKind);
    }
    let Some(GameplayCommandValue::I64(seat_id)) = entry.command.payload().get("seat") else {
        return Err(ImportError::Identity);
    };
    if entry.original_source == GameplayCommandSource::LocalAction {
        if !Seat::ALL
            .into_iter()
            .any(|seat| i64::from(seat.id().get()) == *seat_id && seat.player() == player)
        {
            return Err(ImportError::Identity);
        }
    } else if *seat_id != -1 {
        return Err(ImportError::Identity);
    }
    match (
        entry.command.command_type().as_str(),
        entry.command.payload().get("value"),
    ) {
        (MOVE_COMMAND, Some(GameplayCommandValue::F64(value))) => {
            let axis = AxisValue::new(*value as f32).map_err(|_| ImportError::ValueKind)?;
            if f64::from(axis.get()) != *value {
                return Err(ImportError::ValueKind);
            }
        }
        (FIRE_COMMAND, Some(GameplayCommandValue::Bool(_))) => {}
        (MOVE_COMMAND | FIRE_COMMAND, _) => return Err(ImportError::ValueKind),
        _ => return Err(ImportError::Vocabulary),
    }
    Ok(())
}

pub(super) fn assert_rebound(harness: &Harness) {
    let maps = harness.runtime().world().resource::<ActionMaps>();
    for seat in Seat::ALL {
        assert_eq!(
            maps.get(seat.id()).unwrap().bindings(),
            Bindings::ReboundV1.compile().definition().bindings()
        );
        assert_ne!(
            maps.get(seat.id()).unwrap().bindings(),
            Bindings::OriginalV1.compile().definition().bindings()
        );
    }
}
