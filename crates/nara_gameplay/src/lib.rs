//! Gameplay command envelopes and semantic action-to-command bridging.

use std::{
    collections::BTreeMap,
    fmt::{self, Display, Formatter},
};

use nara_app::{App, CoreStage, Plugin, PluginError, RealTime};
use nara_ecs::{
    Res, ResMut, Resource,
    schedule::{IntoScheduleConfigs, SystemSet},
};
use nara_input::{ActionId, ActionOutcomes, ActionPhase, InputSet};
use thiserror::Error;

const GAMEPLAY_COMMAND_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.gameplay.commands");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GameplayCommandTypeId(String);

impl GameplayCommandTypeId {
    pub fn new(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        validate_id("command type id", &id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for GameplayCommandTypeId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneStableId(String);

impl SceneStableId {
    pub fn new(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        validate_id("scene stable id", &id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SceneStableId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PersistentRuntimeId(String);

impl PersistentRuntimeId {
    pub fn parse_str(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        uuid::Uuid::parse_str(&id).map_err(|_| GameplayCommandIdError::InvalidUuid)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for PersistentRuntimeId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameplayCommandIdError {
    #[error("{kind} cannot be empty")]
    Empty { kind: &'static str },
    #[error("{kind} cannot contain control characters")]
    ContainsControl { kind: &'static str },
    #[error("persistent runtime id must be a UUID")]
    InvalidUuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GameplayCommandTarget {
    Scene(SceneStableId),
    Persistent(PersistentRuntimeId),
    Named(String),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GameplayCommandValue {
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
}

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GameplayCommandPayload {
    values: BTreeMap<String, GameplayCommandValue>,
}

impl GameplayCommandPayload {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: GameplayCommandValue,
    ) -> Result<Option<GameplayCommandValue>, GameplayCommandPayloadError> {
        let key = key.into();
        validate_payload_key(&key)?;
        Ok(self.values.insert(key, value))
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&GameplayCommandValue> {
        self.values.get(key)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &GameplayCommandValue)> {
        self.values.iter()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameplayCommandPayloadError {
    #[error("payload key cannot be empty")]
    EmptyKey,
    #[error("payload key cannot contain control characters")]
    ContainsControl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GameplayCommandSource {
    LocalAction { action: ActionId },
    Test,
    Replay { stream: String },
    Ai { agent: String },
    External { producer: String },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GameplayCommandTime {
    pub frame: u64,
    pub fixed_tick: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GameplayCommandEnvelope {
    pub sequence: u64,
    pub command_type: GameplayCommandTypeId,
    pub source: GameplayCommandSource,
    pub time: GameplayCommandTime,
    pub target: Option<GameplayCommandTarget>,
    pub payload: GameplayCommandPayload,
}

impl GameplayCommandEnvelope {
    #[must_use]
    pub fn new(
        command_type: GameplayCommandTypeId,
        source: GameplayCommandSource,
        time: GameplayCommandTime,
    ) -> Self {
        Self {
            sequence: 0,
            command_type,
            source,
            time,
            target: None,
            payload: GameplayCommandPayload::default(),
        }
    }

    #[must_use]
    pub fn with_target(mut self, target: GameplayCommandTarget) -> Self {
        self.target = Some(target);
        self
    }

    #[must_use]
    pub fn with_payload(mut self, payload: GameplayCommandPayload) -> Self {
        self.payload = payload;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionCommandBinding {
    pub action: ActionId,
    pub phase: ActionPhase,
    pub command_type: GameplayCommandTypeId,
    pub target: Option<GameplayCommandTarget>,
    pub payload: GameplayCommandPayload,
}

impl ActionCommandBinding {
    #[must_use]
    pub fn new(action: ActionId, phase: ActionPhase, command_type: GameplayCommandTypeId) -> Self {
        Self {
            action,
            phase,
            command_type,
            target: None,
            payload: GameplayCommandPayload::default(),
        }
    }

    #[must_use]
    pub fn with_target(mut self, target: GameplayCommandTarget) -> Self {
        self.target = Some(target);
        self
    }

    #[must_use]
    pub fn with_payload(mut self, payload: GameplayCommandPayload) -> Self {
        self.payload = payload;
        self
    }
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionCommandMap {
    bindings: Vec<ActionCommandBinding>,
}

impl ActionCommandMap {
    pub fn bind(&mut self, binding: ActionCommandBinding) {
        self.bindings.push(binding);
    }

    pub fn bind_action(
        &mut self,
        action: ActionId,
        phase: ActionPhase,
        command_type: GameplayCommandTypeId,
    ) {
        self.bind(ActionCommandBinding::new(action, phase, command_type));
    }

    #[must_use]
    pub fn bindings(&self) -> &[ActionCommandBinding] {
        &self.bindings
    }
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GameplayCommandQueue {
    next_sequence: u64,
    commands: Vec<GameplayCommandEnvelope>,
}

impl GameplayCommandQueue {
    pub fn push(&mut self, mut command: GameplayCommandEnvelope) -> u64 {
        self.next_sequence = self.next_sequence.saturating_add(1);
        command.sequence = self.next_sequence;
        let sequence = command.sequence;
        self.commands.push(command);
        sequence
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    #[must_use]
    pub fn as_slice(&self) -> &[GameplayCommandEnvelope] {
        &self.commands
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum GameplayCommandSet {
    MapActions,
    Clear,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GameplayCommandPlugin;

impl Plugin for GameplayCommandPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            GAMEPLAY_COMMAND_PLUGIN_ID,
            nara_app::PluginCategory::Runtime,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ActionCommandMap::default())
            .insert_resource(GameplayCommandQueue::default())
            .add_systems(
                CoreStage::PreUpdate,
                map_action_outcomes_to_commands
                    .after(InputSet::ResolveActions)
                    .in_set(GameplayCommandSet::MapActions),
            )
            .add_systems(
                CoreStage::Last,
                clear_gameplay_commands.in_set(GameplayCommandSet::Clear),
            );
        Ok(())
    }
}

fn map_action_outcomes_to_commands(
    command_map: Res<ActionCommandMap>,
    outcomes: Option<Res<ActionOutcomes>>,
    real_time: Res<RealTime>,
    mut queue: ResMut<GameplayCommandQueue>,
) {
    let Some(outcomes) = outcomes else {
        return;
    };
    let time = GameplayCommandTime {
        frame: real_time.frame,
        fixed_tick: None,
    };

    for outcome in outcomes.as_slice() {
        for binding in command_map.bindings() {
            if binding.action != outcome.action || binding.phase != outcome.phase {
                continue;
            }

            let mut command = GameplayCommandEnvelope::new(
                binding.command_type.clone(),
                GameplayCommandSource::LocalAction {
                    action: outcome.action.clone(),
                },
                time,
            );
            command.target = binding.target.clone();
            command.payload = binding.payload.clone();
            queue.push(command);
        }
    }
}

fn clear_gameplay_commands(mut queue: ResMut<GameplayCommandQueue>) {
    queue.clear();
}

fn validate_id(kind: &'static str, id: &str) -> Result<(), GameplayCommandIdError> {
    if id.is_empty() {
        return Err(GameplayCommandIdError::Empty { kind });
    }
    if id.chars().any(|character| character.is_control()) {
        return Err(GameplayCommandIdError::ContainsControl { kind });
    }
    Ok(())
}

fn validate_payload_key(key: &str) -> Result<(), GameplayCommandPayloadError> {
    if key.is_empty() {
        return Err(GameplayCommandPayloadError::EmptyKey);
    }
    if key.chars().any(|character| character.is_control()) {
        return Err(GameplayCommandPayloadError::ContainsControl);
    }
    Ok(())
}

pub mod prelude {
    pub use crate::{
        ActionCommandBinding, ActionCommandMap, GameplayCommandEnvelope, GameplayCommandIdError,
        GameplayCommandPayload, GameplayCommandPayloadError, GameplayCommandPlugin,
        GameplayCommandQueue, GameplayCommandSet, GameplayCommandSource, GameplayCommandTarget,
        GameplayCommandTime, GameplayCommandTypeId, GameplayCommandValue, PersistentRuntimeId,
        SceneStableId,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_app::{CoreStage, FixedTime};
    use nara_ecs::Res;
    use nara_input::{ActionBinding, ActionMap, InputPlugin, KeyCode};

    #[derive(Debug, Default, Resource)]
    struct ObservedCommands(Vec<GameplayCommandEnvelope>);

    fn observe_commands(queue: Res<GameplayCommandQueue>, mut observed: ResMut<ObservedCommands>) {
        observed.0 = queue.as_slice().to_vec();
    }

    #[test]
    fn test_code_can_push_command_without_input_or_transport() {
        let mut queue = GameplayCommandQueue::default();
        let command = GameplayCommandEnvelope::new(
            GameplayCommandTypeId::new("jump").unwrap(),
            GameplayCommandSource::Test,
            GameplayCommandTime {
                frame: 7,
                fixed_tick: Some(3),
            },
        );

        let sequence = queue.push(command);

        assert_eq!(sequence, 1);
        assert_eq!(queue.as_slice()[0].sequence, 1);
        assert_eq!(queue.as_slice()[0].source, GameplayCommandSource::Test);
    }

    #[test]
    fn action_outcomes_map_to_commands_before_fixed_update() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin).unwrap();
        app.insert_resource(ObservedCommands::default())
            .add_systems(CoreStage::FixedUpdate, observe_commands);

        let action = ActionId::new("jump").unwrap();
        app.world_mut()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Space));
        app.world_mut()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action,
                ActionPhase::Started,
                GameplayCommandTypeId::new("player.jump").unwrap(),
            ));
        app.world_mut()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Space);

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let observed = &app.world().resource::<ObservedCommands>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].sequence, 1);
        assert_eq!(observed[0].command_type.as_str(), "player.jump");
        assert_eq!(observed[0].time.frame, 1);
        assert_eq!(
            observed[0].source,
            GameplayCommandSource::LocalAction {
                action: ActionId::new("jump").unwrap()
            }
        );
        assert!(app.world().resource::<GameplayCommandQueue>().is_empty());
    }

    #[test]
    fn action_bridge_preserves_target_and_payload() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin).unwrap();
        app.insert_resource(ObservedCommands::default())
            .add_systems(CoreStage::Update, observe_commands);

        let action = ActionId::new("select").unwrap();
        let mut payload = GameplayCommandPayload::new();
        payload
            .insert("slot", GameplayCommandValue::I64(2))
            .unwrap();
        let target = GameplayCommandTarget::Scene(SceneStableId::new("scene/player").unwrap());
        app.world_mut()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Enter));
        app.world_mut().resource_mut::<ActionCommandMap>().bind(
            ActionCommandBinding::new(
                action,
                ActionPhase::Started,
                GameplayCommandTypeId::new("ui.select").unwrap(),
            )
            .with_target(target.clone())
            .with_payload(payload.clone()),
        );
        app.world_mut()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);

        app.run_once(std::time::Duration::ZERO).unwrap();

        let command = &app.world().resource::<ObservedCommands>().0[0];
        assert_eq!(command.target, Some(target));
        assert_eq!(command.payload, payload);
    }

    #[test]
    fn queue_preserves_multiple_producer_order() {
        let mut queue = GameplayCommandQueue::default();
        queue.push(GameplayCommandEnvelope::new(
            GameplayCommandTypeId::new("first").unwrap(),
            GameplayCommandSource::Test,
            GameplayCommandTime::default(),
        ));
        queue.push(GameplayCommandEnvelope::new(
            GameplayCommandTypeId::new("second").unwrap(),
            GameplayCommandSource::External {
                producer: "driver".to_owned(),
            },
            GameplayCommandTime::default(),
        ));

        assert_eq!(
            queue
                .as_slice()
                .iter()
                .map(|command| command.command_type.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert_eq!(queue.as_slice()[0].sequence, 1);
        assert_eq!(queue.as_slice()[1].sequence, 2);
    }

    #[test]
    fn persistent_runtime_ids_validate_uuid_shape() {
        assert!(PersistentRuntimeId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").is_ok());
        assert_eq!(
            PersistentRuntimeId::parse_str("not-a-uuid"),
            Err(GameplayCommandIdError::InvalidUuid)
        );
    }
}
