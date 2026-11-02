use super::*;
use nara_scene::{
    PrefabEntityOrigin, PrefabInstance, PrefabInstanceHop, SceneAuthoringHistoryUsage,
    ScenePlacementSlot, SourceDocumentKey, SourceDocumentKind,
};

#[derive(Debug, Clone, Copy)]
enum Owner {
    Native,
    Published,
}

fn target(id: &str) -> PrefabEntityTarget {
    PrefabEntityTarget::source(entity_id(id))
}

fn parent(id: &str) -> ScenePlacementParent {
    ScenePlacementParent::Entity(target(id))
}

fn at(id: &str, parent: ScenePlacementParent, slot: ScenePlacementSlot) -> SceneEntityPlacement {
    SceneEntityPlacement::new(target(id), ScenePlacement { parent, slot })
}

fn chain_document(domain: &ScenePlacementParent, owner: Owner) -> SceneDocument {
    // Neither declaration order nor sorted record order is the effective sibling sequence.
    // In particular, replacing b's dangling After(a) with After(c) alone would cycle with c.
    let mut placements = vec![
        at("c", domain.clone(), ScenePlacementSlot::Before(target("b"))),
        at(
            "y-before",
            domain.clone(),
            ScenePlacementSlot::Before(target("a")),
        ),
        at(
            "other-child-b",
            parent("other-parent"),
            ScenePlacementSlot::Before(target("other-child-a")),
        ),
        at("b", domain.clone(), ScenePlacementSlot::After(target("a"))),
        at(
            "a",
            domain.clone(),
            ScenePlacementSlot::After(target("z-lead")),
        ),
        at("tail", domain.clone(), ScenePlacementSlot::Last),
        at("z-lead", domain.clone(), ScenePlacementSlot::First),
        at("a-child", parent("a"), ScenePlacementSlot::Last),
        at(
            "other-parent",
            ScenePlacementParent::Root,
            ScenePlacementSlot::Last,
        ),
        at(
            "other-child-a",
            parent("other-parent"),
            ScenePlacementSlot::Last,
        ),
    ];
    if let ScenePlacementParent::Entity(root) = domain {
        placements.push(SceneEntityPlacement::new(
            root.clone(),
            ScenePlacement::first(ScenePlacementParent::Root),
        ));
    }
    let records = placements.iter().enumerate().map(|(index, entry)| {
        let PrefabEntityOrigin::SourceEntity(id) = &entry.target.entity else {
            unreachable!("this fixture owns only source-local entities")
        };
        let mut component = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            transform_value(index as f64 + 0.5, -(index as f64)),
        );
        if matches!(owner, Owner::Published) {
            component = component.with_owner(
                PersistentComponentSchemaOwnerId::try_new(
                    nara_transform::TRANSFORM_SCHEMA_OWNER_ID.as_str(),
                )
                .unwrap(),
            );
        }
        let mut record = SceneEntityRecord::new(id.clone()).with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            component,
        );
        if id == &entity_id("other-parent") && matches!(owner, Owner::Native) {
            // This unaffected child domain must not require source loading or be rewritten.
            // Published deletion separately requires a Complete source without Prefab anchors.
            record.prefab = Some(PrefabInstance::forest(
                AssetRef::path("prefabs/other.prefab.json").unwrap(),
            ));
        }
        record
    });
    let records = records.collect::<Vec<_>>();
    SceneDocument::new(placements, records)
}

