macro_rules! key_codes {
    (
        existing [$($existing:ident),+]
        named { $($name:ident,)+ }
        compact { $($compact:ident = $character:literal,)+ }
    ) => {
        /// A physical keyboard position, using names from W3C `KeyboardEvent.code` and
        /// additional platform codes. The position is independent of the active layout.
        ///
        /// For positions such as [`Self::KeyW`], deserialization accepts both the W3C name
        /// and the compact V1 `Character` encoding. Serialization keeps the V1 encoding.
        #[non_exhaustive]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(from = "SerializedKeyCode"))]
        pub enum KeyCode {
            $($existing,)+
            /// Compact V1 binding representation. Use the named keys for physical input.
            Character(char),
            /// The additional physical key on an ISO keyboard.
            IntlBackslash,
            $($name,)+
        }

        impl KeyCode {
            $(
                #[allow(non_upper_case_globals)]
                pub const $compact: Self = Self::Character($character);
            )+

            #[cfg(feature = "serde")]
            pub(super) const fn serialized_string_bytes(self) -> usize {
                match self {
                    $(Self::$existing => stringify!($existing).len(),)+
                    Self::Character(character) => "Character".len() + character.len_utf8(),
                    Self::IntlBackslash => "IntlBackslash".len(),
                    $(Self::$name => stringify!($name).len(),)+
                }
            }
        }

        #[cfg(feature = "serde")]
        #[derive(serde::Deserialize)]
        #[serde(rename = "KeyCode")]
        enum SerializedKeyCode {
            $($existing,)+
            Character(char),
            IntlBackslash,
            $($name,)+
            $($compact,)+
        }

        #[cfg(feature = "serde")]
        impl From<SerializedKeyCode> for KeyCode {
            fn from(key: SerializedKeyCode) -> Self {
                match key {
                    $(SerializedKeyCode::$existing => Self::$existing,)+
                    SerializedKeyCode::Character(character) => Self::Character(character),
                    SerializedKeyCode::IntlBackslash => Self::IntlBackslash,
                    $(SerializedKeyCode::$name => Self::$name,)+
                    $(SerializedKeyCode::$compact => Self::$compact,)+
                }
            }
        }
    };
}

key_codes! {
    existing [Escape, Space, Enter, ArrowUp, ArrowDown, ArrowLeft, ArrowRight]
    named {
        IntlRo,
        IntlYen,
        AltLeft,
        AltRight,
        Backspace,
        CapsLock,
        ContextMenu,
        ControlLeft,
        ControlRight,
        MetaLeft,
        MetaRight,
        ShiftLeft,
        ShiftRight,
        Convert,
        KanaMode,
        Lang1,
        Lang2,
        Lang3,
        Lang4,
        Lang5,
        NonConvert,
        Delete,
        End,
        Help,
        Home,
        Insert,
        PageDown,
        PageUp,
        NumLock,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadAdd,
        NumpadBackspace,
        NumpadClear,
        NumpadClearEntry,
        NumpadComma,
        NumpadDecimal,
        NumpadDivide,
        NumpadEnter,
        NumpadEqual,
        NumpadHash,
        NumpadMemoryAdd,
        NumpadMemoryClear,
        NumpadMemoryRecall,
        NumpadMemoryStore,
        NumpadMemorySubtract,
        NumpadMultiply,
        NumpadParenLeft,
        NumpadParenRight,
        NumpadStar,
        NumpadSubtract,
        Fn,
        FnLock,
        PrintScreen,
        ScrollLock,
        Pause,
        BrowserBack,
        BrowserFavorites,
        BrowserForward,
        BrowserHome,
        BrowserRefresh,
        BrowserSearch,
        BrowserStop,
        Eject,
        LaunchApp1,
        LaunchApp2,
        LaunchMail,
        MediaPlayPause,
        MediaSelect,
        MediaStop,
        MediaTrackNext,
        MediaTrackPrevious,
        Power,
        Sleep,
        AudioVolumeDown,
        AudioVolumeMute,
        AudioVolumeUp,
        WakeUp,
        Meta,
        Hyper,
        Turbo,
        Abort,
        Resume,
        Suspend,
        Again,
        Copy,
        Cut,
        Find,
        Open,
        Paste,
        Props,
        Select,
        Undo,
        Hiragana,
        Katakana,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        F13,
        F14,
        F15,
        F16,
        F17,
        F18,
        F19,
        F20,
        F21,
        F22,
        F23,
        F24,
        F25,
        F26,
        F27,
        F28,
        F29,
        F30,
        F31,
        F32,
        F33,
        F34,
        F35,
    }
    compact {
        Backquote = '`',
        Backslash = '\\',
        BracketLeft = '[',
        BracketRight = ']',
        Comma = ',',
        Digit0 = '0',
        Digit1 = '1',
        Digit2 = '2',
        Digit3 = '3',
        Digit4 = '4',
        Digit5 = '5',
        Digit6 = '6',
        Digit7 = '7',
        Digit8 = '8',
        Digit9 = '9',
        Equal = '=',
        KeyA = 'a',
        KeyB = 'b',
        KeyC = 'c',
        KeyD = 'd',
        KeyE = 'e',
        KeyF = 'f',
        KeyG = 'g',
        KeyH = 'h',
        KeyI = 'i',
        KeyJ = 'j',
        KeyK = 'k',
        KeyL = 'l',
        KeyM = 'm',
        KeyN = 'n',
        KeyO = 'o',
        KeyP = 'p',
        KeyQ = 'q',
        KeyR = 'r',
        KeyS = 's',
        KeyT = 't',
        KeyU = 'u',
        KeyV = 'v',
        KeyW = 'w',
        KeyX = 'x',
        KeyY = 'y',
        KeyZ = 'z',
        Minus = '-',
        Period = '.',
        Quote = '\'',
        Semicolon = ';',
        Slash = '/',
        Tab = '\t',
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::KeyCode;

    #[test]
    fn compact_positions_accept_named_bindings_and_preserve_the_v1_encoding() {
        for (key, named, compact) in [
            (KeyCode::KeyW, r#""KeyW""#, r#"{"Character":"w"}"#),
            (KeyCode::Digit1, r#""Digit1""#, r#"{"Character":"1"}"#),
            (KeyCode::Tab, r#""Tab""#, r#"{"Character":"\t"}"#),
            (KeyCode::Backquote, r#""Backquote""#, r#"{"Character":"`"}"#),
        ] {
            assert_eq!(serde_json::from_str::<KeyCode>(named).unwrap(), key);
            assert_eq!(serde_json::from_str::<KeyCode>(compact).unwrap(), key);
            assert_eq!(serde_json::to_string(&key).unwrap(), compact);
            assert_eq!(key.serialized_string_bytes(), "Character".len() + 1);
        }
    }

    #[test]
    fn new_physical_positions_roundtrip_with_their_wire_names() {
        for (key, name) in [
            (KeyCode::MetaRight, "MetaRight"),
            (KeyCode::NumpadMemorySubtract, "NumpadMemorySubtract"),
            (KeyCode::MediaTrackPrevious, "MediaTrackPrevious"),
        ] {
            let encoded = serde_json::to_string(&key).unwrap();
            assert_eq!(encoded, format!("\"{name}\""));
            assert_eq!(serde_json::from_str::<KeyCode>(&encoded).unwrap(), key);
            assert_eq!(key.serialized_string_bytes(), name.len());
        }
    }
}
