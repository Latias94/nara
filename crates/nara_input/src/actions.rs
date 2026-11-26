use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};

use nara_core::Vec2;
use nara_ecs::{Res, ResMut, Resource};

use crate::{
    ActionContext, ActionId, ActionMaps, ActionPhase, ActionValue, ActionValueKind, AxisValue,
    GamepadInputKind, InputBinding, InputEdgeKind, InputFrame, InputPointerId, InputSeatId,
    InputWindowEvent, InputWindowId,
};

/// One change of an aggregate action value in the current frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionEdge {
    pub seat: InputSeatId,
    pub action: ActionId,
    pub context: ActionContext,
    pub phase: ActionPhase,
    pub value: ActionValue,
    /// The physical binding for an individual change. Bulk loss and map changes have no binding.
    pub binding: Option<InputBinding>,
}

/// Held actions and ordered changes. Held values are stable across every fixed tick in a frame.
/// Frame edges remain visible through `FixedUpdate` and are cleared at frame cleanup.
#[derive(Debug, Default, Resource)]
pub struct ActionState {
    held: BTreeMap<ActionKey, HeldAction>,
    edges: Vec<ActionEdge>,
}

impl ActionState {
    #[must_use]
    pub fn button(&self, action: &str) -> bool {
        self.for_seat(InputSeatId::PRIMARY).button(action)
    }

    #[must_use]
    pub fn axis(&self, action: &str) -> f32 {
        self.for_seat(InputSeatId::PRIMARY).axis(action)
    }

    #[must_use]
    pub fn axis_2d(&self, action: &str) -> Vec2 {
        self.for_seat(InputSeatId::PRIMARY).axis_2d(action)
    }

    #[must_use]
    pub const fn for_seat(&self, seat: InputSeatId) -> ActionSeat<'_> {
        ActionSeat { state: self, seat }
    }

    #[must_use]
    pub fn edges(&self) -> &[ActionEdge] {
        &self.edges
    }

    pub(crate) fn clear_edges(&mut self) {
        self.edges.clear();
    }
}

/// Reads actions for a selected seat. Unqualified names use its `gameplay` context.
#[derive(Debug, Clone, Copy)]
pub struct ActionSeat<'a> {
    state: &'a ActionState,
    seat: InputSeatId,
}

