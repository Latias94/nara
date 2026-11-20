macro_rules! standard_buttons {
    ($($name:ident = $code:literal),+ $(,)?) => {
        /// Standard gamepad button identities used by input producers and consumers.
        /// Bindings still accept other producer-specific numeric codes.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u16)]
        pub enum GamepadButton {
            $($name = $code,)+
        }

        impl GamepadButton {
            /// Returns the stable numeric code used in action bindings and timeline events.
            #[must_use]
            pub const fn code(self) -> u16 {
                self as u16
            }

            /// Resolves a numeric code in the shared standard button set.
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

standard_buttons! {
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
