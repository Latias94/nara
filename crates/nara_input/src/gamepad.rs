use nara_ecs::bevy_ecs::message::Message;

use crate::{AxisValue, ButtonTransitionPhase, InputSeatId, InputWindowId};

macro_rules! standard_controls {
    ($(#[$doc:meta])* $control:ident { $($name:ident = $code:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u16)]
        pub enum $control {
            $($name = $code,)+
        }

        impl $control {
            /// Returns the stable numeric code used in action bindings.
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
    /// Standard gamepad button identities.
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

/// A connection change or standard gamepad control sample. A producer assigns a fresh
/// connection ID on reconnect.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct GamepadInput {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub gamepad: u64,
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
