use super::{
    tests::{add, choice_port, refuses, state, story},
    *,
};

#[test]
fn add_outcomes_report_allocated_outputs_without_reusing_retired_identities() {
    let mut graph = DialogueGraph::new();
    for (recipe, node, outputs) in [
        (
            NodeRecipe::Line,
            NodeId(1),
            vec![OutputPort::Next { node: NodeId(1) }],
        ),
        (
            NodeRecipe::Choice,
            NodeId(2),
            vec![OutputPort::Choice {
                node: NodeId(2),
                choice: ChoiceId(1),
            }],
        ),
        (NodeRecipe::End, NodeId(3), Vec::new()),
    ] {
        assert_eq!(
            graph
                .apply(&DialogueCommand::Add {
                    recipe,
                    position: [12., 24.],
                })
                .unwrap(),
            DialogueCommandOutcome::Created {
                node,
                outputs: outputs.clone(),
                link: None,
            }
        );
        let view = graph.view();
        assert_eq!(
            view.node(node).unwrap().output_ports().collect::<Vec<_>>(),
            outputs
        );
    }
    graph.undo().unwrap();
    graph.undo().unwrap();
    assert_eq!(
        graph
            .apply(&DialogueCommand::Add {
                recipe: NodeRecipe::Choice,
                position: [0., 0.],
            })
            .unwrap(),
        DialogueCommandOutcome::Created {
            node: NodeId(4),
            outputs: vec![OutputPort::Choice {
                node: NodeId(4),
                choice: ChoiceId(2),
            }],
            link: None,
        }
    );
    assert_eq!(graph.history_status().redo_depth, 0);
}

#[test]
fn choice_outcomes_report_existing_and_allocated_ids_in_authored_order() {
    let mut graph = story();
    let before = state(&graph);
    assert_eq!(
        graph
            .apply(&DialogueCommand::SetChoices {
                node: NodeId(2),
                prompt: "Choose again".to_owned(),
                choices: vec![
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
                    ChoiceDraft {
                        id: Some(ChoiceId(1)),
                        label: "Finish".to_owned(),
                        target: Some(NodeId(3)),
                    },
                ],
            })
            .unwrap(),
        DialogueCommandOutcome::ChoicesChanged {
            node: NodeId(2),
            choices: vec![ChoiceId(2), ChoiceId(3), ChoiceId(1)],
        }
    );
    assert_eq!(graph.history_status().undo_depth, before.2.undo_depth + 1);
    let view = graph.view();
    assert_eq!(
        view.node(NodeId(2))
            .unwrap()
            .output_ports()
            .collect::<Vec<_>>(),
        vec![choice_port(2), choice_port(3), choice_port(1)]
    );
    assert_eq!(view.target(choice_port(3)), Some(NodeId(2)));
    assert_eq!(
        graph
            .apply(&DialogueCommand::SetChoices {
                node: NodeId(2),
                prompt: String::new(),
                choices: Vec::new(),
            })
            .unwrap(),
        DialogueCommandOutcome::ChoicesChanged {
            node: NodeId(2),
            choices: Vec::new(),
        }
    );
    assert!(
        graph
            .view()
            .links()
            .all(|link| link.from.node() != NodeId(2))
    );
}

