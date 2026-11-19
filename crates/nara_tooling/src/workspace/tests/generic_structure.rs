use super::*;

mod deletion;

#[test]
fn repeated_placement_preserves_exact_source_and_redo() {
    let parent = entity_id("parent");
    let child = entity_id("child");
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(parent.clone()),
        SceneEntityRecord::new(child.clone()),
    ]);
    let target = PrefabEntityTarget::source(child);
    let registry = frozen_empty_registry();
    let bundle = authoring_basis_bundle("Unused schema");

    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        for published in [false, true] {
            let (mut workspace, mut persistence) = EditorWorkspace::new_hosted();
            let document = if published {
                open_published_structure(&mut workspace, kind, &original, &bundle)
            } else {
                match kind {
                    EditorDocumentKind::Scene => {
                        open_scene(&mut workspace, "structure", original.clone())
                    }
                    EditorDocumentKind::Prefab => open_prefab(
                        &mut workspace,
                        "structure",
                        PrefabDocument::new(original.placements.clone(), original.entities.clone()),
                    ),
                    EditorDocumentKind::Extension(_) => unreachable!(),
                }
            };
            let apply = |workspace: &mut EditorWorkspace, command| {
                if published {
                    workspace.apply_authoring_command(command)
                } else {
                    workspace.apply_command(&registry, command)
                }
            };
            let current_tab = |workspace: &EditorWorkspace| {
                let model = if published {
                    workspace.authoring_model()
                } else {
                    workspace.model(&registry, None)
                };
                model.document(document).unwrap().clone()
            };
            let move_to = |base_revision, to| {
                command(
                    document,
                    base_revision,
                    kind,
                    SceneCompositionCommand::MoveEntity {
                        target: target.clone(),
                        to,
                    },
                )
            };

            let initial = persistence.capture(&workspace, document).unwrap();
            let moved = apply(
                &mut workspace,
                move_to(
                    initial.revision(),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        parent.clone(),
                    ))),
                ),
            );
            assert!(
                moved.applied,
                "{kind:?} published={published}: {:?}",
                moved.diagnostics
            );
            let moved_source = persistence.capture(&workspace, document).unwrap();
            let undone = apply(
                &mut workspace,
                EditorWorkspaceCommand::Undo {
                    document,
                    base_revision: moved_source.revision(),
                },
            );
            assert!(
                undone.applied,
                "{kind:?} published={published}: {:?}",
                undone.diagnostics
            );
            let before = persistence.capture(&workspace, document).unwrap();
            assert_eq!(before.scene(), initial.scene());
            assert_eq!(before.prefab(), initial.prefab());
            let before_tab = current_tab(&workspace);
            assert!(!before_tab.dirty);
            assert_eq!(
                before_tab.history,
                DocumentHistoryStatus {
                    undo_depth: 0,
                    redo_depth: 1,
                }
            );

            let no_op = apply(
                &mut workspace,
                move_to(
                    before.revision(),
                    ScenePlacement::last(ScenePlacementParent::Root),
                ),
            );
            assert!(
                !no_op.applied && no_op.diagnostics.is_empty(),
                "{kind:?} published={published}: {no_op:?}"
            );
            assert_eq!(no_op.revision, Some(before.revision()));
            assert_eq!(no_op.dirty, Some(false));
            assert!(no_op.patch_report.is_none());
            assert_eq!(
                no_op.prefab_composition_outcome,
                (kind == EditorDocumentKind::Prefab).then_some(PrefabCompositionOutcome::NoOp {
                    target: target.clone(),
                })
            );
            assert_eq!(current_tab(&workspace), before_tab);
            let unchanged = persistence.capture(&workspace, document).unwrap();
            assert_eq!(unchanged.content_state(), before.content_state());
            if let Some(scene) = before.scene() {
                assert!(std::ptr::eq(unchanged.scene().unwrap(), scene));
            } else {
                assert!(std::ptr::eq(
                    unchanged.prefab().unwrap(),
                    before.prefab().unwrap()
                ));
            }

            let redone = apply(
                &mut workspace,
                EditorWorkspaceCommand::Redo {
                    document,
                    base_revision: before.revision(),
                },
            );
            assert!(
                redone.applied,
                "{kind:?} published={published}: {:?}",
                redone.diagnostics
            );
            let after = persistence.capture(&workspace, document).unwrap();
            assert_eq!(after.content_state(), moved_source.content_state());
            assert_eq!(after.scene(), moved_source.scene());
            assert_eq!(after.prefab(), moved_source.prefab());
        }
    }
}

