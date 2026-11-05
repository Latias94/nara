use nara::reflect::{ComponentValue, ComponentValueKind, ComponentValueShape};

use super::super::value_draft::{self, ChildDraft, RootDraft, ValueDraft};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::frontend) struct DraftPath(Vec<PathSegment>);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PathSegment {
    Member(String),
    Child { id: u64, role: ChildRole },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ChildRole {
    Some,
    PendingSome,
    ListItem,
    PendingListItem,
    MapEntry,
    PendingMapEntry,
}

impl DraftPath {
    fn member(&self, name: &str) -> Self {
        let mut path = self.clone();
        path.0.push(PathSegment::Member(name.to_owned()));
        path
    }

    fn child(&self, child: &ChildDraft, role: ChildRole) -> Self {
        let mut path = self.clone();
        path.0.push(PathSegment::Child { id: child.id, role });
        path
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::frontend) enum DraftInput {
    Value(DraftPath),
    MapKey { parent: DraftPath, child: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::frontend) struct DraftAction {
    pub path: DraftPath,
    pub operation: StructureOperation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::frontend) enum StructureOperation {
    Open,
    Boolean(bool),
    EmptyString,
    EmptyRecord,
    SetNull,
    BeginSome,
    ConfirmSome,
    CancelSome,
    EmptyList,
    BeginItem,
    ConfirmItem,
    CancelItem,
    RemoveItem(u64),
    EmptyMap,
    BeginEntry,
    ConfirmEntry,
    CancelEntry,
    RemoveEntry(u64),
    EmptyKey,
}

pub(in crate::frontend) enum StructuredRow {
    Label(String),
    Text {
        input: DraftInput,
        label: String,
        text: String,
    },
    Button {
        action: DraftAction,
        label: &'static str,
        name: String,
        enabled: bool,
    },
}

pub(in crate::frontend) fn rows(
    root: &RootDraft,
    shape: &ComponentValueShape,
) -> Vec<StructuredRow> {
    let mut rows = Vec::new();
    if !value_draft::supported(shape) {
        rows.push(StructuredRow::Label(
            "This field shape is read-only in the generic Inspector".to_owned(),
        ));
    } else if let Some(node) = &root.node {
        node_rows(&mut rows, &DraftPath::default(), "root", node, shape);
    } else {
        button(
            &mut rows,
            &DraftPath::default(),
            "root",
            StructureOperation::Open,
            "Edit value",
            true,
        );
    }
    rows
}

fn button(
    rows: &mut Vec<StructuredRow>,
    path: &DraftPath,
    display_path: &str,
    operation: StructureOperation,
    label: &'static str,
    enabled: bool,
) {
    rows.push(StructuredRow::Button {
        action: DraftAction {
            path: path.clone(),
            operation,
        },
        label,
        name: format!("{display_path}: {label}"),
        enabled,
    });
}

fn node_rows(
    rows: &mut Vec<StructuredRow>,
    path: &DraftPath,
    display_path: &str,
    node: &ValueDraft,
    shape: &ComponentValueShape,
) {
    match (node, shape) {
        (
            ValueDraft::Null,
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::Null,
                ..
            },
        ) => rows.push(StructuredRow::Label(format!("{display_path}: Null"))),
        (
            ValueDraft::Bool(value),
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::Bool,
                ..
            },
        ) => {
            rows.push(StructuredRow::Label(display_path.to_owned()));
            rows.push(StructuredRow::Label(
                value
                    .map_or(
                        "Choose a boolean",
                        |value| if value { "true" } else { "false" },
                    )
                    .to_owned(),
            ));
            for (value, label) in [(true, "true"), (false, "false")] {
                button(
                    rows,
                    path,
                    display_path,
                    StructureOperation::Boolean(value),
                    label,
                    true,
                );
            }
        }
        (ValueDraft::Text { text, entered }, _) if text_shape(shape) => {
            rows.push(StructuredRow::Text {
                input: DraftInput::Value(path.clone()),
                label: display_path.to_owned(),
                text: text.clone(),
            });
            if !entered {
                rows.push(StructuredRow::Label(
                    "Enter a value before confirming".to_owned(),
                ));
                if matches!(
                    shape,
                    ComponentValueShape::Scalar {
                        kind: ComponentValueKind::String,
                        ..
                    }
                ) {
                    button(
                        rows,
                        path,
                        display_path,
                        StructureOperation::EmptyString,
                        "Use empty string",
                        true,
                    );
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
            rows.push(StructuredRow::Label(display_path.to_owned()));
            if !confirmed {
                button(
                    rows,
                    path,
                    display_path,
                    StructureOperation::EmptyRecord,
                    "Use empty record",
                    true,
                );
            }
            for member in members {
                if let Some((name, child)) = values.iter().find(|(name, _)| name == &member.name) {
                    rows.push(StructuredRow::Label(name.clone()));
                    node_rows(
                        rows,
                        &path.member(name),
                        &format!("{display_path}.{name}"),
                        child,
                        &member.shape,
                    );
                }
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
            rows.push(StructuredRow::Label(display_path.to_owned()));
            rows.push(StructuredRow::Label(
                if value.is_some() {
                    "Some"
                } else if *chosen {
                    "Null"
                } else {
                    "Choose Null or Some"
                }
                .to_owned(),
            ));
            button(
                rows,
                path,
                display_path,
                StructureOperation::SetNull,
                "Set Null",
                true,
            );
            button(
                rows,
                path,
                display_path,
                StructureOperation::BeginSome,
                "Some...",
                pending.is_none(),
            );
            if let Some(child) = value {
                node_rows(
                    rows,
                    &path.child(child, ChildRole::Some),
                    &format!("{display_path}.some"),
                    &child.node,
                    shape,
                );
            }
            if let Some(child) = pending {
                let pending_path = path.child(child, ChildRole::PendingSome);
                rows.push(StructuredRow::Label(
                    "New Some value (unconfirmed)".to_owned(),
                ));
                node_rows(
                    rows,
                    &pending_path,
                    &format!("{display_path}.new"),
                    &child.node,
                    shape,
                );
                let admission = child.node.value(shape);
                if let Err(error) = &admission {
                    rows.push(StructuredRow::Label(error.clone()));
                }
                button(
                    rows,
                    &pending_path,
                    display_path,
                    StructureOperation::ConfirmSome,
                    "Use Some",
                    admission.is_ok(),
                );
                button(
                    rows,
                    &pending_path,
                    display_path,
                    StructureOperation::CancelSome,
                    "Cancel Some",
                    true,
                );
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
            rows.push(StructuredRow::Label(display_path.to_owned()));
            for (index, child) in items.iter().enumerate() {
                let child_display = format!("{display_path}[{index}]");
                rows.push(StructuredRow::Label(format!("Item {index}")));
                node_rows(
                    rows,
                    &path.child(child, ChildRole::ListItem),
                    &child_display,
                    &child.node,
                    item,
                );
                button(
                    rows,
                    path,
                    &child_display,
                    StructureOperation::RemoveItem(child.id),
                    "Remove item",
                    true,
                );
            }
            if !confirmed {
                button(
                    rows,
                    path,
                    display_path,
                    StructureOperation::EmptyList,
                    "Use empty list",
                    true,
                );
            }
            button(
                rows,
                path,
                display_path,
                StructureOperation::BeginItem,
                "Add...",
                pending.is_none(),
            );
            if let Some(child) = pending {
                let pending_path = path.child(child, ChildRole::PendingListItem);
                rows.push(StructuredRow::Label(
                    "New list item (unconfirmed)".to_owned(),
                ));
                node_rows(
                    rows,
                    &pending_path,
                    &format!("{display_path}.new"),
                    &child.node,
                    item,
                );
                let admission = child.node.value(item);
                if let Err(error) = &admission {
                    rows.push(StructuredRow::Label(error.clone()));
                }
                button(
                    rows,
                    &pending_path,
                    display_path,
                    StructureOperation::ConfirmItem,
                    "Add value",
                    admission.is_ok(),
                );
                button(
                    rows,
                    &pending_path,
                    display_path,
                    StructureOperation::CancelItem,
                    "Cancel item",
                    true,
                );
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
            rows.push(StructuredRow::Label(display_path.to_owned()));
            for (key, child) in entries {
                let child_display = format!("{display_path}[{key:?}]");
                rows.push(StructuredRow::Label(key.clone()));
                node_rows(
                    rows,
                    &path.child(child, ChildRole::MapEntry),
                    &child_display,
                    &child.node,
                    shape,
                );
                button(
                    rows,
                    path,
                    &child_display,
                    StructureOperation::RemoveEntry(child.id),
                    "Remove entry",
                    true,
                );
            }
            if !confirmed {
                button(
                    rows,
                    path,
                    display_path,
                    StructureOperation::EmptyMap,
                    "Use empty map",
                    true,
                );
            }
            button(
                rows,
                path,
                display_path,
                StructureOperation::BeginEntry,
                "Insert...",
                pending.is_none(),
            );
            if let Some((key, key_entered, child)) = pending {
                let pending_path = path.child(child, ChildRole::PendingMapEntry);
                rows.push(StructuredRow::Label(
                    "New map entry (unconfirmed)".to_owned(),
                ));
                rows.push(StructuredRow::Text {
                    input: DraftInput::MapKey {
                        parent: path.clone(),
                        child: child.id,
                    },
                    label: format!("{display_path}.key"),
                    text: key.clone(),
                });
                if !key_entered {
                    button(
                        rows,
                        &pending_path,
                        display_path,
                        StructureOperation::EmptyKey,
                        "Use empty key",
                        true,
                    );
                }
                node_rows(
                    rows,
                    &pending_path,
                    &format!("{display_path}.new"),
                    &child.node,
                    shape,
                );
                let admission = child.node.value(shape);
                if let Err(error) = &admission {
                    rows.push(StructuredRow::Label(error.clone()));
                }
                let duplicate = entries.contains_key(key);
                if duplicate {
                    rows.push(StructuredRow::Label("This key already exists".to_owned()));
                }
                button(
                    rows,
                    &pending_path,
                    display_path,
                    StructureOperation::ConfirmEntry,
                    "Insert value",
                    admission.is_ok() && !duplicate && *key_entered,
                );
                button(
                    rows,
                    &pending_path,
                    display_path,
                    StructureOperation::CancelEntry,
                    "Cancel entry",
                    true,
                );
            }
        }
        _ => rows.push(StructuredRow::Label(
            "The field shape changed; cancel and reopen the draft".to_owned(),
        )),
    }
}

pub(in crate::frontend) fn apply(
    root: &mut RootDraft,
    shape: &ComponentValueShape,
    source: Option<&ComponentValue>,
    action: &DraftAction,
) -> Result<(), String> {
    if !value_draft::supported(shape) {
        return Err("This field shape is read-only in the generic Inspector".to_owned());
    }
    if action.operation == StructureOperation::Open {
        if !action.path.0.is_empty() || root.node.is_some() {
            return Err(unavailable());
        }
        root.node = Some(ValueDraft::from_value(shape, source));
        return Ok(());
    }
    let pending_role = match action.operation {
        StructureOperation::ConfirmSome | StructureOperation::CancelSome => {
            Some(ChildRole::PendingSome)
        }
        StructureOperation::ConfirmItem | StructureOperation::CancelItem => {
            Some(ChildRole::PendingListItem)
        }
        StructureOperation::ConfirmEntry
        | StructureOperation::CancelEntry
        | StructureOperation::EmptyKey => Some(ChildRole::PendingMapEntry),
        _ => None,
    };
    let (segments, pending_id) = if let Some(expected) = pending_role {
        match action.path.0.split_last() {
            Some((PathSegment::Child { id, role }, parent)) if *role == expected => {
                (parent, Some(*id))
            }
            _ => return Err(unavailable()),
        }
    } else {
        (action.path.0.as_slice(), None)
    };
    let shape = shape_at(shape, segments).ok_or_else(unavailable)?;
    let node = node_mut(root, segments).ok_or_else(unavailable)?;
    match (node, shape, action.operation) {
        (
            ValueDraft::Bool(value),
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::Bool,
                ..
            },
            StructureOperation::Boolean(next),
        ) => {
            *value = Some(next);
        }
        (
            ValueDraft::Text { entered, .. },
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::String,
                ..
            },
            StructureOperation::EmptyString,
        ) if !*entered => {
            *entered = true;
        }
        (
            ValueDraft::Record { confirmed, .. },
            ComponentValueShape::Record { .. },
            StructureOperation::EmptyRecord,
        ) if !*confirmed => {
            *confirmed = true;
        }
        (
            ValueDraft::Optional {
                value,
                pending,
                chosen,
            },
            ComponentValueShape::Optional { value: shape },
            operation,
        ) => match operation {
            StructureOperation::SetNull => {
                *value = None;
                *pending = None;
                *chosen = true;
            }
            StructureOperation::BeginSome if pending.is_none() => {
                *pending = Some(Box::new(ChildDraft::new(ValueDraft::new(shape))));
            }
            StructureOperation::ConfirmSome => {
                pending_child(pending, pending_id)?.node.value(shape)?;
                *value = pending.take();
                *chosen = true;
            }
            StructureOperation::CancelSome => {
                pending_child(pending, pending_id)?;
                *pending = None;
            }
            _ => return Err(unavailable()),
        },
        (
            ValueDraft::List {
                items,
                pending,
                confirmed,
            },
            ComponentValueShape::List { item },
            operation,
        ) => match operation {
            StructureOperation::EmptyList if !*confirmed => *confirmed = true,
            StructureOperation::BeginItem if pending.is_none() => {
                *pending = Some(Box::new(ChildDraft::new(ValueDraft::new(item))));
            }
            StructureOperation::ConfirmItem => {
                pending_child(pending, pending_id)?.node.value(item)?;
                let child = pending.take().ok_or_else(unavailable)?;
                items.push(*child);
                *confirmed = true;
            }
            StructureOperation::CancelItem => {
                pending_child(pending, pending_id)?;
                *pending = None;
            }
            StructureOperation::RemoveItem(id) => {
                let index = items
                    .iter()
                    .position(|child| child.id == id)
                    .ok_or_else(unavailable)?;
                items.remove(index);
            }
            _ => return Err(unavailable()),
        },
        (
            ValueDraft::Map {
                entries,
                pending,
                confirmed,
            },
            ComponentValueShape::Map { value: shape },
            operation,
        ) => match operation {
            StructureOperation::EmptyMap if !*confirmed => *confirmed = true,
            StructureOperation::BeginEntry if pending.is_none() => {
                *pending = Some((
                    String::new(),
                    false,
                    Box::new(ChildDraft::new(ValueDraft::new(shape))),
                ));
            }
            StructureOperation::ConfirmEntry => {
                let (key, key_entered, child) = pending
                    .as_ref()
                    .filter(|(_, _, child)| Some(child.id) == pending_id)
                    .ok_or_else(unavailable)?;
                if !key_entered {
                    return Err("Enter a key explicitly".to_owned());
                }
                child.node.value(shape)?;
                if entries.contains_key(key) {
                    return Err("This key already exists".to_owned());
                }
                let (key, _, child) = pending.take().ok_or_else(unavailable)?;
                entries.insert(key, *child);
                *confirmed = true;
            }
            StructureOperation::CancelEntry => {
                if !pending
                    .as_ref()
                    .is_some_and(|(_, _, child)| Some(child.id) == pending_id)
                {
                    return Err(unavailable());
                }
                *pending = None;
            }
            StructureOperation::RemoveEntry(id) => {
                let key = entries
                    .iter()
                    .find(|(_, child)| child.id == id)
                    .map(|(key, _)| key.clone())
                    .ok_or_else(unavailable)?;
                entries.remove(&key);
            }
            StructureOperation::EmptyKey => {
                let (_, key_entered, _) = pending
                    .as_mut()
                    .filter(|(_, _, child)| Some(child.id) == pending_id)
                    .ok_or_else(unavailable)?;
                if *key_entered {
                    return Err(unavailable());
                }
                *key_entered = true;
            }
            _ => return Err(unavailable()),
        },
        _ => return Err(unavailable()),
    }
    Ok(())
}

fn pending_child(
    pending: &Option<Box<ChildDraft>>,
    id: Option<u64>,
) -> Result<&ChildDraft, String> {
    pending
        .as_deref()
        .filter(|child| Some(child.id) == id)
        .ok_or_else(unavailable)
}

fn unavailable() -> String {
    "This draft action is no longer available".to_owned()
}

fn text_shape(shape: &ComponentValueShape) -> bool {
    match shape {
        ComponentValueShape::Scalar { kind, .. } => {
            !matches!(kind, ComponentValueKind::Null | ComponentValueKind::Bool)
        }
        ComponentValueShape::EntityRef | ComponentValueShape::AssetRef { .. } => true,
        _ => false,
    }
}

pub(in crate::frontend) fn is_entity_reference(
    shape: &ComponentValueShape,
    input: &DraftInput,
) -> bool {
    matches!(input, DraftInput::Value(path)
        if shape_at(shape, &path.0) == Some(&ComponentValueShape::EntityRef))
}

fn shape_at<'a>(
    mut shape: &'a ComponentValueShape,
    segments: &[PathSegment],
) -> Option<&'a ComponentValueShape> {
    for segment in segments {
        shape = match (shape, segment) {
            (ComponentValueShape::Record { members }, PathSegment::Member(name)) => {
                &members.iter().find(|member| &member.name == name)?.shape
            }
            (
                ComponentValueShape::Optional { value },
                PathSegment::Child {
                    role: ChildRole::Some | ChildRole::PendingSome,
                    ..
                },
            ) => value,
            (
                ComponentValueShape::List { item },
                PathSegment::Child {
                    role: ChildRole::ListItem | ChildRole::PendingListItem,
                    ..
                },
            ) => item,
            (
                ComponentValueShape::Map { value },
                PathSegment::Child {
                    role: ChildRole::MapEntry | ChildRole::PendingMapEntry,
                    ..
                },
            ) => value,
            _ => return None,
        };
    }
    Some(shape)
}

fn node<'a>(root: &'a RootDraft, segments: &[PathSegment]) -> Option<&'a ValueDraft> {
    let mut node = root.node.as_ref()?;
    for segment in segments {
        node = match (node, segment) {
            (ValueDraft::Record { members, .. }, PathSegment::Member(name)) => members
                .iter()
                .find(|(member, _)| member == name)
                .map(|(_, node)| node),
            (
                ValueDraft::Optional { value, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::Some,
                },
            ) => value
                .as_deref()
                .filter(|child| child.id == *id)
                .map(|child| &child.node),
            (
                ValueDraft::Optional { pending, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::PendingSome,
                },
            )
            | (
                ValueDraft::List { pending, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::PendingListItem,
                },
            ) => pending
                .as_deref()
                .filter(|child| child.id == *id)
                .map(|child| &child.node),
            (
                ValueDraft::List { items, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::ListItem,
                },
            ) => items
                .iter()
                .find(|child| child.id == *id)
                .map(|child| &child.node),
            (
                ValueDraft::Map { entries, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::MapEntry,
                },
            ) => entries
                .values()
                .find(|child| child.id == *id)
                .map(|child| &child.node),
            (
                ValueDraft::Map { pending, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::PendingMapEntry,
                },
            ) => pending
                .as_ref()
                .filter(|(_, _, child)| child.id == *id)
                .map(|(_, _, child)| &child.node),
            _ => return None,
        }?;
    }
    Some(node)
}

