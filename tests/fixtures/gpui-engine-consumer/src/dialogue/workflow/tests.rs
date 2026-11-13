use super::*;
use crate::dialogue::tests::story;
use nara_engine::tooling::EditorExtensionDocument;

fn snapshot(graph: &DialogueGraph, revision: DocumentRevision) -> DialogueSnapshot {
    DialogueSnapshot {
        document: EditorDocumentId::from_raw(1),
        revision,
        view: graph.view(),
        dirty: true,
        history: graph.history_status(),
        persistence_pending: false,
    }
}

#[test]
fn review_merges_unedited_fields_and_requires_explicit_same_field_resolution() {
    for resolution in [ConflictResolution::Current, ConflictResolution::Draft] {
        let mut graph = story();
        let initial = snapshot(&graph, DocumentRevision::initial());
        let mut draft = DialogueDraft::new(initial.clone(), NodeId(1)).unwrap();
        draft.edit(|value| {
            value.text = "Local text\n本地".into();
            value.position[0] = "12".into();
        });
        graph
            .apply(&DialogueCommand::SetLine {
                node: NodeId(1),
                speaker: "Peer speaker".into(),
                text: "Peer text\n当前".into(),
            })
            .unwrap();
        graph
            .apply(&DialogueCommand::Move(vec![NodePosition {
                node: NodeId(1),
                position: [0., 24.],
            }]))
            .unwrap();
        let current = snapshot(&graph, initial.revision.next());
        draft.observe(&current);
        assert!(draft.review(&current));
        assert_eq!(draft.value().speaker, "Peer speaker");
        assert_eq!(draft.value().position, ["12", "24"]);
        assert_eq!(
            draft.conflicts(),
            vec![DraftConflict {
                field: DraftField::Text,
                current: "Peer text\n当前".into(),
                draft: "Local text\n本地".into(),
            }]
        );
        assert!(draft.prepare(&current, false).is_err());
        assert!(draft.prepare(&current, true).is_err());
        assert!(draft.review(&current));
        assert_eq!(draft.conflicts().len(), 1);
        assert!(draft.resolve(DraftField::Text, resolution, &current));
        let expected = if resolution == ConflictResolution::Current {
            "Peer text\n当前"
        } else {
            "Local text\n本地"
        };
        assert_eq!(draft.value().text, expected);
        assert_eq!(draft.state(), DraftState::Editing);
        let command = draft.prepare(&current, false).unwrap();
        assert_eq!(command.document(), current.document);
        assert_eq!(command.base_revision(), current.revision);
        assert_eq!(draft.state(), DraftState::Pending);
    }
}

#[test]
fn review_preserves_peer_choice_targets_labels_additions_and_stable_ids() {
    let mut graph = story();
    let initial = snapshot(&graph, DocumentRevision::initial());
    let mut draft = DialogueDraft::new(initial.clone(), NodeId(2)).unwrap();
    let first = draft.value().choices[0].id.unwrap();
    let second = draft.value().choices[1].id.unwrap();
    draft.edit(|value| {
        value.choices[0].label = "Local label".into();
        value.choices.reverse();
    });
    let mut peer = NodeDraft::capture(&initial, NodeId(2)).unwrap();
    peer.choices[0].target = Some(NodeId(1));
    peer.choices[1].label = "Peer label".into();
    peer.choices.push(ChoiceDraft {
        id: None,
        label: "Peer addition".into(),
        target: Some(NodeId(3)),
    });
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Peer prompt".into(),
            choices: peer.choices,
        })
        .unwrap();
    let current = snapshot(&graph, initial.revision.next());
    assert!(draft.review(&current));
    assert!(draft.conflicts().is_empty());
    assert_eq!(draft.value().text, "Peer prompt");
    assert_eq!(draft.value().choices[0].id, Some(second));
    assert_eq!(draft.value().choices[0].label, "Peer label");
    assert_eq!(draft.value().choices[1].id, Some(first));
    assert_eq!(draft.value().choices[1].label, "Local label");
    assert_eq!(draft.value().choices[1].target, Some(NodeId(1)));
    assert_eq!(draft.value().choices[2].label, "Peer addition");
    assert!(draft.value().choices[2].id.is_some());
    assert!(draft.prepare(&current, false).is_ok());
}