#[test]
fn creating_from_an_output_replaces_its_target_in_one_edit_for_every_recipe() {
    for (recipe, outputs) in [
        (NodeRecipe::Line, vec![OutputPort::Next { node: NodeId(4) }]),
        (
            NodeRecipe::Choice,
            vec![OutputPort::Choice {
                node: NodeId(4),
                choice: ChoiceId(3),
            }],
        ),
        (NodeRecipe::End, Vec::new()),
    ] {
        let mut graph = story();
        let before = state(&graph);
        let previous = graph.view();
        let port = choice_port(1);
        let node = NodeId(4);
        assert_eq!(
            graph
                .apply(&DialogueCommand::CreateFromWire {
                    recipe,
                    position: [48., -24.],
                    endpoint: WireEndpoint::Output(port),
                })
                .unwrap(),
            DialogueCommandOutcome::Created {
                node,
                outputs: outputs.clone(),
                link: Some(Link {
                    from: port,
                    to: node
                }),
            }
        );
        let after = state(&graph);
        let created = graph.view();
        assert_eq!(after.2.undo_depth, before.2.undo_depth + 1);
        assert_eq!(created.target(port), Some(node));
        assert_eq!(previous.target(port), Some(NodeId(3)));
        assert_eq!(created.node(node).unwrap().position, [48., -24.]);
        assert_eq!(
            created
                .node(node)
                .unwrap()
                .output_ports()
                .collect::<Vec<_>>(),
            outputs
        );
        assert_eq!(
            created.link_incarnation(port),
            previous.link_incarnation(port)
        );
        assert_eq!(created.target(choice_port(2)), Some(NodeId(1)));
        graph.undo().unwrap();
        assert_eq!(graph.encode().unwrap(), before.0);
        assert_eq!(graph.content_state(), before.1);
        assert!(graph.view().node(node).is_none());
        graph.redo().unwrap();
        assert_eq!(graph.encode().unwrap(), after.0);
        assert_eq!(graph.content_state(), after.1);
        let restored = graph.view();
        assert_ne!(
            restored.node_incarnation(node),
            created.node_incarnation(node)
        );
        for output in outputs {
            assert_ne!(
                restored.output_incarnation(output),
                created.output_incarnation(output)
            );
        }
        assert_eq!(
            restored.link_incarnation(port),
            previous.link_incarnation(port)
        );
    }
}

#[test]
fn creating_into_an_input_uses_the_new_nodes_allocated_primary_output_atomically() {
    for (recipe, output) in [
        (NodeRecipe::Line, OutputPort::Next { node: NodeId(4) }),
        (
            NodeRecipe::Choice,
            OutputPort::Choice {
                node: NodeId(4),
                choice: ChoiceId(3),
            },
        ),
    ] {
        let mut graph = story();
        let before = state(&graph);
        let previous = graph.view();
        let node = NodeId(4);
        assert_eq!(
            graph
                .apply(&DialogueCommand::CreateFromWire {
                    recipe,
                    position: [-64., 128.],
                    endpoint: WireEndpoint::Input(NodeId(3)),
                })
                .unwrap(),
            DialogueCommandOutcome::Created {
                node,
                outputs: vec![output],
                link: Some(Link {
                    from: output,
                    to: NodeId(3),
                }),
            }
        );
        let after = state(&graph);
        let created = graph.view();
        assert_eq!(after.2.undo_depth, before.2.undo_depth + 1);
        assert_eq!(created.target(output), Some(NodeId(3)));
        assert!(created.contains_output(output));
        for link in previous.links() {
            assert_eq!(created.target(link.from), Some(link.to));
            assert_eq!(
                created.link_incarnation(link.from),
                previous.link_incarnation(link.from)
            );
        }
        graph.undo().unwrap();
        assert_eq!(graph.encode().unwrap(), before.0);
        assert_eq!(graph.content_state(), before.1);
        assert!(graph.view().node(node).is_none());
        assert_eq!(graph.view().link_incarnation(output), None);
        graph.redo().unwrap();
        assert_eq!(graph.encode().unwrap(), after.0);
        assert_eq!(graph.content_state(), after.1);
        let restored = graph.view();
        assert_ne!(
            restored.node_incarnation(node),
            created.node_incarnation(node)
        );
        assert_ne!(
            restored.output_incarnation(output),
            created.output_incarnation(output)
        );
        assert_ne!(
            restored.link_incarnation(output),
            created.link_incarnation(output)
        );
    }
}