impl ActionSeat<'_> {
    #[must_use]
    pub fn button(&self, action: &str) -> bool {
        self.gameplay_value(action) == Some(ActionValue::Button(true))
    }

    #[must_use]
    pub fn axis(&self, action: &str) -> f32 {
        match self.gameplay_value(action) {
            Some(ActionValue::Axis(value)) => value.get(),
            _ => 0.0,
        }
    }

    /// Reads the independent `<action>.x` and `<action>.y` scalars.
    #[must_use]
    pub fn axis_2d(&self, action: &str) -> Vec2 {
        let mut axes = Vec2::ZERO;
        for (name, value) in self.gameplay_values() {
            let ActionValue::Axis(value) = value else {
                continue;
            };
            match name.strip_prefix(action) {
                Some(".x") => axes.x = value.get(),
                Some(".y") => axes.y = value.get(),
                _ => {}
            }
        }
        axes
    }

    #[must_use]
    pub fn value(&self, action: &ActionId, context: &ActionContext) -> Option<ActionValue> {
        self.state.held.iter().find_map(|(key, held)| {
            (key.seat == self.seat && &key.action == action && &key.context == context)
                .then_some(held.value)
        })
    }

    pub fn edges(&self) -> impl Iterator<Item = &ActionEdge> + '_ {
        self.state
            .edges
            .iter()
            .filter(|edge| edge.seat == self.seat)
    }

    fn gameplay_value(&self, action: &str) -> Option<ActionValue> {
        self.gameplay_values()
            .find_map(|(name, value)| (name == action).then_some(value))
    }

    fn gameplay_values(&self) -> impl Iterator<Item = (&str, ActionValue)> + '_ {
        self.state.held.iter().filter_map(|(key, held)| {
            (key.seat == self.seat && key.context.as_str() == ActionContext::GAMEPLAY_NAME)
                .then_some((key.action.as_str(), held.value))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ActionKey {
    seat: InputSeatId,
    context: ActionContext,
    action: ActionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Device {
    Keyboard,
    Pointer(InputPointerId),
    Gamepad(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Control {
    window: InputWindowId,
    device: Device,
    binding: InputBinding,
}

#[derive(Debug)]
struct HeldAction {
    value: ActionValue,
    map: Weak<()>,
    context: Weak<()>,
    contributions: BTreeMap<Control, ActionValue>,
}

impl HeldAction {
    fn aggregate(&self) -> ActionValue {
        match self.value.kind() {
            ActionValueKind::Button => ActionValue::Button(!self.contributions.is_empty()),
            ActionValueKind::Axis => {
                let sum = self
                    .contributions
                    .values()
                    .filter_map(|value| match value {
                        ActionValue::Axis(axis) => Some(axis.get()),
                        ActionValue::Button(_) => None,
                    })
                    .sum::<f32>();
                ActionValue::Axis(AxisValue::new(sum.clamp(-1.0, 1.0)).unwrap_or(AxisValue::ZERO))
            }
        }
    }
}

fn neutral(kind: ActionValueKind) -> ActionValue {
    match kind {
        ActionValueKind::Button => ActionValue::Button(false),
        ActionValueKind::Axis => ActionValue::Axis(AxisValue::ZERO),
    }
}

fn record(
    edges: &mut Vec<ActionEdge>,
    key: &ActionKey,
    before: ActionValue,
    value: ActionValue,
    binding: Option<InputBinding>,
) {
    let phase = match (before.is_active(), value.is_active()) {
        (false, true) => ActionPhase::Started,
        (true, false) => ActionPhase::Released,
        (true, true) if before != value => ActionPhase::Changed,
        _ => return,
    };
    edges.push(ActionEdge {
        seat: key.seat,
        action: key.action.clone(),
        context: key.context.clone(),
        phase,
        value,
        binding,
    });
}

fn matches_token(weak: &Weak<()>, live: &Arc<()>) -> bool {
    std::ptr::eq(weak.as_ptr(), Arc::as_ptr(live))
}

impl ActionState {
    fn reconcile(&mut self, maps: &ActionMaps) {
        self.held.retain(|key, held| {
            let same_mapping = maps.get(key.seat).is_some_and(|map| {
                matches_token(&held.map, &map.revision)
                    && map
                        .context_token(&key.context)
                        .is_some_and(|context| matches_token(&held.context, context))
            });
            if !same_mapping {
                record(
                    &mut self.edges,
                    key,
                    held.value,
                    neutral(held.value.kind()),
                    None,
                );
            }
            same_mapping
        });
    }

    fn release(&mut self, matches: impl Fn(InputSeatId, Control) -> bool) {
        self.held.retain(|key, held| {
            let count = held.contributions.len();
            held.contributions
                .retain(|control, _| !matches(key.seat, *control));
            if held.contributions.len() != count {
                let next = held.aggregate();
                record(&mut self.edges, key, held.value, next, None);
                held.value = next;
            }
            !held.contributions.is_empty()
        });
    }

    fn apply(
        &mut self,
        maps: &ActionMaps,
        seat: InputSeatId,
        control: Control,
        sample: Sample,
        consumed: bool,
    ) {
        let Some(map) = maps.get(seat) else {
            return;
        };
        for index in map.binding_indices_for_input(control.binding) {
            let Some(binding) = map.binding_at(index) else {
                continue;
            };
            let Some(context) = map.binding_context_token(index) else {
                continue;
            };
            let value = match sample {
                Sample::Button(phase) => binding.value_for_button(phase),
                Sample::Axis(value) => binding.value_for_axis(value),
            };
            let Some(value) = value else {
                continue;
            };
            let key = ActionKey {
                seat,
                context: binding.context.clone(),
                action: binding.action.clone(),
            };
            let held = if value.is_active() {
                if consumed {
                    continue;
                }
                let held = self.held.entry(key.clone()).or_insert_with(|| HeldAction {
                    value: neutral(value.kind()),
                    map: Arc::downgrade(&map.revision),
                    context: Arc::downgrade(context),
                    contributions: BTreeMap::new(),
                });
                held.contributions.insert(control, value);
                held
            } else {
                let Some(held) = self.held.get_mut(&key) else {
                    continue;
                };
                if held.contributions.remove(&control).is_none() {
                    continue;
                }
                held
            };
            let next = held.aggregate();
            record(
                &mut self.edges,
                &key,
                held.value,
                next,
                Some(control.binding),
            );
            held.value = next;
            if held.contributions.is_empty() {
                self.held.remove(&key);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Sample {
    Button(crate::ButtonTransitionPhase),
    Axis(AxisValue),
}

pub(crate) fn resolve_actions(
    mut frame: ResMut<InputFrame>,
    maps: Res<ActionMaps>,
    mut state: ResMut<ActionState>,
) {
    if frame.resolved {
        return;
    }
    frame.resolved = true;
    state.reconcile(&maps);
    for (index, edge) in frame.edges().iter().enumerate() {
        let (device, binding, sample) = match edge.kind {
            InputEdgeKind::Key { key, phase } => (
                Device::Keyboard,
                InputBinding::Key(key),
                Sample::Button(phase),
            ),
            InputEdgeKind::PointerButton {
                pointer,
                button,
                phase,
                ..
            } => (
                Device::Pointer(pointer),
                InputBinding::Mouse(button),
                Sample::Button(phase),
            ),
            InputEdgeKind::Gamepad {
                gamepad,
                kind: GamepadInputKind::Button { button, phase },
            } => (
                Device::Gamepad(gamepad),
                InputBinding::GamepadButton(button.code()),
                Sample::Button(phase),
            ),
            InputEdgeKind::Gamepad {
                gamepad,
                kind: GamepadInputKind::Axis { axis, value },
            } => (
                Device::Gamepad(gamepad),
                InputBinding::GamepadAxis(axis.code()),
                Sample::Axis(value),
            ),
            InputEdgeKind::Gamepad {
                gamepad,
                kind: GamepadInputKind::Disconnected,
            } => {
                state.release(|seat, control| {
                    seat == edge.seat
                        && control.window == edge.window
                        && control.device == Device::Gamepad(gamepad)
                });
                continue;
            }
            InputEdgeKind::Window(
                InputWindowEvent::Focused(false)
                | InputWindowEvent::CloseRequested
                | InputWindowEvent::SurfaceRetired,
            ) => {
                state.release(|_, control| control.window == edge.window);
                continue;
            }
            InputEdgeKind::Window(
                InputWindowEvent::Resized { .. } | InputWindowEvent::ScaleFactorChanged { .. },
            ) => {
                state.release(|_, control| {
                    control.window == edge.window && matches!(control.device, Device::Pointer(_))
                });
                continue;
            }
            InputEdgeKind::PointerLeft { pointer } => {
                state.release(|seat, control| {
                    seat == edge.seat
                        && control.window == edge.window
                        && control.device == Device::Pointer(pointer)
                });
                continue;
            }
            _ => continue,
        };
        state.apply(
            &maps,
            edge.seat,
            Control {
                window: edge.window,
                device,
                binding,
            },
            sample,
            frame.is_consumed(index),
        );
    }
}
