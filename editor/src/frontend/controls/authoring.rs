use nara::{
    scene::{
        AuthoringDocumentReadiness, PrefabEntityOrigin, PrefabEntityTarget, ScenePlacement,
        ScenePlacementParent, ScenePlacementSlot, SourceDocumentKey, SourceDocumentOrigin,
    },
    tooling::{
        EditorAction, EditorDocumentId, EditorDocumentKind, EditorDocumentMutation,
        EditorFrameModel, EditorPersistenceOperation, EditorPersistenceRejection,
        EditorPersistenceRequestKind, EditorPersistenceResult, EditorPlayState,
        EditorRuntimeAvailability, EditorSceneModel, PrefabCompositionCommand,
        PrefabCompositionEntityDraft, SceneCompositionCommand,
    },
};

pub(super) fn entity_removal_unavailable_reason(
    frame: &EditorFrameModel,
    scene: &EditorSceneModel,
    has_prefab: bool,
) -> Option<&'static str> {
    if !scene.editor.inspector.composition.generic_authoring {
        return None;
    }
    if frame
        .workspace()
        .document(scene.document)
        .and_then(|tab| tab.authoring_readiness)
        != Some(AuthoringDocumentReadiness::Complete)
    {
        return Some("Entity deletion requires a Complete authoring document");
    }
    if has_prefab {
        return Some("Entity deletion requires a source without Prefab anchors");
    }
    None
}

pub(in crate::frontend) fn editing_enabled(frame: &EditorFrameModel) -> bool {
    (matches!(
        frame.runtime_availability(),
        EditorRuntimeAvailability::ExternalSupervised
    ) || matches!(
        frame.play().state(),
        EditorPlayState::Empty | EditorPlayState::Paused | EditorPlayState::Faulted
    )) && frame.workspace_intent().intent().is_none()
        && matches!(
            frame.persistence().operation(),
            None | Some(EditorPersistenceOperation::Saving { .. })
        )
}

pub(in crate::frontend) fn persistence_allows(
    result: Option<EditorPersistenceResult>,
    request: EditorPersistenceRequestKind,
    document: EditorDocumentId,
) -> bool {
    match result {
        None => true,
        Some(EditorPersistenceResult::Rejected {
            document: target,
            reason: EditorPersistenceRejection::RequiresReconcile,
            ..
        }) => request == EditorPersistenceRequestKind::Reopen && target == Some(document),
        Some(EditorPersistenceResult::Rejected {
            request: failed,
            document: target,
            ..
        })
        | Some(EditorPersistenceResult::Failed {
            request: failed,
            document: target,
            ..
        }) => failed == request && target.is_none_or(|target| target == document),
        Some(EditorPersistenceResult::PersistenceUncertain {
            document: target, ..
        }) => request == EditorPersistenceRequestKind::Reopen && target == document,
        Some(EditorPersistenceResult::Saved { .. } | EditorPersistenceResult::Opened { .. }) => {
            false
        }
    }
}

pub(in crate::frontend) fn compose_action(
    scene: &EditorSceneModel,
    command: SceneCompositionCommand,
) -> EditorAction {
    if scene.kind == EditorDocumentKind::Prefab {
        let command = match command {
            SceneCompositionCommand::CreateEntity { placement, .. } => {
                PrefabCompositionCommand::CreateEntity {
                    draft: PrefabCompositionEntityDraft::new(Default::default())
                        .expect("empty composition draft is bounded"),
                    placement,
                }
            }
            SceneCompositionCommand::AddComponent { entity, component } => {
                PrefabCompositionCommand::AddComponent { entity, component }
            }
            SceneCompositionCommand::RemoveComponent { entity, component } => {
                PrefabCompositionCommand::RemoveComponent { entity, component }
            }
            SceneCompositionCommand::MoveEntity { target, to } => {
                PrefabCompositionCommand::MoveEntity { target, to }
            }
            SceneCompositionCommand::RevertPlacement { target } => {
                PrefabCompositionCommand::RevertPlacement { target }
            }
            SceneCompositionCommand::RemoveEntity { entity } => {
                PrefabCompositionCommand::RemoveEntity { entity }
            }
        };
        EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
            document: scene.document,
            base_revision: scene.revision,
            command,
        })
    } else {
        EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
            document: scene.document,
            base_revision: scene.revision,
            command,
        })
    }
}

