use nara_ecs::{
    Entity, Query, ResMut,
    bevy_ecs::message::{Message, MessageWriter},
};
use nara_input::{
    ButtonTransitionPhase, GamepadButton, GamepadInputKind, InputEdgeKind, InputFrame,
    InputWindowEvent, KeyCode, MouseButton,
};

use crate::kernel::{UiFrame, UiHitRegion, UiKey, UiPoint, UiRect};
use crate::runtime::{Surface, UiButtonBinding, UiRuntimeState, UiTarget};
use crate::{UiActionId, UiButton, UiSurfaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiActivationSource {
    Pointer,
    Keyboard,
    Gamepad,
}

/// A button activation from the last layout accepted by the presenter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiActionEvent {
    action: UiActionId,
    target: Entity,
    surface_id: UiSurfaceId,
    sequence: u64,
    source: UiActivationSource,
}

impl Message for UiActionEvent {}

impl UiActionEvent {
    pub const fn action(&self) -> &UiActionId {
        &self.action
    }
    pub const fn target(&self) -> Entity {
        self.target
    }
    pub const fn surface_id(&self) -> UiSurfaceId {
        self.surface_id
    }
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    pub const fn source(&self) -> UiActivationSource {
        self.source
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct UiPress {
    pub(crate) target: UiTarget,
    hit: UiHitRegion,
}

#[derive(Default)]
pub(crate) struct SurfaceInteraction {
    pub(crate) focused: Option<UiTarget>,
    pub(crate) hovered: Option<UiTarget>,
    pub(crate) pressed: Option<UiPress>,
    pointer: Option<UiPoint>,
    awaiting_extent: bool,
    awaiting_scale: bool,
    shift: [bool; 2],
}

impl SurfaceInteraction {
    fn cancel_pointer(&mut self) {
        self.pointer = None;
        self.hovered = None;
        self.pressed = None;
    }

    pub(crate) fn reconcile(&mut self, frame: &UiFrame) {
        self.awaiting_extent = false;
        self.awaiting_scale = false;
        self.hovered = hit_test(frame, self.pointer, &|_| true).map(|hit| hit.target);
        if self.pressed.as_ref().is_some_and(|press| {
            !in_modal_scope(frame, &press.target.key)
                || binding(frame, &press.target.key) != Some(&press.target.binding)
                || !frame.hit_regions().contains(&press.hit)
        }) {
            self.pressed = None;
        }
        if self
            .focused
            .as_ref()
            .is_some_and(|focus| focus_targets(frame, &|_| true).all(|target| target != *focus))
        {
            self.focused = None;
        }
    }

    fn pointer_button(
        &mut self,
        frame: &UiFrame,
        position: Option<UiPoint>,
        phase: ButtonTransitionPhase,
        live: &impl Fn(&UiButtonBinding) -> bool,
    ) -> (bool, Option<Activation>) {
        self.pointer = position;
        let hit = hit_test(frame, position, live);
        self.hovered = hit.as_ref().map(|hit| hit.target.clone());
        match phase {
            ButtonTransitionPhase::Pressed => {
                self.focused.clone_from(&self.hovered);
                self.pressed = hit;
                (self.pressed.is_some(), None)
            }
            ButtonTransitionPhase::Released => {
                let press = self.pressed.take();
                let handled = press.is_some() || hit.is_some();
                let activation = press
                    .filter(|press| hit.as_ref() == Some(press))
                    .map(|press| (press.target.binding, UiActivationSource::Pointer));
                (handled, activation)
            }
        }
    }

    fn focus_command(
        &mut self,
        frame: &UiFrame,
        command: FocusCommand,
        live: &impl Fn(&UiButtonBinding) -> bool,
    ) -> (bool, Option<Activation>) {
        let current = self.focused.as_ref();
        match command {
            FocusCommand::Cycle { previous } => {
                let targets = focus_targets(frame, live).collect::<Vec<_>>();
                if targets.is_empty() {
                    return (false, None);
                }
                let index =
                    current.and_then(|current| targets.iter().position(|target| target == current));
                let next = match (index, previous) {
                    (Some(0) | None, true) => targets.len() - 1,
                    (Some(index), true) => index - 1,
                    (Some(index), false) => (index + 1) % targets.len(),
                    (None, false) => 0,
                };
                self.focused = targets.into_iter().nth(next);
                (true, None)
            }
            FocusCommand::Move(direction) => {
                let destination = directional_focus(frame, current, direction, live);
                if destination.is_some() {
                    self.focused = destination;
                }
                (self.focused.is_some(), None)
            }
            FocusCommand::Activate(source) => {
                let target = focus_targets(frame, live)
                    .find(|target| Some(target) == current)
                    .or_else(|| {
                        frame
                            .blocks_unhandled_input()
                            .then(|| focus_targets(frame, live).next())
                            .flatten()
                    });
                let activation = target.map(|target| (target.binding, source));
                (activation.is_some(), activation)
            }
            FocusCommand::Clear => (self.focused.take().is_some(), None),
        }
    }
}

type Activation = (UiButtonBinding, UiActivationSource);

pub(crate) fn route_input(
    mut input: ResMut<InputFrame>,
    mut runtime: ResMut<UiRuntimeState>,
    buttons: Query<&UiButton>,
    mut actions: MessageWriter<UiActionEvent>,
) {
    if input.is_resolved() {
        return;
    }
    let live = |binding: &UiButtonBinding| {
        buttons
            .get(binding.entity)
            .is_ok_and(|button| button.enabled() && button.action() == &binding.action)
    };
    for index in 0..input.edges().len() {
        let edge = &input.edges()[index];
        let seat = (!matches!(edge.kind, InputEdgeKind::Window(_))).then_some(edge.seat);
        let Some(surface) = runtime.input_surface(edge.window, seat) else {
            continue;
        };
        let surface_id = surface.slot.surface_id();
        let (handled, activation) =
            route_edge(surface, &edge.kind, input.is_consumed(index), &live);
        if handled {
            input.consume(index);
        }
        if let Some((binding, source)) = activation {
            runtime.action_sequence = runtime.action_sequence.saturating_add(1);
            actions.write(UiActionEvent {
                target: binding.entity,
                action: binding.action,
                surface_id,
                source,
                sequence: runtime.action_sequence,
            });
        }
    }
}

fn route_edge(
    surface: &mut Surface,
    kind: &InputEdgeKind,
    consumed: bool,
    live: &impl Fn(&UiButtonBinding) -> bool,
) -> (bool, Option<Activation>) {
    match *kind {
        InputEdgeKind::Window(
            InputWindowEvent::Focused(false)
            | InputWindowEvent::CloseRequested
            | InputWindowEvent::SurfaceRetired,
        ) => {
            surface.interaction.cancel_pointer();
            surface.interaction.focused = None;
            surface.interaction.shift = [false; 2];
        }
        InputEdgeKind::Window(InputWindowEvent::Resized { logical_size }) => {
            surface.interaction.cancel_pointer();
            let transform = surface.slot.transform();
            let physical = transform.physical_surface_size();
            let reported = logical_size * transform.platform_scale_factor();
            surface.interaction.awaiting_extent = (reported.x - physical.width()).abs() > 0.5
                || (reported.y - physical.height()).abs() > 0.5;
        }
        InputEdgeKind::Window(InputWindowEvent::ScaleFactorChanged { scale_factor }) => {
            surface.interaction.cancel_pointer();
            surface.interaction.awaiting_scale =
                scale_factor as f32 != surface.slot.transform().platform_scale_factor();
        }
        InputEdgeKind::PointerLeft { pointer } if pointer == surface.slot.endpoint().pointer() => {
            surface.interaction.cancel_pointer();
        }
        InputEdgeKind::Key {
            key: KeyCode::ShiftLeft | KeyCode::ShiftRight,
            phase,
        } => {
            let right = matches!(
                kind,
                InputEdgeKind::Key {
                    key: KeyCode::ShiftRight,
                    ..
                }
            );
            surface.interaction.shift[usize::from(right)] = phase == ButtonTransitionPhase::Pressed;
        }
        _ => {}
    }
    if consumed {
        if let InputEdgeKind::PointerButton {
            pointer,
            button: MouseButton::Left,
            phase: ButtonTransitionPhase::Released,
            ..
        } = *kind
            && pointer == surface.slot.endpoint().pointer()
        {
            surface.interaction.pressed = None;
        }
        return (false, None);
    }
    let Some((_, frame)) = &surface.presented else {
        return (false, None);
    };
    let pointer_geometry_current =
        !surface.interaction.awaiting_extent && !surface.interaction.awaiting_scale;
    let outcome = match *kind {
        InputEdgeKind::PointerMoved { pointer, position }
            if pointer_geometry_current && pointer == surface.slot.endpoint().pointer() =>
        {
            let position = surface.authored_pointer(position.to_array());
            surface.interaction.pointer = position;
            surface.interaction.hovered = hit_test(frame, position, live).map(|hit| hit.target);
            (false, None)
        }
        InputEdgeKind::PointerButton {
            pointer,
            button: MouseButton::Left,
            phase,
            position,
        } if pointer_geometry_current && pointer == surface.slot.endpoint().pointer() => {
            let position =
                position.and_then(|position| surface.authored_pointer(position.to_array()));
            surface
                .interaction
                .pointer_button(frame, position, phase, live)
        }
        InputEdgeKind::Key {
            key,
            phase: ButtonTransitionPhase::Pressed,
        } => {
            let shift = surface.interaction.shift.iter().any(|pressed| *pressed);
            keyboard_command(key, shift).map_or((false, None), |command| {
                surface.interaction.focus_command(frame, command, live)
            })
        }
        InputEdgeKind::Gamepad {
            kind:
                GamepadInputKind::Button {
                    button,
                    phase: ButtonTransitionPhase::Pressed,
                },
            ..
        } => gamepad_command(button).map_or((false, None), |command| {
            surface.interaction.focus_command(frame, command, live)
        }),
        _ => (false, None),
    };
    let modal = matches!(
        kind,
        InputEdgeKind::Key { .. }
            | InputEdgeKind::PointerButton { .. }
            | InputEdgeKind::Wheel { .. }
            | InputEdgeKind::Gamepad { .. }
    ) && frame.blocks_unhandled_input();
    (outcome.0 || modal, outcome.1)
}

fn in_modal_scope(frame: &UiFrame, key: &UiKey) -> bool {
    frame
        .active_modal_root()
        .is_none_or(|root| key.root_key() == root)
}

fn binding<'a>(frame: &'a UiFrame, key: &UiKey) -> Option<&'a UiButtonBinding> {
    frame.bindings.as_ref()?.buttons.get(key)
}

