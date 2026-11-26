use std::{collections::HashMap, rc::Rc};

use nara_editor_core::{
    EditorGesture, InspectorComponent, InspectorValue,
    field_draft::{self, DraftAction, DraftInput, StructuredRow},
    value_draft::{self, RootDraft, ValueDraft},
};
use nara_reflect::{
    ComponentFieldEditorHint, ComponentFieldPathSegment, ComponentValue, ComponentValueKind,
    ComponentValueShape,
};
use nara_scene::authoring::SceneEntity;
use open_gpui::{
    AnyElement, App, Context, Div, Entity, EntityId, IntoElement, Render, Subscription, Window,
    div, prelude::*, px, rgb,
};
use open_gpui_base::{
    Checkbox, ColorPicker, MenuEvent, MenuItem, NumericEvent, NumericRange, NumericScrubber,
    Select, TextInput, TextInputEvent,
};

use crate::{theme, workspace::Workspace};

use super::{
    Draft,
    color::ColorSeed,
    field_row, native_text, numeric, popup_style,
    target::{self, CHANGED, FieldKey, FieldTarget, Selection},
};

pub(super) struct Property {
    pub target: FieldTarget,
    label: String,
    values: Vec<Option<ComponentValue>>,
    value: InspectorValue,
    pub editable: bool,
}

impl Property {
    pub fn for_component(
        selection: &Rc<Selection>,
        rows: &[&SceneEntity],
        component: &InspectorComponent,
    ) -> Vec<Self> {
        let mut properties = Vec::new();
        for field in &component.fields {
            let values = rows
                .iter()
                .map(|row| {
                    row.components
                        .get(&component.id)
                        .and_then(|record| target::field_value(record, &field.schema))
                        .cloned()
                })
                .collect();
            let label = field
                .schema
                .aliases()
                .first()
                .map_or_else(|| field.schema.path().to_string(), String::clone);
            Self {
                target: FieldTarget {
                    selection: selection.clone(),
                    key: FieldKey {
                        component: component.id.clone(),
                        path: field.schema.path().clone(),
                        member: Default::default(),
                    },
                    version: component.version,
                    field: Rc::new(field.schema.clone()),
                    shape: field.schema.shape().clone(),
                },
                label,
                values,
                value: field.value.clone(),
                editable: field.editable,
            }
            .expand(&mut properties);
        }
        properties
    }

    fn expand(self, output: &mut Vec<Self>) {
        let color = self.is_color();
        if let ComponentValueShape::Record { members } = &self.target.shape
            && !members.is_empty()
            && self
                .values
                .iter()
                .all(|value| matches!(value, Some(ComponentValue::Map(_))))
        {
            let start = output.len();
            for member in members {
                let values: Vec<_> = self
                    .values
                    .iter()
                    .map(|value| {
                        value
                            .as_ref()
                            .and_then(|value| value.get(&member.name))
                            .cloned()
                    })
                    .collect();
                let mut target = self.target.clone();
                target.key.member = target
                    .key
                    .member
                    .into_segments()
                    .into_iter()
                    .chain([ComponentFieldPathSegment::field(&member.name)])
                    .collect();
                target.shape = member.shape.clone();
                Self {
                    target,
                    label: if color && member.name != "a" {
                        format!(
                            "{} · Linear {}",
                            self.label,
                            member.name.to_ascii_uppercase()
                        )
                    } else if color {
                        format!("{} · Alpha", self.label)
                    } else {
                        format!("{} · {}", self.label, member.name)
                    },
                    value: target::agreement(&values),
                    values,
                    editable: self.editable,
                }
                .expand(output);
            }
            if color {
                output.insert(start, self);
            }
        } else {
            output.push(self);
        }
    }

    fn is_color(&self) -> bool {
        self.target.key.member.is_empty()
            && self.target.field.editor_hint() == Some(ComponentFieldEditorHint::LinearRgba)
    }

    fn color_seed(&self) -> Option<ColorSeed> {
        ColorSeed::admit(&self.values, shared(&self.value))
    }

    fn choices(&self) -> &[String] {
        if self.target.key.member.is_empty()
            && matches!(
                self.target.shape,
                ComponentValueShape::Scalar {
                    kind: ComponentValueKind::String,
                    ..
                }
            )
        {
            self.target.field.choices()
        } else {
            &[]
        }
    }

