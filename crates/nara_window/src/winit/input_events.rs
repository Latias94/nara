use nara_core::Vec2;
use nara_input::{ImeEventKind, InputEdgeKind, InputPointerId, MouseWheelUnit, TouchPhase};
use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase as WinitTouchPhase};

use super::{AppRunError, WindowId, WinitApp, WinitWindowEvent};

impl WinitApp {
    fn accepts_device_input(&self) -> bool {
        self.input_installed() && self.input_focused
    }

    pub(super) fn apply_text_input(
        &mut self,
        window_id: WindowId,
        state: ElementState,
        text: Option<&str>,
        is_synthetic: bool,
    ) -> Result<(), AppRunError> {
        if !self.accepts_device_input() || is_synthetic || state != ElementState::Pressed {
            return Ok(());
        }
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return Ok(());
        };
        self.submit_input(window_id, InputEdgeKind::Text(text.to_owned()))?;
        Ok(())
    }

    pub(super) fn apply_native_device_event(
        &mut self,
        window_id: WindowId,
        event: WinitWindowEvent,
    ) -> Result<(), AppRunError> {
        if !self.accepts_device_input() {
            return Ok(());
        }
        let scale = self
            .platform_windows
            .get(&window_id)
            .map_or(1.0, |window| window.scale_factor());
        let kind = match event {
            WinitWindowEvent::Ime(ime) => {
                let kind = match ime {
                    Ime::Enabled => ImeEventKind::Enabled,
                    Ime::Preedit(text, cursor) => ImeEventKind::Preedit { text, cursor },
                    Ime::Commit(text) => ImeEventKind::Commit(text),
                    Ime::Disabled => ImeEventKind::Disabled,
                };
                InputEdgeKind::Ime(kind)
            }
            WinitWindowEvent::MouseWheel { delta, phase, .. } => {
                let (delta, unit) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (Vec2::new(x, y), MouseWheelUnit::Lines),
                    MouseScrollDelta::PixelDelta(position) => {
                        (logical_position(position, scale), MouseWheelUnit::Pixels)
                    }
                };
                InputEdgeKind::Wheel {
                    pointer: InputPointerId::PRIMARY,
                    delta,
                    unit,
                    phase: touch_phase(phase),
                    position: None,
                }
            }
            WinitWindowEvent::Touch(touch) => InputEdgeKind::Touch {
                id: touch.id,
                phase: touch_phase(touch.phase),
                position: logical_position(touch.location, scale),
                force: touch.force.map(|force| force.normalized() as f32),
            },
            _ => return Ok(()),
        };
        self.submit_input(window_id, kind)?;
        Ok(())
    }
}

pub(super) fn logical_position(position: winit::dpi::PhysicalPosition<f64>, scale: f64) -> Vec2 {
    Vec2::new((position.x / scale) as f32, (position.y / scale) as f32)
}

const fn touch_phase(phase: WinitTouchPhase) -> TouchPhase {
    match phase {
        WinitTouchPhase::Started => TouchPhase::Started,
        WinitTouchPhase::Moved => TouchPhase::Moved,
        WinitTouchPhase::Ended => TouchPhase::Ended,
        WinitTouchPhase::Cancelled => TouchPhase::Cancelled,
    }
}

#[cfg(test)]
mod tests;
