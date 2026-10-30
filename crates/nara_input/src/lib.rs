//! Input state primitives and semantic action outcomes.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_app::{
    App, CoreStage, FixedUpdateSet, Plugin, PluginError, RuntimeDriverScope,
    RuntimeWorldAccessError,
};
use nara_core::Vec2;
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};

mod definition;
#[cfg(feature = "serde")]
mod format;
#[cfg(feature = "serde")]
mod runtime;
mod schedule;
mod semantic;
mod state;
mod timeline;

pub use definition::{
    ActionContextDefinition, ActionMapBindings, ActionMapBuilder, ActionMapCompileCost,
    ActionMapDefinition, ActionMapError, ActionMapLimitKind, ActionMapLimits, MAX_ACTION_BINDINGS,
    MAX_ACTION_CONTEXTS, MAX_ACTION_IDENTIFIER_BYTES, MAX_BINDINGS_PER_INPUT,
};
#[cfg(feature = "serde")]
pub use format::{ActionMapFileCandidate, ActionMapFileError, ActionMapFileLimits};
#[cfg(feature = "serde")]
pub use runtime::{ActionMapAsset, ActionMapDomain, ActionMapRuntimeCodec};
use schedule::InputSet;
use semantic::{ActionStaging, StagedActionRevision};
pub use semantic::{SemanticActionCause, SemanticActionOutcome, SemanticActions};
pub use state::{ActionMap, InputSelectionError, finalize_input_selection};

#[doc(hidden)]
pub use timeline::{__InputDriverEvent, __InputDriverOutput};
pub use timeline::{
    ActionInputProvenance, InputAdmission, InputConfigurationError, InputConsumerCursors,
    InputEndpointRetirement, InputEventKind, InputEventOrigin, InputFrameObservations,
    InputGeometryGeneration, InputIngressError, InputInvalidationReason, InputLimits,
    InputPointerId, InputPresentationEpoch, InputPresentedRoute, InputRouteGeneration,
    InputRouteStamp, InputSeatId, InputSlotGeneration, InputSurfaceId, InputTimeline,
    InputTimelineEntry, InputUiCursorError, InputUiDisposition, InputWindowEvent, InputWindowId,
    MAX_INPUT_ROUTES, MAX_INPUT_TIMELINE_ENTRIES,
};

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

#[doc(hidden)]
pub mod __private {
    pub use crate::schedule::InputSet;
    pub use crate::timeline::{
        InputEndpointClaim, InputRouteCommit, PreparedInputRoutePublication, abort_presented_route,
        claim_input_endpoint, commit_presented_route, prepare_claimed_route,
        prepare_presented_route, prepared_route_invalidation, retire_claimed_endpoint,
        supersede_presented_route, unbind_claimed_endpoint,
    };
}

/// Physical edge recorded for a retained button state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonTransitionPhase {
    Pressed,
    Released,
}

/// One ordered physical button edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonTransition<T> {
    provenance: ActionInputProvenance,
    press_sequence: u64,
    button: T,
    phase: ButtonTransitionPhase,
}

