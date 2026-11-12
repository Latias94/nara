use super::{
    preview::{Conversation, ConversationStep, PreviewError},
    *,
};

pub(super) fn add(graph: &mut DialogueGraph, recipe: NodeRecipe) -> NodeId {
    let DialogueCommandOutcome::Created { node, .. } = graph
        .apply(&DialogueCommand::Add {
            recipe,
            position: [0., 0.],
        })
        .unwrap()
    else {
        panic!("created node");
    };
    node
}

pub(super) fn story() -> DialogueGraph {
    let mut graph = DialogueGraph::new();
    assert_eq!(add(&mut graph, NodeRecipe::Line), NodeId(1));
    assert_eq!(add(&mut graph, NodeRecipe::Choice), NodeId(2));
    assert_eq!(add(&mut graph, NodeRecipe::End), NodeId(3));
    graph
        .apply(&DialogueCommand::SetLine {
            node: NodeId(1),
            speaker: "Guide".to_owned(),
            text: "Welcome, traveller.".to_owned(),
        })
        .unwrap();
    graph
        .apply(&DialogueCommand::SetTarget {
            port: OutputPort::Next { node: NodeId(1) },
            target: Some(NodeId(2)),
        })
        .unwrap();
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Where next?".to_owned(),
            choices: vec![
                ChoiceDraft {
                    id: Some(ChoiceId(1)),
                    label: "Finish".to_owned(),
                    target: Some(NodeId(3)),
                },
                ChoiceDraft {
                    id: None,
                    label: "Again".to_owned(),
                    target: Some(NodeId(1)),
                },
            ],
        })
        .unwrap();
    graph
}

pub(super) fn state(
    graph: &DialogueGraph,
) -> (Vec<u8>, DocumentContentState, DocumentHistoryStatus) {
    (
        graph.encode().unwrap(),
        graph.content_state(),
        graph.history_status(),
    )
}

pub(super) fn refuses(graph: &mut DialogueGraph, command: DialogueCommand) {
    let before = state(graph);
    let view = graph.view();
    let undo = graph.undo.clone();
    let redo = graph.redo.clone();
    let allocation = (
        graph.next_node,
        graph.next_choice,
        graph.next_incarnation,
        graph.clock,
    );
    assert!(graph.apply(&command).is_err());
    assert_eq!(state(graph), before);
    assert!(Arc::ptr_eq(&view, &graph.view()));
    assert_eq!(graph.undo, undo);
    assert_eq!(graph.redo, redo);
    assert_eq!(
        (
            graph.next_node,
            graph.next_choice,
            graph.next_incarnation,
            graph.clock,
        ),
        allocation
    );
}

pub(super) fn choice_port(choice: u64) -> OutputPort {
    OutputPort::Choice {
        node: NodeId(2),
        choice: ChoiceId(choice),
    }
}

#[test]
fn empty_and_unconnected_dialogues_are_valid_authoring_states() {
    let mut graph = DialogueGraph::new();
    assert_eq!(
        Conversation::new(graph.view()).step(),
        Err(PreviewError::EntryMissing)
    );
    let initial = graph.encode().unwrap();
    assert_eq!(
        DialogueGraph::decode(&initial).unwrap().encode().unwrap(),
        initial
    );
    let line = add(&mut graph, NodeRecipe::Line);
    assert_eq!(graph.view().entry(), Some(line));
    let mut conversation = Conversation::new(graph.view());
    assert!(matches!(
        conversation.step().unwrap(),
        ConversationStep::Line { next: None, .. }
    ));
    assert_eq!(
        conversation.advance(),
        Err(PreviewError::TargetMissing(OutputPort::Next { node: line }))
    );
    assert_eq!(conversation.current(), Some(line));
    graph.apply(&DialogueCommand::SetEntry(None)).unwrap();
    assert_eq!(
        Conversation::new(graph.view()).step(),
        Err(PreviewError::EntryMissing)
    );
}