#[test]
fn review_choice_field_and_removal_conflicts_never_resurrect_retired_choice_ids() {
    let mut graph = story();
    let initial = snapshot(&graph, DocumentRevision::initial());
    let mut draft = DialogueDraft::new(initial.clone(), NodeId(2)).unwrap();
    let first = draft.value().choices[0].id.unwrap();
    let second = draft.value().choices[1].id.unwrap();
    draft.edit(|value| {
        value.choices[0].label = "Local label".into();
        value.choices[1].label = "Retained proposal".into();
    });
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Where next?".into(),
            choices: vec![ChoiceDraft {
                id: Some(first),
                label: "Peer label".into(),
                target: Some(NodeId(3)),
            }],
        })
        .unwrap();
    let current = snapshot(&graph, initial.revision.next());
    assert!(draft.review(&current));
    assert_eq!(draft.conflicts().len(), 2);
    assert!(draft.prepare(&current, false).is_err());
    assert!(draft.resolve(
        DraftField::ChoiceLabel(first),
        ConflictResolution::Current,
        &current
    ));
    assert!(draft.resolve(
        DraftField::Choice(second),
        ConflictResolution::Draft,
        &current
    ));
    assert!(draft.conflicts().is_empty());
    assert_eq!(draft.value().choices[0].id, Some(first));
    assert_eq!(draft.value().choices[0].label, "Peer label");
    assert_eq!(draft.value().choices[1].id, None);
    assert_eq!(draft.value().choices[1].label, "Retained proposal");
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: draft.value().text.clone(),
            choices: draft.value().choices.clone(),
        })
        .unwrap();
    let accepted =
        NodeDraft::capture(&snapshot(&graph, current.revision.next()), NodeId(2)).unwrap();
    assert_ne!(accepted.choices[1].id, Some(second));
}

#[test]
fn current_choice_field_restores_member_removed_after_review_before_apply() {
    for target_conflict in [false, true] {
        let mut graph = story();
        let initial = snapshot(&graph, DocumentRevision::initial());
        let mut draft = DialogueDraft::new(initial.clone(), NodeId(2)).unwrap();
        let mut peer = draft.value().choices.clone();
        let id = peer[0].id.unwrap();
        let prompt = draft.value().text.clone();
        draft.edit(|value| {
            value.text = "Keep local prompt".into();
            if target_conflict {
                value.choices[0].target = None;
            } else {
                value.choices[0].label = "Local label".into();
            }
        });
        peer[0].label = "Peer label".into();
        peer[0].target = Some(NodeId(1));
        graph
            .apply(&DialogueCommand::SetChoices {
                node: NodeId(2),
                prompt,
                choices: peer.clone(),
            })
            .unwrap();
        let current = snapshot(&graph, initial.revision.next());
        let field = if target_conflict {
            DraftField::ChoiceTarget(id)
        } else {
            DraftField::ChoiceLabel(id)
        };
        assert!(draft.review(&current));
        assert_eq!(draft.conflicts().len(), 1);
        assert_eq!(draft.conflicts()[0].field, field);
        assert!(draft.edit(|value| value.choices.retain(|entry| entry.id != Some(id))));
        assert_eq!(draft.conflicts()[0].draft, "Removed");
        assert!(draft.prepare(&current, false).is_err());
        assert!(draft.resolve(field, ConflictResolution::Current, &current));
        assert!(draft.conflicts().is_empty());
        assert_eq!(draft.value().choices, peer);
        assert!(draft.prepare(&current, false).is_ok());
        graph
            .apply(&DialogueCommand::SetChoices {
                node: NodeId(2),
                prompt: draft.value().text.clone(),
                choices: draft.value().choices.clone(),
            })
            .unwrap();
        let accepted =
            NodeDraft::capture(&snapshot(&graph, current.revision.next()), NodeId(2)).unwrap();
        assert_eq!(accepted.choices, peer);
        assert_eq!(accepted.text, "Keep local prompt");
        assert_eq!(
            graph.history_status().undo_depth,
            current.history.undo_depth + 1
        );
    }
}