#[test]
fn generic_scene_and_prefab_structure_preserve_selection_history_and_exact_action_basis() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let root = entity_id("root");
        let moving = entity_id("moving");
        let child = entity_id("child");
        let peer = entity_id("peer");
        let destination = entity_id("destination");
        let original = SceneDocument::new(
            [
                SceneEntityPlacement::root(root.clone()),
                SceneEntityPlacement::root(destination.clone()),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(moving.clone()),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        root.clone(),
                    ))),
                ),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(peer.clone()),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        root.clone(),
                    ))),
                ),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(child.clone()),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        moving.clone(),
                    ))),
                ),
            ],
            [
                SceneEntityRecord::new(root.clone()),
                SceneEntityRecord::new(moving.clone()),
                SceneEntityRecord::new(child.clone()),
                SceneEntityRecord::new(peer),
                SceneEntityRecord::new(destination.clone()),
            ],
        );
        let bundle = authoring_basis_bundle("Unused schema");
        let mut workspace = EditorWorkspace::new();
        let document = open_published_structure(&mut workspace, kind, &original, &bundle);
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
            target: PrefabEntityTarget::source(moving.clone()),
            to: ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                destination.clone(),
            ))),
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
                    target: PrefabEntityTarget::source(moving.clone())
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
        assert!(stale.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "tooling.workspace-stale-document-revision"
        }));
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
                    target: PrefabEntityTarget::source(moving.clone())
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

fn open_published_structure(
    workspace: &mut EditorWorkspace,
    kind: EditorDocumentKind,
    original: &SceneDocument,
    bundle: &AuthoringSchemaBundle,
) -> EditorDocumentId {
    match kind {
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
                    bundle,
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
                    PrefabDocument::new(original.placements.clone(), original.entities.clone())
                        .to_json_string()
                        .unwrap()
                        .as_bytes(),
                )
                .unwrap()
                .admit_authoring(
                    AuthoringDocumentSource::new(
                        AssetRef::path("prefabs/structure.prefab.json").unwrap(),
                    ),
                    bundle,
                    &[],
                )
                .unwrap()
                .publish(),
            )
            .unwrap(),
        EditorDocumentKind::Extension(_) => {
            unreachable!("this fixture exercises Scene and Prefab structure")
        }
    }
    .opened_document
    .unwrap()
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
                SceneCompositionCommand::MoveEntity { target, to } => {
                    PrefabCompositionCommand::MoveEntity { target, to }
                }
                SceneCompositionCommand::RemoveEntity { entity } => {
                    PrefabCompositionCommand::RemoveEntity { entity }
                }
                _ => unreachable!("this fixture exercises move and remove commands"),
            },
        },
        EditorDocumentKind::Extension(_) => {
            unreachable!("this fixture exercises Scene and Prefab structure")
        }
    }
}

fn document_value(workspace: &EditorWorkspace, document: EditorDocumentId) -> SceneDocument {
    match &workspace.documents[&document] {
        EditorDocumentSlot::Scene(slot) => slot.session.document().clone(),
        EditorDocumentSlot::Prefab(slot) => slot.session.document().instantiate(),
        EditorDocumentSlot::Extension(_) => {
            unreachable!("this fixture exercises Scene and Prefab structure")
        }
    }
}
