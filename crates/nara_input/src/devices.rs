use std::collections::{BTreeMap, btree_map::Entry};

use nara_core::Vec2;
use nara_ecs::{
    SystemParam,
    bevy_ecs::message::{Message, MessageWriter},
};

use crate::{GamepadInput, InputEdge, InputEdgeKind, InputPointerId, InputSeatId, InputWindowId};

/// Committed text from the current keyboard layout or input method.
#[derive(Debug, Clone, PartialEq, Eq, Message)]
pub struct TextInput {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub text: String,
}

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
    /// Nonempty commits also appear in [`TextInput`].
    Commit(String),
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseWheelUnit {
    Lines,
    Pixels,
}

/// A scroll sample with the pointer position at the time of the event.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct MouseWheel {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub pointer: InputPointerId,
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

/// A contact in logical window coordinates. Its ID is scoped to the window and seat.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct TouchInput {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub id: u64,
    pub phase: TouchPhase,
    pub position: Vec2,
    pub force: Option<f32>,
}

type Endpoint = (InputWindowId, InputSeatId);

#[derive(Default)]
pub(crate) struct WindowSamples {
    composing: BTreeMap<Endpoint, bool>,
    touches: BTreeMap<(Endpoint, u64), (Vec2, Option<f32>)>,
}

impl WindowSamples {
    pub(crate) fn accept(&mut self, edge: &mut InputEdge) -> bool {
        let endpoint = (edge.window, edge.seat);
        match &mut edge.kind {
            InputEdgeKind::Text(text) => {
                !text.is_empty() && self.composing.get(&endpoint) != Some(&true)
            }
            InputEdgeKind::Ime(kind) => match kind {
                ImeEventKind::Enabled => match self.composing.entry(endpoint) {
                    Entry::Vacant(entry) => {
                        entry.insert(false);
                        true
                    }
                    Entry::Occupied(_) => false,
                },
                ImeEventKind::Preedit { text, cursor } => {
                    *cursor = cursor.filter(|(start, end)| {
                        text.is_char_boundary(*start) && text.is_char_boundary(*end)
                    });
                    self.composing.insert(endpoint, !text.is_empty());
                    true
                }
                ImeEventKind::Commit(_) => {
                    if let Some(composing) = self.composing.get_mut(&endpoint) {
                        *composing = false;
                    }
                    true
                }
                ImeEventKind::Disabled => self.composing.remove(&endpoint).is_some(),
            },
            InputEdgeKind::Touch {
                id,
                phase,
                position,
                force,
            } => {
                let key = (endpoint, *id);
                *force = force.filter(|force| (0.0..=1.0).contains(force));
                match phase {
                    TouchPhase::Started if position.is_finite() => match self.touches.entry(key) {
                        Entry::Vacant(entry) => {
                            entry.insert((*position, *force));
                            true
                        }
                        Entry::Occupied(_) => false,
                    },
                    TouchPhase::Moved if position.is_finite() => {
                        let Some(contact) = self.touches.get_mut(&key) else {
                            return false;
                        };
                        *contact = (*position, *force);
                        true
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        let Some((previous, _)) = self.touches.remove(&key) else {
                            return false;
                        };
                        if !position.is_finite() {
                            *position = previous;
                        }
                        true
                    }
                    _ => false,
                }
            }
            _ => true,
        }
    }

    pub(crate) fn cancel(
        &mut self,
        window: InputWindowId,
        mut emit: impl FnMut(InputSeatId, InputEdgeKind),
    ) {
        for ((_, seat), _) in self
            .composing
            .extract_if(.., |(owner, _), _| *owner == window)
        {
            emit(seat, InputEdgeKind::Ime(ImeEventKind::Disabled));
        }
        for (((_, seat), id), (position, force)) in self
            .touches
            .extract_if(.., |((owner, _), _), _| *owner == window)
        {
            emit(
                seat,
                InputEdgeKind::Touch {
                    id,
                    phase: TouchPhase::Cancelled,
                    position,
                    force,
                },
            );
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct DeviceMessageWriters<'w> {
    wheel: MessageWriter<'w, MouseWheel>,
    gamepad: MessageWriter<'w, GamepadInput>,
    text: MessageWriter<'w, TextInput>,
    ime: MessageWriter<'w, ImeEvent>,
    touch: MessageWriter<'w, TouchInput>,
}

impl DeviceMessageWriters<'_> {
    pub(crate) fn write(&mut self, edge: &InputEdge) {
        let InputEdge { window, seat, kind } = edge;
        let (window, seat) = (*window, *seat);
        match kind {
            InputEdgeKind::Text(text) => {
                self.text.write(TextInput {
                    window,
                    seat,
                    text: text.clone(),
                });
            }
            InputEdgeKind::Ime(kind) => {
                if let ImeEventKind::Commit(text) = kind
                    && !text.is_empty()
                {
                    self.text.write(TextInput {
                        window,
                        seat,
                        text: text.clone(),
                    });
                }
                self.ime.write(ImeEvent {
                    window,
                    seat,
                    kind: kind.clone(),
                });
            }
            InputEdgeKind::Wheel {
                pointer,
                delta,
                unit,
                phase,
                position,
            } => {
                self.wheel.write(MouseWheel {
                    window,
                    seat,
                    pointer: *pointer,
                    position: *position,
                    delta: *delta,
                    unit: *unit,
                    phase: *phase,
                });
            }
            InputEdgeKind::Touch {
                id,
                phase,
                position,
                force,
            } => {
                self.touch.write(TouchInput {
                    window,
                    seat,
                    id: *id,
                    phase: *phase,
                    position: *position,
                    force: *force,
                });
            }
            InputEdgeKind::Gamepad { gamepad, kind } => {
                self.gamepad.write(GamepadInput {
                    window,
                    seat,
                    gamepad: *gamepad,
                    kind: *kind,
                });
            }
            _ => {}
        }
    }
}