fn target(
    frame: &UiFrame,
    key: &UiKey,
    live: &impl Fn(&UiButtonBinding) -> bool,
) -> Option<UiTarget> {
    if !in_modal_scope(frame, key) {
        return None;
    }
    let binding = binding(frame, key).filter(|binding| live(binding))?;
    Some(UiTarget {
        key: key.clone(),
        binding: binding.clone(),
    })
}

fn hit_test(
    frame: &UiFrame,
    point: Option<UiPoint>,
    live: &impl Fn(&UiButtonBinding) -> bool,
) -> Option<UiPress> {
    frame
        .matching_hit_regions(point?)
        .filter(|hit| hit.enabled())
        .find_map(|hit| {
            Some(UiPress {
                target: target(frame, hit.key(), live)?,
                hit: hit.clone(),
            })
        })
}

fn focus_targets<'a>(
    frame: &'a UiFrame,
    live: &'a impl Fn(&UiButtonBinding) -> bool,
) -> impl Iterator<Item = UiTarget> + 'a {
    frame
        .focus_order()
        .iter()
        .filter(|fact| fact.enabled())
        .filter_map(|fact| target(frame, fact.key(), live))
}

enum FocusCommand {
    Cycle { previous: bool },
    Move([f32; 2]),
    Activate(UiActivationSource),
    Clear,
}

