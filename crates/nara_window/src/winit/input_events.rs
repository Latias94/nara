use nara_core::Vec2;
use nara_input::{
    ButtonInputDriverError, ImeEvent, ImeEventKind, InputIngressError, InputPointerId, InputSeatId,
    InputWindowId, MouseWheelUnit, TextInput, TouchInput, TouchPhase, WindowDeviceInput,
    apply_mouse_wheel_driver_input_at, apply_window_device_input,
};
use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase as WinitTouchPhase};

use super::{AppRunError, RuntimeDriverScope, WindowId, WinitApp, WinitWindowEvent};

impl WinitApp {
    fn accepts_device_input(&self) -> bool {
        self.input_timeline_installed() && self.ordinary_input_enabled() && self.input_gate.focused
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
        let input = TextInput {
            window: InputWindowId::new(window_id.get()),
            seat: InputSeatId::PRIMARY,
            text: text.to_owned(),
        };
        self.submit_device_input(|scope| {
            apply_window_device_input(scope, WindowDeviceInput::Text(input))
        })
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
        let window = InputWindowId::new(window_id.get());
        let seat = InputSeatId::PRIMARY;
        match event {
            WinitWindowEvent::Ime(ime) => {
                let kind = match ime {
                    Ime::Enabled => ImeEventKind::Enabled,
                    Ime::Preedit(text, cursor) => ImeEventKind::Preedit { text, cursor },
                    Ime::Commit(text) => ImeEventKind::Commit(text),
                    Ime::Disabled => ImeEventKind::Disabled,
                };
                self.submit_device_input(|scope| {
                    apply_window_device_input(
                        scope,
                        WindowDeviceInput::Ime(ImeEvent { window, seat, kind }),
                    )
                })
            }
            WinitWindowEvent::MouseWheel { delta, phase, .. } => {
                let (delta, unit) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (Vec2::new(x, y), MouseWheelUnit::Lines),
                    MouseScrollDelta::PixelDelta(position) => {
                        (logical_position(position, scale), MouseWheelUnit::Pixels)
                    }
                };
                self.submit_device_input(|scope| {
                    apply_mouse_wheel_driver_input_at(
                        scope,
                        window,
                        seat,
                        InputPointerId::PRIMARY,
                        delta,
                        unit,
                        touch_phase(phase),
                    )
                    .map(|_| ())
                })
            }
            WinitWindowEvent::Touch(touch) => {
                let input = TouchInput {
                    window,
                    seat,
                    id: touch.id,
                    phase: touch_phase(touch.phase),
                    position: logical_position(touch.location, scale),
                    force: touch.force.map(|force| force.normalized() as f32),
                };
                self.submit_device_input(|scope| {
                    apply_window_device_input(scope, WindowDeviceInput::Touch(input))
                })
            }
            _ => Ok(()),
        }
    }

    fn submit_device_input(
        &mut self,
        operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> Result<(), ButtonInputDriverError>,
    ) -> Result<(), AppRunError> {
        self.with_driver_scope(|scope| match operation(scope) {
            Err(ButtonInputDriverError::Timeline(
                InputIngressError::CapacityExceeded { .. } | InputIngressError::NonFinitePosition,
            )) => Ok(()),
            result => result,
        })
    }
}

fn logical_position(position: winit::dpi::PhysicalPosition<f64>, scale: f64) -> Vec2 {
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
