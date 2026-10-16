use std::collections::BTreeSet;

use egui::Rect;
use nara::{
    input::{ButtonDriverInput, KeyCode, MouseButton, PointerDriverInput},
    prelude::Vec2,
    project_host::{EditorGameInputBatch, EditorGameInputEnvelope, EditorGameInputEvent},
    render::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d},
    winit::{convert_mouse_button, convert_physical_key},
};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, WindowEvent},
    keyboard::{KeyCode as WinitKeyCode, PhysicalKey},
};

use super::viewport::EditorGamePresentedStamp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EditorGameInputLease {
    view_id: nara::render_wgpu::__private::WgpuEditorViewId,
    window_id: nara::window::WindowId,
    runtime_generation: u64,
    source_handoff_generation: u64,
    target_generation: u64,
    geometry_generation: u64,
    route_generation: u64,
}

impl EditorGameInputLease {
    fn from_stamp(stamp: EditorGamePresentedStamp, route_generation: u64) -> Self {
        Self {
            view_id: stamp.view_id(),
            window_id: stamp.window_id(),
            runtime_generation: stamp.runtime_generation(),
            source_handoff_generation: stamp.source_handoff_generation(),
            target_generation: stamp.target_generation(),
            geometry_generation: stamp.geometry_generation(),
            route_generation,
        }
    }

    fn accepts(self, stamp: EditorGamePresentedStamp) -> bool {
        self.view_id == stamp.view_id()
            && self.window_id == stamp.window_id()
            && self.runtime_generation == stamp.runtime_generation()
            && self.source_handoff_generation == stamp.source_handoff_generation()
            && self.target_generation == stamp.target_generation()
            && self.geometry_generation == stamp.geometry_generation()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EditorGameContentTransform {
    mapping: DisplayMapping2d,
    logical_scale_factor: f32,
}

impl EditorGameContentTransform {
    fn from_points(
        rect: Rect,
        pixels_per_point: f32,
        stamp: EditorGamePresentedStamp,
    ) -> Option<Self> {
        if !rect.is_finite() || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
            return None;
        }
        let window = stamp.logical_window();
        let scale_factor = window.resolution.scale_factor as f32;
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return None;
        }
        let target = Extent2d::new(
            window.resolution.physical_width,
            window.resolution.physical_height,
        )?;
        let mapping = DisplayMapping2d::fit(
            target,
            DisplayRect {
                min: [rect.min.x, rect.min.y],
                max: [rect.max.x, rect.max.y],
            },
            pixels_per_point,
            DisplayScale::Fit,
        )?;
        Some(Self {
            mapping,
            logical_scale_factor: scale_factor,
        })
    }

