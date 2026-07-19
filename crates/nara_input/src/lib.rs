//! Input state primitives and semantic action outcomes.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    error::Error,
    fmt::{self, Display, Formatter},
    hash::Hash,
};

use nara_app::{
    __RuntimeDriverPort, App, CoreStage, Plugin, PluginError, RuntimeDriverScope,
    RuntimeWorldAccessError,
};
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

/// Maximum retained button edges for one input domain before frame cleanup.
pub const MAX_BUTTON_TRANSITIONS_PER_FRAME: usize = 256;

/// Physical edge recorded for a retained button state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonTransitionPhase {
    Pressed,
    Released,
}

/// One ordered physical button edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonTransition<T> {
    sequence: u64,
    button: T,
    phase: ButtonTransitionPhase,
}

impl<T: Copy> ButtonTransition<T> {
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn button(self) -> T {
        self.button
    }

    #[must_use]
    pub const fn phase(self) -> ButtonTransitionPhase {
        self.phase
    }
}

/// Rejection from bounded button-state mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonInputError {
    TransitionLimitExceeded { limit: usize },
    TransitionSequenceExhausted,
}

impl Display for ButtonInputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TransitionLimitExceeded { limit } => {
                write!(formatter, "button transition limit {limit} was exceeded")
            }
            Self::TransitionSequenceExhausted => {
                formatter.write_str("button transition sequence was exhausted")
            }
        }
    }
}

impl Error for ButtonInputError {}

/// Retained button state plus its bounded, ordered edges since frame cleanup.
#[derive(Debug, Clone, Resource)]
pub struct ButtonInput<T> {
    pressed: HashSet<T>,
    transitions: Vec<ButtonTransition<T>>,
    transition_sequence: u64,
}

impl<T> Default for ButtonInput<T> {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            transitions: Vec::new(),
            transition_sequence: 0,
        }
    }
}