#[test]
fn invalid_wire_endpoints_recipes_and_positions_preserve_allocators_and_redo() {
    let mut graph = story();
    add(&mut graph, NodeRecipe::Choice);
    let redo = state(&graph);
    graph.undo().unwrap();
    for (recipe, position, endpoint) in [
        (
            NodeRecipe::Choice,
            [0., 0.],
            WireEndpoint::Output(OutputPort::Next { node: NodeId(5) }),
        ),
        (
            NodeRecipe::Choice,
            [0., 0.],
            WireEndpoint::Output(OutputPort::Choice {
                node: NodeId(5),
                choice: ChoiceId(4),
            }),
        ),
        (
            NodeRecipe::Choice,
            [0., 0.],
            WireEndpoint::Output(OutputPort::Next { node: NodeId(3) }),
        ),
        (
            NodeRecipe::Choice,
            [0., 0.],
            WireEndpoint::Output(choice_port(404)),
        ),
        (NodeRecipe::Choice, [0., 0.], WireEndpoint::Input(NodeId(5))),
        (NodeRecipe::Choice, [0., 0.], WireEndpoint::Input(NodeId(4))),
        (NodeRecipe::End, [0., 0.], WireEndpoint::Input(NodeId(3))),
        (
            NodeRecipe::Choice,
            [f32::NAN, 0.],
            WireEndpoint::Output(choice_port(1)),
        ),
        (
            NodeRecipe::Choice,
            [0., f32::INFINITY],
            WireEndpoint::Input(NodeId(3)),
        ),
        (
            NodeRecipe::Choice,
            [POSITION_LIMIT + 1., 0.],
            WireEndpoint::Output(choice_port(1)),
        ),
        (
            NodeRecipe::Choice,
            [0., -POSITION_LIMIT - 1.],
            WireEndpoint::Input(NodeId(3)),
        ),
    ] {
        refuses(
            &mut graph,
            DialogueCommand::CreateFromWire {
                recipe,
                position,
                endpoint,
            },
        );
    }
    graph.redo().unwrap();
    assert_eq!(graph.encode().unwrap(), redo.0);
    assert_eq!(graph.content_state(), redo.1);
    graph.undo().unwrap();
    assert_eq!(
        graph
            .apply(&DialogueCommand::CreateFromWire {
                recipe: NodeRecipe::Choice,
                position: [0., 0.],
                endpoint: WireEndpoint::Input(NodeId(3)),
            })
            .unwrap(),
        DialogueCommandOutcome::Created {
            node: NodeId(5),
            outputs: vec![OutputPort::Choice {
                node: NodeId(5),
                choice: ChoiceId(4),
            }],
            link: Some(Link {
                from: OutputPort::Choice {
                    node: NodeId(5),
                    choice: ChoiceId(4),
                },
                to: NodeId(3),
            }),
        }
    );
}

#[test]
fn incarnation_exhaustion_refuses_allocated_results_before_commit() {
    let mut graph = story();
    add(&mut graph, NodeRecipe::Choice);
    graph.undo().unwrap();
    graph.next_incarnation = u64::MAX;
    for command in [
        DialogueCommand::Add {
            recipe: NodeRecipe::Choice,
            position: [0., 0.],
        },
        DialogueCommand::CreateFromWire {
            recipe: NodeRecipe::Choice,
            position: [0., 0.],
            endpoint: WireEndpoint::Output(choice_port(1)),
        },
        DialogueCommand::CreateFromWire {
            recipe: NodeRecipe::Choice,
            position: [0., 0.],
            endpoint: WireEndpoint::Input(NodeId(3)),
        },
        DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Rejected".to_owned(),
            choices: vec![ChoiceDraft {
                id: None,
                label: "New".to_owned(),
                target: Some(NodeId(3)),
            }],
        },
    ] {
        refuses(&mut graph, command);
    }
}

