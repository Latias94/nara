//! Typed physical bindings and their scalar action transformations.

use crate::{
    ActionContext, ActionId, ActionValue, AxisDeadZone, AxisValue, ButtonTransitionPhase,
    InputBinding, KeyCode, MouseButton,
};

/// The value domain shared by every binding of an action in one context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActionValueKind {
    Button,
    Axis,
}

/// The signed endpoint contributed while a digital axis binding is pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AxisDirection {
    Negative,
    Positive,
}

/// Explicit conversion from a physical input to an action value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub enum ActionTransform {
    #[default]
    Button,
    AxisButton {
        direction: AxisDirection,
    },
    Axis {
        dead_zone: AxisDeadZone,
        inverted: bool,
    },
}

/// One physical contribution to a typed action in a declared context.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionBinding {
    pub action: ActionId,
    pub input: InputBinding,
    pub context: ActionContext,
    #[cfg_attr(feature = "serde", serde(default))]
    pub transform: ActionTransform,
}

impl ActionBinding {
    #[must_use]
    pub fn key(action: ActionId, key: KeyCode) -> Self {
        Self {
            action,
            input: InputBinding::Key(key),
            context: ActionContext::default(),
            transform: ActionTransform::Button,
        }
    }

    #[must_use]
    pub fn mouse(action: ActionId, button: MouseButton) -> Self {
        Self {
            action,
            input: InputBinding::Mouse(button),
            context: ActionContext::default(),
            transform: ActionTransform::Button,
        }
    }

    #[must_use]
    pub fn gamepad_button(action: ActionId, button: u16) -> Self {
        Self {
            action,
            input: InputBinding::GamepadButton(button),
            context: ActionContext::default(),
            transform: ActionTransform::Button,
        }
    }

    #[must_use]
    pub fn key_axis(action: ActionId, key: KeyCode, direction: AxisDirection) -> Self {
        Self {
            action,
            input: InputBinding::Key(key),
            context: ActionContext::default(),
            transform: ActionTransform::AxisButton { direction },
        }
    }

    #[must_use]
    pub fn gamepad_axis(
        action: ActionId,
        axis: u16,
        dead_zone: AxisDeadZone,
        inverted: bool,
    ) -> Self {
        Self {
            action,
            input: InputBinding::GamepadAxis(axis),
            context: ActionContext::default(),
            transform: ActionTransform::Axis {
                dead_zone,
                inverted,
            },
        }
    }

    #[must_use]
    pub fn with_context(mut self, context: ActionContext) -> Self {
        self.context = context;
        self
    }

    #[must_use]
    pub const fn value_kind(&self) -> ActionValueKind {
        match self.transform {
            ActionTransform::Button => ActionValueKind::Button,
            ActionTransform::AxisButton { .. } | ActionTransform::Axis { .. } => {
                ActionValueKind::Axis
            }
        }
    }

    pub(crate) fn is_transform_valid(&self) -> bool {
        matches!(
            (self.input, self.transform),
            (
                InputBinding::Key(_) | InputBinding::Mouse(_) | InputBinding::GamepadButton(_),
                ActionTransform::Button | ActionTransform::AxisButton { .. },
            ) | (InputBinding::GamepadAxis(_), ActionTransform::Axis { .. })
        )
    }

    pub(crate) fn value_for_button(&self, phase: ButtonTransitionPhase) -> Option<ActionValue> {
        if !self.is_transform_valid() {
            return None;
        }
        let pressed = phase == ButtonTransitionPhase::Pressed;
        match self.transform {
            ActionTransform::Button => Some(ActionValue::Button(pressed)),
            ActionTransform::AxisButton { direction } => {
                let value = match (pressed, direction) {
                    (false, _) => AxisValue::ZERO,
                    (true, AxisDirection::Negative) => AxisValue::new(-1.0).ok()?,
                    (true, AxisDirection::Positive) => AxisValue::new(1.0).ok()?,
                };
                Some(ActionValue::Axis(value))
            }
            ActionTransform::Axis { .. } => None,
        }
    }