    fn apply(
        &self,
        workspace: &Entity<Workspace>,
        expected: &[Option<ComponentValue>],
        value: ComponentValue,
        cx: &mut Context<PropertyEditor>,
    ) -> Result<(), String> {
        workspace.update(cx, |workspace, cx| {
            let Some(command) = self.target.command(&workspace.session, expected, value)? else {
                return Ok(());
            };
            if workspace.session.gesture_active() {
                return Err("Finish the active gesture before editing this field.".into());
            }
            if !workspace.apply(command, cx) {
                return Err("The value was rejected. See the Console for details.".into());
            }
            Ok(())
        })
    }
}

struct Scalar {
    input: Entity<TextInput>,
    _subscription: Subscription,
    initial_text: String,
    expected: Option<Vec<Option<ComponentValue>>>,
    scrub: Option<Scrub>,
}

struct Choice {
    select: Entity<Select<Option<String>>>,
    _subscription: Subscription,
}

struct Scrub {
    view: Entity<NumericScrubber>,
    _subscription: Subscription,
    range: NumericRange,
    gesture: Option<Gesture>,
    rejected: bool,
}

struct Gesture {
    owner: EditorGesture,
    expected: Vec<Option<ComponentValue>>,
}

impl Gesture {
    fn cancel(self, workspace: &mut Workspace, cx: &mut Context<Workspace>) {
        workspace.cancel_gesture(&self.owner, cx);
    }
}

struct Form {
    id: Rc<()>,
    expected: Vec<Option<ComponentValue>>,
    initial: Rc<RootDraft>,
    draft: Rc<RootDraft>,
    inputs: HashMap<DraftInput, (Entity<TextInput>, Subscription)>,
}

struct ColorDraft {
    seed: ColorSeed,
    expected: Vec<Option<ComponentValue>>,
    picker: Entity<ColorPicker>,
}

enum Control {
    ReadOnly,
    Boolean,
    Choice(Choice),
    Scalar(Scalar),
    Structure(Option<Form>),
    Color(Option<ColorDraft>),
}

pub(super) struct PropertyEditor {
    workspace: Entity<Workspace>,
    property: Property,
    control: Control,
    error: Option<String>,
    active: bool,
}

impl PropertyEditor {
    pub fn new(workspace: Entity<Workspace>, property: Property, cx: &mut Context<Self>) -> Self {
        let control = if !property.editable {
            Control::ReadOnly
        } else if property.is_color() {
            Control::Color(None)
        } else if !property.choices().is_empty() {
            Control::Choice(new_choice(&property, cx))
        } else {
            match &property.target.shape {
                ComponentValueShape::Scalar {
                    kind: ComponentValueKind::Bool,
                    ..
                } => Control::Boolean,
                ComponentValueShape::Scalar {
                    kind: ComponentValueKind::Null,
                    ..
                } => Control::ReadOnly,
                ComponentValueShape::Scalar { .. }
                | ComponentValueShape::EntityRef
                | ComponentValueShape::AssetRef { .. } => {
                    let input = cx.new(|cx| {
                        native_text(
                            text_value(&property),
                            &property.label,
                            display_state(&property.value),
                            cx,
                        )
                    });
                    let subscription = cx.subscribe(&input, Self::text_event);
                    Control::Scalar(Scalar {
                        initial_text: input.read(cx).value().to_owned(),
                        input,
                        _subscription: subscription,
                        expected: None,
                        scrub: new_scrub(&property, cx),
                    })
                }
                _ => Control::Structure(None),
            }
        };
        cx.on_release(|editor, cx| {
            if let Some(gesture) = editor.take_gesture() {
                editor
                    .workspace
                    .update(cx, |workspace, cx| gesture.cancel(workspace, cx));
            }
        })
        .detach();
        Self {
            workspace,
            property,
            control,
            error: None,
            active: true,
        }
    }

    pub fn can_refresh(&self, next: &Property) -> bool {
        self.property.target == next.target && self.property.editable == next.editable
    }

    pub(super) fn draft(&self, cx: &App) -> Option<Draft> {
        match &self.control {
            Control::Scalar(text) => {
                let expected = text.expected.as_ref()?;
                let value = text.input.read(cx).value();
                let was_shared = expected.first().is_some_and(|first| {
                    first.is_some() && expected.iter().all(|value| value == first)
                });
                (value != text.initial_text || !was_shared).then(|| Draft::Text(value.to_owned()))
            }
            Control::Structure(Some(form)) if form.draft != form.initial => {
                Some(Draft::Structure(form.draft.clone()))
            }
            Control::Color(Some(draft)) => {
                let value = draft.picker.read(cx).value();
                (value != draft.seed.display()).then_some(Draft::Color(value))
            }
            _ => None,
        }
    }