fn node_mut<'a>(root: &'a mut RootDraft, segments: &[PathSegment]) -> Option<&'a mut ValueDraft> {
    let mut node = root.node.as_mut()?;
    for segment in segments {
        node = match (node, segment) {
            (ValueDraft::Record { members, .. }, PathSegment::Member(name)) => members
                .iter_mut()
                .find(|(member, _)| member == name)
                .map(|(_, node)| node),
            (
                ValueDraft::Optional { value, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::Some,
                },
            ) => value
                .as_deref_mut()
                .filter(|child| child.id == *id)
                .map(|child| &mut child.node),
            (
                ValueDraft::Optional { pending, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::PendingSome,
                },
            )
            | (
                ValueDraft::List { pending, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::PendingListItem,
                },
            ) => pending
                .as_deref_mut()
                .filter(|child| child.id == *id)
                .map(|child| &mut child.node),
            (
                ValueDraft::List { items, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::ListItem,
                },
            ) => items
                .iter_mut()
                .find(|child| child.id == *id)
                .map(|child| &mut child.node),
            (
                ValueDraft::Map { entries, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::MapEntry,
                },
            ) => entries
                .values_mut()
                .find(|child| child.id == *id)
                .map(|child| &mut child.node),
            (
                ValueDraft::Map { pending, .. },
                PathSegment::Child {
                    id,
                    role: ChildRole::PendingMapEntry,
                },
            ) => pending
                .as_mut()
                .filter(|(_, _, child)| child.id == *id)
                .map(|(_, _, child)| &mut child.node),
            _ => return None,
        }?;
    }
    Some(node)
}

