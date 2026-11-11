use super::*;

fn add(graph: &mut ValueGraph, kind: NodeKind) -> NodeId {
    graph
        .apply(&GraphCommand::Add {
            kind,
            position: [0.0, 0.0],
        })
        .unwrap();
    graph.view().nodes.last().unwrap().id
}

fn connect(graph: &mut ValueGraph, from: NodeId, to: NodeId, input: u8) {
    graph
        .apply(&GraphCommand::Connect(Link { from, to, input }))
        .unwrap();
}

fn sum() -> ValueGraph {
    let mut graph = ValueGraph::new();
    let left = add(&mut graph, NodeKind::Scalar(3.0));
    let right = add(&mut graph, NodeKind::Scalar(4.0));
    let sum = add(&mut graph, NodeKind::Add);
    let output = add(&mut graph, NodeKind::Output);
    connect(&mut graph, left, sum, 0);
    connect(&mut graph, right, sum, 1);
    connect(&mut graph, sum, output, 0);
    graph
}

fn refused(graph: &mut ValueGraph, command: GraphCommand, code: &str) {
    let before = (
        graph.view(),
        graph.content_state(),
        graph.history_status(),
        graph.encode().unwrap(),
    );
    let allocator = graph.next_id;
    let clock = graph.clock;
    assert_eq!(graph.apply(&command).unwrap_err().code().as_str(), code);
    assert_eq!(
        before,
        (
            graph.view(),
            graph.content_state(),
            graph.history_status(),
            graph.encode().unwrap()
        )
    );
    assert_eq!((allocator, clock), (graph.next_id, graph.clock));
}

fn decode_error(view: &GraphView, code: &str) {
    let result = ValueGraph::decode(&serde_json::to_vec(view).unwrap());
    assert_eq!(result.err().unwrap().code().as_str(), code);
}

#[test]
fn typed_ports_and_complete_scalar_evaluation() {
    let graph = sum();
    assert_eq!(ValueGraph::TYPE_ID, "consumer.value-graph");
    assert_eq!(evaluate(&graph.view()).unwrap(), Some(7.0));
    assert_eq!(
        NodeKind::Vector([1.0, 2.0]).output_type(),
        Some(ValueType::Vector)
    );
    assert_eq!(NodeKind::Output.output_type(), None);
    assert_eq!(NodeKind::Add.input_type(1), Some(ValueType::Scalar));
    assert_eq!(NodeKind::Add.input_type(2), None);
    let mut reordered = graph.view();
    reordered.nodes.reverse();
    reordered.links.reverse();
    assert_eq!(evaluate(&reordered).unwrap(), Some(7.0));
    let mut decoded = ValueGraph::decode(&serde_json::to_vec(&reordered).unwrap()).unwrap();
    assert_eq!(decoded.view(), reordered);
    assert_eq!(add(&mut decoded, NodeKind::Add), NodeId(5));
    assert_eq!(evaluate(&ValueGraph::new().view()).unwrap(), None);
}

#[test]
fn link_refusals_preserve_content_history_and_allocators() {
    let mut graph = sum();
    let vector = add(&mut graph, NodeKind::Vector([1.0, 2.0]));
    let first = add(&mut graph, NodeKind::Add);
    let second = add(&mut graph, NodeKind::Add);
    connect(&mut graph, first, second, 0);
    for (from, to, input, code) in [
        (vector, first, 0, "graph.type-mismatch"),
        (second, first, 0, "graph.cycle"),
        (first, first, 0, "graph.self-link"),
        (first, second, 0, "graph.input-occupied"),
        (first, second, 2, "graph.input-invalid"),
        (NodeId(999), first, 0, "graph.node-missing"),
        (NodeId(4), first, 0, "graph.output-invalid"),
    ] {
        refused(
            &mut graph,
            GraphCommand::Connect(Link { from, to, input }),
            code,
        );
    }
}

#[test]
fn history_restores_exact_content_identity_and_source_bytes() {
    let mut graph = sum();
    let saved = (graph.content_state(), graph.view(), graph.encode().unwrap());
    graph
        .apply(&GraphCommand::SetScalar {
            node: NodeId(1),
            value: 13.0,
        })
        .unwrap();
    let edited = (graph.content_state(), graph.view(), graph.encode().unwrap());
    assert_eq!(evaluate(&graph.view()).unwrap(), Some(17.0));
    graph.undo().unwrap();
    assert_eq!(
        saved,
        (graph.content_state(), graph.view(), graph.encode().unwrap())
    );
    graph.redo().unwrap();
    assert_eq!(
        edited,
        (graph.content_state(), graph.view(), graph.encode().unwrap())
    );
    graph.undo().unwrap();
    graph
        .apply(&GraphCommand::Move {
            node: NodeId(1),
            position: [1.0, -2.0],
        })
        .unwrap();
    assert_ne!(graph.content_state(), edited.0);
    assert_eq!(graph.history_status().redo_depth, 0);
}

