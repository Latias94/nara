use nara_editor_core::{
    InspectorValue,
    field_draft::{self, StructuredRow},
};
use nara_reflect::{ComponentValue, ComponentValueShape};
use open_gpui::{AnyElement, Context, Div, IntoElement, Render, Window, div, prelude::*, px, rgb};
use open_gpui_base::Checkbox;

use crate::theme;

use super::super::{field_row, native_text, target::CHANGED};
use super::{ColorDraft, Control, Form, Property, PropertyEditor, display_state, shared};

impl Render for PropertyEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_id = window.window_handle().window_id();
        if let Control::Scalar(text) = &mut self.control
            && text.window_id != Some(window_id)
        {
            text.window_id = Some(window_id);
            text._subscription = cx.subscribe_in(&text.input, window, Self::text_event_in);
        }
        let property = &self.property;
        let mut form_view = None;
        let (control, expected): (AnyElement, _) = match &mut self.control {
            Control::ReadOnly => (
                div()
                    .text_color(rgb(theme::MUTED))
                    .child(format!("{} · Read only", display_value(property)))
                    .into_any_element(),
                None,
            ),
            Control::Boolean => {
                let checked = matches!(shared(&property.value), Some(ComponentValue::Bool(true)));
                let mark = if checked {
                    "✓"
                } else if property.value == InspectorValue::Mixed {
                    "−"
                } else {
                    ""
                };
                let expected = property.values.clone();
                let view = cx.weak_entity();
                let label = if property.value == InspectorValue::Mixed {
                    format!("{} (mixed)", property.label)
                } else {
                    property.label.clone()
                };
                let checkbox = Checkbox::with_content(
                    "boolean",
                    label,
                    checked,
                    div()
                        .size(px(16.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(rgb(theme::BORDER))
                        .bg(rgb(theme::INPUT))
                        .text_color(rgb(theme::ACCENT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(mark),
                )
                .size(px(24.))
                .flex()
                .items_center()
                .on_toggle(move |next, _, cx| {
                    let _ = view.update(cx, |editor, cx| {
                        editor.set_value(&expected, ComponentValue::Bool(next), cx)
                    });
                });
                (checkbox.into_any_element(), None)
            }
            Control::Choice(choice) => (choice.select.clone().into_any_element(), None),
            Control::Scalar(text) => (
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().min_w_0().flex_1().child(text.input.clone()))
                    .children(text.scrub.as_ref().map(|scrub| scrub.view.clone()))
                    .into_any_element(),
                text.expected.as_ref(),
            ),
            Control::Structure(form) => {
                if let Some(form) = form {
                    form_view = Some(render_form(form, &property.target.shape, cx));
                    (
                        div()
                            .text_color(rgb(theme::MUTED))
                            .child(display_value(property))
                            .into_any_element(),
                        Some(&form.expected),
                    )
                } else {
                    let expected = property.values.clone();
                    let label = if property.value == InspectorValue::Mixed {
                        "Replace mixed values…"
                    } else {
                        "Edit value…"
                    };
                    let action = theme::button("open-value-draft", label).on_click(
                        cx.listener(move |editor, _, _, cx| editor.open_form(&expected, cx)),
                    );
                    (
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .truncate()
                                    .text_color(rgb(theme::MUTED))
                                    .child(display_value(property)),
                            )
                            .child(action.flex_shrink_0())
                            .into_any_element(),
                        None,
                    )
                }
            }
            Control::Color(draft) => {
                if let Some(draft) = draft {
                    form_view = Some(render_color_picker(draft, &property.values, cx));
                }
                let seed = property.color_seed();
                let value = match (&property.value, seed) {
                    (InspectorValue::Shared(_), Some(seed)) => div()
                        .size(px(16.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(rgb(theme::BORDER))
                        .bg(seed.display()),
                    (value, Some(_)) => div()
                        .text_color(rgb(theme::MUTED))
                        .child(display_state(value)),
                    (_, None) => div()
                        .text_color(rgb(theme::MUTED))
                        .child("Use linear channels"),
                };
                let open = (draft.is_none() && seed.is_some()).then(|| {
                    let expected = property.values.clone();
                    let label = if property.value == InspectorValue::Mixed {
                        "Replace mixed colors…"
                    } else {
                        "Edit sRGB…"
                    };
                    theme::button("edit-color", label).on_click(
                        cx.listener(move |editor, _, _, cx| editor.open_color(&expected, cx)),
                    )
                });
                (
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(value)
                        .children(open)
                        .into_any_element(),
                    draft.as_ref().map(|draft| &draft.expected),
                )
            }
        };
        let conflict = expected.is_some_and(|expected| expected != &property.values);
        let error = if conflict {
            Some(CHANGED)
        } else {
            self.error.as_deref()
        };
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .child(field_row(&property.label, control))
            .children(form_view)
            .children(error.map(|error| {
                div()
                    .px_2()
                    .text_sm()
                    .text_color(rgb(theme::ERROR))
                    .child(error.to_owned())
            }))
    }
}

fn render_color_picker(
    draft: &ColorDraft,
    live: &[Option<ComponentValue>],
    cx: &mut Context<PropertyEditor>,
) -> Div {
    let picker_id = draft.picker.entity_id();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_1()
        .child(
            div()
                .text_sm()
                .text_color(rgb(theme::MUTED))
                .child("sRGB · straight alpha"),
        )
        .child(draft.picker.clone())
        .child(div().flex().items_center().gap_1().children(
            [(true, "Apply color"), (false, "Cancel")].map(|(apply, label)| {
                theme::button(label, label)
                    .disabled(apply && draft.expected != live)
                    .on_click(
                        cx.listener(move |editor, _, _, cx| {
                            editor.close_color(picker_id, apply, cx)
                        }),
                    )
            }),
        ))
}

fn render_form(
    form: &mut Form,
    shape: &ComponentValueShape,
    cx: &mut Context<PropertyEditor>,
) -> Div {
    let mut rows = div()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_1()
        .ml_2()
        .pl_2()
        .border_l_1()
        .border_color(rgb(theme::BORDER));
    for (index, row) in field_draft::rows(&form.draft, shape)
        .into_iter()
        .enumerate()
    {
        match row {
            StructuredRow::Label(label) if label != "root" => {
                rows = rows.child(div().text_sm().text_color(rgb(theme::MUTED)).child(label));
            }
            StructuredRow::Label(_) => {}
            StructuredRow::Text { input, label, text } => {
                let label = label.strip_prefix("root.").unwrap_or(&label);
                let (view, _) = form.inputs.entry(input.clone()).or_insert_with(|| {
                    let view = cx.new(|cx| native_text(text, label, "", cx));
                    let subscription = cx.subscribe(&view, move |editor, view, event, cx| {
                        editor.form_text(&input, view, event, cx)
                    });
                    (view, subscription)
                });
                rows = rows.child(field_row(label, view.clone()));
            }
            StructuredRow::Button {
                action,
                label,
                enabled,
                ..
            } => {
                let id = form.id.clone();
                rows = rows.child(
                    div().id(("structure-action", index)).child(
                        theme::button("draft-button", label)
                            .disabled(!enabled)
                            .on_click(cx.listener(move |editor, _, _, cx| {
                                editor.form_action(&id, &action, cx)
                            })),
                    ),
                );
            }
        }
    }
    rows.child(div().flex().items_center().gap_1().children(
        [(true, "Apply"), (false, "Cancel")].map(|(commit, label)| {
            let id = form.id.clone();
            theme::button(label, label)
                .on_click(cx.listener(move |editor, _, _, cx| editor.close_form(&id, commit, cx)))
        }),
    ))
}

fn display_value(property: &Property) -> String {
    match shared(&property.value) {
        Some(ComponentValue::Null) => "Null".into(),
        Some(ComponentValue::Bool(value)) => value.to_string(),
        Some(ComponentValue::I64(value)) => value.to_string(),
        Some(ComponentValue::U64(value)) => value.to_string(),
        Some(ComponentValue::F64(value)) => value.get().to_string(),
        Some(ComponentValue::String(value)) if value.is_empty() => "\"\"".into(),
        Some(ComponentValue::String(value)) => value.clone(),
        Some(ComponentValue::EntityReference(value)) => value.to_string(),
        Some(ComponentValue::AssetReference(value)) => value.to_string(),
        Some(ComponentValue::PackedU16(values)) => format!("{} values", values.len()),
        Some(ComponentValue::PackedU32(values)) => format!("{} values", values.len()),
        Some(ComponentValue::PackedI32(values)) => format!("{} values", values.len()),
        Some(ComponentValue::PackedF32(values)) => format!("{} values", values.len()),
        Some(ComponentValue::Bytes(values)) => format!("{} bytes", values.len()),
        Some(ComponentValue::Map(values)) => format!("{} entries", values.len()),
        Some(ComponentValue::List(values)) => format!("{} items", values.len()),
        None => display_state(&property.value).into(),
    }
}
