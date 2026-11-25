use nara_input::{ActionMapDefinition, InputBinding, KeyCode};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};

/// Converts a Winit event's physical key without using its logical key or text.
#[must_use]
pub fn convert_physical_key(key: PhysicalKey) -> Option<KeyCode> {
    match key {
        PhysicalKey::Code(code) => convert_key_code(code),
        PhysicalKey::Unidentified(_) => None,
    }
}

macro_rules! physical_key_map {
    ($($native:ident $(=> $name:ident)?,)+) => {
        /// Converts a Winit key code known to this version of Nara.
        #[must_use]
        pub fn convert_key_code(code: WinitKeyCode) -> Option<KeyCode> {
            match code {
                $(WinitKeyCode::$native => Some(physical_key_map!(@key $native $(=> $name)?)),)+
                _ => None,
            }
        }

        pub(super) fn winit_key_code(key: KeyCode) -> Option<WinitKeyCode> {
            match key {
                $(physical_key_map!(@key $native $(=> $name)?) => Some(WinitKeyCode::$native),)+
                _ => None,
            }
        }

        #[cfg(test)]
        const MAPPED_WINIT_CODES: &[WinitKeyCode] = &[$(WinitKeyCode::$native,)+];
    };
    (@key $native:ident => $name:ident) => { KeyCode::$name };
    (@key $native:ident) => { KeyCode::$native };
}

physical_key_map! {
    Backquote,
    Backslash,
    BracketLeft,
    BracketRight,
    Comma,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Equal,
    IntlBackslash,
    IntlRo,
    IntlYen,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Minus,
    Period,
    Quote,
    Semicolon,
    Slash,
    AltLeft,
    AltRight,
    Backspace,
    CapsLock,
    ContextMenu,
    ControlLeft,
    ControlRight,
    Enter,
    SuperLeft => MetaLeft,
    SuperRight => MetaRight,
    ShiftLeft,
    ShiftRight,
    Space,
    Tab,
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
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
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
    Escape,
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

/// A keyboard binding outside the stock Winit dispatch map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedKeyboardBinding {
    /// Zero-based index in [`ActionMapDefinition::bindings`].
    pub index: usize,
    pub key: KeyCode,
}

impl std::fmt::Display for UnsupportedKeyboardBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "unsupported Winit key {:?} at binding index {}",
            self.key, self.index
        )
    }
}

impl std::error::Error for UnsupportedKeyboardBinding {}

/// Checks keyboard bindings against the stock Winit physical dispatch map.
///
/// Returns the first unsupported keyboard binding in definition order. Native hold
/// reconciliation after focus recovery has its own platform snapshot coverage.
pub fn validate_action_map_keyboard(
    definition: &ActionMapDefinition,
) -> Result<(), UnsupportedKeyboardBinding> {
    for (index, binding) in definition.bindings().iter().enumerate() {
        if let InputBinding::Key(key) = binding.input
            && winit_key_code(key).is_none()
        {
            return Err(UnsupportedKeyboardBinding { index, key });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_winit_codes_roundtrip_through_the_physical_map() {
        assert_eq!(MAPPED_WINIT_CODES.len(), 194);
        for &native in MAPPED_WINIT_CODES {
            let key = convert_physical_key(PhysicalKey::Code(native))
                .expect("every pinned Winit code has a Nara physical key");
            assert_eq!(winit_key_code(key), Some(native), "{key:?}");
        }
    }
}
