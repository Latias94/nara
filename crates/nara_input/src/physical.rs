use std::collections::{BTreeMap, BTreeSet};

use nara_core::Vec2;
use nara_ecs::{ResMut, Resource, bevy_ecs::message::Messages};

use crate::{
    ButtonTransitionPhase, GamepadInputKind, InputEdge, InputEdgeKind, InputFrame, InputPointerId,
    InputSeatId, InputWindowEvent, InputWindowId, KeyCode, MouseButton,
    devices::{DeviceMessageWriters, WindowSamples},
};

type Endpoint = (InputWindowId, InputSeatId);
type Pointer = (Endpoint, InputPointerId);
type ButtonOwner = (Endpoint, Option<InputPointerId>);

/// One physical button change in the current app frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonTransition<T> {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub pointer: Option<InputPointerId>,
    button: T,
    phase: ButtonTransitionPhase,
}

impl<T: Copy> ButtonTransition<T> {
    #[must_use]
    pub const fn button(self) -> T {
        self.button
    }

    #[must_use]
    pub const fn phase(self) -> ButtonTransitionPhase {
        self.phase
    }
}

/// Physical held buttons and their current-frame transitions. UI consumption only affects actions.
#[derive(Debug, Resource)]
pub struct ButtonInput<T> {
    held: BTreeSet<(ButtonOwner, T)>,
    transitions: Vec<ButtonTransition<T>>,
}

impl<T> Default for ButtonInput<T> {
    fn default() -> Self {
        Self {
            held: BTreeSet::new(),
            transitions: Vec::new(),
        }
    }
}

impl<T: Copy + Ord> ButtonInput<T> {
    #[must_use]
    pub fn pressed(&self, button: T) -> bool {
        self.held.iter().any(|(_, held)| *held == button)
    }

    #[must_use]
    pub fn pressed_at(&self, window: InputWindowId, seat: InputSeatId, button: T) -> bool {
        self.held
            .iter()
            .any(|((endpoint, _), held)| *endpoint == (window, seat) && *held == button)
    }

    #[must_use]
    pub fn just_pressed(&self, button: T) -> bool {
        self.just_pressed_buttons().any(|pressed| pressed == button)
    }

    #[must_use]
    pub fn just_released(&self, button: T) -> bool {
        self.just_released_buttons()
            .any(|released| released == button)
    }

    pub fn just_pressed_buttons(&self) -> impl Iterator<Item = T> + '_ {
        self.buttons_with_phase(ButtonTransitionPhase::Pressed)
    }

    pub fn just_released_buttons(&self) -> impl Iterator<Item = T> + '_ {
        self.buttons_with_phase(ButtonTransitionPhase::Released)
    }

    pub fn pressed_buttons(&self) -> impl Iterator<Item = T> + '_ {
        self.held
            .iter()
            .map(|(_, button)| *button)
            .collect::<BTreeSet<_>>()
            .into_iter()
    }

    #[must_use]
    pub fn transitions(&self) -> &[ButtonTransition<T>] {
        &self.transitions
    }

    fn buttons_with_phase(&self, phase: ButtonTransitionPhase) -> impl Iterator<Item = T> + '_ {
        self.transitions
            .iter()
            .filter_map(move |edge| (edge.phase == phase).then_some(edge.button))
    }

    fn apply(&mut self, owner: ButtonOwner, button: T, phase: ButtonTransitionPhase) -> bool {
        let changed = match phase {
            ButtonTransitionPhase::Pressed => self.held.insert((owner, button)),
            ButtonTransitionPhase::Released => self.held.remove(&(owner, button)),
        };
        if changed {
            self.transitions.push(transition(owner, button, phase));
        }
        changed
    }

    fn release(
        &mut self,
        matches: impl Fn(ButtonOwner) -> bool,
        mut emit: impl FnMut(ButtonOwner, T),
    ) {
        for (owner, button) in self.held.extract_if(.., |(owner, _)| matches(*owner)) {
            self.transitions
                .push(transition(owner, button, ButtonTransitionPhase::Released));
            emit(owner, button);
        }
    }

    pub(crate) fn clear_transitions(&mut self) {
        if !self.transitions.is_empty() {
            self.transitions.clear();
        }
    }
}

fn transition<T>(
    ((window, seat), pointer): ButtonOwner,
    button: T,
    phase: ButtonTransitionPhase,
) -> ButtonTransition<T> {
    ButtonTransition {
        window,
        seat,
        pointer,
        button,
        phase,
    }
}

/// Latest logical pointer positions, retained until leave, focus loss, or geometry changes.
#[derive(Debug, Default, Resource)]
pub struct PointerState {
    positions: BTreeMap<Pointer, Vec2>,
    last: Option<Pointer>,
}

impl PointerState {
    #[must_use]
    pub fn position(&self) -> Option<Vec2> {
        self.last
            .and_then(|owner| self.positions.get(&owner).copied())
    }

    #[must_use]
    pub fn position_at(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
    ) -> Option<Vec2> {
        self.positions.get(&((window, seat), pointer)).copied()
    }