#[test]
fn one_choice_edit_adds_a_real_port_and_target_with_one_history_entry() {
    let mut graph = story();
    let before = state(&graph);
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Take a path".to_owned(),
            choices: vec![
                ChoiceDraft {
                    id: Some(ChoiceId(1)),
                    label: "Finish".to_owned(),
                    target: Some(NodeId(3)),
                },
                ChoiceDraft {
                    id: Some(ChoiceId(2)),
                    label: "Again".to_owned(),
                    target: Some(NodeId(1)),
                },
                ChoiceDraft {
                    id: None,
                    label: "Stay".to_owned(),
                    target: Some(NodeId(2)),
                },
            ],
        })
        .unwrap();
    assert_eq!(graph.history_status().undo_depth, before.2.undo_depth + 1);
    assert!(graph.view().contains_output(choice_port(3)));
    assert_eq!(graph.view().target(choice_port(3)), Some(NodeId(2)));
    let after = state(&graph);
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), before.0);
    assert_eq!(graph.content_state(), before.1);
    assert!(!graph.view().contains_output(choice_port(3)));
    assert_eq!(graph.view().target(choice_port(3)), None);
    graph.redo().unwrap();
    assert_eq!(graph.encode().unwrap(), after.0);
    assert_eq!(graph.content_state(), after.1);
}

#[test]
fn choice_reorder_and_label_edit_preserve_port_and_link_incarnations() {
    let mut graph = story();
    let before = graph.view();
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Choose again".to_owned(),
            choices: vec![
                ChoiceDraft {
                    id: Some(ChoiceId(2)),
                    label: "回到开头".to_owned(),
                    target: Some(NodeId(1)),
                },
                ChoiceDraft {
                    id: Some(ChoiceId(1)),
                    label: "结束".to_owned(),
                    target: Some(NodeId(3)),
                },
            ],
        })
        .unwrap();
    let after = graph.view();
    let NodeBody::Choice { choices, .. } = after.node(NodeId(2)).unwrap().body.as_ref() else {
        panic!("choice node");
    };
    assert_eq!(
        choices.iter().map(|choice| choice.id).collect::<Vec<_>>(),
        [ChoiceId(2), ChoiceId(1)]
    );
    for port in [choice_port(1), choice_port(2)] {
        assert_eq!(after.target(port), before.target(port));
        assert_eq!(
            after.output_incarnation(port),
            before.output_incarnation(port)
        );
        assert_eq!(after.link_incarnation(port), before.link_incarnation(port));
    }
}

#[test]
fn removing_a_choice_retires_only_its_output_and_incident_link() {
    let mut graph = story();
    let before = graph.view();
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "One path".to_owned(),
            choices: vec![ChoiceDraft {
                id: Some(ChoiceId(1)),
                label: "Finish".to_owned(),
                target: Some(NodeId(3)),
            }],
        })
        .unwrap();
    let removed = graph.view();
    assert!(!removed.contains_output(choice_port(2)));
    assert_eq!(removed.output_incarnation(choice_port(2)), None);
    assert_eq!(removed.link_incarnation(choice_port(2)), None);
    assert_eq!(
        removed.target(OutputPort::Next { node: NodeId(1) }),
        Some(NodeId(2))
    );
    graph.undo().unwrap();
    let restored = graph.view();
    assert_eq!(restored.target(choice_port(2)), Some(NodeId(1)));
    assert_ne!(
        restored.output_incarnation(choice_port(2)),
        before.output_incarnation(choice_port(2))
    );
    assert_ne!(
        restored.link_incarnation(choice_port(2)),
        before.link_incarnation(choice_port(2))
    );
    assert_eq!(
        restored.output_incarnation(choice_port(1)),
        before.output_incarnation(choice_port(1))
    );
}

#[test]
fn refusal_after_new_choices_does_not_consume_their_identifiers() {
    let mut graph = story();
    refuses(
        &mut graph,
        DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Invalid".to_owned(),
            choices: vec![
                ChoiceDraft {
                    id: None,
                    label: "Temporary".to_owned(),
                    target: Some(NodeId(3)),
                },
                ChoiceDraft {
                    id: Some(ChoiceId(999)),
                    label: "Foreign".to_owned(),
                    target: None,
                },
            ],
        },
    );
    graph
        .apply(&DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Valid".to_owned(),
            choices: vec![ChoiceDraft {
                id: None,
                label: "New".to_owned(),
                target: None,
            }],
        })
        .unwrap();
    assert!(graph.view().contains_output(choice_port(3)));
    assert!(!graph.view().contains_output(choice_port(4)));
}