#[test]
fn reconnect_changes_both_endpoints_with_one_undo_and_redo_and_fresh_link_lifetimes() {
    let mut graph = story();
    let destination = add(&mut graph, NodeRecipe::Line);
    let from = choice_port(1);
    let to = OutputPort::Next { node: destination };
    let before = state(&graph);
    let previous = graph.view();
    assert_eq!(
        graph
            .apply(&DialogueCommand::Reconnect {
                from,
                to,
                target: NodeId(1),
            })
            .unwrap(),
        DialogueCommandOutcome::Changed
    );
    let after = state(&graph);
    let moved = graph.view();
    assert_eq!(after.2.undo_depth, before.2.undo_depth + 1);
    assert_eq!(moved.target(from), None);
    assert_eq!(moved.link_incarnation(from), None);
    assert_eq!(moved.target(to), Some(NodeId(1)));
    assert!(moved.link_incarnation(to).is_some());
    assert_eq!(
        moved.link_incarnation(choice_port(2)),
        previous.link_incarnation(choice_port(2))
    );
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), before.0);
    assert_eq!(graph.content_state(), before.1);
    let restored = graph.view();
    assert_eq!(restored.target(from), Some(NodeId(3)));
    assert_eq!(restored.link_incarnation(to), None);
    assert_ne!(
        restored.link_incarnation(from),
        previous.link_incarnation(from)
    );
    graph.redo().unwrap();
    assert_eq!(graph.encode().unwrap(), after.0);
    assert_eq!(graph.content_state(), after.1);
    let replayed = graph.view();
    assert_eq!(replayed.link_incarnation(from), None);
    assert_ne!(replayed.link_incarnation(to), moved.link_incarnation(to));
    for port in [from, to, choice_port(2)] {
        assert_eq!(
            replayed.output_incarnation(port),
            previous.output_incarnation(port)
        );
    }
    for node in previous.nodes() {
        assert_eq!(
            replayed.node_incarnation(node.id),
            previous.node_incarnation(node.id)
        );
        assert!(Arc::ptr_eq(
            &replayed.node(node.id).unwrap().body,
            &node.body
        ));
    }
}

#[test]
fn reconnect_on_the_same_output_retargets_without_retiring_the_link() {
    let mut graph = story();
    let port = choice_port(1);
    let before = state(&graph);
    let previous = graph.view();
    assert_eq!(
        graph
            .apply(&DialogueCommand::Reconnect {
                from: port,
                to: port,
                target: NodeId(1),
            })
            .unwrap(),
        DialogueCommandOutcome::Changed
    );
    let after = state(&graph);
    assert_eq!(after.2.undo_depth, before.2.undo_depth + 1);
    assert_eq!(graph.view().target(port), Some(NodeId(1)));
    assert_eq!(
        graph.view().link_incarnation(port),
        previous.link_incarnation(port)
    );
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), before.0);
    assert_eq!(graph.content_state(), before.1);
    graph.redo().unwrap();
    assert_eq!(graph.encode().unwrap(), after.0);
    assert_eq!(graph.content_state(), after.1);
    assert_eq!(
        graph.view().link_incarnation(port),
        previous.link_incarnation(port)
    );
}

#[test]
fn reconnect_refuses_missing_endpoints_and_occupied_outputs_without_losing_redo() {
    let mut graph = story();
    let unlinked = OutputPort::Next {
        node: add(&mut graph, NodeRecipe::Line),
    };
    graph
        .apply(&DialogueCommand::SetEnd {
            node: NodeId(3),
            text: "Pending redo".to_owned(),
        })
        .unwrap();
    let redo = state(&graph);
    graph.undo().unwrap();
    for (from, to, target) in [
        (unlinked, choice_port(1), NodeId(3)),
        (OutputPort::Next { node: NodeId(404) }, unlinked, NodeId(3)),
        (choice_port(1), choice_port(2), NodeId(1)),
        (
            choice_port(1),
            OutputPort::Next { node: NodeId(1) },
            NodeId(2),
        ),
        (
            choice_port(1),
            OutputPort::Next { node: NodeId(3) },
            NodeId(3),
        ),
        (choice_port(1), choice_port(404), NodeId(3)),
        (
            choice_port(1),
            OutputPort::Next { node: NodeId(404) },
            NodeId(3),
        ),
        (choice_port(1), unlinked, NodeId(404)),
        (choice_port(1), choice_port(1), NodeId(404)),
    ] {
        refuses(&mut graph, DialogueCommand::Reconnect { from, to, target });
    }
    graph.redo().unwrap();
    assert_eq!(graph.encode().unwrap(), redo.0);
    assert_eq!(graph.content_state(), redo.1);
}