#[test]
fn unresolved_current_choice_field_stays_conflicted_until_member_resolution() {
    let mut graph = story();
    let initial = snapshot(&graph, DocumentRevision::initial());
    let mut draft = DialogueDraft::new(initial.clone(), NodeId(2)).unwrap();
    let mut peer = draft.value().choices.clone();
    let id = peer[0].id.unwrap();
    draft.edit(|value| value.choices[0].label = "Local label".into());
    peer[0].label = "Peer label".into();
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: draft.value().text.clone(),
            choices: peer.clone(),
        })
        .unwrap();
    let current = snapshot(&graph, initial.revision.next());
    assert!(draft.review(&current));
    peer.remove(0);
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: draft.value().text.clone(),
            choices: peer,
        })
        .unwrap();
    let removed = snapshot(&graph, current.revision.next());
    assert!(draft.review(&removed));
    let conflicts = draft.conflicts();
    assert!(!draft.resolve(
        DraftField::ChoiceLabel(id),
        ConflictResolution::Current,
        &removed
    ));
    assert_eq!(draft.conflicts(), conflicts);
    assert_eq!(draft.state(), DraftState::Conflict);
    assert!(draft.prepare(&removed, false).is_err());
    assert!(draft.resolve(
        DraftField::Choice(id),
        ConflictResolution::Current,
        &removed
    ));
    assert!(draft.conflicts().is_empty());
    assert!(
        draft
            .value()
            .choices
            .iter()
            .all(|entry| entry.id != Some(id))
    );
}

#[test]
fn review_choice_order_and_local_delete_against_peer_edit_require_resolution() {
    let mut graph = story();
    let initial_choices =
        NodeDraft::capture(&snapshot(&graph, DocumentRevision::initial()), NodeId(2)).unwrap();
    let mut entries = initial_choices.choices;
    entries.push(ChoiceDraft {
        id: None,
        label: "Third".into(),
        target: None,
    });
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Prompt".into(),
            choices: entries,
        })
        .unwrap();
    let initial = snapshot(&graph, DocumentRevision::initial());
    let mut draft = DialogueDraft::new(initial.clone(), NodeId(2)).unwrap();
    draft.edit(|value| value.choices.swap(0, 1));
    let mut peer = draft.original.choices.clone();
    peer.swap(1, 2);
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Prompt".into(),
            choices: peer.clone(),
        })
        .unwrap();
    let current = snapshot(&graph, initial.revision.next());
    assert!(draft.review(&current));
    assert!(
        draft
            .conflicts()
            .iter()
            .any(|conflict| conflict.field == DraftField::ChoiceOrder)
    );
    assert!(draft.resolve(
        DraftField::ChoiceOrder,
        ConflictResolution::Current,
        &current
    ));
    assert_eq!(draft.value().choices, peer);
    let mut deleted = DialogueDraft::new(current.clone(), NodeId(2)).unwrap();
    let removed = deleted.value().choices[0].id.unwrap();
    deleted.edit(|value| {
        value.choices.remove(0);
    });
    peer[0].label = "Peer edited before local delete".into();
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Prompt".into(),
            choices: peer.clone(),
        })
        .unwrap();
    let updated = snapshot(&graph, current.revision.next());
    assert!(deleted.review(&updated));
    assert!(deleted.resolve(
        DraftField::Choice(removed),
        ConflictResolution::Current,
        &updated
    ));
    assert_eq!(deleted.value().choices, peer);
}

