use dear_imgui_rs::Ui;
use nara::{
    reflect::{ComponentValue, ComponentValueShape},
    tooling::{EditorFieldTarget, SceneInspectorEntityRow},
};

use super::{
    super::{ImguiTextOwner, observe_text_owner},
    value_draft::{ChildDraft, RootDraft, ValueDraft},
};

pub(in crate::frontend::imgui) fn show_root(
    ui: &Ui,
    target: &EditorFieldTarget,
    entities: &[SceneInspectorEntityRow],
    shape: &ComponentValueShape,
    source: Option<&ComponentValue>,
    draft: &mut RootDraft,
    editable: bool,
    can_submit: bool,
    text_owner: &mut Option<ImguiTextOwner>,
) -> bool {
    if draft.node.is_none() {
        ui.text_wrapped(super::value_label(source));
        if button(ui, target, "root", "Edit value", editable) {
            draft.node = Some(ValueDraft::from_value(shape, source));
        }
        return false;
    }
    ui.with_disabled_if(!editable, || {
        if let Some(node) = &mut draft.node {
            show_node(ui, target, entities, "root", shape, node, text_owner);
        }
    });
    let apply = button(ui, target, "root", "Apply value", editable && can_submit);
    ui.same_line();
    if button(ui, target, "root", "Cancel value", editable) {
        draft.node = None;
    }
    apply
}

