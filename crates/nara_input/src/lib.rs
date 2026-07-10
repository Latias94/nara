//! Input state primitives and semantic action outcomes.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    error::Error,
    fmt::{self, Display, Formatter},
    hash::Hash,
};

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_core::Vec2;
use nara_ecs::{
    Res, ResMut, Resource,
    schedule::{IntoScheduleConfigs, SystemSet},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum InputSet {
    ResolveActions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum KeyCode {
    Escape,
    Space,
    Enter,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Character(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

#[derive(Debug, Clone, Resource)]
pub struct ButtonInput<T> {
    pressed: HashSet<T>,
    just_pressed: HashSet<T>,
    just_released: HashSet<T>,
}

impl<T> Default for ButtonInput<T> {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
        }
    }
}

impl<T> ButtonInput<T>
where
    T: Copy + Eq + Hash,
{
    pub fn press(&mut self, button: T) {
        if self.pressed.insert(button) {
            self.just_pressed.insert(button);
        }
        self.just_released.remove(&button);
    }

    pub fn release(&mut self, button: T) {
        if self.pressed.remove(&button) {
            self.just_released.insert(button);
        }
        self.just_pressed.remove(&button);
    }

    #[must_use]
    pub fn pressed(&self, button: T) -> bool {
        self.pressed.contains(&button)
    }

    #[must_use]
    pub fn just_pressed(&self, button: T) -> bool {
        self.just_pressed.contains(&button)
    }

    #[must_use]
    pub fn just_released(&self, button: T) -> bool {
        self.just_released.contains(&button)
    }

    pub fn clear_transitions(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }

    #[must_use]
    pub fn has_transitions(&self) -> bool {
        !self.just_pressed.is_empty() || !self.just_released.is_empty()
    }

    pub fn just_pressed_buttons(&self) -> impl Iterator<Item = T> + '_ {
        self.just_pressed.iter().copied()
    }

    pub fn just_released_buttons(&self) -> impl Iterator<Item = T> + '_ {
        self.just_released.iter().copied()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Resource)]
pub struct PointerState {
    position: Option<Vec2>,
}

impl PointerState {
    #[must_use]
    pub const fn position(self) -> Option<Vec2> {
        self.position
    }

    pub fn set_position(&mut self, position: Vec2) {
        self.position = Some(position);
    }

