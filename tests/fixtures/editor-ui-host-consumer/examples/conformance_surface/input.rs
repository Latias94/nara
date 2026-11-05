use nara::tooling::EditorDocumentId;
use nara_editor::{
    EditorPanelContext,
    native::{
        EditorPanelImeArea, EditorPanelInput, EditorPanelInputKind, EditorPanelServices,
        EditorPanelTextInputScope,
    },
};
use nara_editor_ui_host_consumer::{
    Result,
    conformance::{ConformancePanel, Control, Input, Snapshot},
};
use nara_text::TextEdit;
use winit::{
    event::{ElementState, MouseButton},
    keyboard::{Key, NamedKey},
    window::CursorIcon,
};

use super::InputEvidence;

pub fn reduce(
    panel: &mut ConformancePanel,
    context: &mut EditorPanelContext<'_, '_>,
    event: &EditorPanelInput<Snapshot>,
    services: &mut EditorPanelServices<'_>,
    evidence: &mut InputEvidence,
    document: EditorDocumentId,
) -> Result<()> {
    let before = panel.compose();
    let mut apply = false;
    let mut sync_text = false;
    match &event.kind {
        EditorPanelInputKind::KeyboardFocused => {
            panel.reduce(&event.snapshot, &Input::Focus(Control::Field))?;
            sync_text = true;
        }
        EditorPanelInputKind::Cancelled(_) => {
            panel.reduce(&event.snapshot, &Input::Blur)?;
            evidence.scope = None;
            evidence.cancellations += 1;
            return Ok(());
        }
        EditorPanelInputKind::PointerMoved => {
            evidence.mapped_point = event.target_position;
            services.set_cursor(
                if event.target_position.is_some_and(|point| {
                    event.snapshot.hit_test(point[0], point[1]) == Some(Control::Field)
                }) {
                    CursorIcon::Text
                } else {
                    CursorIcon::Default
                },
            )?;
        }
        EditorPanelInputKind::PointerButton {
            button: MouseButton::Left,
            state,
        } => {
            if let Some([x, y]) = event.target_position {
                if *state == ElementState::Pressed {
                    services.request_pointer_capture(MouseButton::Left)?;
                    apply = panel.reduce(&event.snapshot, &Input::PointerDown { x, y })?;
                } else {
                    apply = panel.reduce(&event.snapshot, &Input::PointerUp { x, y })?;
                }
                sync_text = true;
            }
        }
        EditorPanelInputKind::Key(key) if key.state == ElementState::Pressed && !key.repeat => {
            let command = if cfg!(target_os = "macos") {
                event.modifiers.super_key()
            } else {
                event.modifiers.control_key() && !event.modifiers.alt_key()
            };
            if key.logical_key == Key::Named(NamedKey::F7) {
                assert_eq!(
                    services.set_ime_area(EditorPanelImeArea {
                        position: [0.0, 20.0],
                        size: [0.0, 16.0]
                    }),
                    Err(nara_editor::native::EditorPanelServiceError::InvalidImeArea)
                );
                evidence.clipped_ime_refusals += 1;
            } else if key.logical_key == Key::Named(NamedKey::F6) {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(60);
                services.request_redraw_at(deadline + std::time::Duration::from_millis(10))?;
                services.request_redraw_at(deadline)?;
                evidence.deadline = Some(deadline);
            } else if key.logical_key == Key::Named(NamedKey::Tab) {
                let target = if event.snapshot.popup_open() {
                    Control::Apply
                } else if panel.focused() == Some(Control::Field) {
                    Control::Review
                } else {
                    Control::Field
                };
                apply = panel.reduce(&event.snapshot, &Input::Focus(target))?;
            } else if key.logical_key == Key::Named(NamedKey::Escape) {
                apply = panel.reduce(
                    &event.snapshot,
                    &if event.snapshot.popup_open() {
                        Input::DismissModal
                    } else {
                        Input::Blur
                    },
                )?;
            } else if key.logical_key == Key::Named(NamedKey::Enter) && panel.focused().is_some() {
                apply =
                    panel.reduce(&event.snapshot, &Input::Activate(panel.focused().unwrap()))?;
            } else if let Some(token) = panel.input_token() {
                let edit = match &key.logical_key {
                    Key::Character(key) if command && key.eq_ignore_ascii_case("a") => {
                        Some(TextEdit::SelectAll)
                    }
                    Key::Character(key) if command && key.eq_ignore_ascii_case("z") => {
                        Some(if event.modifiers.shift_key() {
                            TextEdit::Redo
                        } else {
                            TextEdit::Undo
                        })
                    }
                    Key::Character(key) if command && key.eq_ignore_ascii_case("y") => {
                        Some(TextEdit::Redo)
                    }
                    Key::Character(key) if command && key.eq_ignore_ascii_case("v") => {
                        Some(TextEdit::Insert(services.read_clipboard()?))
                    }
                    Key::Character(key)
                        if command
                            && (key.eq_ignore_ascii_case("c") || key.eq_ignore_ascii_case("x")) =>
                    {
                        let selected = &panel.editor().text()[panel.editor().selection_bytes()];
                        match services.write_clipboard(selected) {
                            Ok(()) if key.eq_ignore_ascii_case("x") => {
                                Some(TextEdit::Insert(String::new()))
                            }
                            Ok(()) => None,
                            Err(_) => {
                                evidence.clipboard_errors += 1;
                                None
                            }
                        }
                    }
                    Key::Named(NamedKey::ArrowLeft) => Some(TextEdit::MoveLeft {
                        extend: event.modifiers.shift_key(),
                        word: command,
                    }),
                    Key::Named(NamedKey::ArrowRight) => Some(TextEdit::MoveRight {
                        extend: event.modifiers.shift_key(),
                        word: command,
                    }),
                    Key::Named(NamedKey::Home) => Some(TextEdit::Home {
                        extend: event.modifiers.shift_key(),
                    }),
                    Key::Named(NamedKey::End) => Some(TextEdit::End {
                        extend: event.modifiers.shift_key(),
                    }),
                    Key::Named(NamedKey::Backspace) => Some(TextEdit::Backspace),
                    Key::Named(NamedKey::Delete) => Some(TextEdit::Delete),
                    _ => None,
                };
                if let Some(edit) = edit {
                    panel.reduce(&event.snapshot, &Input::Text { token, edit })?;
                }
            }
            sync_text = true;
        }
        EditorPanelInputKind::Text(text) | EditorPanelInputKind::ImeCommit(text) => {
            if evidence.scope.as_ref().map(|(_, scope)| scope) == event.text_scope.as_ref() {
                if let Some((token, _)) = &evidence.scope {
                    panel.reduce(
                        &event.snapshot,
                        &Input::Text {
                            token: *token,
                            edit: TextEdit::Commit(text.clone()),
                        },
                    )?;
                }
            }
            sync_text = true;
        }
        EditorPanelInputKind::ImePreedit { text, cursor } => {
            if evidence.scope.as_ref().map(|(_, scope)| scope) == event.text_scope.as_ref() {
                if let Some((token, _)) = &evidence.scope {
                    panel.reduce(
                        &event.snapshot,
                        &Input::Text {
                            token: *token,
                            edit: TextEdit::Preedit {
                                text: text.clone(),
                                cursor: *cursor,
                            },
                        },
                    )?;
                }
            }
            sync_text = true;
        }
        EditorPanelInputKind::ImeDisabled => {
            if evidence.scope.as_ref().map(|(_, scope)| scope) == event.text_scope.as_ref() {
                if let Some((token, _)) = &evidence.scope {
                    panel.reduce(
                        &event.snapshot,
                        &Input::Text {
                            token: *token,
                            edit: TextEdit::CancelComposition,
                        },
                    )?;
                }
            }
        }
        EditorPanelInputKind::ImeEnabled => {
            assert!(services.ime_ready()?);
            sync_text = true;
        }
        EditorPanelInputKind::Wheel { delta, .. } => {
            evidence.wheel.push(*delta);
        }
        _ => {}
    }
    if apply {
        let basis = context.document_query_basis(document)?;
        let prepared = panel.prepare_edit(context, &basis, super::entity("caption"))?;
        context
            .commit_authoring_edit(prepared)
            .map_err(|(error, _)| error)?;
    }
    if sync_text {
        if let Some(token) = panel.input_token() {
            if evidence
                .scope
                .as_ref()
                .is_none_or(|(current, _)| *current != token)
            {
                evidence.scope = Some((token, EditorPanelTextInputScope::new()));
            }
            services.set_text_input(evidence.scope.as_ref().map(|(_, scope)| scope.clone()))?;
            if let Some(area) = panel.compose().ime_area() {
                services
                    .set_ime_area(EditorPanelImeArea {
                        position: [area.x0, area.y0],
                        size: [(area.x1 - area.x0).max(0.0), (area.y1 - area.y0).max(0.0)],
                    })
                    .map_err(|error| {
                        std::io::Error::other(format!(
                            "{error}; requested caret {area:?} after {:?}",
                            event.kind
                        ))
                    })?;
            }
        } else {
            evidence.scope = None;
            services.set_text_input(None)?;
        }
    }
    if panel.compose() != before {
        services.request_redraw()?;
    }
    Ok(())
}
