use nara_ecs::bevy_ecs::message::Message;

use crate::{
    AxisValue, ButtonTransitionPhase, InputEventKind, InputGamepadGeneration, InputRouteStamp,
    InputTimelineEntry,
};

macro_rules! standard_controls {
    ($(#[$doc:meta])* $control:ident { $($name:ident = $code:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u16)]
        pub enum $control {
            $($name = $code,)+
        }

        impl $control {
            /// Returns the stable numeric code used in action bindings and timeline events.
            #[must_use]
            pub const fn code(self) -> u16 {
                self as u16
            }

            /// Resolves a numeric code in the shared standard control set.
            #[must_use]
            pub const fn from_code(code: u16) -> Option<Self> {
                match code {
                    $($code => Some(Self::$name),)+
                    _ => None,
                }
            }
        }
    };
}

standard_controls! {
    /// Standard gamepad button identities. Bindings also accept producer-specific codes.
    GamepadButton {
        South = 0,
        East = 1,
        West = 2,
        North = 3,
        LeftShoulder = 4,
        RightShoulder = 5,
        LeftTrigger = 6,
        RightTrigger = 7,
        Select = 8,
        Start = 9,
        Mode = 10,
        LeftThumb = 11,
        RightThumb = 12,
        DPadUp = 13,
        DPadDown = 14,
        DPadLeft = 15,
        DPadRight = 16,
        C = 17,
        Z = 18,
    }
}

standard_controls! {
    /// Portable normalized gamepad axes. The positive Y direction follows Gilrs mappings.
    GamepadAxis {
        LeftStickX = 0,
        LeftStickY = 1,
        RightStickX = 2,
        RightStickY = 3,
        LeftZ = 4,
        RightZ = 5,
        DPadX = 6,
        DPadY = 7,
    }
}

/// A connection change or standard gamepad control sample from the input timeline.
/// Use the generation to distinguish reconnects that reuse a numeric gamepad label.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct GamepadInput {
    pub route: InputRouteStamp,
    pub gamepad: u16,
    pub connection_generation: InputGamepadGeneration,
    pub kind: GamepadInputKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadInputKind {
    Connected,
    Disconnected,
    Button {
        button: GamepadButton,
        phase: ButtonTransitionPhase,
    },
    Axis {
        axis: GamepadAxis,
        value: AxisValue,
    },
}

impl GamepadInput {
    pub(crate) fn from_timeline(entry: InputTimelineEntry) -> Option<Self> {
        let (gamepad, connection_generation, kind) = match entry.kind() {
            InputEventKind::GamepadConnected {
                gamepad,
                connection_generation,
            } => (gamepad, connection_generation, GamepadInputKind::Connected),
            InputEventKind::GamepadDisconnected {
                gamepad,
                connection_generation,
            } => (
                gamepad,
                connection_generation,
                GamepadInputKind::Disconnected,
            ),
            InputEventKind::GamepadButton {
                gamepad,
                connection_generation,
                button,
                phase,
            } => (
                gamepad,
                connection_generation,
                GamepadInputKind::Button {
                    button: GamepadButton::from_code(button)?,
                    phase,
                },
            ),
            InputEventKind::GamepadAxis {
                gamepad,
                connection_generation,
                axis,
                value,
            } => (
                gamepad,
                connection_generation,
                GamepadInputKind::Axis {
                    axis: GamepadAxis::from_code(axis)?,
                    value,
                },
            ),
            _ => return None,
        };
        Some(Self {
            route: entry.route(),
            gamepad,
            connection_generation,
            kind,
        })
    }
}