    pub fn clear_position(&mut self) {
        self.position = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionId(String);

impl ActionId {
    pub fn new(id: impl Into<String>) -> Result<Self, ActionIdError> {
        let id = id.into();
        validate_identifier(&id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ActionId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ActionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ActionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionIdError {
    Empty,
    ContainsControl,
}

impl Display for ActionIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("action id cannot be empty"),
            Self::ContainsControl => {
                formatter.write_str("action id cannot contain control characters")
            }
        }
    }
}

impl Error for ActionIdError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionContext(String);

impl Default for ActionContext {
    fn default() -> Self {
        Self::gameplay()
    }
}

impl ActionContext {
    #[must_use]
    pub fn gameplay() -> Self {
        Self("gameplay".to_owned())
    }

    pub fn new(id: impl Into<String>) -> Result<Self, ActionIdError> {
        let id = id.into();
        validate_identifier(&id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ActionContext {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ActionContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ActionContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InputBinding {
    Key(KeyCode),
    Mouse(MouseButton),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionBinding {
    pub action: ActionId,
    pub input: InputBinding,
    pub context: ActionContext,
}

impl ActionBinding {
    #[must_use]
    pub fn key(action: ActionId, key: KeyCode) -> Self {
        Self {
            action,
            input: InputBinding::Key(key),
            context: ActionContext::default(),
        }
    }

    #[must_use]
    pub fn mouse(action: ActionId, button: MouseButton) -> Self {
        Self {
            action,
            input: InputBinding::Mouse(button),
            context: ActionContext::default(),
        }
    }

    #[must_use]
    pub fn with_context(mut self, context: ActionContext) -> Self {
        self.context = context;
        self
    }
}

#[derive(Debug, Default, Clone, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ActionMap {
    bindings: Vec<ActionBinding>,
    disabled_contexts: BTreeSet<ActionContext>,
    #[cfg_attr(feature = "serde", serde(skip))]
    bindings_by_input: BTreeMap<InputBinding, Vec<usize>>,
}

impl PartialEq for ActionMap {
    fn eq(&self, other: &Self) -> bool {
        self.bindings == other.bindings && self.disabled_contexts == other.disabled_contexts
    }
}

impl Eq for ActionMap {}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ActionMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct RawActionMap {
            #[serde(default)]
            bindings: Vec<ActionBinding>,
            #[serde(default)]
            disabled_contexts: BTreeSet<ActionContext>,
        }

        let raw = <RawActionMap as serde::Deserialize>::deserialize(deserializer)?;
        let mut action_map = Self::default();
        for binding in raw.bindings {
            action_map.bind(binding);
        }
        action_map.disabled_contexts = raw.disabled_contexts;
        Ok(action_map)
    }
}

impl ActionMap {
    pub fn bind(&mut self, binding: ActionBinding) {
        self.bindings_by_input
            .entry(binding.input)
            .or_default()
            .push(self.bindings.len());
        self.bindings.push(binding);
    }

    pub fn bind_key(&mut self, action: ActionId, key: KeyCode) {
        self.bind(ActionBinding::key(action, key));
    }

    pub fn bind_mouse(&mut self, action: ActionId, button: MouseButton) {
        self.bind(ActionBinding::mouse(action, button));
    }

    pub fn disable_context(&mut self, context: ActionContext) {
        self.disabled_contexts.insert(context);
    }

    pub fn enable_context(&mut self, context: &ActionContext) {
        self.disabled_contexts.remove(context);
    }

    #[must_use]
    pub fn is_context_enabled(&self, context: &ActionContext) -> bool {
        !self.disabled_contexts.contains(context)
    }

    #[must_use]
    pub fn bindings(&self) -> &[ActionBinding] {
        &self.bindings
    }

    pub fn binding_indices_for_input(
        &self,
        input: InputBinding,
    ) -> impl Iterator<Item = usize> + '_ {
        self.bindings_by_input
            .get(&input)
            .into_iter()
            .flat_map(|indices| indices.iter().copied())
    }

    #[must_use]
    pub fn binding_at(&self, index: usize) -> Option<&ActionBinding> {
        self.bindings.get(index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActionPhase {
    Started,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionValue {
    pub digital: bool,
}

impl ActionValue {
    #[must_use]
    pub const fn pressed() -> Self {
        Self { digital: true }
    }

    #[must_use]
    pub const fn released() -> Self {
        Self { digital: false }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionOutcome {
    pub action: ActionId,
    pub context: ActionContext,
    pub binding: InputBinding,
    pub phase: ActionPhase,
    pub value: ActionValue,
}

/// Frame-transient semantic input outcomes.
///
/// Producer: `resolve_action_outcomes` in `InputSet::ResolveActions`.
/// Consumers: gameplay command mapping and gameplay systems that need local
/// action observations before replay/server boundaries. Retention is one app
/// frame; `InputPlugin` owns cleanup in `CoreStage::Last`. Replay and
/// diagnostics should capture these outcomes before cleanup when they need
/// local physical-input provenance.
#[derive(Debug, Default, Clone, PartialEq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionOutcomes {
    outcomes: Vec<ActionOutcome>,
}

impl ActionOutcomes {
    pub fn push(&mut self, outcome: ActionOutcome) {
        self.outcomes.push(outcome);
    }

    pub fn clear(&mut self) {
        self.outcomes.clear();
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ActionOutcome] {
        &self.outcomes
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outcomes.is_empty()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.input"),
            nara_app::PluginCategory::Runtime,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if !app.world().contains_resource::<ButtonInput<KeyCode>>() {
            app.insert_resource(ButtonInput::<KeyCode>::default())?;
        }
        if !app.world().contains_resource::<ButtonInput<MouseButton>>() {
            app.insert_resource(ButtonInput::<MouseButton>::default())?;
        }
        if !app.world().contains_resource::<ActionMap>() {
            app.insert_resource(ActionMap::default())?;
        }
        if !app.world().contains_resource::<ActionOutcomes>() {
            app.insert_resource(ActionOutcomes::default())?;
        }
        if !app.world().contains_resource::<PointerState>() {
            app.insert_resource(PointerState::default())?;
        }
        app.add_systems(
            CoreStage::PreUpdate,
            resolve_action_outcomes.in_set(InputSet::ResolveActions),
        )?
        .add_systems(CoreStage::Last, clear_input_transitions)?;
        Ok(())
    }
}

fn resolve_action_outcomes(
    action_map: Res<ActionMap>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut outcomes: ResMut<ActionOutcomes>,
) {
    if !outcomes.is_empty() {
        outcomes.clear();
    }
    if !keyboard.has_transitions() && !mouse.has_transitions() {
        return;
    }

    let mut candidate_indices = BTreeSet::new();
    for key in keyboard.just_pressed_buttons() {
        candidate_indices.extend(action_map.binding_indices_for_input(InputBinding::Key(key)));
    }
    for key in keyboard.just_released_buttons() {
        candidate_indices.extend(action_map.binding_indices_for_input(InputBinding::Key(key)));
    }
    for button in mouse.just_pressed_buttons() {
        candidate_indices.extend(action_map.binding_indices_for_input(InputBinding::Mouse(button)));
    }
    for button in mouse.just_released_buttons() {
        candidate_indices.extend(action_map.binding_indices_for_input(InputBinding::Mouse(button)));
    }

    for binding_index in candidate_indices {
        let Some(binding) = action_map.binding_at(binding_index) else {
            continue;
        };
        if !action_map.is_context_enabled(&binding.context) {
            continue;
        }

        let outcome = match binding.input {
            InputBinding::Key(key) if keyboard.just_pressed(key) => Some(ActionOutcome {
                action: binding.action.clone(),
                context: binding.context.clone(),
                binding: binding.input,
                phase: ActionPhase::Started,
                value: ActionValue::pressed(),
            }),
            InputBinding::Key(key) if keyboard.just_released(key) => Some(ActionOutcome {
                action: binding.action.clone(),
                context: binding.context.clone(),
                binding: binding.input,
                phase: ActionPhase::Released,
                value: ActionValue::released(),
            }),
            InputBinding::Mouse(button) if mouse.just_pressed(button) => Some(ActionOutcome {
                action: binding.action.clone(),
                context: binding.context.clone(),
                binding: binding.input,
                phase: ActionPhase::Started,
                value: ActionValue::pressed(),
            }),
            InputBinding::Mouse(button) if mouse.just_released(button) => Some(ActionOutcome {
                action: binding.action.clone(),
                context: binding.context.clone(),
                binding: binding.input,
                phase: ActionPhase::Released,
                value: ActionValue::released(),
            }),
            _ => None,
        };

        if let Some(outcome) = outcome {
            outcomes.push(outcome);
        }
    }
}

fn clear_input_transitions(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut outcomes: ResMut<ActionOutcomes>,
) {
    if keyboard.has_transitions() {
        keyboard.clear_transitions();
    }
    if mouse.has_transitions() {
        mouse.clear_transitions();
    }
    if !outcomes.is_empty() {
        outcomes.clear();
    }
}

fn validate_identifier(id: &str) -> Result<(), ActionIdError> {
    if id.is_empty() {
        return Err(ActionIdError::Empty);
    }
    if id.chars().any(|character| character.is_control()) {
        return Err(ActionIdError::ContainsControl);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_app::CoreStage;
    use nara_ecs::{Res, Resource};
    use std::time::Duration;

    #[derive(Debug, Default, Resource)]
    struct ObservedOutcomes(Vec<ActionOutcome>);

    fn observe_outcomes(outcomes: Res<ActionOutcomes>, mut observed: ResMut<ObservedOutcomes>) {
        observed.0 = outcomes.as_slice().to_vec();
    }

    #[test]
    fn tracks_button_transitions() {
        let mut input = ButtonInput::default();

        input.press(KeyCode::Space);
        assert!(input.pressed(KeyCode::Space));
        assert!(input.just_pressed(KeyCode::Space));

        input.clear_transitions();
        assert!(!input.just_pressed(KeyCode::Space));

        input.release(KeyCode::Space);
        assert!(input.just_released(KeyCode::Space));
        assert!(!input.pressed(KeyCode::Space));
    }

    #[test]
    fn tracks_pointer_position() {
        let mut pointer = PointerState::default();

        pointer.set_position(Vec2::new(12.0, 24.0));
        assert_eq!(pointer.position(), Some(Vec2::new(12.0, 24.0)));

        pointer.clear_position();
        assert_eq!(pointer.position(), None);
    }

    #[test]
    fn key_press_binding_produces_frame_transient_action_outcome() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind_key(ActionId::new("jump").unwrap(), KeyCode::Space);
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].action.as_str(), "jump");
        assert_eq!(observed[0].binding, InputBinding::Key(KeyCode::Space));
        assert_eq!(observed[0].phase, ActionPhase::Started);
        assert!(observed[0].value.digital);
        assert!(app.world().resource::<ActionOutcomes>().is_empty());
    }

    #[test]
    fn key_release_binding_produces_release_outcome() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind_key(ActionId::new("jump").unwrap(), KeyCode::Space);
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.run_once(Duration::ZERO).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Space);

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].phase, ActionPhase::Released);
        assert!(!observed[0].value.digital);
    }

    #[test]
    fn mouse_press_binding_produces_frame_transient_action_outcome() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind_mouse(ActionId::new("select").unwrap(), MouseButton::Left);
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].action.as_str(), "select");
        assert_eq!(observed[0].binding, InputBinding::Mouse(MouseButton::Left));
        assert_eq!(observed[0].phase, ActionPhase::Started);
        assert!(observed[0].value.digital);
        assert!(app.world().resource::<ActionOutcomes>().is_empty());
    }

    #[test]
    fn mouse_release_binding_produces_release_outcome() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind_mouse(ActionId::new("select").unwrap(), MouseButton::Left);
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.run_once(Duration::ZERO).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].binding, InputBinding::Mouse(MouseButton::Left));
        assert_eq!(observed[0].phase, ActionPhase::Released);
        assert!(!observed[0].value.digital);
    }

    #[test]
    fn disabled_action_contexts_do_not_emit_outcomes() {
        let mut app = App::new();
        let menu = ActionContext::new("menu").unwrap();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        app.world_mut().unwrap().resource_mut::<ActionMap>().bind(
            ActionBinding::key(ActionId::new("confirm").unwrap(), KeyCode::Enter)
                .with_context(menu.clone()),
        );
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .disable_context(menu);
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);

        app.run_once(Duration::ZERO).unwrap();

        assert!(app.world().resource::<ObservedOutcomes>().0.is_empty());
    }

    #[test]
    fn multiple_bindings_emit_in_binding_order() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        let action = ActionId::new("move-up").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind_key(action.clone(), KeyCode::Character('w'));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind_key(action, KeyCode::ArrowUp);
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowUp);
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Character('w'));

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(
            observed
                .iter()
                .map(|outcome| outcome.binding)
                .collect::<Vec<_>>(),
            vec![
                InputBinding::Key(KeyCode::Character('w')),
                InputBinding::Key(KeyCode::ArrowUp)
            ]
        );
    }

    #[test]
    fn action_ids_reject_empty_and_control_characters() {
        assert_eq!(ActionId::new(""), Err(ActionIdError::Empty));
        assert_eq!(
            ActionId::new("bad\nid"),
            Err(ActionIdError::ContainsControl)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_invalid_action_ids_and_contexts() {
        assert!(serde_json::from_str::<ActionId>("\"\"").is_err());
        assert!(serde_json::from_str::<ActionId>("\"bad\\nid\"").is_err());
        assert!(serde_json::from_str::<ActionContext>("\"\"").is_err());
        assert!(serde_json::from_str::<ActionContext>("\"bad\\nid\"").is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rebuilds_action_map_input_index() {
        let action_map = serde_json::from_str::<ActionMap>(
            r#"{
                "bindings": [
                    {
                        "action": "confirm",
                        "input": { "Key": "Enter" },
                        "context": "gameplay"
                    }
                ],
                "disabled_contexts": []
            }"#,
        )
        .unwrap();
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(action_map)
            .unwrap()
            .insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].action.as_str(), "confirm");
    }
}
