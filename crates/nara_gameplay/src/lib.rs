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
use nara_input::{ActionContext, ActionId, ActionOutcomes, ActionPhase, InputSet};
use thiserror::Error;

const GAMEPLAY_COMMAND_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.gameplay.commands");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[cfg(feature = "serde")]
impl serde::Serialize for GameplayCommandTypeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for GameplayCommandTypeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[cfg(feature = "serde")]
impl serde::Serialize for SceneStableId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SceneStableId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PersistentRuntimeId(String);

impl PersistentRuntimeId {
    pub fn parse_str(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        let id = uuid::Uuid::parse_str(&id).map_err(|_| GameplayCommandIdError::InvalidUuid)?;
        Ok(Self(id.to_string()))
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

#[cfg(feature = "serde")]
impl serde::Serialize for PersistentRuntimeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PersistentRuntimeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::parse_str(id).map_err(serde::de::Error::custom)
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum GameplayCommandTarget {
    Scene(SceneStableId),
    Persistent(PersistentRuntimeId),
    Named(String),
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for GameplayCommandTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        enum RawTarget {
            Scene(SceneStableId),
            Persistent(PersistentRuntimeId),
            Named(String),
        }

        match <RawTarget as serde::Deserialize>::deserialize(deserializer)? {
            RawTarget::Scene(scene) => Ok(Self::Scene(scene)),
            RawTarget::Persistent(id) => Ok(Self::Persistent(id)),
            RawTarget::Named(name) => {
                validate_id("named command target", &name).map_err(serde::de::Error::custom)?;
                Ok(Self::Named(name))
            }
        }
    }
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for GameplayCommandPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct RawPayload {
            #[serde(default)]
            values: BTreeMap<String, GameplayCommandValue>,
        }

        let raw = <RawPayload as serde::Deserialize>::deserialize(deserializer)?;
        let mut payload = Self::new();
        for (key, value) in raw.values {
            payload
                .insert(key, value)
                .map_err(serde::de::Error::custom)?;
        }
        Ok(payload)
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum GameplayCommandSource {
    LocalAction { action: ActionId },
    Test,
    Replay { stream: String },
    Ai { agent: String },
    External { producer: String },
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for GameplayCommandSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        enum RawSource {
            LocalAction { action: ActionId },
            Test,
            Replay { stream: String },
            Ai { agent: String },
            External { producer: String },
        }

        match <RawSource as serde::Deserialize>::deserialize(deserializer)? {
            RawSource::LocalAction { action } => Ok(Self::LocalAction { action }),
            RawSource::Test => Ok(Self::Test),
            RawSource::Replay { stream } => {
                validate_id("replay stream", &stream).map_err(serde::de::Error::custom)?;
                Ok(Self::Replay { stream })
            }
            RawSource::Ai { agent } => {
                validate_id("AI agent", &agent).map_err(serde::de::Error::custom)?;
                Ok(Self::Ai { agent })
            }
            RawSource::External { producer } => {
                validate_id("external producer", &producer).map_err(serde::de::Error::custom)?;
                Ok(Self::External { producer })
            }
        }
    }
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
    #[cfg_attr(feature = "serde", serde(default))]
    pub context: ActionContext,
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
            context: ActionContext::gameplay(),
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
    pub fn with_context(mut self, context: ActionContext) -> Self {
        self.context = context;
        self
    }

    #[must_use]
    pub fn with_payload(mut self, payload: GameplayCommandPayload) -> Self {
        self.payload = payload;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ActionCommandKey {
    action: ActionId,
    context: ActionContext,
    phase: ActionPhase,
}

impl ActionCommandKey {
    fn new(action: ActionId, context: ActionContext, phase: ActionPhase) -> Self {
        Self {
            action,
            context,
            phase,
        }
    }

    fn from_binding(binding: &ActionCommandBinding) -> Self {
        Self::new(
            binding.action.clone(),
            binding.context.clone(),
            binding.phase,
        )
    }
}

#[derive(Debug, Default, Clone, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ActionCommandMap {
    bindings: Vec<ActionCommandBinding>,
    #[cfg_attr(feature = "serde", serde(skip))]
    bindings_by_action: BTreeMap<ActionCommandKey, Vec<usize>>,
}

impl PartialEq for ActionCommandMap {
    fn eq(&self, other: &Self) -> bool {
        self.bindings == other.bindings
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ActionCommandMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct RawActionCommandMap {
            #[serde(default)]
            bindings: Vec<ActionCommandBinding>,
        }

        let raw = <RawActionCommandMap as serde::Deserialize>::deserialize(deserializer)?;
        let mut command_map = Self::default();
        for binding in raw.bindings {
            command_map.bind(binding);
        }
        Ok(command_map)
    }
}

impl ActionCommandMap {
    pub fn bind(&mut self, binding: ActionCommandBinding) {
        self.bindings_by_action
            .entry(ActionCommandKey::from_binding(&binding))
            .or_default()
            .push(self.bindings.len());
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

    pub fn matching_bindings(
        &self,
        action: &ActionId,
        context: &ActionContext,
        phase: ActionPhase,
    ) -> impl Iterator<Item = &ActionCommandBinding> {
        let key = ActionCommandKey::new(action.clone(), context.clone(), phase);
        self.bindings_by_action
            .get(&key)
            .into_iter()
            .flat_map(|indices| indices.iter().filter_map(|index| self.bindings.get(*index)))
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
        if !app.world().contains_resource::<ActionCommandMap>() {
            app.insert_resource(ActionCommandMap::default())?;
        }
        if !app.world().contains_resource::<GameplayCommandQueue>() {
            app.insert_resource(GameplayCommandQueue::default())?;
        }
        app.add_systems(
            CoreStage::PreUpdate,
            map_action_outcomes_to_commands
                .after(InputSet::ResolveActions)
                .in_set(GameplayCommandSet::MapActions),
        )?
        .add_systems(
            CoreStage::Last,
            clear_gameplay_commands.in_set(GameplayCommandSet::Clear),
        )?;
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
        for binding in
            command_map.matching_bindings(&outcome.action, &outcome.context, outcome.phase)
        {
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
    if !queue.is_empty() {
        queue.clear();
    }
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
    use nara_input::{ActionBinding, ActionContext, ActionMap, InputPlugin, KeyCode};

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
            .unwrap()
            .add_systems(CoreStage::FixedUpdate, observe_commands)
            .unwrap();

        let action = ActionId::new("jump").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Space));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action,
                ActionPhase::Started,
                GameplayCommandTypeId::new("player.jump").unwrap(),
            ));
        app.world_mut()
            .unwrap()
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
            .unwrap()
            .add_systems(CoreStage::Update, observe_commands)
            .unwrap();

        let action = ActionId::new("select").unwrap();
        let mut payload = GameplayCommandPayload::new();
        payload
            .insert("slot", GameplayCommandValue::I64(2))
            .unwrap();
        let target = GameplayCommandTarget::Scene(SceneStableId::new("scene/player").unwrap());
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Enter));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(
                ActionCommandBinding::new(
                    action,
                    ActionPhase::Started,
                    GameplayCommandTypeId::new("ui.select").unwrap(),
                )
                .with_target(target.clone())
                .with_payload(payload.clone()),
            );
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);

        app.run_once(std::time::Duration::ZERO).unwrap();

        let command = &app.world().resource::<ObservedCommands>().0[0];
        assert_eq!(command.target, Some(target));
        assert_eq!(command.payload, payload);
    }

    #[test]
    fn action_bridge_filters_by_context() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin).unwrap();
        app.insert_resource(ObservedCommands::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_commands)
            .unwrap();

        let menu = ActionContext::new("menu").unwrap();
        let action = ActionId::new("confirm").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Enter).with_context(menu.clone()));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(
                ActionCommandBinding::new(
                    action.clone(),
                    ActionPhase::Started,
                    GameplayCommandTypeId::new("gameplay.confirm").unwrap(),
                )
                .with_context(ActionContext::gameplay()),
            );
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(
                ActionCommandBinding::new(
                    action,
                    ActionPhase::Started,
                    GameplayCommandTypeId::new("menu.confirm").unwrap(),
                )
                .with_context(menu),
            );
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);

        app.run_once(std::time::Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedCommands>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].command_type.as_str(), "menu.confirm");
    }

    #[test]
    fn action_bridge_filters_by_phase() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin).unwrap();
        app.insert_resource(ObservedCommands::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_commands)
            .unwrap();

        let action = ActionId::new("cancel").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Escape));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action.clone(),
                ActionPhase::Started,
                GameplayCommandTypeId::new("cancel.started").unwrap(),
            ));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action,
                ActionPhase::Released,
                GameplayCommandTypeId::new("cancel.released").unwrap(),
            ));

        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.run_once(std::time::Duration::ZERO).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .release(KeyCode::Escape);
        app.run_once(std::time::Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedCommands>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].command_type.as_str(), "cancel.released");
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

    #[test]
    fn persistent_runtime_ids_canonicalize_uuid_spellings() {
        let canonical =
            PersistentRuntimeId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap();
        let uppercase =
            PersistentRuntimeId::parse_str("2F0D71C7-14FC-4ED4-B48B-1C61BBA8B97F").unwrap();
        let simple = PersistentRuntimeId::parse_str("2f0d71c714fc4ed4b48b1c61bba8b97f").unwrap();

        assert_eq!(canonical, uppercase);
        assert_eq!(canonical, simple);
        assert_eq!(canonical.as_str(), "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_defaults_action_command_binding_context_to_gameplay() {
        let binding = serde_json::from_str::<ActionCommandBinding>(
            r#"{
                "action": "jump",
                "phase": "Started",
                "command_type": "movement.jump",
                "target": null,
                "payload": { "values": {} }
            }"#,
        )
        .unwrap();

        assert_eq!(binding.context, ActionContext::gameplay());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_invalid_command_identity_and_payload_keys() {
        assert!(serde_json::from_str::<GameplayCommandTypeId>("\"\"").is_err());
        assert!(serde_json::from_str::<SceneStableId>("\"scene\\nplayer\"").is_err());
        assert!(serde_json::from_str::<PersistentRuntimeId>("\"not-a-uuid\"").is_err());

        let invalid_payload = r#"{"values":{"bad\nkey":{"Bool":true}}}"#;

        assert!(serde_json::from_str::<GameplayCommandPayload>(invalid_payload).is_err());
        assert!(serde_json::from_str::<GameplayCommandTarget>(r#"{"Named":""}"#).is_err());
        assert!(
            serde_json::from_str::<GameplayCommandSource>(r#"{"Replay":{"stream":"bad\nstream"}}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<GameplayCommandSource>(r#"{"External":{"producer":""}}"#)
                .is_err()
        );
    }
}