    pub fn suspend(&mut self, cx: &mut Context<Self>) -> bool {
        self.active = false;
        if let Some(gesture) = self.take_gesture() {
            self.workspace
                .update(cx, |workspace, cx| gesture.cancel(workspace, cx));
        }
        if let Control::Scalar(text) = &mut self.control {
            text.scrub = None;
        }
        self.draft(cx).is_some()
    }

    fn take_gesture(&mut self) -> Option<Gesture> {
        match &mut self.control {
            Control::Scalar(text) => text.scrub.as_mut()?.gesture.take(),
            _ => None,
        }
    }

    pub fn cancel_scrub(&mut self, cx: &mut Context<Self>) {
        if let Some(gesture) = self.take_gesture() {
            self.workspace
                .update(cx, |workspace, cx| gesture.cancel(workspace, cx));
            if let Control::Scalar(text) = &mut self.control {
                text.scrub = new_scrub(&self.property, cx);
            }
            cx.notify();
        }
    }

    pub fn refresh(&mut self, next: Property, cx: &mut Context<Self>) {
        let changed = !self.active || self.property.values != next.values;
        self.active = true;
        self.property = next;
        if changed {
            if self.draft(cx).is_none() {
                self.error = None;
            }
            match &mut self.control {
                Control::Scalar(text) if text.expected.is_none() => {
                    text.input
                        .update(cx, |input, cx| sync_text(input, &self.property, cx));
                    text.initial_text = text.input.read(cx).value().to_owned();
                    if !text
                        .scrub
                        .as_ref()
                        .is_some_and(|scrub| scrub.gesture.is_some() || scrub.rejected)
                    {
                        let next = scrub_spec(&self.property);
                        match (text.scrub.as_mut(), next) {
                            (Some(scrub), Some((value, range))) if scrub.range == range => {
                                scrub
                                    .view
                                    .update(cx, |control, cx| control.set_value(value, cx));
                            }
                            _ => text.scrub = new_scrub(&self.property, cx),
                        }
                    }
                }
                Control::Choice(choice) => {
                    if choice.select.read(cx).is_open() {
                        *choice = new_choice(&self.property, cx);
                    } else {
                        refresh_select(&choice.select, &self.property, cx);
                    }
                }
                _ => {}
            }
            cx.notify();
        }
    }

