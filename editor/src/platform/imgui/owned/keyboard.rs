use nara_text::TextEdit;
use nara_ui_kernel::{UiActivationSource, UiInputEvent, UiKey, UiNavigation, UiSeatId};
use winit::keyboard::{Key, NamedKey};

use super::*;

impl OwnedPanelSurface {
    fn focused_control(&self) -> Result<Option<UiKey>, EditorRunError> {
        let Some(route) = self.route else {
            return Ok(None);
        };
        Ok(self
            .interaction
            .semantic_snapshot(&self.journal, route, UiSeatId::PRIMARY)
            .map_err(imgui_error)?
            .nodes()
            .iter()
            .find(|node| {
                node.state().focused() && node.state().enabled() && node.state().focusable()
            })
            .map(|node| node.key().clone()))
    }
}

impl NativeImgui {
    #[cfg(test)]
    pub(in crate::platform::imgui) fn owned_focused_control_for_test(
        &self,
    ) -> Option<(EditorPanelInstanceId, UiKey)> {
        let (panel, _) = self.owned_input.keyboard?;
        Some((panel, self.owned.get(&panel)?.focused_control().ok()??))
    }

    pub(super) fn sync_owned_keyboard_focus(
        &mut self,
        panel: EditorPanelInstanceId,
        window: WindowId,
    ) -> Result<(), EditorRunError> {
        let Some(surface) = self.owned.get(&panel) else {
            return self.cancel_owned_focus();
        };
        let Some(frame) = surface
            .presented
            .as_ref()
            .cloned()
            .filter(|_| surface.route.is_some())
        else {
            return self.cancel_owned_focus();
        };
        let field = surface.focused_control()?;
        let text = field.and_then(|field| {
            self.frontend
                .owned_field_token(panel, &frame, &field)
                .map(|token| (field, token))
        });
        let unchanged = self.owned_input.focus.as_ref().is_some_and(|focus| {
            text.as_ref().is_some_and(|(field, token)| {
                focus.panel == panel
                    && focus.window == window
                    && focus.field == *field
                    && focus.token == *token
            })
        });
        if !unchanged {
            self.release_owned_focus(Some(panel))?;
            if let Some((field, token)) = text {
                self.context.io_mut().clear_non_modifier_keys();
                self.input.invalidate(false);
                self.platform.set_ime_auto_management(false);
                self.platform.set_ime_allowed(false).map_err(imgui_error)?;
                self.platform.set_ime_allowed(true).map_err(imgui_error)?;
                self.owned_input.focus = Some(OwnedTextFocus {
                    panel,
                    frame,
                    field,
                    token,
                    window,
                });
            }
        } else if let Some(focus) = &mut self.owned_input.focus {
            focus.frame = frame;
        }
        if self.owned_input.keyboard != Some((panel, window)) {
            self.context.io_mut().clear_non_modifier_keys();
            self.input.invalidate(false);
        }
        self.owned_input.keyboard = Some((panel, window));
        self.update_owned_ime_area()
    }

    fn dispatch_owned_key(
        &mut self,
        panel: EditorPanelInstanceId,
        event: UiInputEvent,
        application: &crate::EditorApplication,
    ) -> Result<(), EditorRunError> {
        if let Some((frame, outcome)) = self
            .owned
            .get_mut(&panel)
            .expect("keyboard panel exists")
            .dispatch(event)?
            && let Some(batch) = outcome.consumed()
            && let Some(model) = application.current_frame()
        {
            let clipboard = self.clipboard.clone();
            let _ = self.frontend.reduce_owned_input(
                panel,
                &frame,
                batch,
                &model,
                application.current_ui_authoring_frame().as_deref(),
                &mut |text| clipboard.write(text),
            );
        }
        Ok(())
    }

    pub(super) fn edit_owned_text(&mut self, focus: &OwnedTextFocus, action: &TextEdit) {
        if let Err(error) =
            self.frontend
                .owned_edit(focus.panel, &focus.frame, &focus.field, focus.token, action)
        {
            self.frontend.owned_input_error(
                focus.panel,
                &focus.frame,
                &focus.field,
                focus.token,
                error,
            );
        }
    }

    fn owned_clipboard_key(&mut self, focus: &OwnedTextFocus, key: &str) -> Result<(), String> {
        if key.eq_ignore_ascii_case("v") {
            let text = self.clipboard.read()?;
            return self.frontend.owned_edit(
                focus.panel,
                &focus.frame,
                &focus.field,
                focus.token,
                &TextEdit::Insert(text),
            );
        }
        if let Some(text) = self.frontend.owned_selected_text(
            focus.panel,
            &focus.frame,
            &focus.field,
            focus.token,
        )? {
            self.clipboard.write(&text)?;
            if key.eq_ignore_ascii_case("x") {
                self.frontend.owned_edit(
                    focus.panel,
                    &focus.frame,
                    &focus.field,
                    focus.token,
                    &TextEdit::Insert(String::new()),
                )?;
            }
        }
        Ok(())
    }