#[test]
fn live_selection_requires_original_source_and_node_incarnation() {
    let mut graph = story();
    let initial = snapshot(&graph, DocumentRevision::initial());
    let mut draft = DialogueDraft::new(initial.clone(), NodeId(1)).unwrap();
    assert!(draft.has_live_node(&initial));
    let reopened = snapshot(&graph, DocumentRevision::initial());
    assert!(!draft.has_live_node(&reopened));
    graph
        .apply(&DialogueCommand::Remove {
            nodes: vec![NodeId(1)],
            links: Vec::new(),
        })
        .unwrap();
    graph.undo().unwrap();
    let restored = snapshot(&graph, initial.revision.next());
    assert!(!draft.has_live_node(&restored));
    draft.observe(&restored);
    assert_eq!(draft.state(), DraftState::Retired);
    assert!(!draft.resolve(DraftField::Text, ConflictResolution::Draft, &restored));
    assert!(
        DialogueDraft::new(restored.clone(), NodeId(1))
            .unwrap()
            .has_live_node(&restored)
    );
}

#[test]
fn local_addition_preserves_peer_reorder_and_its_own_insertion_point() {
    for insertion in 0..=2 {
        let mut graph = story();
        let initial = snapshot(&graph, DocumentRevision::initial());
        let mut draft = DialogueDraft::new(initial.clone(), NodeId(2)).unwrap();
        let original = draft.value().choices.clone();
        draft.edit(|value| {
            value.choices.insert(
                insertion,
                ChoiceDraft {
                    id: None,
                    label: "Local new choice".into(),
                    target: None,
                },
            )
        });
        let mut peer = draft.original.choices.clone();
        peer.reverse();
        graph
            .apply(&DialogueCommand::SetChoices {
                node: NodeId(2),
                prompt: draft.value().text.clone(),
                choices: peer.clone(),
            })
            .unwrap();
        let current = snapshot(&graph, initial.revision.next());
        assert!(draft.review(&current));
        assert!(draft.conflicts().is_empty());
        assert_eq!(
            draft
                .value()
                .choices
                .iter()
                .filter(|choice| choice.id.is_some())
                .cloned()
                .collect::<Vec<_>>(),
            peer
        );
        let local_index = draft
            .value()
            .choices
            .iter()
            .position(|choice| choice.id.is_none())
            .unwrap();
        if insertion < 2 {
            assert_eq!(
                draft.value().choices[local_index + 1].id,
                original[insertion].id
            );
        } else {
            assert_eq!(local_index, 2);
        }
    }
}

#[test]
fn resolving_one_peer_choice_preserves_unrelated_local_reordering() {
    let mut graph = story();
    let mut entries = NodeDraft::capture(&snapshot(&graph, DocumentRevision::initial()), NodeId(2))
        .unwrap()
        .choices;
    entries.push(ChoiceDraft {
        id: None,
        label: "Third".into(),
        target: None,
    });
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Prompt".into(),
            choices: entries,
        })
        .unwrap();
    let initial = snapshot(&graph, DocumentRevision::initial());
    let mut draft = DialogueDraft::new(initial.clone(), NodeId(2)).unwrap();
    let mut peer = draft.value().choices.clone();
    let removed = peer[2].id.unwrap();
    draft.edit(|value| {
        value.choices.swap(0, 1);
        value.choices.remove(2);
    });
    let expected_order = draft
        .value()
        .choices
        .iter()
        .map(|choice| choice.id)
        .chain([Some(removed)])
        .collect::<Vec<_>>();
    peer[2].label = "Peer edit of removed choice".into();
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Prompt".into(),
            choices: peer,
        })
        .unwrap();
    let current = snapshot(&graph, initial.revision.next());
    assert!(draft.review(&current));
    assert!(draft.resolve(
        DraftField::Choice(removed),
        ConflictResolution::Current,
        &current
    ));
    assert_eq!(
        draft
            .value()
            .choices
            .iter()
            .map(|choice| choice.id)
            .collect::<Vec<_>>(),
        expected_order
    );
    assert_eq!(
        draft.value().choices[2].label,
        "Peer edit of removed choice"
    );
    assert!(draft.conflicts().is_empty());
}