impl<T: Copy> ButtonTransition<T> {
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.provenance.sequence()
    }

    /// Identity of the admitted member shared by its press and release.
    #[must_use]
    pub const fn press_sequence(self) -> u64 {
        self.press_sequence
    }

    #[must_use]
    pub const fn provenance(self) -> ActionInputProvenance {
        self.provenance
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

/// Read-only button observations projected from accepted [`InputTimeline`] members.
///
/// Held membership uses the original press sequence, so a release removes exactly its admitted
/// member. Frame edges are copies of timeline entries; deferred action ownership stays in the
/// timeline. Both projection buffers are allocated from its admitted limits at plugin build.
#[derive(Debug, Resource)]
pub struct ButtonInput<T> {
    // Sorted by button, then member identity. No historical empty endpoint rows are retained.
    pressed: Vec<(T, u64)>,
    transitions: Vec<ButtonTransition<T>>,
}

impl<T> Default for ButtonInput<T> {
    fn default() -> Self {
        Self::with_limits(InputLimits::default()).expect("default button projection storage fits")
    }
}

impl<T> ButtonInput<T> {
    fn with_limits(limits: InputLimits) -> Result<Self, InputConfigurationError> {
        let mut pressed = Vec::new();
        pressed
            .try_reserve_exact(limits.members())
            .map_err(|_| InputConfigurationError::AllocationRefused)?;
        let mut transitions = Vec::new();
        transitions
            .try_reserve_exact(limits.events())
            .map_err(|_| InputConfigurationError::AllocationRefused)?;
        Ok(Self {
            pressed,
            transitions,
        })
    }
}

impl<T: Copy + Ord> ButtonInput<T> {
    #[must_use]
    pub fn pressed(&self, button: T) -> bool {
        self.pressed
            .binary_search_by_key(&button, |(held, _)| *held)
            .is_ok()
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
        self.pressed
            .iter()
            .enumerate()
            .filter_map(|(index, (button, _))| {
                (index == 0 || self.pressed[index - 1].0 != *button).then_some(*button)
            })
    }

    /// Ordered physical edges visible during the current app frame.
    ///
    /// Paused action retention belongs to the timeline and does not extend these frame copies.
    #[must_use]
    pub fn transitions(&self) -> &[ButtonTransition<T>] {
        &self.transitions
    }

    fn finish_frame_transitions(&mut self) {
        self.transitions.clear();
    }

    fn apply_timeline_transition(
        &mut self,
        entry: InputTimelineEntry,
        button: T,
        phase: ButtonTransitionPhase,
    ) {
        let press_sequence = entry
            .press_sequence()
            .expect("an admitted button edge identifies its member");
        let member = (button, press_sequence);
        let position = self.pressed.binary_search(&member);
        match phase {
            ButtonTransitionPhase::Pressed => {
                let index = position.expect_err("an admitted press begins a fresh member");
                assert!(self.pressed.len() < self.pressed.capacity());
                self.pressed.insert(index, member);
            }
            ButtonTransitionPhase::Released => {
                let index = position.expect("an admitted release ends its projected member");
                self.pressed.remove(index);
            }
        }
        assert!(self.transitions.len() < self.transitions.capacity());
        self.transitions.push(ButtonTransition {
            provenance: entry.action_provenance(),
            press_sequence,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonInputDriverError {
    WorldAccess(RuntimeWorldAccessError),
    Timeline(InputIngressError),
}

impl Display for ButtonInputDriverError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorldAccess(error) => Display::fmt(error, formatter),
            Self::Timeline(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ButtonInputDriverError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorldAccess(error) => Some(error),
            Self::Timeline(error) => Some(error),
        }
    }
}

impl From<RuntimeWorldAccessError> for ButtonInputDriverError {
    fn from(error: RuntimeWorldAccessError) -> Self {
        Self::WorldAccess(error)
    }
}

impl From<InputIngressError> for ButtonInputDriverError {
    fn from(error: InputIngressError) -> Self {
        Self::Timeline(error)
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

    fn set_position(&mut self, position: Vec2) {
        self.position = Some(position);
    }

    fn clear_position(&mut self) {
        self.position = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerDriverInput {
    Moved(Vec2),
    Left,
}

/// Connects one explicitly named platform endpoint before its first ingress operation.
pub fn connect_input_endpoint(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
) -> Result<(), ButtonInputDriverError> {
    scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::ConnectEndpoint { window, seat })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok(())
}

/// Connects a pointer child of an explicitly connected endpoint.
pub fn connect_input_pointer(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
) -> Result<(), ButtonInputDriverError> {
    scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::ConnectPointer {
            window,
            seat,
            pointer,
        })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok(())
}

/// Retires one connected pointer without retiring its keyboard/window endpoint.
pub fn retire_input_pointer(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
) -> Result<(InputAdmission, Vec<MouseButton>), ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::RetirePointer {
            window,
            seat,
            pointer,
        })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok((output.admission, output.released_mouse_buttons))
}

/// Sends synthetic input through the owner's once-admitted default endpoint.
pub fn apply_keyboard_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: ButtonDriverInput<KeyCode>,
) -> Result<Vec<KeyCode>, ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::SyntheticKeyboard { input })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok(output.released_keys)
}

