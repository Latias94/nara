use std::sync::Arc;

use nara::project_host::EditorUiAuthoringFrame;
use nara_text::TextEdit;
use nara_ui_kernel::{
    UiActivationSource, UiButtonPhase, UiFrame, UiFrameJournal, UiInputEvent, UiInteractionSession,
    UiKey, UiModifiers, UiNavigation, UiPoint, UiPointerButton, UiPointerId, UiPointerInput,
    UiRect, UiRouteStamp, UiSeatId, UiSemanticRole,
};
use nara_ui_widgets::UiTextInputToken;
use winit::{
    event::{ElementState, MouseButton},
    keyboard::{Key, NamedKey},
    window::CursorIcon,
};

use crate::{
    EditorPanelContext,
    native::{
        EditorPanelCancelReason, EditorPanelImeArea, EditorPanelInput, EditorPanelInputKind,
        EditorPanelServices, EditorPanelTextInputScope, EditorPanelWheelDelta,
    },
};

use super::{DrawnPanel, text_controls::TextControls};

pub(super) fn clipboard_edit(
    owner: &mut TextControls<'_>,
    frame: &Arc<UiFrame>,
    field: &UiKey,
    token: UiTextInputToken,
    key: &str,
    read: impl FnOnce() -> Result<String, String>,
    write: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Option<TextEdit>, String> {
    let result = if key.eq_ignore_ascii_case("v") {
        read().map(|text| Some(TextEdit::Insert(text)))
    } else if let Some(text) = owner.selected_text(frame, field, token)? {
        write(&text).map(|()| {
            key.eq_ignore_ascii_case("x")
                .then(|| TextEdit::Insert(String::new()))
        })
    } else {
        Ok(None)
    };
    match result {
        Ok(edit) => Ok(edit),
        Err(error) => {
            owner.input_error(frame, field, token, error);
            Ok(None)
        }
    }
}

#[derive(Default)]
pub(crate) struct PanelInput {
    interaction: UiInteractionSession,
    text: Option<TextFocus>,
    native_focus_requested: bool,
}

struct TextFocus {
    field: UiKey,
    token: UiTextInputToken,
    scope: EditorPanelTextInputScope,
}

impl PanelInput {
    pub(crate) fn reconcile(
        &mut self,
        panel: &mut DrawnPanel,
        journal: &UiFrameJournal,
        frame: &Arc<UiFrame>,
        previous: Option<&Arc<UiFrame>>,
        context: &mut EditorPanelContext<'_, '_>,
        ui: Option<&EditorUiAuthoringFrame>,
    ) -> Result<(), String> {
        let surface = frame.environment().surface_id();
        let presented = journal.latest(surface).map_err(|error| error.to_string())?;
        let frame = Arc::clone(presented.frame());
        if !panel.retains_frame(&frame) {
            return Err("Presented controls belong to a retired projection".into());
        }
        let invalidated = previous
            .filter(|previous| panel.retains_frame(previous))
            .map(|previous| panel.invalidated_controls(previous, &frame))
            .transpose()?
            .unwrap_or_default();
        if let Some(batch) = self
            .interaction
            .reconcile_latest_presentation(journal, surface, &invalidated)
            .map_err(|error| error.to_string())?
        {
            panel.reduce(&frame, &batch, context, ui)?;
        }
        if let Some(target) = panel.take_focus_request(&frame) {
            let outcome = self
                .interaction
                .dispatch(
                    journal,
                    UiRouteStamp::from_receipt(presented.receipt()),
                    UiInputEvent::FocusTarget {
                        seat: UiSeatId::PRIMARY,
                        target,
                    },
                )
                .map_err(|error| error.to_string())?;
            if let Some(batch) = outcome.consumed() {
                panel.reduce(&frame, batch, context, ui)?;
                self.native_focus_requested = true;
            }
        }
        self.retire_text(panel, &frame);
        Ok(())
    }

    pub(crate) fn take_native_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.native_focus_requested)
    }

    fn retire_text(&mut self, panel: &mut DrawnPanel, frame: &Arc<UiFrame>) {
        let current = self.text.as_ref().is_some_and(|focus| {
            panel
                .text_controls()
                .and_then(|owner| owner.input_token(frame, &focus.field).ok().flatten())
                == Some(focus.token)
        });
        if !current {
            self.text = None;
        }
    }

    pub(crate) fn deliver(
        &mut self,
        panel: &mut DrawnPanel,
        context: &mut EditorPanelContext<'_, '_>,
        event: &EditorPanelInput<UiFrame>,
        services: &mut EditorPanelServices<'_>,
        journal: &UiFrameJournal,
        ui: Option<&EditorUiAuthoringFrame>,
    ) -> Result<(), String> {
        let frame = &event.snapshot;
        let surface = frame.environment().surface_id();
        if matches!(event.kind, EditorPanelInputKind::Cancelled(_)) {
            if matches!(
                event.kind,
                EditorPanelInputKind::Cancelled(EditorPanelCancelReason::OutputReplaced)
            ) && (!panel.input_basis_is_current(frame)
                || !journal
                    .latest(surface)
                    .is_ok_and(|presented| Arc::ptr_eq(presented.frame(), frame)))
            {
                return Ok(());
            }
            if let Ok(presented) = journal.latest(surface) {
                let route = UiRouteStamp::from_receipt(presented.receipt());
                let outcome = self
                    .interaction
                    .dispatch(
                        journal,
                        route,
                        UiInputEvent::SeatCancelled {
                            seat: UiSeatId::PRIMARY,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                if let Some(batch) = outcome
                    .consumed()
                    .filter(|_| panel.retains_frame(presented.frame()))
                {
                    panel.reduce(presented.frame(), batch, context, ui)?;
                }
            }
            panel.cancel_input()?;
            self.native_focus_requested = false;
            self.text = None;
            self.interaction.clear_surface_state(surface);
            return Ok(());
        }
        if !panel.retains_frame(frame) {
            return Err("Input controls belong to a retired projection".into());
        }
        let accepted = journal.latest(surface).map_err(|error| error.to_string())?;
        if !Arc::ptr_eq(accepted.frame(), frame) {
            return Err("Native input and UI presentation do not match".into());
        }
        let route = UiRouteStamp::from_receipt(accepted.receipt());
        if matches!(
            event.kind,
            EditorPanelInputKind::ModalOutsidePointer {
                state: ElementState::Pressed,
                ..
            }
        ) {
            panel.dismiss_menu()?;
            services
                .request_redraw()
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        self.retire_text(panel, frame);
        let transform = frame.environment().content_transform();
        let point = event.target_position.map(|[x, y]| {
            transform
                .physical_to_authored(UiPoint::new(x, y))
                .unwrap_or_else(|| {
                    let origin = transform.physical_content_origin();
                    UiPoint::new(
                        (x - origin.x()) / transform.content_scale_factor(),
                        (y - origin.y()) / transform.content_scale_factor(),
                    )
                })
        });
        let modifiers = UiModifiers::new(
            event.modifiers.shift_key(),
            event.modifiers.control_key(),
            event.modifiers.alt_key(),
            event.modifiers.super_key(),
        );
        if matches!(
            event.kind,
            EditorPanelInputKind::PointerButton {
                button: MouseButton::Left,
                state: ElementState::Pressed
            }
        ) && point.is_some_and(|point| panel.menu_outside(frame, point))
        {
            panel.dismiss_menu()?;
            services
                .request_redraw()
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        let pointer = match &event.kind {
            EditorPanelInputKind::PointerMoved => point
                .map(|point| UiPointerInput::moved(UiSeatId::PRIMARY, UiPointerId::PRIMARY, point)),
            EditorPanelInputKind::PointerLeft => Some(UiPointerInput::cancelled(
                UiSeatId::PRIMARY,
                UiPointerId::PRIMARY,
            )),
            EditorPanelInputKind::PointerButton {
                button: MouseButton::Left,
                state,
            } => {
                if *state == ElementState::Pressed {
                    services
                        .request_pointer_capture(MouseButton::Left)
                        .map_err(|error| error.to_string())?;
                }
                Some(UiPointerInput::button(
                    UiSeatId::PRIMARY,
                    UiPointerId::PRIMARY,
                    UiPointerButton::Primary,
                    if *state == ElementState::Pressed {
                        UiButtonPhase::Pressed
                    } else {
                        UiButtonPhase::Released
                    },
                    point,
                ))
            }
            EditorPanelInputKind::Wheel { delta, .. } => {
                let delta = match delta {
                    EditorPanelWheelDelta::Lines([_, y]) => *y * 42.0,
                    EditorPanelWheelDelta::Pixels([_, y]) => {
                        *y as f32 / transform.content_scale_factor()
                    }
                };
                panel.scroll(frame, -delta)?;
                None
            }
            _ => None,
        };
        let mut routed =
            pointer.map(|pointer| UiInputEvent::Pointer(pointer.with_modifiers(modifiers)));
        let key = match &event.kind {
            EditorPanelInputKind::Key(key) if key.state == ElementState::Pressed => Some(key),
            _ => None,
        };
        let composing = self
            .text
            .as_ref()
            .and_then(|focus| {
                panel
                    .text_controls()
                    .map(|owner| owner.is_composing(frame, &focus.field, focus.token))
            })
            .transpose()?
            .unwrap_or(false);
        if composing && key.is_some() {
            if key.is_some_and(|key| key.logical_key == Key::Named(NamedKey::Escape)) {
                if let (Some(focus), Some(mut owner)) = (self.text.as_ref(), panel.text_controls())
                {
                    owner.edit(
                        frame,
                        &focus.field,
                        focus.token,
                        &TextEdit::CancelComposition,
                    )?;
                }
            }
            services
                .request_redraw()
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        if let Some(key) = key {
            routed = match key.logical_key {
                Key::Named(NamedKey::Tab) if panel.navigate_menu(frame, modifiers.shift())? => None,
                Key::Named(NamedKey::Tab) => Some(UiInputEvent::Navigate {
                    seat: UiSeatId::PRIMARY,
                    direction: if modifiers.shift() {
                        UiNavigation::Previous
                    } else {
                        UiNavigation::Next
                    },
                }),
                Key::Named(NamedKey::Enter) if self.text.is_none() => {
                    Some(UiInputEvent::ActivateFocused {
                        seat: UiSeatId::PRIMARY,
                        source: UiActivationSource::Keyboard,
                    })
                }
                _ => routed,
            };
        }
        if let Some(routed) = routed {
            let outcome = self
                .interaction
                .dispatch(journal, route, routed)
                .map_err(|error| error.to_string())?;
            if let Some(batch) = outcome.consumed() {
                panel.reduce(frame, batch, context, ui)?;
            }
        }
        let focused = self
            .interaction
            .semantic_snapshot(journal, route, UiSeatId::PRIMARY)
            .map_err(|error| error.to_string())?
            .nodes()
            .iter()
            .find(|node| node.state().focused() && node.role() == UiSemanticRole::TextInput)
            .map(|node| node.key().clone());
        let next_text = focused.and_then(|field| {
            panel
                .text_controls()
                .and_then(|owner| owner.input_token(frame, &field).ok().flatten())
                .map(|token| (field, token))
        });
        if !self.text.as_ref().is_some_and(|current| {
            next_text
                .as_ref()
                .is_some_and(|(field, token)| current.field == *field && current.token == *token)
        }) {
            self.text = next_text.map(|(field, token)| TextFocus {
                field,
                token,
                scope: EditorPanelTextInputScope::new(),
            });
        }
        let text_event = matches!(
            event.kind,
            EditorPanelInputKind::Text(_)
                | EditorPanelInputKind::ImePreedit { .. }
                | EditorPanelInputKind::ImeCommit(_)
                | EditorPanelInputKind::ImeDisabled
                | EditorPanelInputKind::ImeEnabled
        );
        let original_scope = self.text.as_ref().map(|focus| &focus.scope);
        let scope_current = !text_event || event.text_scope.as_ref() == original_scope;
        let mut clear_finished_focus = false;
        if let Some(focus) = self.text.as_ref() {
            if scope_current {
                if let Some(mut owner) = panel.text_controls() {
                    let edit = match &event.kind {
                        EditorPanelInputKind::Text(text)
                            if !modifiers.control()
                                && !modifiers.meta()
                                && text.chars().all(|character| !character.is_control()) =>
                        {
                            Some(TextEdit::Insert(text.clone()))
                        }
                        EditorPanelInputKind::ImeCommit(text) => {
                            Some(TextEdit::Commit(text.clone()))
                        }
                        EditorPanelInputKind::ImePreedit { text, cursor } => {
                            Some(TextEdit::Preedit {
                                text: text.clone(),
                                cursor: *cursor,
                            })
                        }
                        EditorPanelInputKind::ImeDisabled => Some(TextEdit::CancelComposition),
                        _ => None,
                    };
                    let edit = if let Some(key) = key {
                        let command = if cfg!(target_os = "macos") {
                            modifiers.meta()
                        } else {
                            modifiers.control() && !modifiers.alt()
                        };
                        match &key.logical_key {
                            Key::Character(value) if command && value.eq_ignore_ascii_case("a") => {
                                Some(TextEdit::SelectAll)
                            }
                            Key::Character(value)
                                if command
                                    && (value.eq_ignore_ascii_case("c")
                                        || value.eq_ignore_ascii_case("x")
                                        || value.eq_ignore_ascii_case("v")) =>
                            {
                                clipboard_edit(
                                    &mut owner,
                                    frame,
                                    &focus.field,
                                    focus.token,
                                    value.as_str(),
                                    || services.read_clipboard().map_err(|error| error.to_string()),
                                    |text| {
                                        services
                                            .write_clipboard(text)
                                            .map_err(|error| error.to_string())
                                    },
                                )?
                            }
                            Key::Character(value) if command && value.eq_ignore_ascii_case("z") => {
                                Some(if modifiers.shift() {
                                    TextEdit::Redo
                                } else {
                                    TextEdit::Undo
                                })
                            }
                            Key::Character(value) if command && value.eq_ignore_ascii_case("y") => {
                                Some(TextEdit::Redo)
                            }
                            Key::Named(NamedKey::ArrowLeft) => Some(TextEdit::MoveLeft {
                                extend: modifiers.shift(),
                                word: command,
                            }),
                            Key::Named(NamedKey::ArrowRight) => Some(TextEdit::MoveRight {
                                extend: modifiers.shift(),
                                word: command,
                            }),
                            Key::Named(NamedKey::Home) => Some(TextEdit::Home {
                                extend: modifiers.shift(),
                            }),
                            Key::Named(NamedKey::End) => Some(TextEdit::End {
                                extend: modifiers.shift(),
                            }),
                            Key::Named(NamedKey::Backspace) => Some(TextEdit::Backspace),
                            Key::Named(NamedKey::Delete) => Some(TextEdit::Delete),
                            Key::Named(NamedKey::Escape) => {
                                if !owner.escape(frame)? {
                                    owner.discard_input(frame, &focus.field, context)?;
                                    clear_finished_focus =
                                        owner.input_token(frame, &focus.field).ok().flatten()
                                            != Some(focus.token);
                                }
                                None
                            }
                            Key::Named(NamedKey::Enter) if !key.repeat => {
                                owner.apply_input(frame, &focus.field, context)?;
                                None
                            }
                            _ => edit,
                        }
                    } else {
                        edit
                    };
                    if let Some(edit) = edit {
                        if let Err(error) = owner.edit(frame, &focus.field, focus.token, &edit) {
                            owner.input_error(frame, &focus.field, focus.token, error);
                        }
                    }
                    if matches!(
                        event.kind,
                        EditorPanelInputKind::PointerButton {
                            button: MouseButton::Left,
                            state: ElementState::Pressed
                        }
                    ) {
                        if let Some(point) = point {
                            let field_rect = frame
                                .layouts()
                                .iter()
                                .find(|layout| layout.key() == Some(&focus.field))
                                .ok_or("The presented text field has no geometry")?
                                .rect();
                            if let Err(error) = owner.point(
                                frame,
                                &focus.field,
                                focus.token,
                                UiPoint::new(
                                    point.x() - field_rect.origin().x(),
                                    point.y() - field_rect.origin().y(),
                                ),
                                modifiers.shift(),
                            ) {
                                owner.input_error(frame, &focus.field, focus.token, error);
                            }
                        }
                    }
                }
            }
        } else if key.is_some_and(|key| key.logical_key == Key::Named(NamedKey::Escape)) {
            panel.dismiss_menu()?;
            if let Some(mut owner) = panel.text_controls() {
                owner.escape(frame)?;
            }
        }
        if clear_finished_focus {
            let outcome = self
                .interaction
                .dispatch(
                    journal,
                    route,
                    UiInputEvent::SeatCancelled {
                        seat: UiSeatId::PRIMARY,
                    },
                )
                .map_err(|error| error.to_string())?;
            if let Some(batch) = outcome.consumed() {
                panel.reduce(frame, batch, context, ui)?;
            }
        }
        self.retire_text(panel, frame);
        let sync_keyboard = !matches!(
            event.kind,
            EditorPanelInputKind::PointerMoved
                | EditorPanelInputKind::PointerLeft
                | EditorPanelInputKind::Wheel { .. }
                | EditorPanelInputKind::ModifiersChanged
                | EditorPanelInputKind::PointerButton {
                    state: ElementState::Released,
                    ..
                }
                | EditorPanelInputKind::ModalOutsidePointer {
                    state: ElementState::Released,
                    ..
                }
        );
        if sync_keyboard && scope_current {
            services
                .set_text_input(self.text.as_ref().map(|focus| focus.scope.clone()))
                .map_err(|error| {
                    format!("Updating text services after {:?}: {error}", event.kind)
                })?;
            if let Some(focus) = self.text.as_ref() {
                if let Some(mut owner) = panel.text_controls() {
                    let area = owner.ime_area(frame, &focus.field, focus.token)?;
                    let field = frame
                        .layouts()
                        .iter()
                        .find(|layout| layout.key() == Some(&focus.field))
                        .ok_or("The presented text field has no IME geometry")?;
                    let area = UiRect::from_origin_size(
                        field.rect().origin().x() + area.origin().x(),
                        field.rect().origin().y() + area.origin().y(),
                        area.size().width(),
                        area.size().height(),
                    )
                    .and_then(|area| {
                        field
                            .clip_rect()
                            .map_or(Some(area), |clip| area.intersect(clip))
                    });
                    if let Some(rect) = area.and_then(|area| {
                        transform.authored_rect_to_physical(area, frame.environment().snap_policy())
                    }) {
                        if let Err(error) = services.set_ime_area(EditorPanelImeArea {
                            position: [rect.min_x() as f32, rect.min_y() as f32],
                            size: [rect.width() as f32, rect.height() as f32],
                        }) {
                            owner.input_error(frame, &focus.field, focus.token, error.to_string());
                        }
                    }
                }
            }
        }
        if matches!(
            event.kind,
            EditorPanelInputKind::PointerMoved | EditorPanelInputKind::PointerButton { .. }
        ) {
            let over_text = point.is_some_and(|point| {
                frame.semantics().iter().any(|node| {
                    node.role() == UiSemanticRole::TextInput
                        && node.rect().contains(point)
                        && node.clip_rect().is_none_or(|clip| clip.contains(point))
                })
            });
            services
                .set_cursor(if over_text {
                    CursorIcon::Text
                } else {
                    CursorIcon::Default
                })
                .map_err(|error| error.to_string())?;
        }
        if !matches!(
            event.kind,
            EditorPanelInputKind::KeyboardFocused | EditorPanelInputKind::ModifiersChanged
        ) {
            services
                .request_redraw()
                .map_err(|error| error.to_string())?;
        }
        panel.flush_services(services)?;
        Ok(())
    }
}
