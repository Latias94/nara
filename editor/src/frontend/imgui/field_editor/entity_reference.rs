//! Source-local choices come from the same document projection as the field revision.

use dear_imgui_rs::Ui;
use nara::tooling::SceneInspectorEntityRow;

pub(in crate::frontend::imgui) fn show(
    ui: &Ui,
    rows: &[SceneInspectorEntityRow],
    text: &mut String,
) -> bool {
    let selected = text.strip_prefix("scene-local:").unwrap_or(text);
    let selected_label = rows
        .iter()
        .position(|row| row.id.as_str() == selected)
        .map(|index| entity_label(rows, index));
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
        for (id, label) in choices(rows) {
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

fn entity_label(rows: &[SceneInspectorEntityRow], index: usize) -> String {
    let selected = &rows[index];
    let name = |row: &SceneInspectorEntityRow| {
        row.name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(row.id.as_str())
            .to_owned()
    };
    let mut path = vec![name(selected)];
    let mut depth = selected.depth;
    for row in rows[..index].iter().rev() {
        if row.depth < depth {
            path.push(name(row));
            depth = row.depth;
        }
        if depth == 0 {
            break;
        }
    }
    path.reverse();
    format!("{} [{}]", path.join(" / "), selected.id)
}

fn choices(rows: &[SceneInspectorEntityRow]) -> Vec<(&str, String)> {
    let mut path = Vec::new();
    rows.iter()
        .map(|row| {
            path.truncate(row.depth);
            path.push(
                row.name
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .unwrap_or(row.id.as_str()),
            );
            (
                row.id.as_str(),
                format!("{} [{}]", path.join(" / "), row.id),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::scene::SceneEntityId;

    #[test]
    fn duplicate_names_keep_hierarchy_and_exact_source_identity() {
        let rows = [
            ("room", Some("Room"), 0),
            ("door-a", Some("Door"), 1),
            ("door-b", Some("Door"), 1),
            ("root", None, 0),
        ]
        .map(|(id, name, depth)| SceneInspectorEntityRow {
            id: SceneEntityId::new(id).unwrap(),
            name: name.map(str::to_owned),
            parent: (depth == 1).then(|| SceneEntityId::new("room").unwrap()),
            depth,
            inspectable_component_count: 0,
            has_prefab: false,
            selected: false,
            live_locator: None,
        });
        assert_eq!(
            choices(&rows),
            vec![
                ("room", "Room [room]".to_owned()),
                ("door-a", "Room / Door [door-a]".to_owned()),
                ("door-b", "Room / Door [door-b]".to_owned()),
                ("root", "root [root]".to_owned()),
            ]
        );
        for (index, (_, label)) in choices(&rows).iter().enumerate() {
            assert_eq!(&entity_label(&rows, index), label);
        }
    }
}