#[test]
fn foreign_duplicate_and_dangling_choice_edits_are_atomic() {
    let mut graph = story();
    let other = add(&mut graph, NodeRecipe::Choice);
    assert_eq!(other, NodeId(4));
    for choices in [
        vec![ChoiceDraft {
            id: Some(ChoiceId(3)),
            label: "Foreign owner".to_owned(),
            target: None,
        }],
        vec![
            ChoiceDraft {
                id: Some(ChoiceId(1)),
                label: "First".to_owned(),
                target: None,
            },
            ChoiceDraft {
                id: Some(ChoiceId(1)),
                label: "Duplicate".to_owned(),
                target: None,
            },
        ],
        vec![ChoiceDraft {
            id: None,
            label: "Missing target".to_owned(),
            target: Some(NodeId(404)),
        }],
    ] {
        refuses(
            &mut graph,
            DialogueCommand::SetChoices {
                node: NodeId(2),
                prompt: "Rejected".to_owned(),
                choices,
            },
        );
    }
}

#[test]
fn batch_removal_preserves_atomicity_and_undo_restores_source_with_fresh_lifetimes() {
    let mut graph = story();
    let source = state(&graph);
    let previous = graph.view();
    refuses(
        &mut graph,
        DialogueCommand::Remove {
            nodes: vec![NodeId(1), NodeId(404)],
            links: Vec::new(),
        },
    );
    refuses(
        &mut graph,
        DialogueCommand::Remove {
            nodes: vec![NodeId(1), NodeId(1)],
            links: Vec::new(),
        },
    );
    graph
        .apply(&DialogueCommand::Remove {
            nodes: vec![NodeId(1), NodeId(2)],
            links: Vec::new(),
        })
        .unwrap();
    assert_eq!(graph.view().entry(), None);
    assert_eq!(graph.view().nodes().len(), 1);
    assert_eq!(graph.view().links().len(), 0);
    assert_eq!(graph.history_status().undo_depth, source.2.undo_depth + 1);
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), source.0);
    assert_eq!(graph.content_state(), source.1);
    let restored = graph.view();
    for node in [NodeId(1), NodeId(2)] {
        assert_ne!(
            restored.node_incarnation(node),
            previous.node_incarnation(node)
        );
    }
    assert_eq!(
        restored.node_incarnation(NodeId(3)),
        previous.node_incarnation(NodeId(3))
    );
    assert_ne!(
        restored.output_incarnation(choice_port(1)),
        previous.output_incarnation(choice_port(1))
    );
    assert_eq!(previous.node(NodeId(1)).unwrap().position, [0., 0.]);
}

#[test]
fn disconnect_and_reconnect_cannot_revive_a_coalesced_old_link() {
    let mut graph = story();
    let port = choice_port(1);
    let previous = graph.view();
    let source = graph.encode().unwrap();
    graph
        .apply(&DialogueCommand::SetTarget { port, target: None })
        .unwrap();
    graph.undo().unwrap();
    let restored = graph.view();
    assert_eq!(graph.encode().unwrap(), source);
    assert_eq!(
        restored.output_incarnation(port),
        previous.output_incarnation(port)
    );
    assert_ne!(
        restored.link_incarnation(port),
        previous.link_incarnation(port)
    );
}

#[test]
fn multi_node_move_is_one_edit_and_shares_unchanged_text_payloads() {
    let mut graph = story();
    let before = graph.view();
    let history = graph.history_status();
    graph
        .apply(&DialogueCommand::Move(vec![
            NodePosition {
                node: NodeId(1),
                position: [24., 48.],
            },
            NodePosition {
                node: NodeId(2),
                position: [-16., 128.],
            },
        ]))
        .unwrap();
    let after = graph.view();
    assert_eq!(after.node(NodeId(1)).unwrap().position, [24., 48.]);
    assert_eq!(after.node(NodeId(2)).unwrap().position, [-16., 128.]);
    assert_eq!(graph.history_status().undo_depth, history.undo_depth + 1);
    for node in [NodeId(1), NodeId(2), NodeId(3)] {
        assert!(Arc::ptr_eq(
            &before.node(node).unwrap().body,
            &after.node(node).unwrap().body
        ));
        assert_eq!(after.node_incarnation(node), before.node_incarnation(node));
    }
    assert_eq!(before.node(NodeId(1)).unwrap().position, [0., 0.]);
}

