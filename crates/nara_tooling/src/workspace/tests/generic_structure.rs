use super::*;

#[test]
fn generic_scene_and_prefab_structure_preserve_selection_history_and_exact_action_basis() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let root = entity_id("root");
        let moving = entity_id("moving");
        let child = entity_id("child");
        let peer = entity_id("peer");
        let destination = entity_id("destination");
        let original = SceneDocument::new(
            [root.clone(), destination.clone()],
            [
                SceneEntityRecord::new(root.clone()).with_children([moving.clone(), peer.clone()]),
                SceneEntityRecord::new(moving.clone()).with_children([child.clone()]),
                SceneEntityRecord::new(child.clone()),
                SceneEntityRecord::new(peer),
                SceneEntityRecord::new(destination.clone()),
            ],
        );
        let bundle = authoring_basis_bundle("Unused schema");
        let mut workspace = EditorWorkspace::new();
        let document = match kind {
            EditorDocumentKind::Scene => workspace
                .open_authoring_scene_document(
                    "structure",
                    EditorDocumentSourceBinding::new("scenes/structure.scene.json"),
                    SceneDocumentCandidate::decode_json_bytes(
                        original.to_json_string().unwrap().as_bytes(),
                    )
                    .unwrap()
                    .admit_authoring(
                        AuthoringDocumentSource::new(
                            AssetRef::path("scenes/structure.scene.json").unwrap(),
                        ),
                        &bundle,
                        &[],
                    )
                    .unwrap()
                    .publish(),
                )
                .unwrap(),
            EditorDocumentKind::Prefab => workspace
                .open_authoring_prefab_document(
                    "structure",
                    EditorDocumentSourceBinding::new("prefabs/structure.prefab.json"),
                    PrefabDocumentCandidate::decode_json_bytes(
                        PrefabDocument::new(original.roots.clone(), original.entities.clone())
                            .to_json_string()
                            .unwrap()
                            .as_bytes(),
                    )
                    .unwrap()
                    .admit_authoring(
                        AuthoringDocumentSource::new(
                            AssetRef::path("prefabs/structure.prefab.json").unwrap(),
                        ),
                        &bundle,
                        &[],
                    )
                    .unwrap()
                    .publish(),
                )
                .unwrap(),
        }
        .opened_document
        .unwrap();
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document,
                    entity: Some(child.clone()),
                })
                .applied
        );
        let baseline = workspace.document_revision(document).unwrap();
        let move_command = SceneCompositionCommand::MoveEntity {
            entity: moving.clone(),
            to: ScenePlacement::last(ScenePlacementOwner::Parent {
                parent: destination.clone(),
            }),
        };
        let moved = workspace.apply_authoring_command(command(
            document,
            baseline,
            kind,
            move_command.clone(),
        ));
        assert!(moved.applied, "{kind:?}: {:?}", moved.diagnostics);
        let after_move = document_value(&workspace, document);
        assert_eq!(
            workspace.documents[&document].selection().top_entity(),
            Some(&child)
        );
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            1
        );
        if kind == EditorDocumentKind::Prefab {
            assert_eq!(
                moved.prefab_composition_outcome,
                Some(PrefabCompositionOutcome::Moved {
                    entity: moving.clone()
                })
            );
        }
        let stale = workspace.apply_authoring_command(command(
            document,
            baseline,
            kind,
            move_command.clone(),
        ));
        assert!(!stale.applied);
        assert_eq!(document_value(&workspace, document), after_move);
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            1
        );
        if kind == EditorDocumentKind::Prefab {
            let no_op = workspace.apply_authoring_command(command(
                document,
                moved.revision.unwrap(),
                kind,
                move_command,
            ));
            assert!(!no_op.applied);
            assert_eq!(
                no_op.prefab_composition_outcome,
                Some(PrefabCompositionOutcome::NoOp {
                    entity: moving.clone()
                })
            );
            assert_eq!(
                workspace.documents[&document].history_status().undo_depth,
                1
            );
        }
        let removed = workspace.apply_authoring_command(command(
            document,
            workspace.document_revision(document).unwrap(),
            kind,
            SceneCompositionCommand::RemoveEntity {
                entity: moving.clone(),
            },
        ));
        assert!(removed.applied, "{kind:?}: {:?}", removed.diagnostics);
        assert_eq!(
            workspace.documents[&document].selection().top_entity(),
            Some(&destination)
        );
        if kind == EditorDocumentKind::Prefab {
            assert_eq!(
                removed.prefab_composition_outcome,
                Some(PrefabCompositionOutcome::Removed {
                    entity: moving,
                    selection: Some(destination),
                })
            );
        }
        let undo = workspace.apply_authoring_command(EditorWorkspaceCommand::Undo {
            document,
            base_revision: removed.revision.unwrap(),
        });
        assert!(undo.applied, "{kind:?}: {:?}", undo.diagnostics);
        assert_eq!(document_value(&workspace, document), after_move);
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision: undo.revision.unwrap(),
                })
                .applied
        );
        assert_eq!(document_value(&workspace, document), original);
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            0
        );
        let redo = workspace.apply_authoring_command(EditorWorkspaceCommand::Redo {
            document,
            base_revision: workspace.document_revision(document).unwrap(),
        });
        assert!(redo.applied);
        assert_eq!(document_value(&workspace, document), after_move);
    }
}

fn command(
    document: EditorDocumentId,
    base_revision: SceneAuthoringRevision,
    kind: EditorDocumentKind,
    command: SceneCompositionCommand,
) -> EditorWorkspaceCommand {
    match kind {
        EditorDocumentKind::Scene => EditorWorkspaceCommand::ComposeScene {
            document,
            base_revision,
            command,
        },
        EditorDocumentKind::Prefab => EditorWorkspaceCommand::ComposePrefab {
            document,
            base_revision,
            command: match command {
                SceneCompositionCommand::MoveEntity { entity, to } => {
                    PrefabCompositionCommand::MoveEntity { entity, to }
                }
                SceneCompositionCommand::RemoveEntity { entity } => {
                    PrefabCompositionCommand::RemoveEntity { entity }
                }
                _ => unreachable!("this fixture exercises move and remove commands"),
            },
        },
    }
}

fn document_value(workspace: &EditorWorkspace, document: EditorDocumentId) -> SceneDocument {
    match &workspace.documents[&document] {
        EditorDocumentSlot::Scene(slot) => slot.session.document().clone(),
        EditorDocumentSlot::Prefab(slot) => slot.session.document().instantiate(),
    }
}