#[test]
fn branching_and_removal_never_reuse_live_owner_ids() {
    let mut graph = ValueGraph::default();
    let first = add(&mut graph, NodeKind::Add);
    let discarded = add(&mut graph, NodeKind::Add);
    graph.undo().unwrap();
    refused(
        &mut graph,
        GraphCommand::Add {
            kind: NodeKind::Scalar(f64::NAN),
            position: [0.0; 2],
        },
        "graph.value-invalid",
    );
    assert_eq!(graph.history_status().redo_depth, 1);
    let branch = add(&mut graph, NodeKind::Add);
    assert_eq!(branch.0, discarded.0 + 1);
    graph.apply(&GraphCommand::Remove(branch)).unwrap();
    let successor = add(&mut graph, NodeKind::Add);
    assert!(successor > branch && branch > first);
    refused(
        &mut graph,
        GraphCommand::Remove(discarded),
        "graph.node-missing",
    );
}

#[test]
fn remove_and_disconnect_are_atomic_history_entries() {
    let mut graph = sum();
    let saved = graph.encode().unwrap();
    let depth = graph.history_status().undo_depth;
    graph.apply(&GraphCommand::Remove(NodeId(3))).unwrap();
    assert!(graph.view().links.is_empty());
    assert_eq!(evaluate(&graph.view()).unwrap(), None);
    assert_eq!(graph.history_status().undo_depth, depth + 1);
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), saved);
    graph
        .apply(&GraphCommand::Disconnect {
            to: NodeId(3),
            input: 0,
        })
        .unwrap();
    assert_eq!(evaluate(&graph.view()).unwrap(), None);
    refused(
        &mut graph,
        GraphCommand::Disconnect {
            to: NodeId(3),
            input: 0,
        },
        "graph.link-missing",
    );
    graph.undo().unwrap();
    assert_eq!(graph.encode().unwrap(), saved);
}

#[test]
fn finite_values_positions_and_single_output_are_enforced() {
    let mut graph = sum();
    for value in [f64::NAN, f64::INFINITY, -f64::INFINITY, 10_001.0, -10_001.0] {
        refused(
            &mut graph,
            GraphCommand::SetScalar {
                node: NodeId(1),
                value,
            },
            "graph.value-invalid",
        );
        refused(
            &mut graph,
            GraphCommand::Add {
                kind: NodeKind::Vector([0.0, value]),
                position: [0.0; 2],
            },
            "graph.value-invalid",
        );
    }
    for position in [[f32::NAN, 0.0], [0.0, f32::INFINITY], [-10_001.0, 0.0]] {
        refused(
            &mut graph,
            GraphCommand::Move {
                node: NodeId(1),
                position,
            },
            "graph.value-invalid",
        );
    }
    refused(
        &mut graph,
        GraphCommand::Add {
            kind: NodeKind::Output,
            position: [0.0; 2],
        },
        "graph.output-limit",
    );
    refused(
        &mut graph,
        GraphCommand::SetScalar {
            node: NodeId(3),
            value: 0.0,
        },
        "graph.scalar-required",
    );
    graph
        .apply(&GraphCommand::SetScalar {
            node: NodeId(1),
            value: -10_000.0,
        })
        .unwrap();
    graph
        .apply(&GraphCommand::Move {
            node: NodeId(1),
            position: [-10_000.0, 10_000.0],
        })
        .unwrap();
    graph
        .apply(&GraphCommand::SetScalar {
            node: NodeId(2),
            value: -10_000.0,
        })
        .unwrap();
    assert_eq!(evaluate(&graph.view()).unwrap(), Some(-20_000.0));
}

