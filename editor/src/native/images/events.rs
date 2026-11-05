use winit::{
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::{Key, ModifiersState, PhysicalKey},
};

use super::EditorPanelImageError;

/// A public keyboard value that can be constructed without Winit's private `KeyEvent` fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorPanelKeyEvent {
    pub logical_key: Key,
    pub physical_key: Option<PhysicalKey>,
    pub state: ElementState,
    pub repeat: bool,
    pub synthetic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditorPanelWheelDelta {
    Lines([f32; 2]),
    /// Original native physical-pixel scroll delta, without line conversion or DPI rounding.
    Pixels([f64; 2]),
}

/// Normalized events for exactly this Host's original native window.
///
/// Positions and pixel-wheel deltas retain native physical units. Callers filter the Winit
/// window ID before forwarding an event; this value contains no device or event-loop ownership.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorPanelPlatformEvent {
    PointerMoved {
        physical_position: [f64; 2],
    },
    PointerEntered,
    PointerLeft,
    PointerButton {
        button: MouseButton,
        state: ElementState,
    },
    Wheel {
        delta: EditorPanelWheelDelta,
        phase: TouchPhase,
    },
    Keyboard {
        key: EditorPanelKeyEvent,
        text: Option<String>,
    },
    /// Explicit text insertion; never inferred from a logical key label.
    Text(String),
    ModifiersChanged(ModifiersState),
    Ime(Ime),
    Focused(bool),
    Resized([u32; 2]),
    ScaleFactorChanged(f64),
    CloseRequested,
    Destroyed,
}

impl EditorPanelPlatformEvent {
    /// Normalizes the supported native event vocabulary, including original key-produced text.
    #[must_use]
    pub fn from_window_event(event: &WindowEvent) -> Option<Self> {
        Some(match event {
            WindowEvent::CursorMoved { position, .. } => Self::PointerMoved {
                physical_position: [position.x, position.y],
            },
            WindowEvent::CursorEntered { .. } => Self::PointerEntered,
            WindowEvent::CursorLeft { .. } => Self::PointerLeft,
            WindowEvent::MouseInput { button, state, .. } => Self::PointerButton {
                button: *button,
                state: *state,
            },
            WindowEvent::MouseWheel { delta, phase, .. } => Self::Wheel {
                delta: match delta {
                    MouseScrollDelta::LineDelta(x, y) => EditorPanelWheelDelta::Lines([*x, *y]),
                    MouseScrollDelta::PixelDelta(position) => {
                        EditorPanelWheelDelta::Pixels([position.x, position.y])
                    }
                },
                phase: *phase,
            },
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => Self::Keyboard {
                key: EditorPanelKeyEvent {
                    logical_key: event.logical_key.clone(),
                    physical_key: Some(event.physical_key),
                    state: event.state,
                    repeat: event.repeat,
                    synthetic: *is_synthetic,
                },
                text: event.text.as_ref().map(ToString::to_string),
            },
            WindowEvent::ModifiersChanged(modifiers) => Self::ModifiersChanged(modifiers.state()),
            WindowEvent::Ime(ime) => Self::Ime(ime.clone()),
            WindowEvent::Focused(focused) => Self::Focused(*focused),
            WindowEvent::Resized(size) => Self::Resized([size.width, size.height]),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                Self::ScaleFactorChanged(*scale_factor)
            }
            WindowEvent::CloseRequested => Self::CloseRequested,
            WindowEvent::Destroyed => Self::Destroyed,
            _ => return None,
        })
    }

    /// Constructs injected logical keyboard input, retaining explicit produced text separately.
    #[must_use]
    pub fn logical_key(
        logical_key: Key,
        state: ElementState,
        repeat: bool,
        text: Option<String>,
    ) -> Self {
        Self::Keyboard {
            key: EditorPanelKeyEvent {
                logical_key,
                physical_key: None,
                state,
                repeat,
                synthetic: false,
            },
            text,
        }
    }

    pub(super) fn validate(&self) -> Result<(), EditorPanelImageError> {
        let valid = match self {
            Self::PointerMoved { physical_position } => {
                physical_position.iter().all(|value| value.is_finite())
            }
            Self::Wheel {
                delta: EditorPanelWheelDelta::Lines(delta),
                ..
            } => delta.iter().all(|value| value.is_finite()),
            Self::Wheel {
                delta: EditorPanelWheelDelta::Pixels(delta),
                ..
            } => delta.iter().all(|value| value.is_finite()),
            Self::ScaleFactorChanged(scale) => scale.is_finite() && *scale > 0.0,
            Self::Ime(Ime::Preedit(text, Some((start, end)))) => {
                start <= end && text.is_char_boundary(*start) && text.is_char_boundary(*end)
            }
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(EditorPanelImageError::InvalidEvent)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorPanelCancelReason {
    AttachmentRetired,
    OutputReplaced,
    WindowGeometryChanged,
    FocusLost,
    WindowClosed,
    NativeInvalidated,
    KeyboardTransferred,
    ModalAdmitted,
    ApplicationInputLocked,
}

/// Frontend input vocabulary in delivery order. Key text follows its key as a separate `Text`
/// delivery, and only reaches a keyboard owner that explicitly enabled text input.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorPanelInputKind {
    /// An explicit provider-defined request admitted through an exact image observation.
    /// The caller captures its typed payload in the ordinary input callback; the provider
    /// resolves it against this delivery's original snapshot and its normal behavior owner.
    ObservedOperation,
    PointerMoved,
    PointerLeft,
    PointerButton {
        button: MouseButton,
        state: ElementState,
    },
    /// A modal provider may close its own popup after an outside click. The Host consumes it
    /// across providers; it grants no out-of-panel drawing or pointer-capture permission.
    ModalOutsidePointer {
        button: MouseButton,
        state: ElementState,
    },
    Wheel {
        delta: EditorPanelWheelDelta,
        phase: TouchPhase,
    },
    /// The Host granted keyboard ownership through an explicit panel focus request. The frontend
    /// chooses its local focus and may request a text-input scope; native IME readiness is separate.
    KeyboardFocused,
    Key(EditorPanelKeyEvent),
    Text(String),
    ModifiersChanged,
    ImeEnabled,
    ImeDisabled,
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit(String),
    Cancelled(EditorPanelCancelReason),
}