    fn clear(&mut self, matches: impl Fn(Pointer) -> bool) {
        self.positions.retain(|owner, _| !matches(*owner));
        if self.last.is_some_and(matches) {
            self.last = None;
        }
    }
}

#[derive(Default, Resource)]
pub(crate) struct PhysicalInput {
    blurred: BTreeSet<InputWindowId>,
    samples: WindowSamples,
}

pub(crate) fn project_input(
    mut incoming: ResMut<Messages<InputEdge>>,
    mut physical: ResMut<PhysicalInput>,
    mut frame: ResMut<InputFrame>,
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut pointers: ResMut<PointerState>,
    mut messages: DeviceMessageWriters,
) {
    if frame.projected {
        return;
    }
    frame.projected = true;
    for mut edge in incoming.drain() {
        let endpoint = (edge.window, edge.seat);
        if physical.blurred.contains(&edge.window)
            && !matches!(
                edge.kind,
                InputEdgeKind::Window(_)
                    | InputEdgeKind::PointerMoved { .. }
                    | InputEdgeKind::PointerLeft { .. }
                    | InputEdgeKind::Gamepad {
                        kind: GamepadInputKind::Connected | GamepadInputKind::Disconnected,
                        ..
                    }
            )
        {
            continue;
        }
        let loss = match &mut edge.kind {
            InputEdgeKind::Key { key, phase } => {
                if !keyboard.apply((endpoint, None), *key, *phase) {
                    continue;
                }
                None
            }
            InputEdgeKind::PointerMoved { pointer, position } => {
                if !position.is_finite() {
                    continue;
                }
                let owner = (endpoint, *pointer);
                pointers.positions.insert(owner, *position);
                pointers.last = Some(owner);
                None
            }
            InputEdgeKind::PointerButton {
                pointer,
                button,
                phase,
                position,
            } => {
                if !mouse.apply((endpoint, Some(*pointer)), *button, *phase) {
                    continue;
                }
                *position = position
                    .filter(|value| value.is_finite())
                    .or_else(|| pointers.position_at(edge.window, edge.seat, *pointer));
                None
            }
            InputEdgeKind::Wheel {
                pointer,
                delta,
                position,
                ..
            } => {
                if !delta.is_finite() {
                    continue;
                }
                *position = position
                    .filter(|value| value.is_finite())
                    .or_else(|| pointers.position_at(edge.window, edge.seat, *pointer));
                None
            }
            InputEdgeKind::PointerLeft { pointer } => Some(PhysicalLoss::Pointer(*pointer)),
            InputEdgeKind::Window(event) => match event {
                InputWindowEvent::Focused(true) => {
                    physical.blurred.remove(&edge.window);
                    None
                }
                InputWindowEvent::Focused(false) => {
                    physical.blurred.insert(edge.window);
                    Some(PhysicalLoss::Window)
                }
                InputWindowEvent::CloseRequested => {
                    physical.blurred.insert(edge.window);
                    Some(PhysicalLoss::Window)
                }
                InputWindowEvent::SurfaceRetired => {
                    physical.blurred.remove(&edge.window);
                    Some(PhysicalLoss::Window)
                }
                InputWindowEvent::Resized { logical_size } if logical_size.is_finite() => {
                    Some(PhysicalLoss::WindowPointers)
                }
                InputWindowEvent::ScaleFactorChanged { scale_factor }
                    if scale_factor.is_finite() && *scale_factor > 0.0 =>
                {
                    Some(PhysicalLoss::WindowPointers)
                }
                _ => continue,
            },
            _ => {
                if !physical.samples.accept(&mut edge) {
                    continue;
                }
                None
            }
        };
        messages.write(&edge);
        frame.edges.push(edge);
        let Some(loss) = loss else {
            continue;
        };
        let mut emit = |seat, kind| {
            let edge = InputEdge::new(endpoint.0, seat, kind);
            messages.write(&edge);
            frame.edges.push(edge);
        };
        let affected = |(owner, pointer): ButtonOwner| {
            owner.0 == endpoint.0
                && match loss {
                    PhysicalLoss::Pointer(only) => owner == endpoint && pointer == Some(only),
                    _ => true,
                }
        };
        if let PhysicalLoss::Window = loss {
            keyboard.release(affected, |((_, seat), _), key| {
                emit(
                    seat,
                    InputEdgeKind::Key {
                        key,
                        phase: ButtonTransitionPhase::Released,
                    },
                )
            });
            physical.samples.cancel(endpoint.0, &mut emit);
        }
        mouse.release(affected, |((_, seat), id), button| {
            if let Some(pointer) = id {
                emit(
                    seat,
                    InputEdgeKind::PointerButton {
                        pointer,
                        button,
                        phase: ButtonTransitionPhase::Released,
                        position: None,
                    },
                );
            }
        });
        pointers.clear(|(owner, pointer)| affected((owner, Some(pointer))));
    }
    let edge_count = frame.edges.len();
    frame.consumed.resize(edge_count, false);
}

#[derive(Clone, Copy)]
enum PhysicalLoss {
    Window,
    WindowPointers,
    Pointer(InputPointerId),
}
