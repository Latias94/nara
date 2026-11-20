use nara_input::GamepadButton;

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

macro_rules! button_mapping {
    ($($button:ident => $native:ident),+ $(,)?) => {
        pub(crate) const BUTTONS: &[(GamepadButton, gilrs::Button)] = &[
            $((GamepadButton::$button, gilrs::Button::$native),)+
        ];

        pub(crate) fn button_from_native(button: gilrs::Button) -> Option<GamepadButton> {
            match button {
                $(gilrs::Button::$native => Some(GamepadButton::$button),)+
                _ => None,
            }
        }
    };
}

button_mapping! {
    South => South,
    East => East,
    West => West,
    North => North,
    LeftShoulder => LeftTrigger,
    RightShoulder => RightTrigger,
    LeftTrigger => LeftTrigger2,
    RightTrigger => RightTrigger2,
    Select => Select,
    Start => Start,
    Mode => Mode,
    LeftThumb => LeftThumb,
    RightThumb => RightThumb,
    DPadUp => DPadUp,
    DPadDown => DPadDown,
    DPadLeft => DPadLeft,
    DPadRight => DPadRight,
    C => C,
    Z => Z,
}

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