#[test]
fn invalid_moves_and_wrong_kind_commands_leave_all_owner_state_unchanged() {
    let mut graph = story();
    for command in [
        DialogueCommand::Move(vec![
            NodePosition {
                node: NodeId(1),
                position: [12., 12.],
            },
            NodePosition {
                node: NodeId(404),
                position: [0., 0.],
            },
        ]),
        DialogueCommand::Move(vec![
            NodePosition {
                node: NodeId(1),
                position: [0., 0.],
            },
            NodePosition {
                node: NodeId(1),
                position: [12., 12.],
            },
        ]),
        DialogueCommand::Move(vec![NodePosition {
            node: NodeId(1),
            position: [f32::NAN, 0.],
        }]),
        DialogueCommand::Move(vec![NodePosition {
            node: NodeId(1),
            position: [POSITION_LIMIT + 1., 0.],
        }]),
        DialogueCommand::SetLine {
            node: NodeId(3),
            speaker: String::new(),
            text: "Wrong kind".to_owned(),
        },
        DialogueCommand::SetTarget {
            port: OutputPort::Next { node: NodeId(3) },
            target: Some(NodeId(1)),
        },
        DialogueCommand::SetEntry(Some(NodeId(404))),
    ] {
        refuses(&mut graph, command);
    }
}

#[test]
fn undo_and_branching_restore_content_without_reusing_live_identifiers() {
    let mut graph = DialogueGraph::new();
    add(&mut graph, NodeRecipe::Line);
    let saved = state(&graph);
    let retired = add(&mut graph, NodeRecipe::Choice);
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), saved.0);
    assert_eq!(graph.content_state(), saved.1);
    let replacement = add(&mut graph, NodeRecipe::Choice);
    assert_eq!(retired, NodeId(2));
    assert_eq!(replacement, NodeId(3));
    let view = graph.view();
    let NodeBody::Choice { choices, .. } = view.node(replacement).unwrap().body.as_ref() else {
        panic!("choice node");
    };
    assert_eq!(choices[0].id, ChoiceId(2));
    assert_eq!(graph.history_status().redo_depth, 0);
    assert!(graph.redo().is_err());
}

#[test]
fn no_op_preserves_content_identity_and_pending_redo() {
    let mut graph = DialogueGraph::new();
    let node = add(&mut graph, NodeRecipe::Line);
    graph
        .apply(&DialogueCommand::SetLine {
            node,
            speaker: "Guide".to_owned(),
            text: "Changed".to_owned(),
        })
        .unwrap();
    graph.undo().unwrap();
    let before = state(&graph);
    let view = graph.view();
    graph
        .apply(&DialogueCommand::SetLine {
            node,
            speaker: "Guide".to_owned(),
            text: "New line".to_owned(),
        })
        .unwrap();
    assert_eq!(state(&graph), before);
    assert!(Arc::ptr_eq(&view, &graph.view()));
    graph.redo().unwrap();
    let view = graph.view();
    assert!(
        matches!(view.node(node).unwrap().body.as_ref(), NodeBody::Line { text, .. } if text == "Changed")
    );
}

#[test]
fn history_has_one_shared_bound_across_undo_and_redo() {
    let mut graph = DialogueGraph::new();
    let node = add(&mut graph, NodeRecipe::End);
    let mut oldest = None;
    for index in 0..(HISTORY_LIMIT + 6) {
        graph
            .apply(&DialogueCommand::SetEnd {
                node,
                text: format!("Ending {index}"),
            })
            .unwrap();
        if index == 5 {
            oldest = Some((graph.encode().unwrap(), graph.content_state()));
        }
    }
    assert_eq!(graph.history_status().undo_depth, HISTORY_LIMIT);
    for _ in 0..HISTORY_LIMIT {
        graph.undo().unwrap();
    }
    let oldest = oldest.unwrap();
    assert_eq!(graph.encode().unwrap(), oldest.0);
    assert_eq!(graph.content_state(), oldest.1);
    assert!(graph.undo().is_err());
    assert_eq!(graph.history_status().redo_depth, HISTORY_LIMIT);
    for _ in 0..HISTORY_LIMIT {
        graph.redo().unwrap();
    }
    assert!(graph.redo().is_err());
}