pub(in crate::frontend) fn text<'a>(root: &'a RootDraft, input: &DraftInput) -> Option<&'a str> {
    match input {
        DraftInput::Value(path) => match node(root, &path.0)? {
            ValueDraft::Text { text, .. } => Some(text),
            _ => None,
        },
        DraftInput::MapKey { parent, child } => match node(root, &parent.0)? {
            ValueDraft::Map {
                pending: Some((key, _, pending)),
                ..
            } if pending.id == *child => Some(key),
            _ => None,
        },
    }
}

pub(in crate::frontend) fn set_text(
    root: &mut RootDraft,
    input: &DraftInput,
    text: &str,
) -> Result<(), String> {
    let (target, entered) = match input {
        DraftInput::Value(path) => match node_mut(root, &path.0) {
            Some(ValueDraft::Text { text, entered }) => (text, entered),
            _ => return Err("This draft input is no longer available".to_owned()),
        },
        DraftInput::MapKey { parent, child } => match node_mut(root, &parent.0) {
            Some(ValueDraft::Map {
                pending: Some((key, entered, pending)),
                ..
            }) if pending.id == *child => (key, entered),
            _ => return Err("This draft input is no longer available".to_owned()),
        },
    };
    *target = text.to_owned();
    *entered = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(root: &RootDraft, shape: &ComponentValueShape, name: &str) -> DraftAction {
        rows(root, shape)
            .into_iter()
            .find_map(|row| match row {
                StructuredRow::Button {
                    action,
                    name: current,
                    ..
                } if current == name => Some(action),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing button {name}"))
    }

    fn input(root: &RootDraft, shape: &ComponentValueShape, label: &str) -> DraftInput {
        rows(root, shape)
            .into_iter()
            .find_map(|row| match row {
                StructuredRow::Text {
                    input,
                    label: current,
                    ..
                } if current == label => Some(input),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing input {label}"))
    }

    fn press(root: &mut RootDraft, shape: &ComponentValueShape, name: &str) {
        let action = action(root, shape, name);
        apply(root, shape, None, &action).unwrap();
    }

    fn open(shape: &ComponentValueShape, source: Option<&ComponentValue>) -> RootDraft {
        let mut root = RootDraft::default();
        let action = action(&root, shape, "root: Edit value");
        apply(&mut root, shape, source, &action).unwrap();
        root
    }

    fn last_child_id(input: &DraftInput) -> u64 {
        match input {
            DraftInput::Value(DraftPath(segments)) => match segments.last() {
                Some(PathSegment::Child { id, .. }) => *id,
                _ => panic!("child input"),
            },
            DraftInput::MapKey { child, .. } => *child,
        }
    }

    #[test]
    fn retired_optional_identity_cannot_target_a_replacement() {
        let shape =
            ComponentValueShape::optional(ComponentValueShape::scalar(ComponentValueKind::String));
        let mut root = open(&shape, Some(&ComponentValue::String("original".to_owned())));
        let original = input(&root, &shape, "root.some");
        press(&mut root, &shape, "root: Some...");
        let retired = input(&root, &shape, "root.new");
        let retired_confirm = action(&root, &shape, "root: Use Some");
        let retired_cancel = action(&root, &shape, "root: Cancel Some");
        set_text(&mut root, &retired, "unconfirmed 原文").unwrap();
        assert!(
            root.node
                .as_ref()
                .unwrap()
                .copy_text()
                .contains("unconfirmed 原文")
        );
        press(&mut root, &shape, "root: Cancel Some");
        press(&mut root, &shape, "root: Some...");
        let replacement = input(&root, &shape, "root.new");
        assert_ne!(last_child_id(&retired), last_child_id(&replacement));
        let before = root.clone();
        assert!(set_text(&mut root, &retired, "stale text").is_err());
        assert_eq!(root, before);
        for stale in [retired_confirm, retired_cancel] {
            assert!(apply(&mut root, &shape, None, &stale).is_err());
            assert_eq!(root, before);
        }
        set_text(&mut root, &replacement, "replacement").unwrap();
        press(&mut root, &shape, "root: Use Some");
        let confirmed = input(&root, &shape, "root.some");
        assert_eq!(last_child_id(&replacement), last_child_id(&confirmed));
        assert_ne!(replacement, confirmed);
        assert_eq!(text(&root, &replacement), None);
        let before = root.clone();
        assert!(set_text(&mut root, &original, "retired original").is_err());
        assert_eq!(root, before);
        assert_eq!(text(&root, &confirmed), Some("replacement"));
        press(&mut root, &shape, "root: Set Null");
        press(&mut root, &shape, "root: Some...");
        press(&mut root, &shape, "root.new: Use empty string");
        press(&mut root, &shape, "root: Use Some");
        let before = root.clone();
        assert!(set_text(&mut root, &confirmed, "retired Some").is_err());
        assert_eq!(root, before);
        assert_eq!(
            root.node.as_ref().unwrap().value(&shape).unwrap(),
            ComponentValue::String(String::new())
        );
    }

    #[test]
    fn unrelated_list_child_identity_survives_removal_and_pending_confirmation() {
        let shape = ComponentValueShape::list(ComponentValueShape::optional(
            ComponentValueShape::scalar(ComponentValueKind::I64),
        ));
        let source = ComponentValue::List(vec![ComponentValue::I64(1), ComponentValue::I64(2)]);
        let mut root = open(&shape, Some(&source));
        let survivor = input(&root, &shape, "root[1].some");
        let remove = action(&root, &shape, "root[0]: Remove item");
        apply(&mut root, &shape, None, &remove).unwrap();
        assert_eq!(input(&root, &shape, "root[0].some"), survivor);
        assert_eq!(text(&root, &survivor), Some("2"));
        let before = root.clone();
        assert!(apply(&mut root, &shape, None, &remove).is_err());
        assert_eq!(root, before);
        press(&mut root, &shape, "root: Add...");
        press(&mut root, &shape, "root.new: Some...");
        let new_some = input(&root, &shape, "root.new.new");
        set_text(&mut root, &new_some, "3").unwrap();
        press(&mut root, &shape, "root.new: Use Some");
        let pending_leaf = input(&root, &shape, "root.new.some");
        let pending_id = match root.node.as_ref().unwrap() {
            ValueDraft::List {
                pending: Some(child),
                ..
            } => child.id,
            _ => panic!("pending list item"),
        };
        press(&mut root, &shape, "root: Add value");
        let ValueDraft::List { items, .. } = root.node.as_ref().unwrap() else {
            panic!("list draft");
        };
        assert_eq!(items[1].id, pending_id);
        assert_eq!(
            last_child_id(&input(&root, &shape, "root[1].some")),
            last_child_id(&new_some)
        );
        assert_eq!(text(&root, &pending_leaf), None);
        assert_eq!(input(&root, &shape, "root[0].some"), survivor);
        set_text(&mut root, &survivor, "-5").unwrap();
        assert_eq!(
            root.node.as_ref().unwrap().value(&shape).unwrap(),
            ComponentValue::List(vec![ComponentValue::I64(-5), ComponentValue::I64(3)])
        );
    }

    #[test]
    fn invalid_pending_children_and_unavailable_actions_are_rejected_atomically() {
        let scalar = ComponentValueShape::scalar(ComponentValueKind::I64);
        for (shape, begin, confirm) in [
            (
                ComponentValueShape::optional(scalar.clone()),
                "root: Some...",
                "root: Use Some",
            ),
            (
                ComponentValueShape::list(scalar.clone()),
                "root: Add...",
                "root: Add value",
            ),
            (
                ComponentValueShape::map(scalar),
                "root: Insert...",
                "root: Insert value",
            ),
        ] {
            let mut root = open(&shape, None);
            press(&mut root, &shape, begin);
            if matches!(shape, ComponentValueShape::Map { .. }) {
                let key = input(&root, &shape, "root.key");
                set_text(&mut root, &key, "key").unwrap();
            }
            let leaf = input(&root, &shape, "root.new");
            let invalid = "not an integer 数字\n+";
            set_text(&mut root, &leaf, invalid).unwrap();
            assert_eq!(text(&root, &leaf), Some(invalid));
            assert!(rows(&root, &shape).iter().any(|row| matches!(row, StructuredRow::Button { name, enabled: false, .. } if name == confirm)));
            let before = root.clone();
            let confirm_action = action(&root, &shape, confirm);
            assert_eq!(
                apply(&mut root, &shape, None, &confirm_action).unwrap_err(),
                "Expected a signed integer"
            );
            assert_eq!(root, before);
            let begin_action = action(&root, &shape, begin);
            assert!(apply(&mut root, &shape, None, &begin_action).is_err());
            assert_eq!(root, before);
            let wrong = DraftAction {
                path: DraftPath::default(),
                operation: StructureOperation::Boolean(true),
            };
            assert!(apply(&mut root, &shape, None, &wrong).is_err());
            assert_eq!(root, before);
            assert!(root.node.as_ref().unwrap().copy_text().contains(invalid));
            set_text(&mut root, &leaf, "7").unwrap();
            apply(&mut root, &shape, None, &confirm_action).unwrap();
            assert!(root.node.as_ref().unwrap().value(&shape).is_ok());
        }
    }

    #[test]
    fn map_keys_and_leaf_text_preserve_bytes_identity_and_duplicate_rejection() {
        let shape = ComponentValueShape::map(ComponentValueShape::scalar(ComponentValueKind::I64));
        let source = ComponentValue::Map([("taken".to_owned(), ComponentValue::I64(1))].into());
        let mut root = open(&shape, Some(&source));
        let original = input(&root, &shape, "root[\"taken\"]");
        press(&mut root, &shape, "root: Insert...");
        let key_input = input(&root, &shape, "root.key");
        let leaf = input(&root, &shape, "root.new");
        assert_eq!(last_child_id(&key_input), last_child_id(&leaf));
        let empty_key = action(&root, &shape, "root: Use empty key");
        apply(&mut root, &shape, None, &empty_key).unwrap();
        assert_eq!(text(&root, &key_input), Some(""));
        let before = root.clone();
        assert!(apply(&mut root, &shape, None, &empty_key).is_err());
        assert_eq!(root, before);
        set_text(&mut root, &key_input, "taken").unwrap();
        set_text(&mut root, &leaf, "invalid 数字\n").unwrap();
        let projected = rows(&root, &shape);
        assert!(projected.iter().any(
            |row| matches!(row, StructuredRow::Label(error) if error == "This key already exists")
        ));
        assert!(projected.iter().any(
            |row| matches!(row, StructuredRow::Label(error) if error == "Expected a signed integer")
        ));
        set_text(&mut root, &leaf, "009").unwrap();
        let confirm = action(&root, &shape, "root: Insert value");
        let before = root.clone();
        assert_eq!(
            apply(&mut root, &shape, None, &confirm).unwrap_err(),
            "This key already exists"
        );
        assert_eq!(root, before);
        let key = "新[.some]\" \nkey";
        set_text(&mut root, &key_input, key).unwrap();
        assert_eq!(text(&root, &key_input), Some(key));
        apply(&mut root, &shape, None, &confirm).unwrap();
        let confirmed = input(&root, &shape, &format!("root[{key:?}]"));
        assert_eq!(last_child_id(&confirmed), last_child_id(&leaf));
        assert_eq!(text(&root, &confirmed), Some("009"));
        assert_eq!(text(&root, &original), Some("1"));
        let value = root.node.as_ref().unwrap().value(&shape).unwrap();
        assert_eq!(value.get(key), Some(&ComponentValue::I64(9)));
        assert_eq!(text(&root, &key_input), None);
        press(&mut root, &shape, "root: Insert...");
        assert_ne!(input(&root, &shape, "root.key"), key_input);
        let before = root.clone();
        assert!(set_text(&mut root, &key_input, "stale key").is_err());
        assert_eq!(root, before);
        assert!(apply(&mut root, &shape, None, &confirm).is_err());
        assert_eq!(root, before);
        press(&mut root, &shape, "root: Cancel entry");
        let invalid = "  invalid 数字\n";
        set_text(&mut root, &confirmed, invalid).unwrap();
        assert_eq!(text(&root, &confirmed), Some(invalid));
        assert!(root.node.as_ref().unwrap().value(&shape).is_err());
        assert!(root.node.as_ref().unwrap().copy_text().contains(invalid));
        assert_eq!(text(&root, &original), Some("1"));
    }

    #[test]
    fn empty_values_and_booleans_require_explicit_actions_without_a_field_footer() {
        for (shape, name, expected) in [
            (
                ComponentValueShape::scalar(ComponentValueKind::String),
                "root: Use empty string",
                ComponentValue::String(String::new()),
            ),
            (
                ComponentValueShape::record([]),
                "root: Use empty record",
                ComponentValue::Map(Default::default()),
            ),
            (
                ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::String)),
                "root: Use empty list",
                ComponentValue::List(Vec::new()),
            ),
            (
                ComponentValueShape::map(ComponentValueShape::scalar(ComponentValueKind::String)),
                "root: Use empty map",
                ComponentValue::Map(Default::default()),
            ),
        ] {
            let closed = RootDraft::default();
            assert!(
                matches!(rows(&closed, &shape).as_slice(), [StructuredRow::Button { label: "Edit value", name, enabled: true, .. }] if name == "root: Edit value")
            );
            let mut root = open(&shape, None);
            assert!(root.node.as_ref().unwrap().value(&shape).is_err());
            let confirm = action(&root, &shape, name);
            apply(&mut root, &shape, None, &confirm).unwrap();
            assert_eq!(root.node.as_ref().unwrap().value(&shape).unwrap(), expected);
            let before = root.clone();
            assert!(apply(&mut root, &shape, None, &confirm).is_err());
            assert_eq!(root, before);
            assert!(!rows(&root, &shape).iter().any(|row| matches!(
                row,
                StructuredRow::Button {
                    label: "Apply value" | "Cancel value" | "Discard",
                    ..
                }
            )));
        }
        let shape = ComponentValueShape::scalar(ComponentValueKind::Bool);
        let mut root = open(&shape, None);
        assert!(root.node.as_ref().unwrap().value(&shape).is_err());
        for (name, expected) in [("root: true", true), ("root: false", false)] {
            press(&mut root, &shape, name);
            assert_eq!(
                root.node.as_ref().unwrap().value(&shape).unwrap(),
                ComponentValue::Bool(expected)
            );
        }
        let before = root.clone();
        let reopen = DraftAction {
            path: DraftPath::default(),
            operation: StructureOperation::Open,
        };
        assert!(
            apply(
                &mut root,
                &shape,
                Some(&ComponentValue::Bool(true)),
                &reopen
            )
            .is_err()
        );
        assert_eq!(root, before);
        let unsupported = ComponentValueShape::list(ComponentValueShape::record([]));
        let mut closed = RootDraft::default();
        assert!(apply(&mut closed, &unsupported, None, &reopen).is_err());
        assert_eq!(closed, RootDraft::default());
    }

    #[test]
    fn colliding_display_paths_do_not_alias_record_members() {
        let scalar = ComponentValueShape::scalar(ComponentValueKind::I64);
        let shape = ComponentValueShape::record([
            ("a.b".to_owned(), scalar.clone()),
            (
                "a".to_owned(),
                ComponentValueShape::record([("b".to_owned(), scalar)]),
            ),
        ]);
        let mut root = open(&shape, None);
        let inputs: Vec<_> = rows(&root, &shape)
            .into_iter()
            .filter_map(|row| match row {
                StructuredRow::Text { input, label, .. } if label == "root.a.b" => Some(input),
                _ => None,
            })
            .collect();
        assert_eq!(inputs.len(), 2);
        assert_ne!(inputs[0], inputs[1]);
        set_text(&mut root, &inputs[0], "1").unwrap();
        set_text(&mut root, &inputs[1], "2").unwrap();
        let value = root.node.as_ref().unwrap().value(&shape).unwrap();
        assert_eq!(value.get("a.b"), Some(&ComponentValue::I64(1)));
        assert_eq!(
            value.get("a").unwrap().get("b"),
            Some(&ComponentValue::I64(2))
        );
    }
}