#[test]
fn resource_views_share_original_index_and_preserve_independent_cjk_queries_and_selection() {
    use nara_engine::tooling::{
        EditorProjectBrowserBudgetUsage, EditorProjectBrowserEntry,
        EditorProjectBrowserLocalAction, EditorProjectBrowserSnapshot, EditorProjectBrowserState,
        EditorProjectBrowserStatus, EditorProjectSourceInspectReason, EditorProjectSourceKind,
        EditorProjectSourceOpenCapability,
    };
    let resource = "assets/对话参考/森林与旅程/long-project-resource-path-preserved-without-truncating-the-original-source-index-identity-or-query-selection-旅程说明.txt";
    let scene = "scenes/startup.scene.json";
    let entries = vec![
        EditorProjectBrowserEntry::new(
            Arc::from(resource),
            EditorProjectSourceKind::AssetSource,
            EditorProjectSourceOpenCapability::InspectOnly(
                EditorProjectSourceInspectReason::ReadOnlySource,
            ),
        ),
        EditorProjectBrowserEntry::new(
            Arc::from(scene),
            EditorProjectSourceKind::Scene,
            EditorProjectSourceOpenCapability::Scene,
        ),
    ];
    let mut original = EditorProjectBrowserState::new();
    original.synchronize(
        EditorProjectBrowserStatus::Ready,
        Some(Arc::new(EditorProjectBrowserSnapshot::new(
            1,
            entries.clone(),
            [],
            EditorProjectBrowserBudgetUsage::default(),
        ))),
        None,
        None,
        false,
        false,
    );
    let mut views = [
        EditorProjectBrowserState::new(),
        EditorProjectBrowserState::new(),
    ];
    for (index, view) in views.iter_mut().enumerate() {
        view.synchronize(
            EditorProjectBrowserStatus::Ready,
            original.view().shared_snapshot(),
            None,
            None,
            false,
            false,
        );
        assert!(Arc::ptr_eq(
            &view.view().shared_snapshot().unwrap(),
            &original.view().shared_snapshot().unwrap()
        ));
        view.apply_local_action(EditorProjectBrowserLocalAction::SetPathFilter {
            value: Arc::from(if index == 0 {
                "旅程说明"
            } else {
                "startup.scene.json"
            }),
        });
        while view.advance_query(1).pending() {}
        assert_eq!(view.view().rows().len(), 1);
        view.apply_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from(if index == 0 { resource } else { scene })),
        });
    }
    assert!(original.view().selected_path().is_none());
    assert!(original.view().path_filter().is_empty());
    assert_eq!(views[0].view().selected_path(), Some(resource));
    assert!(
        !views[0]
            .view()
            .selected_entry()
            .unwrap()
            .open_capability()
            .is_document()
    );
    assert!(
        views[1]
            .view()
            .selected_entry()
            .unwrap()
            .open_capability()
            .is_document()
    );
    for generation in [2, 3] {
        let entries = if generation == 2 {
            entries.clone()
        } else {
            vec![entries[1].clone()]
        };
        original.synchronize(
            EditorProjectBrowserStatus::Ready,
            Some(Arc::new(EditorProjectBrowserSnapshot::new(
                generation,
                entries,
                [],
                EditorProjectBrowserBudgetUsage::default(),
            ))),
            None,
            None,
            false,
            false,
        );
        for view in &mut views {
            view.synchronize(
                EditorProjectBrowserStatus::Ready,
                original.view().shared_snapshot(),
                None,
                None,
                false,
                false,
            );
            while view.advance_query(1).pending() {}
        }
        assert_eq!(views[0].view().path_filter(), "旅程说明");
        assert_eq!(views[1].view().path_filter(), "startup.scene.json");
        assert_eq!(views[1].view().selected_path(), Some(scene));
        assert_eq!(
            views[0].view().selected_path(),
            (generation == 2).then_some(resource)
        );
    }
}
