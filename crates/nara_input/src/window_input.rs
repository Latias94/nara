use std::collections::BTreeMap;

use nara_core::Vec2;
use nara_ecs::{
    ResMut, SystemParam,
    bevy_ecs::message::{Message, MessageWriter, Messages},
};

use crate::{GamepadInput, InputIngressError, InputRouteStamp, InputSeatId, InputWindowId};

/// Text committed by the current layout or input method, in the order it was entered.
/// Keyboard repeats also produce text. Read [`ImeEvent`] for composition and candidate UI.
#[derive(Debug, Clone, PartialEq, Eq, Message)]
pub struct TextInput {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub text: String,
}

/// Changes to a window's input method session. Disabling the session clears preedit.
#[derive(Debug, Clone, PartialEq, Eq, Message)]
pub struct ImeEvent {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub kind: ImeEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeEventKind {
    Enabled,
    Preedit {
        text: String,
        /// UTF-8 byte offsets in `text`. `None` hides the preedit cursor.
        cursor: Option<(usize, usize)>,
    },
    /// The same committed text also appears in [`TextInput`].
    Commit(String),
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseWheelUnit {
    Lines,
    Pixels,
}

/// A scroll sample with its route and pointer position captured at admission.
/// Pixel deltas and pointer positions are in logical window coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct MouseWheel {
    pub route: InputRouteStamp,
    pub position: Option<Vec2>,
    pub delta: Vec2,
    pub unit: MouseWheelUnit,
    pub phase: TouchPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// A contact sample in logical window coordinates. A contact ID is scoped to a window and seat.
/// Focus loss or window retirement cancels outstanding contacts.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct TouchInput {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub id: u64,
    pub phase: TouchPhase,
    pub position: Vec2,
    /// Normalized pressure in the range `0..=1`, when the device reports it.
    pub force: Option<f32>,
}

/// A transient sample from a connected platform window.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowDeviceInput {
    Text(TextInput),
    Ime(ImeEvent),
    Touch(TouchInput),
}