fn show_node(
    ui: &Ui,
    target: &EditorFieldTarget,
    entities: &[SceneInspectorEntityRow],
    path: &str,
    shape: &ComponentValueShape,
    draft: &mut ValueDraft,
    text_owner: &mut Option<ImguiTextOwner>,
) {
    match (draft, shape) {
        (ValueDraft::Null, _) => ui.text("Null"),
        (ValueDraft::Bool(value), _) => {
            ui.text(value.map_or(
                "Choose a boolean",
                |value| if value { "true" } else { "false" },
            ));
            if button(ui, target, path, "true", true) {
                *value = Some(true);
            }
            ui.same_line();
            if button(ui, target, path, "false", true) {
                *value = Some(false);
            }
        }
        (ValueDraft::Text { text, entered }, ComponentValueShape::EntityRef) => {
            if super::super::field_editor::entity_reference::show(ui, entities, text) {
                *entered = true;
            }
        }
        (ValueDraft::Text { text, entered }, _) => {
            if ui.input_text("##value", text).build() {
                *entered = true;
            }
            record(ui, target, path, "value");
            observe_text_owner(ui, text_owner);
            if !*entered {
                ui.text_disabled("Enter a value before confirming");
                if matches!(
                    shape,
                    ComponentValueShape::Scalar {
                        kind: nara::reflect::ComponentValueKind::String,
                        ..
                    }
                ) && button(ui, target, path, "Use empty string", true)
                {
                    *entered = true;
                }
            }
        }
        (
            ValueDraft::Record {
                members: values,
                confirmed,
            },
            ComponentValueShape::Record { members },
        ) => {
            if !*confirmed && button(ui, target, path, "Use empty record", true) {
                *confirmed = true;
            }
            for ((name, draft), member) in values.iter_mut().zip(members) {
                let _member_id = ui.push_id(name.as_str());
                ui.text(name.as_str());
                ui.indent();
                show_node(
                    ui,
                    target,
                    entities,
                    &format!("{path}.{name}"),
                    &member.shape,
                    draft,
                    text_owner,
                );
                ui.unindent();
            }
        }
        (
            ValueDraft::Optional {
                value,
                pending,
                chosen,
            },
            ComponentValueShape::Optional { value: shape },
        ) => {
            ui.text(if value.is_some() {
                "Some"
            } else if *chosen {
                "Null"
            } else {
                "Choose Null or Some"
            });
            if button(ui, target, path, "Set Null", true) {
                *value = None;
                *pending = None;
                *chosen = true;
            }
            ui.same_line();
            if button(ui, target, path, "Some...", pending.is_none()) {
                *pending = Some(Box::new(ValueDraft::new(shape)));
            }
            if let Some(child) = value {
                let _child_id = ui.push_id("some");
                ui.indent();
                show_node(
                    ui,
                    target,
                    entities,
                    &format!("{path}.some"),
                    shape,
                    child,
                    text_owner,
                );
                ui.unindent();
            }
            if let Some(child) = pending {
                let _child_id = ui.push_id("new-some");
                ui.text("New Some value (unconfirmed)");
                ui.indent();
                show_node(
                    ui,
                    target,
                    entities,
                    &format!("{path}.new"),
                    shape,
                    child,
                    text_owner,
                );
                ui.unindent();
                let admission = child.value(shape);
                if let Err(error) = &admission {
                    ui.text_disabled(error);
                }
                if button(ui, target, path, "Use Some", admission.is_ok()) {
                    *value = pending.take();
                    *chosen = true;
                }
                ui.same_line();
                if button(ui, target, path, "Cancel Some", true) {
                    *pending = None;
                }
            }
        }
        (
            ValueDraft::List {
                items,
                pending,
                confirmed,
            },
            ComponentValueShape::List { item },
        ) => {
            let mut remove = None;
            for (index, child) in items.iter_mut().enumerate() {
                let _row_id = ui.push_id(&child.id.to_string());
                let path = format!("{path}[{index}]");
                ui.text(format!("Item {index}"));
                show_node(
                    ui,
                    target,
                    entities,
                    &path,
                    item,
                    &mut child.node,
                    text_owner,
                );
                if button(ui, target, &path, "Remove item", true) {
                    remove = Some(index);
                }
            }
            if let Some(index) = remove {
                items.remove(index);
            }
            if !*confirmed && button(ui, target, path, "Use empty list", true) {
                *confirmed = true;
            }
            if button(ui, target, path, "Add...", pending.is_none()) {
                *pending = Some(Box::new(ChildDraft::new(ValueDraft::new(item))));
            }
            if let Some(child) = pending {
                let _row_id = ui.push_id(&child.id.to_string());
                ui.text("New list item (unconfirmed)");
                show_node(
                    ui,
                    target,
                    entities,
                    &format!("{path}.new"),
                    item,
                    &mut child.node,
                    text_owner,
                );
                let admission = child.node.value(item);
                if let Err(error) = &admission {
                    ui.text_disabled(error);
                }
                if button(ui, target, path, "Add value", admission.is_ok()) {
                    items.push(*pending.take().expect("the item draft is open"));
                    *confirmed = true;
                }
                ui.same_line();
                if button(ui, target, path, "Cancel item", true) {
                    *pending = None;
                }
            }
        }
        (
            ValueDraft::Map {
                entries,
                pending,
                confirmed,
            },
            ComponentValueShape::Map { value: shape },
        ) => {
            let mut remove = None;
            for (key, child) in entries.iter_mut() {
                let _row_id = ui.push_id(&child.id.to_string());
                let path = format!("{path}[{key:?}]");
                ui.text(key);
                show_node(
                    ui,
                    target,
                    entities,
                    &path,
                    shape,
                    &mut child.node,
                    text_owner,
                );
                if button(ui, target, &path, "Remove entry", true) {
                    remove = Some(key.clone());
                }
            }
            if let Some(key) = remove {
                entries.remove(&key);
            }
            if !*confirmed && button(ui, target, path, "Use empty map", true) {
                *confirmed = true;
            }
            if button(ui, target, path, "Insert...", pending.is_none()) {
                *pending = Some((
                    String::new(),
                    false,
                    Box::new(ChildDraft::new(ValueDraft::new(shape))),
                ));
            }
            if let Some((key, key_entered, child)) = pending {
                let _row_id = ui.push_id(&child.id.to_string());
                ui.text("New map entry (unconfirmed)");
                if ui.input_text("Key", key).build() {
                    *key_entered = true;
                }
                record(ui, target, path, "key");
                observe_text_owner(ui, text_owner);
                if !*key_entered && button(ui, target, path, "Use empty key", true) {
                    *key_entered = true;
                }
                show_node(
                    ui,
                    target,
                    entities,
                    &format!("{path}.new"),
                    shape,
                    &mut child.node,
                    text_owner,
                );
                let admission = child.node.value(shape);
                if let Err(error) = &admission {
                    ui.text_disabled(error);
                }
                let duplicate = entries.contains_key(key);
                if duplicate {
                    ui.text_disabled("This key already exists");
                }
                if button(
                    ui,
                    target,
                    path,
                    "Insert value",
                    admission.is_ok() && !duplicate && *key_entered,
                ) {
                    let (key, _, child) = pending.take().expect("the entry draft is open");
                    entries.insert(key, *child);
                    *confirmed = true;
                }
                ui.same_line();
                if button(ui, target, path, "Cancel entry", true) {
                    *pending = None;
                }
            }
        }
        _ => ui.text_disabled("The field shape changed; cancel and reopen the draft"),
    }
}

fn button(ui: &Ui, target: &EditorFieldTarget, path: &str, label: &str, enabled: bool) -> bool {
    let clicked = ui.with_disabled_if(!enabled, || ui.button(label));
    record(ui, target, path, label);
    clicked
}

fn record(_ui: &Ui, _target: &EditorFieldTarget, _path: &str, _control: &str) {
    #[cfg(test)]
    super::super::interaction_tests::record_control(
        _ui,
        format!(
            "recursive:{}:{}:{_path}:{_control}",
            _target.component().as_str(),
            _target.field().as_str(),
        ),
    );
}