#[test]
fn codec_contains_only_view_and_decode_starts_a_fresh_owner() {
    let mut graph = sum();
    graph.undo().unwrap();
    let bytes = graph.encode().unwrap();
    assert_eq!(bytes, serde_json::to_vec(&graph.view()).unwrap());
    let mut decoded = ValueGraph::decode(&bytes).unwrap();
    assert_eq!(decoded.view(), graph.view());
    assert_eq!(decoded.encode().unwrap(), bytes);
    assert_ne!(
        decoded.content_state().source_id(),
        graph.content_state().source_id()
    );
    assert_eq!(decoded.history_status(), DocumentHistoryStatus::default());
    assert_eq!(
        decoded.undo().unwrap_err().code().as_str(),
        "graph.undo-empty"
    );
    assert_eq!(
        decoded.redo().unwrap_err().code().as_str(),
        "graph.redo-empty"
    );
    assert_eq!(add(&mut decoded, NodeKind::Add), NodeId(5));
}

#[test]
fn decode_rejects_malformed_unknown_and_oversized_source() {
    for bytes in [
        b"{".as_slice(),
        b"{}",
        b"{\"nodes\":[],\"links\":[],\"clock\":1}",
        b"{\"nodes\":[],\"links\":[],\"links\":[]}",
    ] {
        assert_eq!(
            ValueGraph::decode(bytes).err().unwrap().code().as_str(),
            "graph.decode"
        );
    }
    let mut exact = ValueGraph::new().encode().unwrap();
    exact.resize(SOURCE_LIMIT, b' ');
    assert!(ValueGraph::decode(&exact).is_ok());
    exact.push(b' ');
    assert_eq!(
        ValueGraph::decode(&exact).err().unwrap().code().as_str(),
        "graph.source-limit"
    );
}

#[test]
fn codec_validates_complete_topology_including_unused_cycles() {
    let mut view = sum().view();
    view.nodes.push(view.nodes[0].clone());
    decode_error(&view, "graph.node-duplicate");
    view.nodes.pop();
    view.links[0].from = NodeId(999);
    decode_error(&view, "graph.node-missing");
    let mut cyclic = ValueGraph::new();
    let a = add(&mut cyclic, NodeKind::Add);
    let b = add(&mut cyclic, NodeKind::Add);
    let mut view = cyclic.view();
    view.links = vec![
        Link {
            from: a,
            to: b,
            input: 0,
        },
        Link {
            from: b,
            to: a,
            input: 0,
        },
    ];
    view.nodes.reverse();
    decode_error(&view, "graph.cycle");
    assert_eq!(evaluate(&view).unwrap_err().code().as_str(), "graph.cycle");
    let mut exhausted = ValueGraph::new().view();
    exhausted.nodes.push(Node {
        id: NodeId(u32::MAX),
        kind: NodeKind::Add,
        position: [0.0; 2],
    });
    decode_error(&exhausted, "graph.id-exhausted");
    exhausted.nodes[0].id = NodeId(u32::MAX - 1);
    let mut graph = ValueGraph::decode(&serde_json::to_vec(&exhausted).unwrap()).unwrap();
    refused(
        &mut graph,
        GraphCommand::Add {
            kind: NodeKind::Add,
            position: [0.0; 2],
        },
        "graph.id-exhausted",
    );
}

#[test]
fn node_link_and_combined_history_limits_are_bounded() {
    let mut graph = ValueGraph::new();
    for _ in 0..NODE_LIMIT {
        add(&mut graph, NodeKind::Add);
    }
    refused(
        &mut graph,
        GraphCommand::Add {
            kind: NodeKind::Add,
            position: [0.0; 2],
        },
        "graph.topology-limit",
    );
    let mut too_many = graph.view();
    too_many.nodes.push(Node {
        id: NodeId(999),
        kind: NodeKind::Add,
        position: [0.0; 2],
    });
    decode_error(&too_many, "graph.topology-limit");
    too_many = graph.view();
    too_many.links = vec![
        Link {
            from: NodeId(1),
            to: NodeId(2),
            input: 0
        };
        LINK_LIMIT + 1
    ];
    decode_error(&too_many, "graph.topology-limit");
    for value in 0..70 {
        graph
            .apply(&GraphCommand::Move {
                node: NodeId(1),
                position: [value as f32, 0.0],
            })
            .unwrap();
    }
    assert_eq!(graph.history_status().undo_depth, HISTORY_LIMIT);
    for _ in 0..HISTORY_LIMIT {
        graph.undo().unwrap();
    }
    assert!(graph.undo().is_err());
    assert_eq!(graph.history_status().redo_depth, HISTORY_LIMIT);
    for _ in 0..HISTORY_LIMIT {
        graph.redo().unwrap();
    }
    assert!(graph.redo().is_err());
    assert_eq!(graph.view().node(NodeId(1)).unwrap().position, [69.0, 0.0]);
}