#[test]
fn mixed_removal_validates_incident_selections_before_one_atomic_deletion() {
    let mut graph = story();
    let survivor = add(&mut graph, NodeRecipe::Line);
    let retained = OutputPort::Next { node: survivor };
    graph
        .apply(&DialogueCommand::SetTarget {
            port: retained,
            target: Some(NodeId(3)),
        })
        .unwrap();
    let incident = OutputPort::Next { node: NodeId(1) };
    let before = state(&graph);
    let previous = graph.view();
    assert_eq!(
        graph
            .apply(&DialogueCommand::Remove {
                nodes: vec![NodeId(1)],
                links: vec![incident, choice_port(1), choice_port(2)],
            })
            .unwrap(),
        DialogueCommandOutcome::Changed
    );
    let after = state(&graph);
    let removed = graph.view();
    assert_eq!(after.2.undo_depth, before.2.undo_depth + 1);
    assert_eq!(removed.entry(), None);
    assert!(removed.node(NodeId(1)).is_none());
    assert_eq!(removed.nodes().len(), previous.nodes().len() - 1);
    assert_eq!(
        removed.links().collect::<Vec<_>>(),
        vec![Link {
            from: retained,
            to: NodeId(3)
        }]
    );
    for port in [incident, choice_port(1), choice_port(2)] {
        assert_eq!(removed.link_incarnation(port), None);
    }
    for port in [choice_port(1), choice_port(2), retained] {
        assert_eq!(
            removed.output_incarnation(port),
            previous.output_incarnation(port)
        );
    }
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), before.0);
    assert_eq!(graph.content_state(), before.1);
    let restored = graph.view();
    assert_ne!(
        restored.node_incarnation(NodeId(1)),
        previous.node_incarnation(NodeId(1))
    );
    for port in [incident, choice_port(1), choice_port(2)] {
        assert_ne!(
            restored.link_incarnation(port),
            previous.link_incarnation(port)
        );
    }
    assert_eq!(
        restored.link_incarnation(retained),
        previous.link_incarnation(retained)
    );
    graph.redo().unwrap();
    assert_eq!(graph.encode().unwrap(), after.0);
    assert_eq!(graph.content_state(), after.1);
}

#[test]
fn links_only_removal_keeps_nodes_outputs_and_unselected_links_alive() {
    let mut graph = story();
    let previous = graph.view();
    let before = state(&graph);
    let port = choice_port(1);
    assert_eq!(
        graph
            .apply(&DialogueCommand::Remove {
                nodes: Vec::new(),
                links: vec![port],
            })
            .unwrap(),
        DialogueCommandOutcome::Changed
    );
    let after = graph.view();
    assert_eq!(graph.history_status().undo_depth, before.2.undo_depth + 1);
    assert_eq!(after.target(port), None);
    assert_eq!(after.link_incarnation(port), None);
    assert_eq!(after.nodes().len(), previous.nodes().len());
    assert_eq!(after.entry(), previous.entry());
    assert_eq!(after.target(choice_port(2)), Some(NodeId(1)));
    assert_eq!(
        after.output_incarnation(port),
        previous.output_incarnation(port)
    );
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), before.0);
    assert_eq!(graph.content_state(), before.1);
    assert_ne!(
        graph.view().link_incarnation(port),
        previous.link_incarnation(port)
    );
}

