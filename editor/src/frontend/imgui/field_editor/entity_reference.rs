//! Source-local choices come from the same document projection as the field revision.

use dear_imgui_rs::Ui;
use nara::tooling::SceneInspectorEntityRow;

use crate::frontend::controls::field_drafts::references::{entity_choices, entity_label};

pub(in crate::frontend::imgui) fn show(
    ui: &Ui,
    rows: &[SceneInspectorEntityRow],
    text: &mut String,
) -> bool {
    let selected = text.strip_prefix("scene-local:").unwrap_or(text);
    let selected_label = rows
        .iter()
        .find(|row| row.id.as_str() == selected)
        .map(entity_label);
    let preview = selected_label.as_deref().unwrap_or_else(|| {
        if text.is_empty() {
            "Choose an entity"
        } else {
            text.as_str()
        }
    });
    let mut choice = None;
    let combo = ui.begin_combo("Target", preview);
    super::record(ui, "entity-reference:picker".to_owned());
    if let Some(_combo) = combo {
        for (id, label) in entity_choices(rows) {
            let _id = ui.push_id(id);
            if ui
                .selectable_config(&label)
                .selected(id == selected)
                .build()
            {
                choice = Some(id);
            }
            super::record(ui, format!("entity-reference:choice:{id}"));
            if id == selected {
                ui.set_item_default_focus();
            }
        }
    }
    if let Some(id) = choice {
        *text = format!("scene-local:{id}");
        true
    } else {
        false
    }
}