    pub(in crate::platform::imgui) fn owned_key(
        &mut self,
        window: WindowId,
        key: Key<&str>,
        text: Option<&str>,
        state: ElementState,
        repeat: bool,
        application: &crate::EditorApplication,
    ) -> Result<bool, EditorRunError> {
        let Some((panel, owner)) = self
            .owned_input
            .keyboard
            .filter(|(_, owner)| *owner == window)
        else {
            return Ok(false);
        };
        if application.ordinary_input_locked()
            || self.owned.get(&panel).is_none_or(|surface| {
                surface.route.is_none() || surface.mapping.is_none_or(|(window, _)| window != owner)
            })
        {
            self.cancel_owned_focus()?;
            return Ok(true);
        }
        self.sync_owned_keyboard_focus(panel, window)?;
        if state != ElementState::Pressed {
            return Ok(true);
        }
        let focus = self.owned_input.focus.clone();
        if let Some(focus) = &focus
            && self
                .frontend
                .owned_composing(focus.panel, &focus.frame, &focus.field, focus.token)
        {
            if key == Key::Named(NamedKey::Escape) {
                self.edit_owned_text(focus, &TextEdit::CancelComposition);
            }
            return Ok(true);
        }
        let modifiers = self.owned_input.modifiers;
        let extend = modifiers.shift_key();
        let command = if cfg!(target_os = "macos") {
            modifiers.super_key()
        } else {
            modifiers.control_key() && !modifiers.alt_key()
        };
        let word = if cfg!(target_os = "macos") {
            modifiers.alt_key()
        } else {
            command
        };
        if key == Key::Named(NamedKey::Tab) {
            self.dispatch_owned_key(
                panel,
                UiInputEvent::Navigate {
                    seat: UiSeatId::PRIMARY,
                    direction: if extend {
                        UiNavigation::Previous
                    } else {
                        UiNavigation::Next
                    },
                },
                application,
            )?;
            self.sync_owned_keyboard_focus(panel, window)?;
            return Ok(true);
        }
        if let Some(focus) = &focus {
            let action = match key {
                Key::Character(key)
                    if command
                        && ["c", "x", "v"]
                            .iter()
                            .any(|shortcut| key.eq_ignore_ascii_case(shortcut)) =>
                {
                    if let Err(error) = self.owned_clipboard_key(focus, key) {
                        self.frontend.owned_input_error(
                            focus.panel,
                            &focus.frame,
                            &focus.field,
                            focus.token,
                            error,
                        );
                    }
                    None
                }
                Key::Character(key) if command && key.eq_ignore_ascii_case("a") => {
                    Some(TextEdit::SelectAll)
                }
                Key::Character(key) if command && key.eq_ignore_ascii_case("z") => {
                    Some(if extend {
                        TextEdit::Redo
                    } else {
                        TextEdit::Undo
                    })
                }
                Key::Character(key) if command && key.eq_ignore_ascii_case("y") => {
                    Some(TextEdit::Redo)
                }
                Key::Named(NamedKey::ArrowLeft) => Some(TextEdit::MoveLeft { extend, word }),
                Key::Named(NamedKey::ArrowRight) => Some(TextEdit::MoveRight { extend, word }),
                Key::Named(NamedKey::Home | NamedKey::ArrowUp) => Some(TextEdit::Home { extend }),
                Key::Named(NamedKey::End | NamedKey::ArrowDown) => Some(TextEdit::End { extend }),
                Key::Named(NamedKey::Backspace) => Some(TextEdit::Backspace),
                Key::Named(NamedKey::Delete) => Some(TextEdit::Delete),
                Key::Named(NamedKey::Enter | NamedKey::Escape) if !repeat => {
                    let operation = if key == Key::Named(NamedKey::Escape) {
                        crate::frontend::imgui::InspectorOperation::Discard
                    } else {
                        crate::frontend::imgui::InspectorOperation::Apply
                    };
                    if let Some(model) = application.current_frame()
                        && let Err(error) = self.frontend.owned_operation(
                            focus.panel,
                            &focus.frame,
                            &focus.field,
                            operation,
                            &model,
                        )
                    {
                        self.frontend.owned_input_error(
                            focus.panel,
                            &focus.frame,
                            &focus.field,
                            focus.token,
                            error,
                        );
                    }
                    None
                }
                _ if !command
                    && !modifiers.super_key()
                    && (!modifiers.alt_key()
                        || modifiers.control_key()
                        || cfg!(target_os = "macos")) =>
                {
                    text.filter(|text| {
                        !text.is_empty() && text.chars().all(|scalar| !scalar.is_control())
                    })
                    .map(|text| TextEdit::Insert(text.into()))
                }
                _ => None,
            };
            if let Some(action) = action {
                self.edit_owned_text(focus, &action);
            }
        } else if !command {
            let event = match key {
                Key::Named(NamedKey::Enter | NamedKey::Space) if !repeat => {
                    Some(UiInputEvent::ActivateFocused {
                        seat: UiSeatId::PRIMARY,
                        source: UiActivationSource::Keyboard,
                    })
                }
                Key::Named(NamedKey::ArrowLeft) => Some(UiInputEvent::Navigate {
                    seat: UiSeatId::PRIMARY,
                    direction: UiNavigation::Left,
                }),
                Key::Named(NamedKey::ArrowRight) => Some(UiInputEvent::Navigate {
                    seat: UiSeatId::PRIMARY,
                    direction: UiNavigation::Right,
                }),
                Key::Named(NamedKey::ArrowUp) => Some(UiInputEvent::Navigate {
                    seat: UiSeatId::PRIMARY,
                    direction: UiNavigation::Up,
                }),
                Key::Named(NamedKey::ArrowDown) => Some(UiInputEvent::Navigate {
                    seat: UiSeatId::PRIMARY,
                    direction: UiNavigation::Down,
                }),
                Key::Named(NamedKey::Escape) => {
                    let handled = self
                        .owned
                        .get(&panel)
                        .and_then(|surface| surface.presented.as_ref())
                        .is_some_and(|frame| {
                            self.frontend.owned_escape(panel, frame).unwrap_or(false)
                        });
                    if !handled {
                        self.cancel_owned_focus()?;
                    }
                    None
                }
                _ => None,
            };
            if let Some(event) = event {
                self.dispatch_owned_key(panel, event, application)?;
                self.sync_owned_keyboard_focus(panel, window)?;
            }
        }
        self.update_owned_ime_area()?;
        Ok(true)
    }
}
