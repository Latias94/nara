use dear_imgui_rs::{Context, Id};
use nara::{project_host::EditorGameStartRequest, window::WindowId as NaraWindowId};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Ime, WindowEvent},
    keyboard::PhysicalKey,
    window::WindowId,
};

use crate::{
    EditorApplication,
    frontend::imgui::{ImguiImageInteraction, ImguiPreedit, ImguiTextOwner},
};

use super::super::{
    EditorRunError,
    game_input::{EditorGameInputRouter, EditorGamePresentedStamp},
};

#[derive(Default)]
pub(super) struct ImguiInput {
    router: EditorGameInputRouter,
    game_window: Option<WindowId>,
    game_image: Option<ImguiImageInteraction>,
    cursor: Option<PhysicalPosition<f64>>,
    last_cursor_event: Option<(WindowId, WindowEvent)>,
    text_owner: Option<ImguiTextOwner>,
    text_window: Option<WindowId>,
    preedit: Option<ImguiPreedit>,
}

impl ImguiInput {
    #[cfg(test)]
    pub(super) fn text_owner_item(&self) -> Option<u32> {
        self.text_owner.map(|owner| owner.item.raw())
    }

    pub(super) fn is_captured(&self) -> bool {
        self.router.is_captured()
    }
    pub(super) fn preedit(&self) -> Option<&ImguiPreedit> {
        self.preedit.as_ref()
    }

    pub(super) fn handle_event(
        &mut self,
        window: WindowId,
        event: &WindowEvent,
        context: &Context,
    ) -> bool {
        if matches!(event, WindowEvent::Ime(Ime::Commit(_))) && self.text_window != Some(window) {
            return false;
        }
        match event {
            WindowEvent::CursorMoved { .. } => {
                self.last_cursor_event = Some((window, event.clone()))
            }
            WindowEvent::CursorLeft { .. }
            | WindowEvent::Focused(false)
            | WindowEvent::Destroyed
                if self
                    .last_cursor_event
                    .as_ref()
                    .is_some_and(|(owner, _)| *owner == window) =>
            {
                self.last_cursor_event = None;
            }
            _ => {}
        }
        if self.text_window == Some(window) {
            match event {
                WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                    self.preedit =
                        self.text_owner
                            .filter(|_| !text.is_empty())
                            .map(|owner| ImguiPreedit {
                                owner,
                                text: text.clone(),
                                cursor: *cursor,
                            });
                }
                WindowEvent::Ime(Ime::Commit(_) | Ime::Disabled) => self.preedit = None,
                WindowEvent::Focused(false)
                | WindowEvent::CloseRequested
                | WindowEvent::Destroyed => self.clear_text_owner(),
                _ => {}
            }
        }
        if self.game_window != Some(window) {
            return true;
        }
        match event {
            WindowEvent::KeyboardInput { event: key, .. } => {
                self.handle_key(window, key.physical_key, key.state, key.repeat)
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some(*position);
                self.release_if_pointer_is_obscured(context);
                self.router.handle_window_event(event)
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => {
                self.release_if_pointer_is_obscured(context);
                self.router.handle_window_event(event)
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                ..
            } => {
                self.router.handle_window_event(event);
                // Dear saw the press that requested Game capture. Always balance its button
                // state, including releases after the Runtime has taken pointer ownership.
                true
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.router.release_to_editor();
                self.router.handle_window_event(event);
                true
            }
            WindowEvent::Focused(false)
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::CloseRequested
            | WindowEvent::Destroyed => {
                self.router.handle_window_event(event);
                self.game_window = None;
                self.game_image = None;
                self.cursor = None;
                true
            }
            _ => self.router.handle_window_event(event),
        }
    }

