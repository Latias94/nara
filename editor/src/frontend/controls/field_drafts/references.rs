use nara::tooling::SceneInspectorEntityRow;

pub(in crate::frontend) fn entity_choices(
    rows: &[SceneInspectorEntityRow],
) -> impl ExactSizeIterator<Item = (&str, String)> + DoubleEndedIterator {
    rows.iter().map(|row| (row.id.as_str(), entity_label(row)))
}

pub(in crate::frontend) fn entity_label(row: &SceneInspectorEntityRow) -> String {
    let name = row
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or(row.id.as_str());
    format!("{name} [{}]", row.id)
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
            ("unnamed", Some("")),
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
        let choices = entity_choices(&rows).collect::<Vec<_>>();
        assert_eq!(
            choices,
            vec![
                ("room", "Room [room]".to_owned()),
                ("door-a", "Door [door-a]".to_owned()),
                ("door-b", "Door [door-b]".to_owned()),
                ("root", "root [root]".to_owned()),
                ("unnamed", "unnamed [unnamed]".to_owned()),
            ]
        );
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
        assert_eq!(
            entity_choices(&moved).collect::<Vec<_>>(),
            choices.into_iter().rev().collect::<Vec<_>>()
        );
    }
}