impl WindowDeviceInput {
    pub(crate) const fn endpoint(&self) -> (InputWindowId, InputSeatId) {
        match self {
            Self::Text(input) => (input.window, input.seat),
            Self::Ime(input) => (input.window, input.seat),
            Self::Touch(input) => (input.window, input.seat),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct WindowDeviceInputs {
    pending: Vec<WindowDeviceInput>,
    composing: BTreeMap<(InputWindowId, InputSeatId), bool>,
    touches: BTreeMap<(InputWindowId, InputSeatId, u64), TouchInput>,
}

impl WindowDeviceInputs {
    pub(crate) fn admit(
        &mut self,
        input: WindowDeviceInput,
        limit: usize,
    ) -> Result<(), InputIngressError> {
        let endpoint = input.endpoint();
        match input {
            WindowDeviceInput::Text(input) => {
                if !input.text.is_empty() && self.composing.get(&endpoint) != Some(&true) {
                    self.check_capacity(limit, 1)?;
                    self.pending.push(WindowDeviceInput::Text(input));
                }
            }
            WindowDeviceInput::Ime(mut event) => {
                let composing = self.composing.get(&endpoint).copied();
                match &mut event.kind {
                    ImeEventKind::Enabled if composing.is_some() => return Ok(()),
                    ImeEventKind::Enabled => {
                        self.check_capacity(limit, 2)?;
                        self.composing.insert(endpoint, false);
                    }
                    ImeEventKind::Preedit { text, cursor } => {
                        self.check_capacity(limit, 1 + usize::from(composing.is_none()))?;
                        *cursor = cursor.filter(|(start, end)| {
                            text.is_char_boundary(*start) && text.is_char_boundary(*end)
                        });
                        self.composing.insert(endpoint, !text.is_empty());
                    }
                    ImeEventKind::Commit(_) => {
                        self.check_capacity(limit, 1)?;
                        if let Some(composing) = self.composing.get_mut(&endpoint) {
                            *composing = false;
                        }
                    }
                    ImeEventKind::Disabled if self.composing.remove(&endpoint).is_none() => {
                        return Ok(());
                    }
                    ImeEventKind::Disabled => {}
                }
                self.pending.push(WindowDeviceInput::Ime(event));
            }
            WindowDeviceInput::Touch(mut input) => {
                let key = (input.window, input.seat, input.id);
                let previous = self.touches.get(&key).copied();
                match input.phase {
                    TouchPhase::Started if previous.is_none() => self.check_capacity(limit, 2)?,
                    TouchPhase::Moved if previous.is_some() => self.check_capacity(limit, 1)?,
                    TouchPhase::Ended | TouchPhase::Cancelled if previous.is_some() => {}
                    _ => return Ok(()),
                }
                input.force = input.force.filter(|force| (0.0..=1.0).contains(force));
                match input.phase {
                    TouchPhase::Started | TouchPhase::Moved => {
                        if !input.position.is_finite() {
                            return Err(InputIngressError::NonFinitePosition);
                        }
                        self.touches.insert(key, input);
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        if let Some(previous) = self.touches.remove(&key)
                            && !input.position.is_finite()
                        {
                            input.position = previous.position;
                        }
                    }
                }
                self.pending.push(WindowDeviceInput::Touch(input));
            }
        }
        Ok(())
    }

    fn check_capacity(&self, limit: usize, requested: usize) -> Result<(), InputIngressError> {
        let retained = self.pending.len() + self.composing.len() + self.touches.len();
        if retained + requested > limit {
            return Err(InputIngressError::CapacityExceeded {
                limit,
                retained,
                requested,
            });
        }
        Ok(())
    }

    pub(crate) fn cancel(&mut self, window: InputWindowId, seat: InputSeatId) {
        if self.composing.remove(&(window, seat)).is_some() {
            self.pending.push(WindowDeviceInput::Ime(ImeEvent {
                window,
                seat,
                kind: ImeEventKind::Disabled,
            }));
        }
        self.touches.retain(|(owner_window, owner_seat, _), input| {
            if (*owner_window, *owner_seat) != (window, seat) {
                return true;
            }
            self.pending.push(WindowDeviceInput::Touch(TouchInput {
                phase: TouchPhase::Cancelled,
                ..*input
            }));
            false
        });
    }

    pub(crate) fn drain(&mut self) -> impl Iterator<Item = WindowDeviceInput> + '_ {
        self.pending.drain(..)
    }
}

#[derive(SystemParam)]
pub(crate) struct DeviceMessageWriters<'w> {
    pub(crate) wheel: MessageWriter<'w, MouseWheel>,
    pub(crate) gamepad: MessageWriter<'w, GamepadInput>,
    text: MessageWriter<'w, TextInput>,
    ime: MessageWriter<'w, ImeEvent>,
    touch: MessageWriter<'w, TouchInput>,
}

impl DeviceMessageWriters<'_> {
    pub(crate) fn write_window(&mut self, input: WindowDeviceInput) {
        match input {
            WindowDeviceInput::Text(input) => {
                self.text.write(input);
            }
            WindowDeviceInput::Ime(input) => {
                if let ImeEventKind::Commit(text) = &input.kind
                    && !text.is_empty()
                {
                    self.text.write(TextInput {
                        window: input.window,
                        seat: input.seat,
                        text: text.clone(),
                    });
                }
                self.ime.write(input);
            }
            WindowDeviceInput::Touch(input) => {
                self.touch.write(input);
            }
        }
    }
}

pub(crate) fn advance_device_messages(
    mut wheel: ResMut<Messages<MouseWheel>>,
    mut gamepad: ResMut<Messages<GamepadInput>>,
    mut text: ResMut<Messages<TextInput>>,
    mut ime: ResMut<Messages<ImeEvent>>,
    mut touch: ResMut<Messages<TouchInput>>,
) {
    wheel.update();
    gamepad.update();
    text.update();
    ime.update();
    touch.update();
}
