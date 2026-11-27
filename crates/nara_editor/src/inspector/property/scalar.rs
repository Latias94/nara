use nara_editor_core::value_draft::{self, ValueDraft};
use nara_reflect::ComponentValue;
use open_gpui::{Context, Entity, Window, prelude::*, px, rgb};
use open_gpui_base::{
    MenuEvent, MenuItem, NumericEvent, NumericRange, NumericScrubber, Select, TextInput,
    TextInputEvent,
};

use crate::{theme, workspace::Workspace};

use super::super::{numeric, popup_style, target::CHANGED};
use super::{Choice, Control, Gesture, Property, PropertyEditor, Scrub, display_state, shared};

impl PropertyEditor {
    pub(super) fn text_event(
        &mut self,
        input: Entity<TextInput>,
        event: &TextInputEvent,
        cx: &mut Context<Self>,
    ) {
        self.handle_text_event(input, event, None, cx);
    }

    pub(super) fn text_event_in(
        &mut self,
        input: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_text_event(input.clone(), event, Some(window), cx);
    }

    fn handle_text_event(
        &mut self,
        input: Entity<TextInput>,
        event: &TextInputEvent,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if !self.active {
            return;
        }
        let Control::Scalar(text) = &mut self.control else {
            return;
        };
        match event {
            TextInputEvent::Change(_) => {
                text.expected
                    .get_or_insert_with(|| self.property.values.clone());
                self.error = None;
            }
            TextInputEvent::Cancel => {
                text.expected = None;
                self.error = None;
                input.update(cx, |input, cx| {
                    sync_text_in(input, &self.property, window, cx)
                });
                text.initial_text = input.read(cx).value().to_owned();
            }
            TextInputEvent::Submit(_) | TextInputEvent::Blur => {
                let Some(expected) = text.expected.as_ref() else {
                    return;
                };
                let parsed =
                    value_draft::parse_scalar(&self.property.target.shape, input.read(cx).value());
                self.error = parsed
                    .and_then(|value| self.property.apply(&self.workspace, expected, value, cx))
                    .err();
                if self.error.is_none() {
                    text.expected = None;
                    text.initial_text = input.read(cx).value().to_owned();
                }
            }
        }
        input.update(cx, |input, cx| input.set_invalid(self.error.is_some(), cx));
        cx.notify();
    }

    fn choice_event(
        &mut self,
        select: Entity<Select<Option<String>>>,
        event: &MenuEvent<Option<String>>,
        cx: &mut Context<Self>,
    ) {
        let MenuEvent::Selected(Some(value)) = event else {
            return;
        };
        if self.active && self.property.choices().contains(value) {
            self.set_value(
                &self.property.values.clone(),
                ComponentValue::String(value.clone()),
                cx,
            );
        }
        if self.error.is_some() {
            refresh_select(&select, &self.property, cx);
        }
    }

    fn scrub_event(
        &mut self,
        control: Entity<NumericScrubber>,
        event: &NumericEvent,
        cx: &mut Context<Self>,
    ) {
        let Control::Scalar(text) = &mut self.control else {
            return;
        };
        let Some(scrub) = &mut text.scrub else { return };
        if !self.active || scrub.view.entity_id() != control.entity_id() {
            return;
        }
        let terminal = !matches!(event, NumericEvent::Change(_));
        if matches!(event, NumericEvent::Cancel(_)) {
            if let Some(gesture) = scrub.gesture.take() {
                self.workspace
                    .update(cx, |workspace, cx| gesture.cancel(workspace, cx));
            }
            self.error = None;
        } else if text.expected.is_none() && !scrub.rejected {
            let value = match *event {
                NumericEvent::Change(value) | NumericEvent::Commit(value) => value,
                NumericEvent::Cancel(_) => return,
            };
            self.error = self
                .workspace
                .update(cx, |workspace, cx| {
                    apply_scrub(workspace, &self.property, scrub, terminal, value, cx)
                })
                .err();
        }
        if terminal {
            scrub.rejected = false;
            if let Some(value) = selected_number(&self.property) {
                control.update(cx, |control, cx| control.set_value(value, cx));
            }
        } else if self.error.is_some() || text.expected.is_some() {
            scrub.rejected = true;
        }
        cx.notify();
    }
}

pub(super) fn text_value(property: &Property) -> String {
    if !value_draft::supported(&property.target.shape) {
        return String::new();
    }
    match ValueDraft::from_value(&property.target.shape, shared(&property.value)) {
        ValueDraft::Text { text, .. } => text,
        _ => String::new(),
    }
}

pub(super) fn sync_text(input: &mut TextInput, property: &Property, cx: &mut Context<TextInput>) {
    sync_text_in(input, property, None, cx);
}

fn sync_text_in(
    input: &mut TextInput,
    property: &Property,
    window: Option<&mut Window>,
    cx: &mut Context<TextInput>,
) {
    input.set_placeholder(display_state(&property.value), cx);
    let value = text_value(property);
    match window {
        Some(window) => input.set_value_in(value, window, cx),
        None => input.set_value(value, cx),
    }
    input.set_invalid(false, cx);
}

