use std::collections::BTreeMap;

use super::NodeDraft;
use crate::dialogue::{ChoiceDraft, ChoiceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftField {
    Speaker,
    Text,
    Position(usize),
    ChoiceLabel(ChoiceId),
    ChoiceTarget(ChoiceId),
    Choice(ChoiceId),
    ChoiceOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    Current,
    Draft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftConflict {
    pub field: DraftField,
    pub current: String,
    pub draft: String,
}

fn choice(draft: &NodeDraft, id: ChoiceId) -> Option<&ChoiceDraft> {
    draft.choices.iter().find(|choice| choice.id == Some(id))
}

impl DraftField {
    pub(super) fn display(self, draft: &NodeDraft) -> String {
        match self {
            Self::Speaker => draft.speaker.clone(),
            Self::Text => draft.text.clone(),
            Self::Position(axis) => draft.position[axis].clone(),
            Self::ChoiceLabel(id) => {
                choice(draft, id).map_or_else(|| "Removed".into(), |choice| choice.label.clone())
            }
            Self::ChoiceTarget(id) => choice(draft, id)
                .map_or_else(|| "Removed".into(), |choice| format!("{:?}", choice.target)),
            Self::Choice(id) => choice(draft, id).map_or_else(
                || "Removed".into(),
                |choice| format!("{} → {:?}", choice.label, choice.target),
            ),
            Self::ChoiceOrder => format!(
                "{:?}",
                draft
                    .choices
                    .iter()
                    .map(|choice| choice.id)
                    .collect::<Vec<_>>()
            ),
        }
    }

    pub(super) fn equal(self, current: &NodeDraft, draft: &NodeDraft) -> bool {
        match self {
            Self::Speaker => current.speaker == draft.speaker,
            Self::Text => current.text == draft.text,
            Self::Position(axis) => current.position[axis] == draft.position[axis],
            Self::ChoiceLabel(id) => {
                choice(current, id).map(|choice| &choice.label)
                    == choice(draft, id).map(|choice| &choice.label)
            }
            Self::ChoiceTarget(id) => {
                choice(current, id).map(|choice| choice.target)
                    == choice(draft, id).map(|choice| choice.target)
            }
            Self::Choice(id) => choice(current, id) == choice(draft, id),
            Self::ChoiceOrder => current
                .choices
                .iter()
                .map(|choice| choice.id)
                .eq(draft.choices.iter().map(|choice| choice.id)),
        }
    }
}

fn merge_field<Value: Clone + PartialEq>(
    value: &mut Value,
    original: &Value,
    current: &Value,
    field: DraftField,
    conflicts: &mut Vec<DraftField>,
) {
    if value == original {
        *value = current.clone();
    } else if current != original && value != current && !conflicts.contains(&field) {
        conflicts.push(field);
    }
}

fn reorder(choices: &mut Vec<ChoiceDraft>, order: &[ChoiceDraft]) {
    let mut remaining = std::mem::take(choices);
    for preferred in order {
        if let Some(index) = remaining.iter().position(|choice| {
            if preferred.id.is_some() {
                choice.id == preferred.id
            } else {
                choice == preferred
            }
        }) {
            choices.push(remaining.remove(index));
        }
    }
    choices.extend(remaining);
}

fn insert_in_order(
    choices: &mut Vec<ChoiceDraft>,
    entry: ChoiceDraft,
    order: &[ChoiceDraft],
    index: usize,
) {
    let next = order[index + 1..]
        .iter()
        .filter_map(|choice| choice.id)
        .find_map(|id| choices.iter().position(|choice| choice.id == Some(id)));
    choices.insert(next.unwrap_or(choices.len()), entry);
}

impl NodeDraft {
    pub(super) fn merge(
        &mut self,
        original: &Self,
        current: &Self,
        conflicts: &mut Vec<DraftField>,
    ) {
        merge_field(
            &mut self.speaker,
            &original.speaker,
            &current.speaker,
            DraftField::Speaker,
            conflicts,
        );
        merge_field(
            &mut self.text,
            &original.text,
            &current.text,
            DraftField::Text,
            conflicts,
        );
        for axis in 0..2 {
            merge_field(
                &mut self.position[axis],
                &original.position[axis],
                &current.position[axis],
                DraftField::Position(axis),
                conflicts,
            );
        }
        let local = self.clone();
        let common_order = |draft: &Self| {
            draft
                .choices
                .iter()
                .filter_map(|entry| entry.id)
                .filter(|id| {
                    choice(original, *id).is_some()
                        && choice(&local, *id).is_some()
                        && choice(current, *id).is_some()
                })
                .collect::<Vec<_>>()
        };
        let old_order = common_order(original);
        let local_order = common_order(&local);
        let current_order = common_order(current);
        if local_order != old_order
            && current_order != old_order
            && local_order != current_order
            && !conflicts.contains(&DraftField::ChoiceOrder)
        {
            conflicts.push(DraftField::ChoiceOrder);
        }
        let mut merged = BTreeMap::new();
        for entry in &current.choices {
            let Some(id) = entry.id else { continue };
            let mut value = entry.clone();
            match (choice(original, id), choice(&local, id)) {
                (Some(base), Some(draft)) => {
                    value = draft.clone();
                    merge_field(
                        &mut value.label,
                        &base.label,
                        &entry.label,
                        DraftField::ChoiceLabel(id),
                        conflicts,
                    );
                    merge_field(
                        &mut value.target,
                        &base.target,
                        &entry.target,
                        DraftField::ChoiceTarget(id),
                        conflicts,
                    );
                }
                (Some(base), None) => {
                    if entry != base && !conflicts.contains(&DraftField::Choice(id)) {
                        conflicts.push(DraftField::Choice(id));
                    }
                    continue;
                }
                _ => {}
            }
            merged.insert(id, value);
        }
        for entry in &local.choices {
            let Some(id) = entry.id else { continue };
            if choice(current, id).is_none() && choice(original, id) != Some(entry) {
                if !conflicts.contains(&DraftField::Choice(id)) {
                    conflicts.push(DraftField::Choice(id));
                }
                merged.insert(id, entry.clone());
            }
        }
        self.choices.clear();
        let preferred = if local_order != old_order {
            &local.choices
        } else {
            &current.choices
        };
        for entry in preferred {
            if let Some(id) = entry.id
                && let Some(value) = merged.remove(&id)
            {
                self.choices.push(value);
            }
        }
        for source in [&current.choices, &local.choices] {
            for (index, entry) in source.iter().enumerate() {
                if let Some(id) = entry.id
                    && let Some(value) = merged.remove(&id)
                {
                    insert_in_order(&mut self.choices, value, source, index);
                }
            }
        }
        for (index, entry) in local
            .choices
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.id.is_none())
        {
            insert_in_order(&mut self.choices, entry.clone(), &local.choices, index);
        }
    }

    pub(super) fn resolve(
        &mut self,
        field: DraftField,
        resolution: ConflictResolution,
        current: &Self,
    ) -> bool {
        if resolution == ConflictResolution::Draft {
            if let DraftField::Choice(id) = field
                && choice(current, id).is_none()
                && let Some(entry) = self.choices.iter_mut().find(|choice| choice.id == Some(id))
            {
                entry.id = None;
            }
            return true;
        }
        match field {
            DraftField::Speaker => self.speaker = current.speaker.clone(),
            DraftField::Text => self.text = current.text.clone(),
            DraftField::Position(axis) => self.position[axis] = current.position[axis].clone(),
            DraftField::ChoiceLabel(id) | DraftField::ChoiceTarget(id) => {
                let Some((index, source)) = current
                    .choices
                    .iter()
                    .enumerate()
                    .find(|(_, choice)| choice.id == Some(id))
                else {
                    return false;
                };
                if let Some(target) = self.choices.iter_mut().find(|choice| choice.id == Some(id)) {
                    if matches!(field, DraftField::ChoiceLabel(_)) {
                        target.label = source.label.clone();
                    } else {
                        target.target = source.target;
                    }
                } else {
                    insert_in_order(&mut self.choices, source.clone(), &current.choices, index);
                }
            }
            DraftField::Choice(id) => {
                self.choices.retain(|choice| choice.id != Some(id));
                if let Some((index, source)) = current
                    .choices
                    .iter()
                    .enumerate()
                    .find(|(_, choice)| choice.id == Some(id))
                {
                    insert_in_order(&mut self.choices, source.clone(), &current.choices, index);
                }
            }
            DraftField::ChoiceOrder => reorder(&mut self.choices, &current.choices),
        }
        true
    }
}