/// Sends platform input from an explicitly connected endpoint.
pub fn apply_keyboard_driver_input_at(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    input: ButtonDriverInput<KeyCode>,
) -> Result<Vec<KeyCode>, ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::Keyboard {
            window,
            seat,
            input,
        })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok(output.released_keys)
}

pub fn apply_mouse_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: ButtonDriverInput<MouseButton>,
) -> Result<Vec<MouseButton>, ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::SyntheticMouse { input })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok(output.released_mouse_buttons)
}

pub fn apply_mouse_driver_input_at(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
    input: ButtonDriverInput<MouseButton>,
) -> Result<Vec<MouseButton>, ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::Mouse {
            window,
            seat,
            pointer,
            input,
        })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok(output.released_mouse_buttons)
}

pub fn apply_pointer_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: PointerDriverInput,
) -> Result<(InputAdmission, Vec<MouseButton>), ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::SyntheticPointer { input })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok((output.admission, output.released_mouse_buttons))
}

pub fn apply_pointer_driver_input_at(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
    input: PointerDriverInput,
) -> Result<(InputAdmission, Vec<MouseButton>), ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::Pointer {
            window,
            seat,
            pointer,
            input,
        })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok((output.admission, output.released_mouse_buttons))
}

pub fn apply_gamepad_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    gamepad: u16,
    button: u16,
    phase: ButtonTransitionPhase,
) -> Result<InputAdmission, ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::SyntheticGamepad {
            gamepad,
            button,
            phase,
        })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok(output.admission)
}

pub fn apply_gamepad_driver_input_at(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    gamepad: u16,
    button: u16,
    phase: ButtonTransitionPhase,
) -> Result<InputAdmission, ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::Gamepad {
            window,
            seat,
            gamepad,
            button,
            phase,
        })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok(output.admission)
}