#[test]
fn an_invalid_last_removal_item_preserves_original_facts_allocators_and_redo() {
    let mut graph = story();
    let unlinked = OutputPort::Next {
        node: add(&mut graph, NodeRecipe::Line),
    };
    let incident = OutputPort::Next { node: NodeId(1) };
    graph
        .apply(&DialogueCommand::SetEnd {
            node: NodeId(3),
            text: "Pending redo".to_owned(),
        })
        .unwrap();
    let redo = state(&graph);
    graph.undo().unwrap();
    for command in [
        DialogueCommand::Remove {
            nodes: vec![NodeId(1), NodeId(404)],
            links: vec![choice_port(1)],
        },
        DialogueCommand::Remove {
            nodes: vec![NodeId(1), NodeId(1)],
            links: vec![choice_port(1)],
        },
        DialogueCommand::Remove {
            nodes: vec![NodeId(1)],
            links: vec![incident, choice_port(1), unlinked],
        },
        DialogueCommand::Remove {
            nodes: vec![NodeId(1)],
            links: vec![incident, choice_port(1), choice_port(404)],
        },
        DialogueCommand::Remove {
            nodes: vec![NodeId(1)],
            links: vec![incident, choice_port(1), incident],
        },
    ] {
        refuses(&mut graph, command);
    }
    graph.redo().unwrap();
    assert_eq!(graph.encode().unwrap(), redo.0);
    assert_eq!(graph.content_state(), redo.1);
}

#[test]
fn semantic_noops_preserve_source_arc_both_allocators_clock_and_pending_redo() {
    let mut graph = story();
    let unlinked = OutputPort::Next {
        node: add(&mut graph, NodeRecipe::Line),
    };
    graph
        .apply(&DialogueCommand::SetLine {
            node: NodeId(1),
            speaker: "Guide".to_owned(),
            text: "Pending redo".to_owned(),
        })
        .unwrap();
    let pending = state(&graph);
    graph.undo().unwrap();
    let before = state(&graph);
    let view = graph.view();
    let undo = graph.undo.clone();
    let redo = graph.redo.clone();
    let allocation = (
        graph.next_node,
        graph.next_choice,
        graph.next_incarnation,
        graph.clock,
    );
    for command in [
        DialogueCommand::SetEntry(Some(NodeId(1))),
        DialogueCommand::SetLine {
            node: NodeId(1),
            speaker: "Guide".to_owned(),
            text: "Welcome, traveller.".to_owned(),
        },
        DialogueCommand::SetChoices {
            node: NodeId(2),
            prompt: "Where next?".to_owned(),
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
            ],
        },
        DialogueCommand::SetEnd {
            node: NodeId(3),
            text: "End".to_owned(),
        },
        DialogueCommand::SetTarget {
            port: choice_port(1),
            target: Some(NodeId(3)),
        },
        DialogueCommand::SetTarget {
            port: unlinked,
            target: None,
        },
        DialogueCommand::Reconnect {
            from: choice_port(1),
            to: choice_port(1),
            target: NodeId(3),
        },
        DialogueCommand::Move(Vec::new()),
        DialogueCommand::Move(vec![
            NodePosition {
                node: NodeId(1),
                position: [0., 0.],
            },
            NodePosition {
                node: NodeId(2),
                position: [0., 0.],
            },
        ]),
        DialogueCommand::Remove {
            nodes: Vec::new(),
            links: Vec::new(),
        },
    ] {
        assert_eq!(graph.apply(&command).unwrap(), DialogueCommandOutcome::Noop);
        assert_eq!(state(&graph), before);
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
    graph.redo().unwrap();
    assert_eq!(graph.encode().unwrap(), pending.0);
    assert_eq!(graph.content_state(), pending.1);
}