fn choice_items(property: &Property) -> (Vec<MenuItem<Option<String>>>, Option<String>) {
    let mut items: Vec<_> = property
        .choices()
        .iter()
        .map(|choice| MenuItem::new(Some(choice.clone()), choice.clone()))
        .collect();
    let selection = match shared(&property.value) {
        Some(ComponentValue::String(value)) => {
            if !property.choices().contains(value) {
                items.insert(
                    0,
                    MenuItem::new(Some(value.clone()), format!("Unknown ({value})")).disabled(true),
                );
            }
            Some(value.clone())
        }
        _ => {
            items.insert(
                0,
                MenuItem::new(None, display_state(&property.value)).disabled(true),
            );
            None
        }
    };
    (items, selection)
}

pub(super) fn refresh_select(
    select: &Entity<Select<Option<String>>>,
    property: &Property,
    cx: &mut Context<PropertyEditor>,
) {
    let (items, selected) = choice_items(property);
    select.update(cx, |select, cx| {
        select.set_items(items, cx);
        select.set_selected(Some(selected), cx);
    });
}

pub(super) fn new_choice(property: &Property, cx: &mut Context<PropertyEditor>) -> Choice {
    let (items, selected) = choice_items(property);
    let select = cx.new(|cx| {
        let mut select = Select::new(property.label.clone(), items, cx);
        select.set_selected(Some(selected), cx);
        select.set_trigger_style(theme::input(), cx);
        select.set_popup_style(popup_style(), cx);
        select
    });
    let subscription = cx.subscribe(&select, PropertyEditor::choice_event);
    Choice {
        select,
        _subscription: subscription,
    }
}

fn selected_number(property: &Property) -> Option<f64> {
    shared(&property.value).and_then(numeric::displayed)
}

pub(super) fn scrub_spec(property: &Property) -> Option<(f64, NumericRange)> {
    let value = selected_number(property)?;
    Some((value, numeric::range(&property.target.shape, value)?))
}

pub(super) fn new_scrub(property: &Property, cx: &mut Context<PropertyEditor>) -> Option<Scrub> {
    let (value, range) = scrub_spec(property)?;
    let view = cx.new(|cx| {
        let mut scrubber = NumericScrubber::new(value, range, cx).with_text_entry(false);
        if let Some(step) = range.step() {
            scrubber.set_units_per_pixel(step);
        }
        scrubber.set_label(format!("Adjust {}", property.label), cx);
        scrubber.set_style(
            theme::input()
                .w(px(48.))
                .overflow_hidden()
                .text_color(rgb(theme::MUTED)),
            cx,
        );
        scrubber
    });
    let subscription = cx.subscribe(&view, PropertyEditor::scrub_event);
    Some(Scrub {
        view,
        _subscription: subscription,
        range,
        gesture: None,
        rejected: false,
    })
}

fn apply_scrub(
    workspace: &mut Workspace,
    property: &Property,
    scrub: &mut Scrub,
    terminal: bool,
    value: f64,
    cx: &mut Context<Workspace>,
) -> Result<(), String> {
    if scrub
        .gesture
        .as_ref()
        .is_some_and(|gesture| !workspace.session.owns_gesture(&gesture.owner))
    {
        scrub.gesture = None;
        return Err(CHANGED.into());
    }
    let expected = scrub
        .gesture
        .as_ref()
        .map_or(&property.values, |gesture| &gesture.expected);
    let result = value_draft::parse_scalar(
        &property.target.shape,
        &numeric::draft_text(&property.target.shape, value),
    )
    .and_then(|value| {
        property
            .target
            .command(&workspace.session, expected, value.clone())
            .map(|command| (value, command))
    });
    let (value, command) = match result {
        Ok(edit) => edit,
        Err(error) => {
            if let Some(gesture) = scrub.gesture.take() {
                gesture.cancel(workspace, cx);
            }
            return Err(error);
        }
    };
    if let Some(command) = command {
        if scrub.gesture.is_none() {
            if workspace.session.gesture_active() {
                return Err("Finish the active gesture before adjusting this field.".into());
            }
            if !terminal {
                let Some(owner) = workspace.begin_gesture(cx) else {
                    return Err("The adjustment could not begin.".into());
                };
                scrub.gesture = Some(Gesture {
                    owner,
                    expected: Vec::new(),
                });
            }
        }
        let applied = if let Some(gesture) = &scrub.gesture {
            workspace.apply_gesture(&gesture.owner, command, cx)
        } else {
            workspace.apply(command, cx)
        };
        if !applied {
            if let Some(gesture) = scrub.gesture.take() {
                gesture.cancel(workspace, cx);
            }
            return Err("The adjustment was rejected. See the Console for details.".into());
        }
        if let Some(gesture) = &mut scrub.gesture {
            gesture.expected = vec![Some(value); property.values.len()];
        }
    }
    if terminal && let Some(gesture) = scrub.gesture.take() {
        workspace.end_gesture(&gesture.owner, cx);
    }
    Ok(())
}
