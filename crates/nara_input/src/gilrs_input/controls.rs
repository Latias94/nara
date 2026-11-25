use gilrs::{Axis, Button};

use crate::{GamepadAxis, GamepadButton};

macro_rules! native_controls {
    ($table:ident, $convert:ident, $control:ident, $native:ident;
     $($variant:ident => $mapped:ident),+ $(,)?) => {
        pub(super) const $table: &[($control, $native)] = &[
            $(($control::$variant, $native::$mapped),)+
        ];

        pub(super) fn $convert(value: $native) -> Option<$control> {
            match value {
                $($native::$mapped => Some($control::$variant),)+
                _ => None,
            }
        }
    };
}

native_controls! {
    BUTTONS, button_from_native, GamepadButton, Button;
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

native_controls! {
    AXES, axis_from_native, GamepadAxis, Axis;
    LeftStickX => LeftStickX,
    LeftStickY => LeftStickY,
    RightStickX => RightStickX,
    RightStickY => RightStickY,
    LeftZ => LeftZ,
    RightZ => RightZ,
    DPadX => DPadX,
    DPadY => DPadY,
}