    pub(super) fn handle_key(
        &mut self,
        window: WindowId,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> bool {
        if self.game_window != Some(window) {
            return true;
        }
        let route_to_editor = self.router.handle_key(physical_key, state, repeat);
        // Dear must balance a key pressed before Game capture, including a captured release.
        state == ElementState::Released || route_to_editor
    }

    pub(super) fn flush(
        &mut self,
        application: &mut EditorApplication,
    ) -> Result<(), EditorRunError> {
        let active = application.active_game_start_request();
        self.router.forget_retired_route(active);
        if application.ordinary_input_locked() {
            self.invalidate_after_control(active);
        }
        let Some(batch) = self.router.take_batch() else {
            return Ok(());
        };
        if let Err(error) = application.apply_game_view_input_batch(batch) {
            self.router.injection_failed();
            self.game_window = None;
            self.game_image = None;
            self.cursor = None;
            return Err(EditorRunError::Platform(format!(
                "the Game View input transaction was rejected: {error}"
            )));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn observe(
        &mut self,
        image: Option<ImguiImageInteraction>,
        text_owner: Option<ImguiTextOwner>,
        stamp: Option<EditorGamePresentedStamp>,
        primary_window: WindowId,
        primary_viewport: Id,
        secondary_windows: &[(Id, WindowId, NaraWindowId)],
        presentation_advanced: bool,
        ordinary_input_locked: bool,
        active: Option<EditorGameStartRequest>,
    ) {
        let text_window = text_owner.and_then(|owner| {
            native_window(
                owner.viewport,
                primary_window,
                primary_viewport,
                secondary_windows,
            )
            .map(|(window, _)| window)
        });
        if self.text_owner != text_owner || self.text_window != text_window {
            self.preedit = None;
        }
        self.text_owner = text_owner;
        self.text_window = text_window;
        if ordinary_input_locked {
            self.invalidate_after_control(active);
            return;
        }

        let presented = image.zip(stamp).and_then(|(image, stamp)| {
            native_window(
                image.viewport,
                primary_window,
                primary_viewport,
                secondary_windows,
            )
            .filter(|(_, window)| *window == stamp.window_id())
            .map(|(window, _)| (image, stamp, window))
        });
        let Some((image, stamp, window)) = presented else {
            self.invalidate(false);
            return;
        };
        if self.game_window != Some(window) {
            self.router.invalidate(false);
            self.cursor = self.last_cursor_event.as_ref().and_then(|(owner, event)| {
                if *owner != window {
                    return None;
                }
                let WindowEvent::CursorMoved { position, .. } = event else {
                    return None;
                };
                self.router.handle_window_event(event);
                Some(*position)
            });
            self.game_window = Some(window);
        }
        self.game_image = Some(image);
        self.router.update_presented(
            Some(stamp),
            Some(image.rect),
            image.pixels_per_point,
            presentation_advanced,
        );

        if !image.owns_pointer || text_owner.is_some() {
            self.router.release_to_editor();
            return;
        }
        if image.capture_requested && self.cursor.is_some() {
            self.router.capture_requested();
        }
    }

    pub(super) fn invalidate(&mut self, terminal: bool) {
        self.router.invalidate(terminal);
        self.game_window = None;
        self.game_image = None;
        self.cursor = None;
    }

    pub(super) fn mark_presented_stale(&mut self) {
        self.router.mark_presented_stale();
    }

    pub(super) fn invalidate_after_control(&mut self, active: Option<EditorGameStartRequest>) {
        self.router.invalidate_after_control(active);
        self.game_window = None;
        self.game_image = None;
        self.cursor = None;
        self.clear_text_owner();
    }

    fn release_if_pointer_is_obscured(&mut self, context: &Context) {
        if !self.router.is_captured() {
            return;
        }
        let owns_pointer = self
            .game_image
            .zip(self.cursor)
            .is_some_and(|(image, position)| {
                let scale = f64::from(image.pixels_per_point);
                if !scale.is_finite() || scale <= 0.0 {
                    return false;
                }
                let global = [
                    (position.x / scale) as f32 + image.viewport_origin[0],
                    (position.y / scale) as f32 + image.viewport_origin[1],
                ];
                image.owns_native_pointer(context, global)
            });
        if !owns_pointer {
            self.router.release_to_editor();
        }
    }

    fn clear_text_owner(&mut self) {
        self.text_owner = None;
        self.text_window = None;
        self.preedit = None;
    }
}

pub(super) fn native_window(
    viewport: Id,
    primary_window: WindowId,
    primary_viewport: Id,
    secondary_windows: &[(Id, WindowId, NaraWindowId)],
) -> Option<(WindowId, NaraWindowId)> {
    if viewport == primary_viewport {
        return Some((primary_window, NaraWindowId::PRIMARY));
    }
    secondary_windows
        .iter()
        .find_map(|&(id, window, nara)| (id == viewport).then_some((window, nara)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preedit_is_scoped_to_the_active_native_text_owner_and_never_committed() {
        let window = WindowId::from(11);
        let other = WindowId::from(12);
        let owner = ImguiTextOwner {
            item: Id::from(10),
            viewport: Id::from(20),
        };
        let mut input = ImguiInput {
            text_owner: Some(owner),
            text_window: Some(window),
            ..Default::default()
        };

        let context = Context::create();
        assert!(input.handle_event(
            other,
            &WindowEvent::Ime(Ime::Preedit("wrong".into(), None)),
            &context
        ));
        assert!(input.preedit().is_none());
        input.handle_event(
            window,
            &WindowEvent::Ime(Ime::Preedit("中文".into(), Some((0, 3)))),
            &context,
        );
        assert_eq!(input.preedit().unwrap().text, "中文");
        input.handle_event(
            window,
            &WindowEvent::Ime(Ime::Commit("中".into())),
            &context,
        );
        assert!(input.preedit().is_none());
        assert!(input.router.take_batch().is_none());
        input.handle_event(
            window,
            &WindowEvent::Ime(Ime::Preedit("文".into(), None)),
            &context,
        );
        input.handle_event(window, &WindowEvent::Focused(false), &context);
        assert!(input.preedit().is_none());
        assert!(input.text_owner.is_none());
    }

    #[test]
    fn secondary_ownership_requires_the_actual_prepared_native_mapping() {
        let primary = WindowId::from(11);
        let secondary = WindowId::from(12);
        let primary_viewport = Id::from(1);
        let viewport = Id::from(2);
        assert_eq!(
            native_window(viewport, primary, primary_viewport, &[]),
            None
        );
        assert_eq!(
            native_window(
                viewport,
                primary,
                primary_viewport,
                &[(viewport, secondary, NaraWindowId::new(7))]
            ),
            Some((secondary, NaraWindowId::new(7)))
        );
        assert_eq!(
            native_window(primary_viewport, primary, primary_viewport, &[]),
            Some((primary, NaraWindowId::PRIMARY))
        );
    }
}