    fn map(self, position: PhysicalPosition<f64>) -> Option<Vec2> {
        let physical = self.mapping.pixels_per_point();
        let target = self.mapping.panel_to_target([
            (position.x / f64::from(physical)) as f32,
            (position.y / f64::from(physical)) as f32,
        ])?;
        Some(Vec2::new(
            target[0] / self.logical_scale_factor,
            target[1] / self.logical_scale_factor,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
struct EditorGamePresentedInput {
    stamp: EditorGamePresentedStamp,
    lease: EditorGameInputLease,
    transform: EditorGameContentTransform,
}

#[derive(Debug, Default)]
enum EditorGameCaptureState {
    #[default]
    Released,
    Captured {
        stamp: EditorGamePresentedStamp,
        lease: EditorGameInputLease,
        held_keys: BTreeSet<KeyCode>,
        held_mouse_buttons: BTreeSet<MouseButton>,
    },
}

#[derive(Debug, Default)]
pub(super) struct EditorGameInputRouter {
    presented: Option<EditorGamePresentedInput>,
    input_enabled: bool,
    capture: EditorGameCaptureState,
    cursor_position: Option<PhysicalPosition<f64>>,
    pending: Vec<EditorGameInputEnvelope>,
    next_route_generation: u64,
    focus_restore_requested: bool,
}

impl EditorGameInputRouter {
    pub(super) const fn cursor_position(&self) -> Option<PhysicalPosition<f64>> {
        self.cursor_position
    }

    pub(super) fn is_captured(&self) -> bool {
        matches!(self.capture, EditorGameCaptureState::Captured { .. })
    }

    pub(super) fn update_presented(
        &mut self,
        stamp: Option<EditorGamePresentedStamp>,
        content_rect_points: Option<Rect>,
        pixels_per_point: f32,
        presentation_advanced: bool,
    ) {
        let next = stamp.zip(content_rect_points).and_then(|(stamp, rect)| {
            EditorGameContentTransform::from_points(rect, pixels_per_point, stamp)
                .map(|transform| (stamp, transform))
        });
        let Some((stamp, transform)) = next else {
            self.presented = None;
            self.input_enabled = false;
            return;
        };
        let retained_input_authority = self.input_enabled
            && self
                .presented
                .is_some_and(|current| current.lease.accepts(stamp));

        let captured_route = match &self.capture {
            EditorGameCaptureState::Captured { lease, .. } if lease.accepts(stamp) => {
                Some(lease.route_generation)
            }
            EditorGameCaptureState::Captured { .. } | EditorGameCaptureState::Released => None,
        };
        let incompatible_capture = matches!(
            &self.capture,
            EditorGameCaptureState::Captured { lease, .. } if !lease.accepts(stamp)
        );
        if incompatible_capture {
            self.release(false);
        }
        let route_generation = if let Some(route_generation) = captured_route {
            let EditorGameCaptureState::Captured {
                stamp: captured_stamp,
                ..
            } = &mut self.capture
            else {
                unreachable!("a compatible capture remains installed")
            };
            *captured_stamp = stamp;
            route_generation
        } else if let Some(current) = self
            .presented
            .filter(|current| current.lease.accepts(stamp))
        {
            current.lease.route_generation
        } else {
            self.allocate_route_generation()
        };
        self.presented = Some(EditorGamePresentedInput {
            stamp,
            lease: EditorGameInputLease::from_stamp(stamp, route_generation),
            transform,
        });
        self.input_enabled = presentation_advanced || retained_input_authority;
    }

    pub(super) fn mark_presented_stale(&mut self) {
        self.input_enabled = false;
    }

    pub(super) fn invalidate(&mut self, terminal: bool) {
        self.presented = None;
        self.input_enabled = false;
        self.release(terminal);
    }

    pub(super) fn invalidate_after_control(&mut self, terminal: bool) {
        self.pending.clear();
        self.invalidate(terminal);
    }

    pub(super) fn release_to_editor(&mut self) {
        self.release(false);
        self.focus_restore_requested = false;
    }

    pub(super) fn capture_requested(&mut self) -> bool {
        if self.is_captured() || !self.input_enabled {
            return false;
        }
        let Some(presented) = self.presented else {
            return false;
        };
        let Some(position) = self.cursor_position else {
            return false;
        };
        let Some(logical_position) = presented.transform.map(position) else {
            return false;
        };
        self.capture = EditorGameCaptureState::Captured {
            stamp: presented.stamp,
            lease: presented.lease,
            held_keys: BTreeSet::new(),
            held_mouse_buttons: BTreeSet::new(),
        };
        self.focus_restore_requested = false;
        self.push(
            presented.stamp,
            presented.lease,
            EditorGameInputEvent::Focused(true),
        );
        self.push(
            presented.stamp,
            presented.lease,
            EditorGameInputEvent::Pointer(PointerDriverInput::Moved(logical_position)),
        );
        true
    }

    pub(super) fn capture_and_press_key(&mut self, key: KeyCode) -> bool {
        let Some(presented) = self.presented else {
            return false;
        };
        let center = presented
            .transform
            .mapping
            .uv_to_panel([0.5, 0.5])
            .expect("the full Game image contains its center");
        let physical = presented.transform.mapping.pixels_per_point();
        self.cursor_position = Some(PhysicalPosition::new(
            f64::from(center[0] * physical),
            f64::from(center[1] * physical),
        ));
        if !self.capture_requested() || !self.input_enabled {
            return false;
        }
        self.apply_captured_key_transition(key, ElementState::Pressed);
        true
    }

    pub(super) fn take_focus_restore_request(&mut self, game_panel_active: bool) -> bool {
        let requested = std::mem::take(&mut self.focus_restore_requested);
        requested && game_panel_active
    }

    /// Returns whether the event should continue to egui.
    pub(super) fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event, .. } if self.is_captured() => {
                if event.physical_key == PhysicalKey::Code(WinitKeyCode::Escape)
                    && event.state == ElementState::Pressed
                    && !event.repeat
                {
                    self.release(false);
                    return false;
                }
                let Some(key) = convert_physical_key(event.physical_key) else {
                    return false;
                };
                if event.repeat {
                    return false;
                }
                if !self.input_enabled {
                    if event.state == ElementState::Released && self.captured_key_is_held(key) {
                        self.apply_captured_key_transition(key, event.state);
                    }
                    return false;
                }
                self.apply_captured_key_transition(key, event.state);
                false
            }
            WindowEvent::MouseInput { state, button, .. } if self.is_captured() => {
                let button = convert_mouse_button(*button);
                let inside = self
                    .presented
                    .zip(self.cursor_position)
                    .and_then(|(presented, position)| presented.transform.map(position))
                    .is_some();
                if *state == ElementState::Pressed && !inside {
                    self.release_to_editor();
                    return true;
                }
                if !self.input_enabled {
                    let was_held = matches!(
                        &self.capture,
                        EditorGameCaptureState::Captured {
                            held_mouse_buttons,
                            ..
                        } if held_mouse_buttons.contains(&button)
                    );
                    if *state == ElementState::Released && was_held {
                        self.push_captured(EditorGameInputEvent::Mouse(
                            ButtonDriverInput::Release(button),
                        ));
                        if let EditorGameCaptureState::Captured {
                            held_mouse_buttons, ..
                        } = &mut self.capture
                        {
                            held_mouse_buttons.remove(&button);
                        }
                    }
                    return false;
                }
                let inside = self
                    .presented
                    .zip(self.cursor_position)
                    .and_then(|(presented, position)| presented.transform.map(position))
                    .is_some();
                let has_held_buttons = matches!(
                    &self.capture,
                    EditorGameCaptureState::Captured {
                        held_mouse_buttons,
                        ..
                    } if !held_mouse_buttons.is_empty()
                );
                if !inside {
                    if *state == ElementState::Pressed || has_held_buttons {
                        self.release(false);
                    }
                    return true;
                }
                let input = match state {
                    ElementState::Pressed => ButtonDriverInput::Press(button),
                    ElementState::Released => ButtonDriverInput::Release(button),
                };
                self.push_captured(EditorGameInputEvent::Mouse(input));
                if let EditorGameCaptureState::Captured {
                    held_mouse_buttons, ..
                } = &mut self.capture
                {
                    match state {
                        ElementState::Pressed => {
                            held_mouse_buttons.insert(button);
                        }
                        ElementState::Released => {
                            held_mouse_buttons.remove(&button);
                        }
                    }
                }
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = Some(*position);
                if !self.is_captured() {
                    return true;
                }
                let logical = self
                    .presented
                    .and_then(|presented| presented.transform.map(*position));
                if let Some(logical) = logical {
                    if self.input_enabled {
                        self.push_captured(EditorGameInputEvent::Pointer(
                            PointerDriverInput::Moved(logical),
                        ));
                    }
                    return false;
                }
                let has_held_buttons = matches!(
                    &self.capture,
                    EditorGameCaptureState::Captured {
                        held_mouse_buttons,
                        ..
                    } if !held_mouse_buttons.is_empty()
                );
                if has_held_buttons {
                    self.release(false);
                }
                true
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_position = None;
                let has_held_buttons = matches!(
                    &self.capture,
                    EditorGameCaptureState::Captured {
                        held_mouse_buttons,
                        ..
                    } if !held_mouse_buttons.is_empty()
                );
                if has_held_buttons {
                    self.release(false);
                }
                true
            }
            WindowEvent::Focused(false) => {
                self.invalidate(false);
                true
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.invalidate(false);
                true
            }
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.invalidate(true);
                true
            }
            _ => true,
        }
    }

