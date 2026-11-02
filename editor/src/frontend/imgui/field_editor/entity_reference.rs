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

fn entity_label(row: &SceneInspectorEntityRow) -> String {
    let name = row
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or(row.id.as_str());
    format!("{name} [{}]", row.id)
}

fn choices(rows: &[SceneInspectorEntityRow]) -> Vec<(&str, String)> {
    rows.iter()
        .map(|row| (row.id.as_str(), entity_label(row)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::scene::{
        PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceHop, SceneEntityId, ScenePlacement,
        ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
    };

    #[test]
    fn duplicate_names_keep_exact_source_identity_independently_of_placement_and_row_order() {
        let rows = [
            ("room", Some("Room")),
            ("door-a", Some("Door")),
            ("door-b", Some("Door")),
            ("root", None),
        ]
        .map(|(id, name)| SceneInspectorEntityRow {
            id: SceneEntityId::new(id).unwrap(),
            name: name.map(str::to_owned),
            placement: Some(ScenePlacement::last(ScenePlacementParent::Root)),
            inspectable_component_count: 0,
            has_prefab: false,
            selected: false,
            live_locator: None,
        });
        assert_eq!(
            choices(&rows),
            vec![
                ("room", "Room [room]".to_owned()),
                ("door-a", "Door [door-a]".to_owned()),
                ("door-b", "Door [door-b]".to_owned()),
                ("root", "root [root]".to_owned()),
            ]
        );
        for (index, (_, label)) in choices(&rows).iter().enumerate() {
            assert_eq!(&entity_label(&rows[index]), label);
        }
        let mut moved = rows.clone();
        moved[1].placement = Some(ScenePlacement::first(ScenePlacementParent::Entity(
            PrefabEntityTarget::source(SceneEntityId::new("room").unwrap()),
        )));
        moved[2].placement = Some(ScenePlacement::last(ScenePlacementParent::Entity(
            PrefabEntityTarget {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("room").unwrap()),
                    expected_base: SourceDocumentKey::named(
                        "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
                        "root-bound-source",
                        SourceDocumentKind::Prefab,
                    )
                    .unwrap(),
                }],
                entity: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("root").unwrap()),
            },
        )));
        moved.reverse();
        for row in &moved {
            let original = rows.iter().find(|original| original.id == row.id).unwrap();
            assert_eq!(entity_label(row), entity_label(original));
        }
    }
}
