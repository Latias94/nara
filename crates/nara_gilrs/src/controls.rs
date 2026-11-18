macro_rules! controls {
    ($name:ident, $native:ty, $description:literal, $($variant:ident = $code:literal => $mapped:ident),+ $(,)?) => {
        #[doc = $description]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u16)]
        pub enum $name {
            $($variant = $code,)+
        }

        impl $name {
            /// Returns the portable code used by Nara action bindings.
            #[must_use]
            pub const fn code(self) -> u16 {
                self as u16
            }

            pub(crate) const ALL: &[Self] = &[$(Self::$variant,)+];

            pub(crate) fn from_native(value: $native) -> Option<Self> {
                match value {
                    $(<$native>::$mapped => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub(crate) fn native(self) -> $native {
                match self {
                    $(Self::$variant => <$native>::$mapped,)+
                }
            }
        }
    };
}

controls!(GamepadButton, gilrs::Button, "Portable mapped gamepad buttons, independent of native discriminants.",
    South = 0 => South,
    East = 1 => East,
    West = 2 => West,
    North = 3 => North,
    LeftShoulder = 4 => LeftTrigger,
    RightShoulder = 5 => RightTrigger,
    LeftTrigger = 6 => LeftTrigger2,
    RightTrigger = 7 => RightTrigger2,
    Select = 8 => Select,
    Start = 9 => Start,
    Mode = 10 => Mode,
    LeftThumb = 11 => LeftThumb,
    RightThumb = 12 => RightThumb,
    DPadUp = 13 => DPadUp,
    DPadDown = 14 => DPadDown,
    DPadLeft = 15 => DPadLeft,
    DPadRight = 16 => DPadRight,
    C = 17 => C,
    Z = 18 => Z,
);

controls!(GamepadAxis, gilrs::Axis, "Portable normalized gamepad axes; positive Y follows Gilrs mapping.",
    LeftStickX = 0 => LeftStickX,
    LeftStickY = 1 => LeftStickY,
    RightStickX = 2 => RightStickX,
    RightStickY = 3 => RightStickY,
    LeftZ = 4 => LeftZ,
    RightZ = 5 => RightZ,
    DPadX = 6 => DPadX,
    DPadY = 7 => DPadY,
);