pub(in crate::frontend) fn placement_intent_label(placement: Option<&ScenePlacement>) -> String {
    match placement {
        Some(placement) => format!(
            "Desired: {} / {}",
            placement_parent_label(&placement.parent),
            placement_slot_label(&placement.slot),
        ),
        None => "No source placement declaration".to_owned(),
    }
}

fn placement_parent_label(parent: &ScenePlacementParent) -> String {
    match parent {
        ScenePlacementParent::Root => "Root".to_owned(),
        ScenePlacementParent::Entity(target) => placement_target_label(target),
    }
}

fn placement_slot_label(slot: &ScenePlacementSlot) -> String {
    match slot {
        ScenePlacementSlot::First => "First".to_owned(),
        ScenePlacementSlot::Last => "Last".to_owned(),
        ScenePlacementSlot::Before(target) => format!("Before {}", placement_target_label(target)),
        ScenePlacementSlot::After(target) => format!("After {}", placement_target_label(target)),
    }
}

fn placement_target_label(target: &PrefabEntityTarget) -> String {
    let mut route = target
        .through
        .iter()
        .map(|hop| {
            format!(
                "through {} @ {}",
                placement_origin_label(&hop.anchor),
                source_key_label(&hop.expected_base),
            )
        })
        .collect::<Vec<_>>();
    route.push(placement_origin_label(&target.entity));
    route.join(" / ")
}

fn placement_origin_label(origin: &PrefabEntityOrigin) -> String {
    match origin {
        PrefabEntityOrigin::SourceEntity(entity) => format!("source:{entity}"),
        PrefabEntityOrigin::InstanceAddition(entity) => format!("addition:{entity}"),
    }
}

fn source_key_label(source: &SourceDocumentKey) -> String {
    let origin = match source.origin() {
        SourceDocumentOrigin::Named { universe, name } => format!("named:{name} [{universe}]"),
        SourceDocumentOrigin::ProjectLocation { universe, path } => {
            format!("path:{path} [{universe}]")
        }
        SourceDocumentOrigin::StableAsset { stable_id } => format!("asset:{stable_id}"),
    };
    format!("{:?} {origin}", source.kind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::scene::SceneEntityId;

    #[test]
    fn desired_placement_labels_preserve_routed_source_predicates_and_origin_kinds() {
        let route = |name| PrefabEntityTarget {
            through: vec![nara::scene::PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("instance").unwrap()),
                expected_base: SourceDocumentKey::named(
                    "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
                    name,
                    nara::scene::SourceDocumentKind::Prefab,
                )
                .unwrap(),
            }],
            entity: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("root").unwrap()),
        };
        let first = route("first");
        let second = route("second");
        let placement =
            ScenePlacement::after(ScenePlacementParent::Entity(first.clone()), second.clone());
        let label = placement_intent_label(Some(&placement));
        assert!(label.contains("Desired:"));
        assert!(label.contains("named:first"));
        assert!(label.contains("named:second"));
        assert!(label.contains("f9ab807a-c61e-4c92-bfee-b6fa153b8efc"));
        assert_ne!(
            placement_target_label(&first),
            placement_target_label(&second),
        );
        assert_ne!(
            placement_target_label(&PrefabEntityTarget::source(
                SceneEntityId::new("root").unwrap(),
            )),
            placement_target_label(&PrefabEntityTarget::addition(
                SceneEntityId::new("root").unwrap(),
            )),
        );
        assert_eq!(
            placement_intent_label(None),
            "No source placement declaration"
        );
    }
}