impl<T> ButtonInput<T>
where
    T: Copy + Eq + Hash + Ord,
{
    pub fn press(&mut self, button: T) -> Result<bool, ButtonInputError> {
        if self.pressed.contains(&button) {
            return Ok(false);
        }
        self.push_transition(button, ButtonTransitionPhase::Pressed)?;
        self.pressed.insert(button);
        Ok(true)
    }

    pub fn release(&mut self, button: T) -> Result<bool, ButtonInputError> {
        if !self.pressed.contains(&button) {
            return Ok(false);
        }
        self.push_transition(button, ButtonTransitionPhase::Released)?;
        self.pressed.remove(&button);
        Ok(true)
    }

    pub fn release_all(&mut self) -> Result<Vec<T>, ButtonInputError> {
        let mut released = self.pressed.iter().copied().collect::<Vec<_>>();
        released.sort();
        self.preflight_transition_count(released.len())?;
        self.pressed.clear();
        for button in released.iter().copied() {
            self.push_transition_preflighted(button, ButtonTransitionPhase::Released);
        }
        Ok(released)
    }

    #[must_use]
    pub fn pressed(&self, button: T) -> bool {
        self.pressed.contains(&button)
    }

    #[must_use]
    pub fn just_pressed(&self, button: T) -> bool {
        self.transitions.iter().any(|transition| {
            transition.button == button && transition.phase == ButtonTransitionPhase::Pressed
        })
    }

    #[must_use]
    pub fn just_released(&self, button: T) -> bool {
        self.transitions.iter().any(|transition| {
            transition.button == button && transition.phase == ButtonTransitionPhase::Released
        })
    }

    pub fn clear_transitions(&mut self) {
        self.transitions.clear();
    }

    #[must_use]
    pub fn has_transitions(&self) -> bool {
        !self.transitions.is_empty()
    }

    pub fn just_pressed_buttons(&self) -> impl Iterator<Item = T> + '_ {
        self.transitions.iter().filter_map(|transition| {
            (transition.phase == ButtonTransitionPhase::Pressed).then_some(transition.button)
        })
    }

    pub fn just_released_buttons(&self) -> impl Iterator<Item = T> + '_ {
        self.transitions.iter().filter_map(|transition| {
            (transition.phase == ButtonTransitionPhase::Released).then_some(transition.button)
        })
    }

    pub fn pressed_buttons(&self) -> impl Iterator<Item = T> + '_ {
        self.pressed.iter().copied()
    }

    #[must_use]
    pub fn transitions(&self) -> &[ButtonTransition<T>] {
        &self.transitions
    }

    fn push_transition(
        &mut self,
        button: T,
        phase: ButtonTransitionPhase,
    ) -> Result<(), ButtonInputError> {
        self.preflight_transition_count(1)?;
        self.push_transition_preflighted(button, phase);
        Ok(())
    }

    fn preflight_transition_count(&self, count: usize) -> Result<(), ButtonInputError> {
        if self.transitions.len().saturating_add(count) > MAX_BUTTON_TRANSITIONS_PER_FRAME {
            return Err(ButtonInputError::TransitionLimitExceeded {
                limit: MAX_BUTTON_TRANSITIONS_PER_FRAME,
            });
        }
        let count =
            u64::try_from(count).map_err(|_| ButtonInputError::TransitionSequenceExhausted)?;
        self.transition_sequence
            .checked_add(count)
            .ok_or(ButtonInputError::TransitionSequenceExhausted)?;
        Ok(())
    }

    fn push_transition_preflighted(&mut self, button: T, phase: ButtonTransitionPhase) {
        self.transition_sequence = self
            .transition_sequence
            .checked_add(1)
            .expect("button transition sequence was preflighted");
        self.transitions.push(ButtonTransition {
            sequence: self.transition_sequence,
            button,
            phase,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonDriverInput<T> {
    Press(T),
    Release(T),
    ReleaseAll,
}

impl<T> __RuntimeDriverPort for ButtonInput<T>
where
    T: Copy + Eq + Hash + Ord + Send + Sync + 'static,
{
    type Input = ButtonDriverInput<T>;
    type Output = Result<Vec<T>, ButtonInputError>;

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        match input {
            ButtonDriverInput::Press(button) => {
                self.press(button)?;
                Ok(Vec::new())
            }
            ButtonDriverInput::Release(button) => {
                self.release(button)?;
                Ok(Vec::new())
            }
            ButtonDriverInput::ReleaseAll => self.release_all(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonInputDriverError {
    WorldAccess(RuntimeWorldAccessError),
    Input(ButtonInputError),
}

impl Display for ButtonInputDriverError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorldAccess(error) => Display::fmt(error, formatter),
            Self::Input(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ButtonInputDriverError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorldAccess(error) => Some(error),
            Self::Input(error) => Some(error),
        }
    }
}

impl From<RuntimeWorldAccessError> for ButtonInputDriverError {
    fn from(error: RuntimeWorldAccessError) -> Self {
        Self::WorldAccess(error)
    }
}

impl From<ButtonInputError> for ButtonInputDriverError {
    fn from(error: ButtonInputError) -> Self {
        Self::Input(error)
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerDriverInput {
    Moved(Vec2),
    Left,
}

impl __RuntimeDriverPort for PointerState {
    type Input = PointerDriverInput;
    type Output = ();

    fn apply_driver_input(&mut self, input: Self::Input) {
        match input {
            PointerDriverInput::Moved(position) => self.set_position(position),
            PointerDriverInput::Left => self.clear_position(),
        }
    }
}

pub fn apply_keyboard_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: ButtonDriverInput<KeyCode>,
) -> Result<Vec<KeyCode>, ButtonInputDriverError> {
    scope
        .__apply_port::<ButtonInput<KeyCode>>(input)
        .map_err(ButtonInputDriverError::WorldAccess)?
        .map_err(ButtonInputDriverError::Input)
}

pub fn apply_mouse_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: ButtonDriverInput<MouseButton>,
) -> Result<Vec<MouseButton>, ButtonInputDriverError> {
    scope
        .__apply_port::<ButtonInput<MouseButton>>(input)
        .map_err(ButtonInputDriverError::WorldAccess)?
        .map_err(ButtonInputDriverError::Input)
}

pub fn apply_pointer_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: PointerDriverInput,
) -> Result<(), RuntimeWorldAccessError> {
    scope.__apply_port::<PointerState>(input)
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

pub const INPUT_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.input");
pub const INPUT_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(INPUT_PLUGIN_ID, nara_app::PluginCategory::Input);

impl Plugin for InputPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &INPUT_PLUGIN_DECLARATION
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

    // Keyboard and mouse retain independent physical timelines. Resolve each timeline in event
    // order, with deterministic keyboard-before-mouse precedence between the two domains.
    resolve_button_transitions(
        &action_map,
        keyboard.transitions(),
        InputBinding::Key,
        &mut outcomes,
    );
    resolve_button_transitions(
        &action_map,
        mouse.transitions(),
        InputBinding::Mouse,
        &mut outcomes,
    );
}

fn resolve_button_transitions<T: Copy>(
    action_map: &ActionMap,
    transitions: &[ButtonTransition<T>],
    input_binding: impl Fn(T) -> InputBinding,
    outcomes: &mut ActionOutcomes,
) {
    for transition in transitions {
        let input = input_binding(transition.button());
        let (phase, value) = match transition.phase() {
            ButtonTransitionPhase::Pressed => (ActionPhase::Started, ActionValue::pressed()),
            ButtonTransitionPhase::Released => (ActionPhase::Released, ActionValue::released()),
        };
        for binding_index in action_map.binding_indices_for_input(input) {
            let Some(binding) = action_map.binding_at(binding_index) else {
                continue;
            };
            if action_map.is_context_enabled(&binding.context) {
                outcomes.push(ActionOutcome {
                    action: binding.action.clone(),
                    context: binding.context.clone(),
                    binding: input,
                    phase,
                    value,
                });
            }
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

        input.press(KeyCode::Space).unwrap();
        assert!(input.pressed(KeyCode::Space));
        assert!(input.just_pressed(KeyCode::Space));

        input.clear_transitions();
        assert!(!input.just_pressed(KeyCode::Space));

        input.release(KeyCode::Space).unwrap();
        assert!(input.just_released(KeyCode::Space));
        assert!(!input.pressed(KeyCode::Space));
    }

    #[test]
    fn release_all_emits_release_edges_and_clears_retained_buttons() {
        let mut input = ButtonInput::default();
        input.press(KeyCode::Space).unwrap();
        input.press(KeyCode::Enter).unwrap();
        input.clear_transitions();

        let released = input.release_all().unwrap();

        assert_eq!(released, [KeyCode::Space, KeyCode::Enter]);
        assert!(!input.pressed(KeyCode::Space));
        assert!(!input.pressed(KeyCode::Enter));
        assert!(input.just_released(KeyCode::Space));
        assert!(input.just_released(KeyCode::Enter));
    }

    #[test]
    fn same_frame_press_and_release_preserve_both_ordered_edges() {
        let mut input = ButtonInput::default();

        input.press(KeyCode::Space).unwrap();
        input.release(KeyCode::Space).unwrap();

        assert!(!input.pressed(KeyCode::Space));
        assert!(input.just_pressed(KeyCode::Space));
        assert!(input.just_released(KeyCode::Space));
        assert_eq!(
            input
                .transitions()
                .iter()
                .copied()
                .map(|transition| (transition.sequence(), transition.phase()))
                .collect::<Vec<_>>(),
            vec![
                (1, ButtonTransitionPhase::Pressed),
                (2, ButtonTransitionPhase::Released),
            ]
        );
    }

    #[test]
    fn transition_limit_rejects_the_first_extra_edge_atomically() {
        let mut input = ButtonInput::default();
        for _ in 0..(MAX_BUTTON_TRANSITIONS_PER_FRAME / 2) {
            input.press(KeyCode::Space).unwrap();
            input.release(KeyCode::Space).unwrap();
        }
        let before = input.transitions().to_vec();

        assert_eq!(
            input.press(KeyCode::Enter),
            Err(ButtonInputError::TransitionLimitExceeded {
                limit: MAX_BUTTON_TRANSITIONS_PER_FRAME,
            })
        );
        assert!(!input.pressed(KeyCode::Enter));
        assert_eq!(input.transitions(), before);
    }

    #[test]
    fn release_all_is_atomic_when_the_remaining_transition_budget_is_too_small() {
        let mut input = ButtonInput::default();
        input.press(KeyCode::Space).unwrap();
        input.press(KeyCode::Enter).unwrap();
        for _ in 0..126 {
            input.press(KeyCode::ArrowUp).unwrap();
            input.release(KeyCode::ArrowUp).unwrap();
        }
        input.press(KeyCode::ArrowLeft).unwrap();
        assert_eq!(
            input.transitions().len(),
            MAX_BUTTON_TRANSITIONS_PER_FRAME - 1
        );
        let before = input.transitions().to_vec();

        assert_eq!(
            input.release_all(),
            Err(ButtonInputError::TransitionLimitExceeded {
                limit: MAX_BUTTON_TRANSITIONS_PER_FRAME,
            })
        );
        assert!(input.pressed(KeyCode::Space));
        assert!(input.pressed(KeyCode::Enter));
        assert!(input.pressed(KeyCode::ArrowLeft));
        assert_eq!(input.transitions(), before);
    }

    #[test]
    fn transition_sequence_exhaustion_rejects_state_change_atomically() {
        let mut input = ButtonInput {
            transition_sequence: u64::MAX,
            ..ButtonInput::default()
        };

        assert_eq!(
            input.press(KeyCode::Space),
            Err(ButtonInputError::TransitionSequenceExhausted)
        );
        assert!(!input.pressed(KeyCode::Space));
        assert!(input.transitions().is_empty());
    }

    #[test]
    fn release_all_is_atomic_when_transition_sequence_is_exhausted() {
        let mut input = ButtonInput::default();
        input.press(KeyCode::Space).unwrap();
        input.press(KeyCode::Enter).unwrap();
        input.clear_transitions();
        input.transition_sequence = u64::MAX - 1;
        let before_pressed = input.pressed.clone();
        let before_transitions = input.transitions.clone();
        let before_sequence = input.transition_sequence;

        assert_eq!(
            input.release_all(),
            Err(ButtonInputError::TransitionSequenceExhausted)
        );
        assert_eq!(input.pressed, before_pressed);
        assert_eq!(input.transitions, before_transitions);
        assert_eq!(input.transition_sequence, before_sequence);
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
            .press(KeyCode::Space)
            .unwrap();

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
            .press(KeyCode::Space)
            .unwrap();
        app.run_once(Duration::ZERO).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Space)
            .unwrap();

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
            .press(MouseButton::Left)
            .unwrap();

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
            .press(MouseButton::Left)
            .unwrap();
        app.run_once(Duration::ZERO).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left)
            .unwrap();

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
            .press(KeyCode::Enter)
            .unwrap();

        app.run_once(Duration::ZERO).unwrap();

        assert!(app.world().resource::<ObservedOutcomes>().0.is_empty());
    }

    #[test]
    fn multiple_input_edges_emit_in_transition_order() {
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
            .press(KeyCode::ArrowUp)
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Character('w'))
            .unwrap();

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(
            observed
                .iter()
                .map(|outcome| outcome.binding)
                .collect::<Vec<_>>(),
            vec![
                InputBinding::Key(KeyCode::ArrowUp),
                InputBinding::Key(KeyCode::Character('w'))
            ]
        );
    }

    #[test]
    fn bindings_for_one_input_edge_emit_in_registration_order() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        {
            let mut action_map = app.world_mut().unwrap().resource_mut::<ActionMap>();
            action_map.bind_key(ActionId::new("first").unwrap(), KeyCode::Space);
            action_map.bind_key(ActionId::new("second").unwrap(), KeyCode::Space);
        }
        app.world_mut()
            .unwrap()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space)
            .unwrap();

        app.run_once(Duration::ZERO).unwrap();

        assert_eq!(
            app.world()
                .resource::<ObservedOutcomes>()
                .0
                .iter()
                .map(|outcome| outcome.action.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
    }

    #[test]
    fn same_frame_quick_click_emits_started_then_released() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind_key(ActionId::new("confirm").unwrap(), KeyCode::Enter);
        {
            let mut keyboard = app
                .world_mut()
                .unwrap()
                .resource_mut::<ButtonInput<KeyCode>>();
            keyboard.press(KeyCode::Enter).unwrap();
            keyboard.release(KeyCode::Enter).unwrap();
        }

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(
            observed
                .iter()
                .map(|outcome| outcome.phase)
                .collect::<Vec<_>>(),
            [ActionPhase::Started, ActionPhase::Released]
        );
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::Enter)
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
            .press(KeyCode::Enter)
            .unwrap();

        app.run_once(Duration::ZERO).unwrap();

        let observed = &app.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].action.as_str(), "confirm");
    }
}
