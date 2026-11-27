mod render;
mod scalar;

use std::{collections::HashMap, rc::Rc};

use nara_editor_core::{
    EditorGesture, EditorSession, InspectorComponent, InspectorValue,
    field_draft::{self, DraftAction, DraftInput},
    value_draft::{RootDraft, ValueDraft},
};
use nara_reflect::{
    ComponentFieldEditorHint, ComponentFieldPathSegment, ComponentValue, ComponentValueKind,
    ComponentValueShape,
};
use nara_scene::authoring::SceneEntity;
use open_gpui::{App, AppContext as _, Context, Entity, EntityId, Subscription, WindowId};
use open_gpui_base::{
    ColorPicker, NumericRange, NumericScrubber, Select, TextInput, TextInputEvent,
};

use crate::workspace::Workspace;

use super::{
    Draft,
    color::ColorSeed,
    native_text, popup_style,
    target::{self, CHANGED, FieldKey, FieldTarget, Selection},
};
use scalar::{new_choice, new_scrub, refresh_select, scrub_spec, sync_text, text_value};

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
    window_id: Option<WindowId>,
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
                | ComponentValueShape::AssetRef { .. }
                | ComponentValueShape::StableAssetRef { .. } => {
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
                        window_id: None,
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

    pub fn can_resume(&self, session: &EditorSession) -> bool {
        self.property.editable && self.property.target.is_eligible(session)
    }

    #[cfg(all(test, windows))]
    pub(super) fn text_input(&self) -> Option<&Entity<TextInput>> {
        match &self.control {
            Control::Scalar(text) => Some(&text.input),
            _ => None,
        }
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