    pub(super) fn take_batch(&mut self) -> Option<EditorGameInputBatch> {
        (!self.pending.is_empty())
            .then(|| EditorGameInputBatch::new(std::mem::take(&mut self.pending)))
    }

    pub(super) fn injection_failed(&mut self) {
        self.pending.clear();
        self.presented = None;
        self.input_enabled = false;
        self.capture = EditorGameCaptureState::Released;
        self.focus_restore_requested = false;
    }

    fn push_captured(&mut self, event: EditorGameInputEvent) -> bool {
        let EditorGameCaptureState::Captured { stamp, lease, .. } = &self.capture else {
            return false;
        };
        let stamp = *stamp;
        let lease = *lease;
        self.push(stamp, lease, event)
    }

    fn push(
        &mut self,
        stamp: EditorGamePresentedStamp,
        lease: EditorGameInputLease,
        event: EditorGameInputEvent,
    ) -> bool {
        if self.pending.len() >= EditorGameInputBatch::MAX_EVENTS {
            self.pending.clear();
            self.presented = None;
            self.input_enabled = false;
            let was_captured = self.is_captured();
            self.capture = EditorGameCaptureState::Released;
            self.focus_restore_requested |= was_captured;
            let cleanup = if matches!(event, EditorGameInputEvent::SurfaceRetired) {
                EditorGameInputEvent::SurfaceRetired
            } else {
                EditorGameInputEvent::Focused(false)
            };
            self.pending.push(EditorGameInputEnvelope::new(
                stamp.identity(),
                lease.route_generation,
                cleanup,
            ));
            return false;
        }
        self.pending.push(EditorGameInputEnvelope::new(
            stamp.identity(),
            lease.route_generation,
            event,
        ));
        true
    }