#[test]
fn full_utf8_text_survives_codec_and_history_without_truncation() {
    let mut graph = DialogueGraph::new();
    let node = add(&mut graph, NodeRecipe::Line);
    for text in ["a".repeat(TEXT_BYTES), format!("{}a", "界".repeat(1365))] {
        assert_eq!(text.len(), TEXT_BYTES);
        graph
            .apply(&DialogueCommand::SetLine {
                node,
                speaker: "向导".to_owned(),
                text: text.clone(),
            })
            .unwrap();
        let bytes = graph.encode().unwrap();
        let reopened = DialogueGraph::decode(&bytes).unwrap();
        assert_eq!(reopened.encode().unwrap(), bytes);
        let view = reopened.view();
        assert!(
            matches!(view.node(node).unwrap().body.as_ref(), NodeBody::Line { text: stored, .. } if stored == &text)
        );
        assert_eq!(reopened.history_status(), DocumentHistoryStatus::default());
        graph.undo().unwrap();
        graph.redo().unwrap();
        assert_eq!(graph.encode().unwrap(), bytes);
        refuses(
            &mut graph,
            DialogueCommand::SetLine {
                node,
                speaker: "Guide".to_owned(),
                text: format!("{text}!"),
            },
        );
    }
}

#[test]
fn source_size_refusal_does_not_poison_save_or_consume_choice_ids() {
    let mut graph = DialogueGraph::new();
    let node = add(&mut graph, NodeRecipe::Choice);
    refuses(
        &mut graph,
        DialogueCommand::SetChoices {
            node,
            prompt: "Too large".to_owned(),
            choices: (0..600)
                .map(|_| ChoiceDraft {
                    id: None,
                    label: "x".repeat(TEXT_BYTES),
                    target: None,
                })
                .collect(),
        },
    );
    graph
        .apply(&DialogueCommand::SetChoices {
            node,
            prompt: "Small".to_owned(),
            choices: vec![ChoiceDraft {
                id: None,
                label: "Valid".to_owned(),
                target: None,
            }],
        })
        .unwrap();
    assert!(graph.view().contains_output(OutputPort::Choice {
        node,
        choice: ChoiceId(2)
    }));
    assert!(DialogueGraph::decode(&graph.encode().unwrap()).is_ok());
}