fn keyboard_command(key: KeyCode, shift: bool) -> Option<FocusCommand> {
    Some(match key {
        KeyCode::Tab => FocusCommand::Cycle { previous: shift },
        KeyCode::ArrowLeft => FocusCommand::Move([-1.0, 0.0]),
        KeyCode::ArrowRight => FocusCommand::Move([1.0, 0.0]),
        KeyCode::ArrowUp => FocusCommand::Move([0.0, -1.0]),
        KeyCode::ArrowDown => FocusCommand::Move([0.0, 1.0]),
        KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
            FocusCommand::Activate(UiActivationSource::Keyboard)
        }
        KeyCode::Escape => FocusCommand::Clear,
        _ => return None,
    })
}

fn gamepad_command(button: GamepadButton) -> Option<FocusCommand> {
    Some(match button {
        GamepadButton::DPadLeft => FocusCommand::Move([-1.0, 0.0]),
        GamepadButton::DPadRight => FocusCommand::Move([1.0, 0.0]),
        GamepadButton::DPadUp => FocusCommand::Move([0.0, -1.0]),
        GamepadButton::DPadDown => FocusCommand::Move([0.0, 1.0]),
        GamepadButton::South => FocusCommand::Activate(UiActivationSource::Gamepad),
        GamepadButton::East => FocusCommand::Clear,
        _ => return None,
    })
}

fn directional_focus(
    frame: &UiFrame,
    current: Option<&UiTarget>,
    direction: [f32; 2],
    live: &impl Fn(&UiButtonBinding) -> bool,
) -> Option<UiTarget> {
    let current = current.filter(|target| live(&target.binding));
    let Some(origin) = current.and_then(|current| {
        frame
            .focus_order()
            .iter()
            .find(|fact| fact.key() == &current.key)
    }) else {
        return focus_targets(frame, live).next();
    };
    let [origin_x, origin_y] = center(origin.visible_rect());
    frame
        .focus_order()
        .iter()
        .filter(|fact| fact.enabled())
        .filter_map(|fact| {
            let [x, y] = center(fact.visible_rect());
            let [delta_x, delta_y] = [x - origin_x, y - origin_y];
            let ahead = delta_x * direction[0] + delta_y * direction[1];
            if ahead <= 0.0 {
                return None;
            }
            let sideways = delta_x * direction[1] - delta_y * direction[0];
            Some((
                ahead * ahead + 4.0 * sideways * sideways,
                target(frame, fact.key(), live)?,
            ))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, target)| target)
}

fn center(rect: UiRect) -> [f32; 2] {
    let origin = rect.origin();
    [
        origin.x() + rect.size().width() * 0.5,
        origin.y() + rect.size().height() * 0.5,
    ]
}