    pub(crate) fn value_for_axis(&self, value: AxisValue) -> Option<ActionValue> {
        match (self.input, self.transform) {
            (
                InputBinding::GamepadAxis(_),
                ActionTransform::Axis {
                    dead_zone,
                    inverted,
                },
            ) => {
                let value = dead_zone.apply(value);
                let value = if inverted {
                    AxisValue::new(-value.get()).ok()?
                } else {
                    value
                };
                Some(ActionValue::Axis(value))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digital_constructors_preserve_button_values_and_reject_axis_samples() {
        let action = ActionId::new("confirm").unwrap();
        for binding in [
            ActionBinding::key(action.clone(), KeyCode::Enter),
            ActionBinding::mouse(action.clone(), MouseButton::Left),
            ActionBinding::gamepad_button(action, u16::MAX),
        ] {
            assert_eq!(binding.transform, ActionTransform::Button);
            assert_eq!(binding.value_kind(), ActionValueKind::Button);
            assert_eq!(
                binding.value_for_button(ButtonTransitionPhase::Pressed),
                Some(ActionValue::pressed())
            );
            assert_eq!(
                binding.value_for_button(ButtonTransitionPhase::Released),
                Some(ActionValue::released())
            );
            assert_eq!(binding.value_for_axis(AxisValue::ZERO), None);
        }
    }

    #[test]
    fn digital_axis_bindings_contribute_signed_endpoints_and_release_to_zero() {
        for (direction, expected) in [
            (AxisDirection::Negative, -1.0),
            (AxisDirection::Positive, 1.0),
        ] {
            for input in [
                InputBinding::Key(KeyCode::ArrowRight),
                InputBinding::Mouse(MouseButton::Other(u16::MAX)),
                InputBinding::GamepadButton(u16::MAX),
            ] {
                let binding = ActionBinding {
                    input,
                    ..ActionBinding::key_axis(
                        ActionId::new("move").unwrap(),
                        KeyCode::ArrowRight,
                        direction,
                    )
                };
                assert_eq!(binding.value_kind(), ActionValueKind::Axis);
                assert_eq!(
                    binding.value_for_button(ButtonTransitionPhase::Pressed),
                    Some(ActionValue::Axis(AxisValue::new(expected).unwrap()))
                );
                assert_eq!(
                    binding.value_for_button(ButtonTransitionPhase::Released),
                    Some(ActionValue::Axis(AxisValue::ZERO))
                );
                assert_eq!(binding.value_for_axis(AxisValue::ZERO), None);
            }
        }
    }

    #[test]
    fn analog_bindings_apply_explicit_dead_zones_and_inversion_to_one_value() {
        for inverted in [false, true] {
            let binding = ActionBinding::gamepad_axis(
                ActionId::new("move").unwrap(),
                u16::MAX,
                AxisDeadZone::new(0.25).unwrap(),
                inverted,
            );
            assert_eq!(binding.value_kind(), ActionValueKind::Axis);
            for (sample, expected) in [
                (-1.0, -1.0),
                (-0.625, -0.5),
                (-0.25, 0.0),
                (0.0, 0.0),
                (0.25, 0.0),
                (0.625, 0.5),
                (1.0, 1.0),
            ] {
                let expected = if inverted { -expected } else { expected };
                assert_eq!(
                    binding.value_for_axis(AxisValue::new(sample).unwrap()),
                    Some(ActionValue::Axis(AxisValue::new(expected).unwrap()))
                );
            }
            for phase in [
                ButtonTransitionPhase::Pressed,
                ButtonTransitionPhase::Released,
            ] {
                assert_eq!(binding.value_for_button(phase), None);
            }
        }
    }

    #[test]
    fn invalid_input_transform_pairs_produce_no_value() {
        let analog = ActionBinding::gamepad_axis(
            ActionId::new("move").unwrap(),
            0,
            AxisDeadZone::NONE,
            false,
        );
        for binding in [
            ActionBinding {
                input: InputBinding::Key(KeyCode::Space),
                ..analog.clone()
            },
            ActionBinding {
                input: InputBinding::Mouse(MouseButton::Left),
                ..analog.clone()
            },
            ActionBinding {
                input: InputBinding::GamepadButton(0),
                ..analog.clone()
            },
            ActionBinding {
                transform: ActionTransform::Button,
                ..analog.clone()
            },
            ActionBinding {
                transform: ActionTransform::AxisButton {
                    direction: AxisDirection::Positive,
                },
                ..analog
            },
        ] {
            assert_eq!(
                binding.value_for_button(ButtonTransitionPhase::Pressed),
                None
            );
            assert_eq!(
                binding.value_for_button(ButtonTransitionPhase::Released),
                None
            );
            assert_eq!(binding.value_for_axis(AxisValue::new(0.5).unwrap()), None);
        }
    }
}