fn open_owner(
    owner: Owner,
    kind: EditorDocumentKind,
    original: &SceneDocument,
) -> (EditorWorkspace, EditorDocumentId) {
    let mut workspace = EditorWorkspace::new();
    if matches!(owner, Owner::Native) {
        let document = match kind {
            EditorDocumentKind::Scene => open_scene(&mut workspace, "deletion", original.clone()),
            EditorDocumentKind::Prefab => open_prefab(
                &mut workspace,
                "deletion",
                PrefabDocument::new(original.placements.clone(), original.entities.clone()),
            ),
            EditorDocumentKind::Extension(_) => unreachable!("entity-bearing source fixture"),
        };
        return (workspace, document);
    }
    let bundle = AuthoringSchemaBundle::from_trusted_provider_definitions([
        nara_transform::TRANSFORM_SCHEMA_PROVIDER,
    ])
    .unwrap();
    let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
        &bundle.owners()[0],
        &Default::default(),
    )
    .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    let opened = match kind {
        EditorDocumentKind::Scene => workspace
            .open_authoring_scene_document(
                "deletion",
                EditorDocumentSourceBinding::new("scenes/deletion.scene.json"),
                SceneDocumentCandidate::decode_json_bytes(
                    original.to_json_string().unwrap().as_bytes(),
                )
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("scenes/deletion.scene.json").unwrap(),
                    ),
                    &basis,
                )
                .unwrap()
                .publish(),
            )
            .unwrap(),
        EditorDocumentKind::Prefab => workspace
            .open_authoring_prefab_document(
                "deletion",
                EditorDocumentSourceBinding::new("prefabs/deletion.prefab.json"),
                PrefabDocumentCandidate::decode_json_bytes(
                    PrefabDocument::new(original.placements.clone(), original.entities.clone())
                        .to_json_string()
                        .unwrap()
                        .as_bytes(),
                )
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("prefabs/deletion.prefab.json").unwrap(),
                    ),
                    &basis,
                )
                .unwrap()
                .publish(),
            )
            .unwrap(),
        EditorDocumentKind::Extension(_) => unreachable!("entity-bearing source fixture"),
    };
    (workspace, opened.opened_document.unwrap())
}

fn apply(
    workspace: &mut EditorWorkspace,
    registry: &ComponentRegistrySnapshot,
    owner: Owner,
    command: EditorWorkspaceCommand,
) -> EditorWorkspaceCommandReport {
    match owner {
        Owner::Native => workspace.apply_command(registry, command),
        Owner::Published => workspace.apply_authoring_command(command),
    }
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    source: SceneDocument,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    history: SceneAuthoringHistoryStatus,
    history_usage: SceneAuthoringHistoryUsage,
    selection: crate::EditorSelectionSet,
    saved_content: SceneAuthoringContentState,
    dirty: bool,
}

fn snapshot(workspace: &EditorWorkspace, document: EditorDocumentId) -> Snapshot {
    if let Some(slot) = workspace.scene(document) {
        Snapshot {
            source: slot.session().document().clone(),
            revision: slot.revision(),
            content: slot.session().content_state(),
            history: slot.session().history_status(),
            history_usage: slot.session().history_usage(),
            selection: slot.selection().clone(),
            saved_content: slot.saved_content_state(),
            dirty: slot.is_dirty(),
        }
    } else {
        let slot = workspace.prefab(document).unwrap();
        Snapshot {
            source: slot.session().document().instantiate(),
            revision: slot.revision(),
            content: slot.session().content_state(),
            history: slot.session().history_status(),
            history_usage: slot.session().history_usage(),
            selection: slot.selection().clone(),
            saved_content: slot.saved_content_state(),
            dirty: slot.is_dirty(),
        }
    }
}

fn is_removed(target: &PrefabEntityTarget) -> bool {
    target == &self::target("a") || target == &self::target("a-child")
}