#[test]
fn decode_refuses_future_schema_before_interpreting_its_payload() {
    let failure =
        DialogueGraph::decode(br#"{"schema_version":2,"future_payload":{"different":[]}}"#)
            .err()
            .unwrap();
    assert_eq!(
        failure.code(),
        DiagnosticCode::new("dialogue.schema-version").unwrap()
    );
    assert!(DialogueGraph::decode(br#"{"lines":["legacy dialogue"]}"#).is_err());
}

#[test]
fn decode_validates_unreachable_facts_unknown_fields_and_duplicate_links() {
    let graph = story();
    let source: serde_json::Value = serde_json::from_slice(&graph.encode().unwrap()).unwrap();
    let mut unknown = source.clone();
    unknown["extra"] = serde_json::json!(true);
    let mut duplicate_node = source.clone();
    let repeated = duplicate_node["nodes"][0].clone();
    duplicate_node["nodes"]
        .as_array_mut()
        .unwrap()
        .push(repeated);
    let mut duplicate_link = source.clone();
    let repeated = duplicate_link["links"][0].clone();
    duplicate_link["links"]
        .as_array_mut()
        .unwrap()
        .push(repeated);
    let mut foreign_port = source.clone();
    foreign_port["links"][0]["from"] = serde_json::json!({"kind":"next","node":3});
    let mut unreachable = source.clone();
    unreachable["nodes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": 99, "position":[0.,0.], "body":{"kind":"end","text":"x".repeat(TEXT_BYTES + 1)}
        }));
    let mut duplicate_choice = source.clone();
    duplicate_choice["nodes"][1]["body"]["choices"][1]["id"] = serde_json::json!(1);
    let mut missing_target = source.clone();
    missing_target["links"][0]["to"] = serde_json::json!(404);
    for invalid in [
        unknown,
        duplicate_node,
        duplicate_link,
        foreign_port,
        unreachable,
        duplicate_choice,
        missing_target,
    ] {
        assert!(DialogueGraph::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}

#[test]
fn node_limit_refusal_preserves_the_allocator_for_a_later_valid_add() {
    let source = serde_json::json!({
        "schema_version":1,
        "entry":1,
        "nodes":(1..=NODE_LIMIT).map(|id| serde_json::json!({
            "id":id, "position":[0.,0.], "body":{"kind":"end","text":""}
        })).collect::<Vec<_>>(),
        "links":[]
    });
    let mut graph = DialogueGraph::decode(&serde_json::to_vec(&source).unwrap()).unwrap();
    refuses(
        &mut graph,
        DialogueCommand::Add {
            recipe: NodeRecipe::Line,
            position: [0., 0.],
        },
    );
    graph
        .apply(&DialogueCommand::Remove {
            nodes: vec![NodeId(1)],
            links: Vec::new(),
        })
        .unwrap();
    assert_eq!(
        add(&mut graph, NodeRecipe::Line),
        NodeId((NODE_LIMIT + 1) as u64)
    );
}

#[test]
fn identifier_exhaustion_is_atomic_across_node_and_choice_allocation() {
    let source = serde_json::json!({
        "schema_version":1, "entry":u64::MAX-1,
        "nodes":[{"id":u64::MAX-1,"position":[0.,0.],"body":{"kind":"end","text":""}}],
        "links":[]
    });
    let mut graph = DialogueGraph::decode(&serde_json::to_vec(&source).unwrap()).unwrap();
    refuses(
        &mut graph,
        DialogueCommand::Add {
            recipe: NodeRecipe::Line,
            position: [0., 0.],
        },
    );
    let source = serde_json::json!({
        "schema_version":1, "entry":1,
        "nodes":[{"id":1,"position":[0.,0.],"body":{
            "kind":"choice","prompt":"","choices":[{"id":u64::MAX-1,"label":""}]
        }}],
        "links":[]
    });
    let mut graph = DialogueGraph::decode(&serde_json::to_vec(&source).unwrap()).unwrap();
    refuses(
        &mut graph,
        DialogueCommand::Add {
            recipe: NodeRecipe::Choice,
            position: [0., 0.],
        },
    );
    assert_eq!(add(&mut graph, NodeRecipe::Line), NodeId(2));
}

#[test]
fn conversation_follows_one_transition_at_a_time_and_supports_explicit_loops() {
    let graph = story();
    let before = state(&graph);
    let mut conversation = Conversation::new(graph.view());
    for _ in 0..512 {
        assert!(matches!(
            conversation.step().unwrap(),
            ConversationStep::Line {
                node: NodeId(1),
                ..
            }
        ));
        assert_eq!(conversation.advance().unwrap(), NodeId(2));
        assert_eq!(conversation.choose(ChoiceId(2)).unwrap(), NodeId(1));
    }
    conversation.advance().unwrap();
    assert_eq!(conversation.choose(ChoiceId(1)).unwrap(), NodeId(3));
    assert!(matches!(
        conversation.step().unwrap(),
        ConversationStep::End { .. }
    ));
    assert_eq!(
        conversation.advance(),
        Err(PreviewError::NotALine(NodeId(3)))
    );
    assert_eq!(conversation.current(), Some(NodeId(3)));
    assert_eq!(state(&graph), before);
}

#[test]
fn invalid_preview_operations_preserve_the_local_cursor_and_authored_owner() {
    let graph = story();
    let before = state(&graph);
    let mut conversation = Conversation::new(graph.view());
    assert_eq!(
        conversation.choose(ChoiceId(1)),
        Err(PreviewError::NotAChoice(NodeId(1)))
    );
    assert_eq!(conversation.current(), Some(NodeId(1)));
    conversation.advance().unwrap();
    assert_eq!(
        conversation.advance(),
        Err(PreviewError::NotALine(NodeId(2)))
    );
    assert_eq!(
        conversation.choose(ChoiceId(404)),
        Err(PreviewError::ChoiceMissing {
            node: NodeId(2),
            choice: ChoiceId(404)
        })
    );
    assert_eq!(conversation.current(), Some(NodeId(2)));
    assert_eq!(state(&graph), before);
}