    fn captured_key_is_held(&self, key: KeyCode) -> bool {
        matches!(
            &self.capture,
            EditorGameCaptureState::Captured { held_keys, .. } if held_keys.contains(&key)
        )
    }

    fn apply_captured_key_transition(&mut self, key: KeyCode, state: ElementState) {
        let input = match state {
            ElementState::Pressed => ButtonDriverInput::Press(key),
            ElementState::Released => ButtonDriverInput::Release(key),
        };
        if !self.push_captured(EditorGameInputEvent::Keyboard(input)) {
            return;
        }
        let EditorGameCaptureState::Captured { held_keys, .. } = &mut self.capture else {
            return;
        };
        match state {
            ElementState::Pressed => {
                held_keys.insert(key);
            }
            ElementState::Released => {
                held_keys.remove(&key);
            }
        }
    }

    fn release(&mut self, terminal: bool) {
        let previous = std::mem::replace(&mut self.capture, EditorGameCaptureState::Released);
        let EditorGameCaptureState::Captured { stamp, lease, .. } = previous else {
            return;
        };
        self.focus_restore_requested = true;
        self.push(
            stamp,
            lease,
            if terminal {
                EditorGameInputEvent::SurfaceRetired
            } else {
                EditorGameInputEvent::Focused(false)
            },
        );
    }

    fn allocate_route_generation(&mut self) -> u64 {
        self.next_route_generation = self
            .next_route_generation
            .checked_add(1)
            .expect("the private Game input route generation space is exhausted");
        self.next_route_generation
    }
}

#[cfg(test)]
mod tests {
    use super::EditorGameContentTransform;
    use winit::dpi::PhysicalPosition;

    #[test]
    fn content_transform_rejects_bars_and_maps_the_exact_logical_extent() {
        let transform = EditorGameContentTransform {
            mapping: nara::render::DisplayMapping2d::fit(
                nara::render::Extent2d::new(960, 540).unwrap(),
                nara::render::DisplayRect {
                    min: [100.0, 50.0],
                    max: [900.0, 500.0],
                },
                1.0,
                nara::render::DisplayScale::Fit,
            )
            .unwrap(),
            logical_scale_factor: 1.0,
        };

        assert!(transform.map(PhysicalPosition::new(50.0, 200.0)).is_none());
        assert_eq!(
            transform.map(PhysicalPosition::new(500.0, 275.0)),
            Some(nara::prelude::Vec2::new(480.0, 270.0))
        );
    }
}