pub fn apply_window_input_driver_event(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    event: InputWindowEvent,
) -> Result<(InputAdmission, Vec<KeyCode>, Vec<MouseButton>), ButtonInputDriverError> {
    let output = scope
        .__apply_port::<InputTimeline>(__InputDriverEvent::Window {
            window,
            seat,
            event,
        })
        .map_err(ButtonInputDriverError::WorldAccess)??;
    Ok((
        output.admission,
        output.released_keys,
        output.released_mouse_buttons,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionId(Box<str>);

impl ActionId {
    pub fn new(id: impl Into<String>) -> Result<Self, ActionIdError> {
        let id = id.into();
        validate_identifier(&id)?;
        Ok(Self(id.into_boxed_str()))
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
    TooLong { bytes: usize, limit: usize },
}

impl Display for ActionIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("action id cannot be empty"),
            Self::ContainsControl => {
                formatter.write_str("action id cannot contain control characters")
            }
            Self::TooLong { bytes, limit } => write!(
                formatter,
                "action identifier has {bytes} bytes, limit {limit}"
            ),
        }
    }
}

impl Error for ActionIdError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionContext(Box<str>);

impl Default for ActionContext {
    fn default() -> Self {
        Self::gameplay()
    }
}

impl ActionContext {
    #[must_use]
    pub fn gameplay() -> Self {
        Self("gameplay".into())
    }

    pub fn new(id: impl Into<String>) -> Result<Self, ActionIdError> {
        let id = id.into();
        validate_identifier(&id)?;
        Ok(Self(id.into_boxed_str()))
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
    pub provenance: ActionInputProvenance,
}

/// Frame-transient low-level outcomes for each enabled physical binding.
///
/// Producer: `resolve_action_outcomes` in `InputSet::ResolveActions`.
/// Multiple bindings can emit multiple Started/Released edges for the same action. Context changes
/// do not synthesize physical releases. Gameplay held-state and command consumers should use
/// [`SemanticActions`] instead. Retention is one app
/// frame; `InputPlugin` owns cleanup after all ordinary or exact-step frame observers finish.
/// Replay and diagnostics should capture these outcomes before cleanup when they need local
/// physical-input provenance.
#[derive(Debug, Resource)]
pub struct ActionOutcomes {
    outcomes: Vec<ActionOutcome>,
}

impl ActionOutcomes {
    fn with_limits(limits: InputLimits) -> Result<Self, InputConfigurationError> {
        let capacity = limits
            .events()
            .checked_mul(MAX_BINDINGS_PER_INPUT)
            .ok_or(InputConfigurationError::CapacityOverflow)?;
        let mut outcomes = Vec::new();
        outcomes
            .try_reserve_exact(capacity)
            .map_err(|_| InputConfigurationError::AllocationRefused)?;
        Ok(Self { outcomes })
    }

    fn push_projected(&mut self, outcome: ActionOutcome) {
        assert!(self.outcomes.len() < self.outcomes.capacity());
        self.outcomes.push(outcome);
    }

    fn clear(&mut self) {
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
        if !app.world().contains_resource::<InputTimeline>() {
            app.insert_resource(InputTimeline::default())?;
        }
        let limits = app.world().resource::<InputTimeline>().limits();
        let projection_error = |error: InputConfigurationError| PluginError::SetupFailed {
            plugin: INPUT_PLUGIN_ID,
            message: error.to_string(),
        };
        let outcomes = ActionOutcomes::with_limits(limits).map_err(projection_error)?;
        let semantic = SemanticActions::with_limits(limits).map_err(projection_error)?;
        app.insert_resource(
            InputFrameObservations::with_capacity(limits.events()).map_err(projection_error)?,
        )?;
        app.insert_resource(
            ButtonInput::<KeyCode>::with_limits(limits).map_err(projection_error)?,
        )?;
        app.insert_resource(
            ButtonInput::<MouseButton>::with_limits(limits).map_err(projection_error)?,
        )?;
        if !app.world().contains_resource::<ActionMap>() {
            app.insert_resource(ActionMap::pristine())?;
        }
        app.insert_resource(state::InputInstallation::new(
            app.world().resource::<ActionMap>(),
        ))?;
        app.insert_resource(outcomes)?;
        app.insert_resource(semantic)?;
        if !app.world().contains_resource::<ActionStaging>() {
            app.insert_resource(ActionStaging::default())?;
        }
        if !app.world().contains_resource::<PointerState>() {
            app.insert_resource(PointerState::default())?;
        }
        app.configure_sets(
            CoreStage::First,
            (
                InputSet::ProjectTimeline,
                InputSet::PublishObservations,
                InputSet::RouteUi,
                InputSet::StageActions,
            )
                .chain(),
        )?
        .add_systems(
            CoreStage::First,
            project_timeline_input.in_set(InputSet::ProjectTimeline),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            // Exact stepping can admit input without an intervening ordinary First stage.
            // The timeline's projection cursor makes this a no-op for already projected edges.
            project_timeline_input.in_set(FixedUpdateSet::Prepare),
        )?
        .add_systems(
            CoreStage::First,
            publish_timeline_observations.in_set(InputSet::PublishObservations),
        )?
        .add_systems(
            CoreStage::First,
            finalize_timeline_routing.in_set(InputSet::StageActions),
        )?
        .add_systems(
            CoreStage::PreUpdate,
            resolve_action_outcomes.in_set(InputSet::ResolveActions),
        )?
        .add_systems(nara_app::__private::FrameCleanup, clear_input_transitions)?;
        Ok(())
    }
}

fn project_timeline_input(
    mut timeline: ResMut<InputTimeline>,
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut pointer: ResMut<PointerState>,
) {
    for entry in timeline.projection_entries() {
        match entry.kind() {
            InputEventKind::Key { key, phase } => {
                keyboard.apply_timeline_transition(entry, key, phase);
            }
            InputEventKind::PointerButton { button, phase } => {
                mouse.apply_timeline_transition(entry, button, phase);
            }
            InputEventKind::PointerMoved => {
                if let Some(position) = entry.logical_position() {
                    pointer.set_position(position);
                }
            }
            InputEventKind::PointerLeft => pointer.clear_position(),
            InputEventKind::Wheel { .. }
            | InputEventKind::GamepadButton { .. }
            | InputEventKind::FocusChanged { .. }
            | InputEventKind::Resized { .. }
            | InputEventKind::ScaleFactorChanged { .. }
            | InputEventKind::CloseRequested
            | InputEventKind::SurfaceRetired => {}
        }
    }
    timeline.finish_projection();
}

fn publish_timeline_observations(
    mut timeline: ResMut<InputTimeline>,
    mut observations: ResMut<InputFrameObservations>,
) {
    timeline.publish_observations(&mut observations);
}

fn finalize_timeline_routing(
    action_map: Res<ActionMap>,
    mut staging: ResMut<ActionStaging>,
    mut timeline: ResMut<InputTimeline>,
) {
    let revision = staging.observe(&action_map);
    timeline.finalize_ui_routing();
    timeline.stage_actions(&revision);
    timeline.reclaim_completed();
}

fn resolve_action_outcomes(
    action_map: Res<ActionMap>,
    mut staging: ResMut<ActionStaging>,
    mut timeline: ResMut<InputTimeline>,
    mut outcomes: ResMut<ActionOutcomes>,
    mut semantic: ResMut<SemanticActions>,
) {
    if !outcomes.is_empty() {
        outcomes.clear();
    }
    semantic.clear_outcomes();
    let revision = staging.observe(&action_map);
    semantic.reconcile(&action_map, &revision);
    for (entry, staged_revision) in timeline.take_deferred_action_inputs() {
        let Some((input, transition_phase)) = entry.action_transition() else {
            continue;
        };
        resolve_input_binding(
            &action_map,
            input,
            transition_phase,
            entry.action_provenance(),
            &mut outcomes,
        );
        semantic.apply(&action_map, &revision, &staged_revision, entry);
    }
}

fn resolve_input_binding(
    action_map: &ActionMap,
    input: InputBinding,
    transition_phase: ButtonTransitionPhase,
    provenance: ActionInputProvenance,
    outcomes: &mut ActionOutcomes,
) {
    let (phase, value) = match transition_phase {
        ButtonTransitionPhase::Pressed => (ActionPhase::Started, ActionValue::pressed()),
        ButtonTransitionPhase::Released => (ActionPhase::Released, ActionValue::released()),
    };
    for binding_index in action_map.binding_indices_for_input(input) {
        let Some(binding) = action_map.binding_at(binding_index) else {
            continue;
        };
        if action_map.is_context_enabled(&binding.context) {
            outcomes.push_projected(ActionOutcome {
                action: binding.action.clone(),
                context: binding.context.clone(),
                binding: input,
                phase,
                value,
                provenance,
            });
        }
    }
}

fn clear_input_transitions(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut outcomes: ResMut<ActionOutcomes>,
    mut semantic: ResMut<SemanticActions>,
    mut observations: ResMut<InputFrameObservations>,
    mut timeline: ResMut<InputTimeline>,
) {
    if keyboard.has_transitions() {
        keyboard.finish_frame_transitions();
    }
    if mouse.has_transitions() {
        mouse.finish_frame_transitions();
    }
    if !outcomes.is_empty() {
        outcomes.clear();
    }
    if !semantic.as_slice().is_empty() {
        semantic.clear_outcomes();
    }
    observations.clear();
    timeline.finish_frame_observations();
    timeline.reclaim_completed();
}

fn validate_identifier(id: &str) -> Result<(), ActionIdError> {
    if id.is_empty() {
        return Err(ActionIdError::Empty);
    }
    if id.chars().any(|character| character.is_control()) {
        return Err(ActionIdError::ContainsControl);
    }
    if id.len() > MAX_ACTION_IDENTIFIER_BYTES {
        return Err(ActionIdError::TooLong {
            bytes: id.len(),
            limit: MAX_ACTION_IDENTIFIER_BYTES,
        });
    }
    Ok(())
}

#[cfg(test)]
mod projection_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use nara_app::{__RuntimeDriverPort, CoreStage};
    use nara_core::ByteLimit;
    use nara_ecs::{DetectChanges, Res, Resource};
    use std::{sync::Arc, time::Duration};

    #[derive(Debug, Default, Resource)]
    struct ObservedOutcomes(Vec<ActionOutcome>);

    #[derive(Debug, Resource)]
    struct LateReleasePending(bool);

    #[derive(Debug, Default, Resource)]
    struct InputChangeObservations(Vec<(bool, bool)>);

    fn observe_outcomes(outcomes: Res<ActionOutcomes>, mut observed: ResMut<ObservedOutcomes>) {
        observed.0 = outcomes.as_slice().to_vec();
    }

    fn admit_key(app: &mut App, input: ButtonDriverInput<KeyCode>) {
        app.world_mut()
            .unwrap()
            .resource_mut::<InputTimeline>()
            .apply_driver_input(__InputDriverEvent::SyntheticKeyboard { input })
            .unwrap();
    }

    fn admit_mouse(app: &mut App, input: ButtonDriverInput<MouseButton>) {
        app.world_mut()
            .unwrap()
            .resource_mut::<InputTimeline>()
            .apply_driver_input(__InputDriverEvent::SyntheticMouse { input })
            .unwrap();
    }

    fn inject_late_key_release(
        mut timeline: ResMut<InputTimeline>,
        mut pending: ResMut<LateReleasePending>,
    ) {
        if pending.0 {
            timeline
                .apply_driver_input(__InputDriverEvent::SyntheticKeyboard {
                    input: ButtonDriverInput::Release(KeyCode::Space),
                })
                .unwrap();
            pending.0 = false;
        }
    }

    fn observe_input_changes(
        keyboard: Res<ButtonInput<KeyCode>>,
        mouse: Res<ButtonInput<MouseButton>>,
        mut observed: ResMut<InputChangeObservations>,
    ) {
        observed.0.push((keyboard.is_changed(), mouse.is_changed()));
    }

    fn observed_changes_after(binding: InputBinding) -> (bool, bool) {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(InputChangeObservations::default())
            .unwrap()
            .add_systems(
                CoreStage::PreUpdate,
                observe_input_changes.after(InputSet::ResolveActions),
            )
            .unwrap();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        runtime
            .with_driver_scope(|scope| match binding {
                InputBinding::Key(key) => {
                    apply_keyboard_driver_input(scope, ButtonDriverInput::Press(key)).unwrap();
                }
                InputBinding::Mouse(button) => {
                    apply_mouse_driver_input(scope, ButtonDriverInput::Press(button)).unwrap();
                }
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let observed = *runtime
            .world()
            .resource::<InputChangeObservations>()
            .0
            .last()
            .unwrap();
        runtime.finish().unwrap();
        observed
    }

    #[test]
    fn frame_cleanup_retains_edges_added_after_action_resolution() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .insert_resource(LateReleasePending(true))
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap()
            .add_systems(CoreStage::Update, inject_late_key_release)
            .unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key(
                ActionId::new("jump").unwrap(),
                KeyCode::Space,
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        admit_key(&mut app, ButtonDriverInput::Press(KeyCode::Space));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(
            runtime
                .world()
                .resource::<ObservedOutcomes>()
                .0
                .iter()
                .map(|outcome| outcome.phase)
                .collect::<Vec<_>>(),
            [ActionPhase::Started]
        );
        assert_eq!(
            runtime
                .world()
                .resource::<ButtonInput<KeyCode>>()
                .transitions()
                .len(),
            0
        );

        runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(
            runtime
                .world()
                .resource::<ObservedOutcomes>()
                .0
                .iter()
                .map(|outcome| outcome.phase)
                .collect::<Vec<_>>(),
            [ActionPhase::Released]
        );
        assert!(
            runtime
                .world()
                .resource::<ButtonInput<KeyCode>>()
                .transitions()
                .is_empty()
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn keyboard_and_mouse_change_ticks_remain_independent() {
        assert_eq!(
            observed_changes_after(InputBinding::Key(KeyCode::Space)),
            (true, false)
        );
        assert_eq!(
            observed_changes_after(InputBinding::Mouse(MouseButton::Left)),
            (false, true)
        );
    }

    #[test]
    fn key_press_binding_produces_frame_transient_action_outcome() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key(
                ActionId::new("jump").unwrap(),
                KeyCode::Space,
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        admit_key(&mut app, ButtonDriverInput::Press(KeyCode::Space));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let observed = &runtime.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].action.as_str(), "jump");
        assert_eq!(observed[0].binding, InputBinding::Key(KeyCode::Space));
        assert_eq!(observed[0].phase, ActionPhase::Started);
        assert!(observed[0].value.digital);
        assert!(runtime.world().resource::<ActionOutcomes>().is_empty());
        runtime.finish().unwrap();
    }

    #[test]
    fn key_release_binding_produces_release_outcome() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key(
                ActionId::new("jump").unwrap(),
                KeyCode::Space,
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime
            .with_driver_scope(|scope| {
                apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KeyCode::Space))
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        runtime
            .with_driver_scope(|scope| {
                apply_keyboard_driver_input(scope, ButtonDriverInput::Release(KeyCode::Space))
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();

        runtime.drive(Duration::ZERO).unwrap();

        let observed = &runtime.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].phase, ActionPhase::Released);
        assert!(!observed[0].value.digital);
        runtime.finish().unwrap();
    }

    #[test]
    fn mouse_press_binding_produces_frame_transient_action_outcome() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::mouse(
                ActionId::new("select").unwrap(),
                MouseButton::Left,
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        admit_mouse(&mut app, ButtonDriverInput::Press(MouseButton::Left));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let observed = &runtime.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].action.as_str(), "select");
        assert_eq!(observed[0].binding, InputBinding::Mouse(MouseButton::Left));
        assert_eq!(observed[0].phase, ActionPhase::Started);
        assert!(observed[0].value.digital);
        assert!(runtime.world().resource::<ActionOutcomes>().is_empty());
        runtime.finish().unwrap();
    }

    #[test]
    fn mouse_release_binding_produces_release_outcome() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::mouse(
                ActionId::new("select").unwrap(),
                MouseButton::Left,
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime
            .with_driver_scope(|scope| {
                apply_mouse_driver_input(scope, ButtonDriverInput::Press(MouseButton::Left))
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        runtime
            .with_driver_scope(|scope| {
                apply_mouse_driver_input(scope, ButtonDriverInput::Release(MouseButton::Left))
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();

        runtime.drive(Duration::ZERO).unwrap();

        let observed = &runtime.world().resource::<ObservedOutcomes>().0;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].binding, InputBinding::Mouse(MouseButton::Left));
        assert_eq!(observed[0].phase, ActionPhase::Released);
        assert!(!observed[0].value.digital);
        runtime.finish().unwrap();
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
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder.declare_context(menu.clone(), true).unwrap();
        builder
            .bind(
                ActionBinding::key(ActionId::new("confirm").unwrap(), KeyCode::Enter)
                    .with_context(menu.clone()),
            )
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .disable_context(&menu)
            .unwrap();
        admit_key(&mut app, ButtonDriverInput::Press(KeyCode::Enter));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert!(runtime.world().resource::<ObservedOutcomes>().0.is_empty());
        runtime.finish().unwrap();
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
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key(action.clone(), KeyCode::Character('w')))
            .unwrap();
        builder
            .bind(ActionBinding::key(action, KeyCode::ArrowUp))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        admit_key(&mut app, ButtonDriverInput::Press(KeyCode::ArrowUp));
        admit_key(&mut app, ButtonDriverInput::Press(KeyCode::Character('w')));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let observed = &runtime.world().resource::<ObservedOutcomes>().0;
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
        runtime.finish().unwrap();
    }

    #[test]
    fn bindings_for_one_input_edge_emit_in_registration_order() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        for action in ["first", "second"] {
            builder
                .bind(ActionBinding::key(
                    ActionId::new(action).unwrap(),
                    KeyCode::Space,
                ))
                .unwrap();
        }
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        admit_key(&mut app, ButtonDriverInput::Press(KeyCode::Space));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(
            runtime
                .world()
                .resource::<ObservedOutcomes>()
                .0
                .iter()
                .map(|outcome| outcome.action.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn same_frame_quick_click_emits_started_then_released() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.insert_resource(ObservedOutcomes::default())
            .unwrap()
            .add_systems(CoreStage::Update, observe_outcomes)
            .unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key(
                ActionId::new("confirm").unwrap(),
                KeyCode::Enter,
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .replace_bindings(builder.compile().unwrap().into_bindings());
        admit_key(&mut app, ButtonDriverInput::Press(KeyCode::Enter));
        admit_key(&mut app, ButtonDriverInput::Release(KeyCode::Enter));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let observed = &runtime.world().resource::<ObservedOutcomes>().0;
        assert_eq!(
            observed
                .iter()
                .map(|outcome| outcome.phase)
                .collect::<Vec<_>>(),
            [ActionPhase::Started, ActionPhase::Released]
        );
        assert!(
            !runtime
                .world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::Enter)
        );
        runtime.finish().unwrap();
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

    #[test]
    fn rejected_replacement_definition_preserves_handles_and_staged_lifetime() {
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key(
                ActionId::new("jump").unwrap(),
                KeyCode::Space,
            ))
            .unwrap();
        let mut action_map = ActionMap::new(
            builder.compile().unwrap().into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap();
        action_map
            .disable_context(&ActionContext::gameplay())
            .unwrap();
        let bindings = action_map.bindings_handle();
        let revision = Arc::clone(&action_map.revision);
        let mut staging = ActionStaging::default();
        let staged = staging.observe(&action_map);
        let unknown = ActionContext::new("undeclared").unwrap();

        let result: Result<(), ActionMapError> = (|| {
            let mut candidate = ActionMapBuilder::new(ActionMapLimits::default());
            candidate.declare_context(ActionContext::gameplay(), true)?;
            candidate.bind(
                ActionBinding::key(ActionId::new("confirm").unwrap(), KeyCode::Enter)
                    .with_context(unknown.clone()),
            )?;
            action_map.replace_bindings(candidate.compile()?.into_bindings());
            Ok(())
        })();

        assert_eq!(result, Err(ActionMapError::UnknownContext(unknown.clone())));
        assert!(bindings.shares_owner_with(&action_map.bindings_handle()));
        assert!(std::ptr::eq(
            bindings.definition().bindings(),
            action_map.definition().bindings()
        ));
        assert!(Arc::ptr_eq(&revision, &action_map.revision));
        assert!(Arc::ptr_eq(&staged, &staging.observe(&action_map)));
        assert!(!action_map.is_context_enabled(&ActionContext::gameplay()));
        assert_eq!(action_map.bindings(), bindings.definition().bindings());

        assert_eq!(
            action_map.enable_context(&unknown),
            Err(ActionMapError::UnknownContext(unknown))
        );
        assert!(Arc::ptr_eq(&revision, &action_map.revision));
        assert!(Arc::ptr_eq(&staged, &staging.observe(&action_map)));
        assert!(
            action_map
                .enable_context(&ActionContext::gameplay())
                .unwrap()
        );
        assert!(!Arc::ptr_eq(&staged, &staging.observe(&action_map)));
    }
}