    fn text_event(
        &mut self,
        input: Entity<TextInput>,
        event: &TextInputEvent,
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
                input.update(cx, |input, cx| sync_text(input, &self.property, cx));
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

    fn set_value(
        &mut self,
        expected: &[Option<ComponentValue>],
        value: ComponentValue,
        cx: &mut Context<Self>,
    ) {
        if !self.active {
            return;
        }
        self.error = self
            .property
            .apply(&self.workspace, expected, value, cx)
            .err();
        cx.notify();
    }

    fn open_form(&mut self, expected: &[Option<ComponentValue>], cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        let Control::Structure(active @ None) = &mut self.control else {
            return;
        };
        if expected != self.property.values
            || !self
                .property
                .target
                .selection
                .matches(&self.workspace.read(cx).session)
        {
            self.error = Some(CHANGED.into());
            cx.notify();
            return;
        }
        let initial = Rc::new(RootDraft {
            node: Some(ValueDraft::from_value(
                &self.property.target.shape,
                shared(&self.property.value),
            )),
        });
        *active = Some(Form {
            id: Rc::new(()),
            expected: expected.to_vec(),
            draft: initial.clone(),
            initial,
            inputs: HashMap::new(),
        });
        self.error = None;
        cx.notify();
    }

    fn open_color(&mut self, expected: &[Option<ComponentValue>], cx: &mut Context<Self>) {
        let Control::Color(draft @ None) = &mut self.control else {
            return;
        };
        if !self.active
            || expected != self.property.values
            || !self
                .property
                .target
                .selection
                .matches(&self.workspace.read(cx).session)
        {
            return;
        }
        let Some(seed) = self.property.color_seed() else {
            return;
        };
        let picker = cx.new(|cx| {
            let mut picker = ColorPicker::new(seed.display(), cx);
            picker.set_style(popup_style(), cx);
            picker
        });
        *draft = Some(ColorDraft {
            seed,
            expected: expected.to_vec(),
            picker,
        });
        self.error = None;
        cx.notify();
    }

    fn close_color(&mut self, picker: EntityId, apply: bool, cx: &mut Context<Self>) {
        let Control::Color(active) = &mut self.control else {
            return;
        };
        let Some(draft) = active.as_ref() else { return };
        if !self.active || draft.picker.entity_id() != picker {
            return;
        }
        self.error = if apply {
            draft
                .seed
                .value(draft.picker.read(cx).value())
                .and_then(|value| {
                    self.property
                        .apply(&self.workspace, &draft.expected, value, cx)
                })
                .err()
        } else {
            None
        };
        if self.error.is_none() {
            *active = None;
        }
        cx.notify();
    }

    fn form_action(&mut self, id: &Rc<()>, action: &DraftAction, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        let Control::Structure(Some(form)) = &mut self.control else {
            return;
        };
        if !Rc::ptr_eq(id, &form.id) {
            return;
        }
        self.error = field_draft::apply(
            Rc::make_mut(&mut form.draft),
            &self.property.target.shape,
            None,
            action,
        )
        .err();
        form.inputs
            .retain(|key, _| field_draft::text(&form.draft, key).is_some());
        cx.notify();
    }

    fn form_text(
        &mut self,
        key: &DraftInput,
        input: Entity<TextInput>,
        event: &TextInputEvent,
        cx: &mut Context<Self>,
    ) {
        if !self.active {
            return;
        }
        let Control::Structure(active @ Some(_)) = &mut self.control else {
            return;
        };
        let Some(form) = active.as_mut() else { return };
        if !form
            .inputs
            .get(key)
            .is_some_and(|(registered, _)| registered.entity_id() == input.entity_id())
        {
            return;
        }
        match event {
            TextInputEvent::Change(text) | TextInputEvent::Submit(text) => {
                self.error = field_draft::set_text(Rc::make_mut(&mut form.draft), key, text).err();
            }
            TextInputEvent::Cancel => {
                *active = None;
                self.error = None;
            }
            TextInputEvent::Blur => return,
        }
        cx.notify();
    }

    fn close_form(&mut self, id: &Rc<()>, commit: bool, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        let Control::Structure(active @ Some(_)) = &mut self.control else {
            return;
        };
        let Some(form) = active.as_ref() else { return };
        if !Rc::ptr_eq(id, &form.id) {
            return;
        }
        self.error = commit
            .then(|| {
                form.draft
                    .node
                    .as_ref()
                    .ok_or_else(|| CHANGED.to_owned())?
                    .value(&self.property.target.shape)
                    .and_then(|value| {
                        self.property
                            .apply(&self.workspace, &form.expected, value, cx)
                    })
            })
            .and_then(Result::err);
        if self.error.is_none() {
            *active = None;
        }
        cx.notify();
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
                if matches!(event, NumericEvent::Cancel(_))
                    || self.error.is_some()
                    || text.expected.is_some()
                {
                    control.update(cx, |control, cx| control.set_value(value, cx));
                }
            }
        } else if self.error.is_some() || text.expected.is_some() {
            scrub.rejected = true;
        }
        cx.notify();
    }
}

impl Render for PropertyEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

fn shared(value: &InspectorValue) -> Option<&ComponentValue> {
    match value {
        InspectorValue::Shared(value) => Some(value),
        _ => None,
    }
}

fn display_state(value: &InspectorValue) -> &'static str {
    match value {
        InspectorValue::Shared(_) => "",
        InspectorValue::Missing => "Unset",
        InspectorValue::Mixed => "—",
    }
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
        Some(ComponentValue::Map(values)) => format!("{} entries", values.len()),
        Some(ComponentValue::List(values)) => format!("{} items", values.len()),
        None => display_state(&property.value).into(),
    }
}

fn text_value(property: &Property) -> String {
    if !value_draft::supported(&property.target.shape) {
        return String::new();
    }
    match ValueDraft::from_value(&property.target.shape, shared(&property.value)) {
        ValueDraft::Text { text, .. } => text,
        _ => String::new(),
    }
}

fn sync_text(input: &mut TextInput, property: &Property, cx: &mut Context<TextInput>) {
    input.set_placeholder(display_state(&property.value), cx);
    input.set_value(text_value(property), cx);
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

fn refresh_select(
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

fn new_choice(property: &Property, cx: &mut Context<PropertyEditor>) -> Choice {
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

fn scrub_spec(property: &Property) -> Option<(f64, NumericRange)> {
    let value = selected_number(property)?;
    Some((value, numeric::range(&property.target.shape, value)?))
}

fn new_scrub(property: &Property, cx: &mut Context<PropertyEditor>) -> Option<Scrub> {
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