#[test]
fn semantic_removal_repairs_owned_sibling_chains_and_undo_restores_exact_declarations() {
    let registry = composition_registry();
    for owner in [Owner::Native, Owner::Published] {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            for domain in [ScenePlacementParent::Root, parent("ui-root")] {
                let original = chain_document(&domain, owner);
                let hierarchy = original.local_hierarchy(Default::default()).unwrap();
                let siblings = match &domain {
                    ScenePlacementParent::Root => hierarchy.roots(),
                    ScenePlacementParent::Entity(root) => {
                        hierarchy.children(root.entity.entity()).unwrap()
                    }
                };
                assert_eq!(
                    &siblings[..6],
                    &["z-lead", "y-before", "a", "c", "b", "tail"].map(entity_id)
                );
                let (mut workspace, document) = open_owner(owner, kind, &original);
                assert!(
                    apply(
                        &mut workspace,
                        &registry,
                        owner,
                        EditorWorkspaceCommand::SelectEntity {
                            document,
                            entity: Some(entity_id("a-child")),
                        },
                    )
                    .applied
                );
                let before = snapshot(&workspace, document);

                // Exact patches do not acquire semantic repair as a side effect.
                let raw_patch = ScenePatchDocument::new([
                    ScenePatchOperation::RemoveEntity {
                        entity: entity_id("a"),
                    },
                    ScenePatchOperation::RemoveEntity {
                        entity: entity_id("a-child"),
                    },
                    ScenePatchOperation::SetPlacements {
                        expected: original.placements.clone(),
                        value: original
                            .placements
                            .iter()
                            .filter(|entry| !is_removed(&entry.target))
                            .cloned()
                            .collect(),
                    },
                ]);
                let raw_command = match kind {
                    EditorDocumentKind::Scene => EditorWorkspaceCommand::ApplyScenePatch {
                        document,
                        base_revision: before.revision,
                        patch: raw_patch,
                    },
                    EditorDocumentKind::Prefab => EditorWorkspaceCommand::ApplyPrefabPatch {
                        document,
                        base_revision: before.revision,
                        patch: raw_patch,
                    },
                    EditorDocumentKind::Extension(_) => unreachable!(),
                };
                let raw = apply(&mut workspace, &registry, owner, raw_command);
                assert!(!raw.applied, "{owner:?}/{kind:?}/{domain:?}");
                assert_eq!(snapshot(&workspace, document), before);

                let removed = apply(
                    &mut workspace,
                    &registry,
                    owner,
                    command(
                        document,
                        before.revision,
                        kind,
                        SceneCompositionCommand::RemoveEntity {
                            entity: entity_id("a"),
                        },
                    ),
                );
                assert!(
                    removed.applied,
                    "{owner:?}/{kind:?}: {:?}",
                    removed.diagnostics
                );
                let after = snapshot(&workspace, document);
                let expected_records = original
                    .entities
                    .iter()
                    .filter(|record| {
                        record.id != entity_id("a") && record.id != entity_id("a-child")
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                assert_eq!(after.source.entities, expected_records);

                let mut expected_placements = original
                    .placements
                    .iter()
                    .filter(|entry| !is_removed(&entry.target))
                    .cloned()
                    .collect::<Vec<_>>();
                let mut expected_slots = BTreeMap::from([
                    (target("z-lead"), ScenePlacementSlot::First),
                    (
                        target("y-before"),
                        ScenePlacementSlot::After(target("z-lead")),
                    ),
                    (target("c"), ScenePlacementSlot::After(target("y-before"))),
                    (target("b"), ScenePlacementSlot::After(target("c"))),
                    (target("tail"), ScenePlacementSlot::After(target("b"))),
                ]);
                if domain == ScenePlacementParent::Root {
                    expected_slots.insert(
                        target("other-parent"),
                        ScenePlacementSlot::After(target("tail")),
                    );
                }
                for entry in &mut expected_placements {
                    if let Some(slot) = expected_slots.get(&entry.target) {
                        entry.placement.slot = slot.clone();
                    }
                }
                assert_eq!(after.source.placements, expected_placements);
                let surviving_hierarchy = after.source.local_hierarchy(Default::default()).unwrap();
                assert_eq!(
                    surviving_hierarchy.roots(),
                    hierarchy
                        .roots()
                        .iter()
                        .filter(|id| *id != &entity_id("a"))
                        .cloned()
                        .collect::<Vec<_>>()
                );
                for record in &after.source.entities {
                    let expected_children = hierarchy
                        .children(&record.id)
                        .unwrap()
                        .iter()
                        .filter(|id| *id != &entity_id("a"))
                        .cloned()
                        .collect::<Vec<_>>();
                    assert_eq!(
                        surviving_hierarchy.children(&record.id).unwrap(),
                        expected_children
                    );
                }
                assert_eq!(after.history.undo_depth, 1);
                assert_eq!(after.history.redo_depth, 0);
                assert!(after.dirty);
                let replacement = match &domain {
                    ScenePlacementParent::Root => None,
                    ScenePlacementParent::Entity(root) => Some(root.entity.entity().clone()),
                };
                assert_eq!(after.selection.top_entity(), replacement.as_ref());
                if kind == EditorDocumentKind::Prefab {
                    assert_eq!(
                        removed.prefab_composition_outcome,
                        Some(PrefabCompositionOutcome::Removed {
                            entity: entity_id("a"),
                            selection: replacement.clone(),
                        })
                    );
                }
                let inverse = removed
                    .patch_report
                    .as_ref()
                    .unwrap()
                    .inverse
                    .as_ref()
                    .unwrap();
                assert_eq!(inverse.operations.len(), 3);
                let inverse_placements = inverse
                    .operations
                    .iter()
                    .filter_map(|operation| match operation {
                        ScenePatchOperation::SetPlacements { expected, value } => {
                            Some((expected, value))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    inverse_placements,
                    vec![(&after.source.placements, &original.placements)]
                );

                let undo = apply(
                    &mut workspace,
                    &registry,
                    owner,
                    EditorWorkspaceCommand::Undo {
                        document,
                        base_revision: after.revision,
                    },
                );
                assert!(undo.applied, "{owner:?}/{kind:?}: {:?}", undo.diagnostics);
                let undone = snapshot(&workspace, document);
                assert_eq!(undone.source, original);
                assert_eq!(undone.content, before.content);
                assert_eq!(undone.history.undo_depth, 0);
                assert_eq!(undone.history.redo_depth, 1);
                assert!(!undone.dirty);
                assert_eq!(undone.selection.top_entity(), replacement.as_ref());
                let redo = apply(
                    &mut workspace,
                    &registry,
                    owner,
                    EditorWorkspaceCommand::Redo {
                        document,
                        base_revision: undone.revision,
                    },
                );
                assert!(redo.applied, "{owner:?}/{kind:?}: {:?}", redo.diagnostics);
                let redone = snapshot(&workspace, document);
                assert_eq!(redone.source, after.source);
                assert_eq!(redone.content, after.content);
                assert_eq!(redone.history.undo_depth, 1);
                assert_eq!(redone.history.redo_depth, 0);
                assert_eq!(redone.selection, after.selection);
            }
        }
    }
}

#[test]
fn semantic_removal_without_dangling_sibling_intent_preserves_every_surviving_placement() {
    let registry = composition_registry();
    for owner in [Owner::Native, Owner::Published] {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            let mut original = chain_document(&parent("ui-root"), owner);
            for entry in &mut original.placements {
                if [target("b"), target("c"), target("y-before")].contains(&entry.target) {
                    entry.placement.slot = ScenePlacementSlot::Last;
                }
            }
            if matches!(owner, Owner::Native) {
                original
                    .entities
                    .iter_mut()
                    .find(|record| record.id == entity_id("ui-root"))
                    .unwrap()
                    .prefab = Some(PrefabInstance::forest(
                    AssetRef::path("prefabs/ui.prefab.json").unwrap(),
                ));
            }
            original.local_hierarchy(Default::default()).unwrap();
            let (mut workspace, document) = open_owner(owner, kind, &original);
            assert!(
                apply(
                    &mut workspace,
                    &registry,
                    owner,
                    EditorWorkspaceCommand::SelectEntity {
                        document,
                        entity: Some(entity_id("b")),
                    },
                )
                .applied
            );
            let revision = workspace.document_revision(document).unwrap();
            let removed = apply(
                &mut workspace,
                &registry,
                owner,
                command(
                    document,
                    revision,
                    kind,
                    SceneCompositionCommand::RemoveEntity {
                        entity: entity_id("a"),
                    },
                ),
            );
            assert!(
                removed.applied,
                "{owner:?}/{kind:?}: {:?}",
                removed.diagnostics
            );
            let after = snapshot(&workspace, document);
            assert_eq!(
                after.source.placements,
                original
                    .placements
                    .iter()
                    .filter(|entry| !is_removed(&entry.target))
                    .cloned()
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                after.source.entities,
                original
                    .entities
                    .iter()
                    .filter(|record| {
                        record.id != entity_id("a") && record.id != entity_id("a-child")
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            );
            assert_eq!(after.selection.top_entity(), Some(&entity_id("b")));
            assert_eq!(after.history.undo_depth, 1);
            assert!(
                apply(
                    &mut workspace,
                    &registry,
                    owner,
                    EditorWorkspaceCommand::Undo {
                        document,
                        base_revision: after.revision,
                    },
                )
                .applied
            );
            let undone = snapshot(&workspace, document);
            assert_eq!(undone.source, original);
            assert_eq!(undone.selection.top_entity(), Some(&entity_id("b")));
        }
    }
}

#[test]
fn semantic_removal_refuses_reauthoring_below_a_surviving_prefab_anchor() {
    let registry = composition_registry();
    for owner in [Owner::Native, Owner::Published] {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            let mut original = chain_document(&parent("ui-root"), owner);
            original
                .entities
                .iter_mut()
                .find(|record| record.id == entity_id("ui-root"))
                .unwrap()
                .prefab = Some(PrefabInstance::forest(
                AssetRef::path("prefabs/ui.prefab.json").unwrap(),
            ));
            original.local_hierarchy(Default::default()).unwrap();
            let (mut workspace, document) = open_owner(owner, kind, &original);
            // Keep both undo and redo entries live across the refusal.
            for slot in [ScenePlacementSlot::First, ScenePlacementSlot::Last] {
                let revision = workspace.document_revision(document).unwrap();
                let moved = apply(
                    &mut workspace,
                    &registry,
                    owner,
                    command(
                        document,
                        revision,
                        kind,
                        SceneCompositionCommand::MoveEntity {
                            target: target("other-parent"),
                            to: ScenePlacement {
                                parent: ScenePlacementParent::Root,
                                slot,
                            },
                        },
                    ),
                );
                assert!(moved.applied, "{owner:?}/{kind:?}: {:?}", moved.diagnostics);
            }
            let revision = workspace.document_revision(document).unwrap();
            assert!(
                apply(
                    &mut workspace,
                    &registry,
                    owner,
                    EditorWorkspaceCommand::Undo {
                        document,
                        base_revision: revision,
                    },
                )
                .applied
            );
            assert!(
                apply(
                    &mut workspace,
                    &registry,
                    owner,
                    EditorWorkspaceCommand::SelectEntity {
                        document,
                        entity: Some(entity_id("a-child")),
                    },
                )
                .applied
            );
            let before = snapshot(&workspace, document);
            assert_eq!(before.history.undo_depth, 1);
            assert_eq!(before.history.redo_depth, 1);
            let removed = apply(
                &mut workspace,
                &registry,
                owner,
                command(
                    document,
                    before.revision,
                    kind,
                    SceneCompositionCommand::RemoveEntity {
                        entity: entity_id("a"),
                    },
                ),
            );
            assert!(!removed.applied);
            assert!(removed.patch_report.is_none());
            let diagnostic = removed
                .diagnostics
                .iter()
                .find(|entry| {
                    entry.code().as_str() == "tooling.composition-removal-requires-source-context"
                })
                .unwrap();
            assert_eq!(
                diagnostic_field_value(diagnostic, "entity"),
                DiagnosticValueRef::Identifier("ui-root")
            );
            assert_eq!(snapshot(&workspace, document), before);
        }
    }
}

#[test]
fn semantic_removal_keeps_the_shared_local_query_refusal_for_routed_placements() {
    let registry = composition_registry();
    for owner in [Owner::Native, Owner::Published] {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            let mut original = chain_document(&parent("ui-root"), owner);
            original
                .entities
                .iter_mut()
                .find(|record| record.id == entity_id("other-parent"))
                .unwrap()
                .prefab = Some(PrefabInstance::forest(
                AssetRef::path("prefabs/other.prefab.json").unwrap(),
            ));
            let mut projected = target("inherited");
            projected.through.push(PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(entity_id("other-parent")),
                expected_base: SourceDocumentKey::named(
                    "a8451288-26cc-4dc0-83c7-4ea18ecac329".parse().unwrap(),
                    "other-prefab",
                    SourceDocumentKind::Prefab,
                )
                .unwrap(),
            });
            original.placements.push(SceneEntityPlacement::new(
                projected,
                ScenePlacement::last(ScenePlacementParent::Root),
            ));
            assert!(original.local_hierarchy(Default::default()).is_err());
            let (mut workspace, document) = open_owner(owner, kind, &original);
            let before = snapshot(&workspace, document);
            let removed = apply(
                &mut workspace,
                &registry,
                owner,
                command(
                    document,
                    before.revision,
                    kind,
                    SceneCompositionCommand::RemoveEntity {
                        entity: entity_id("a"),
                    },
                ),
            );
            assert!(!removed.applied);
            assert!(removed.patch_report.is_none());
            assert!(removed.diagnostics.iter().any(|entry| {
                entry.code().as_str() == "tooling.composition-removal-requires-local-hierarchy"
            }));
            assert_eq!(snapshot(&workspace, document), before);
        }
    }
}
