//! Immutable Dialogue Graph facts for Jellyflow's core graph model.
//!
//! This is a prerequisite for an external-document native interface, not a native Host adapter.

use std::{collections::BTreeMap, sync::Arc};

use jellyflow_core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node as CoreNode,
    NodeId as CoreNodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind,
};
use jellyflow_runtime::graph::{ExternalGraphError, ExternalGraphFacts};
use nara_core::DocumentRevision;
use nara_engine::tooling::{EditorDocumentId, EditorExtensionDocumentError};
use serde_json::Value;

use super::{DialogueView, NodeBody, NodeId, OutputPort, error, validation};

const GRAPH_NAMESPACE: u64 = 0x6e61_7261_2e64_6c67;
const INPUT_NAMESPACE: u64 = 0x6e61_7261_2e69_6e70;
const NEXT_NAMESPACE: u64 = 0x6e61_7261_2e6e_7874;
const CHOICE_NAMESPACE: u64 = 0x6e61_7261_2e63_6863;

/// Exact document slot and source incarnation/revision supplied by one document observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogueFactsBasis {
    pub document: EditorDocumentId,
    pub revision: DocumentRevision,
}

/// An immutable core graph and the checked identities belonging to its captured source.
///
/// The graph ID combines a fixed high-64-bit namespace with the host-local document ID in its
/// low 64 bits. It is not a persistent or cross-Host identity. Element IDs are graph-local:
/// equal domain IDs in different documents deliberately produce equal element UUIDs. Every
/// mapping therefore requires the captured graph ID, while the basis and live incarnations
/// distinguish revisions, Reopen, and removal followed by Undo.
#[derive(Debug)]
pub struct DialogueFacts {
    basis: DialogueFactsBasis,
    source: Arc<DialogueView>,
    graph: Arc<Graph>,
    nodes: BTreeMap<CoreNodeId, NodeId>,
    inputs: BTreeMap<PortId, NodeId>,
    outputs: BTreeMap<PortId, OutputPort>,
    edges: BTreeMap<EdgeId, OutputPort>,
}

impl DialogueFacts {
    /// Captures facts without retaining a document writer, runtime, history, or native View.
    ///
    /// The caller must obtain `document`, `revision`, and `source` from the same observation.
    /// Authored positions are preserved; intrinsic size remains local to each future View.
    pub fn capture(
        document: EditorDocumentId,
        revision: DocumentRevision,
        source: Arc<DialogueView>,
    ) -> Result<Self, EditorExtensionDocumentError> {
        validation::validate(&source)?;
        let graph_id = GraphId::from_u128(namespaced_id(GRAPH_NAMESPACE, document.raw()));
        let mut builder = GraphBuilder::new(graph_id);
        let mut nodes = BTreeMap::new();
        let mut inputs = BTreeMap::new();
        let mut outputs = BTreeMap::new();
        let mut edges = BTreeMap::new();

        for node in source.nodes() {
            source
                .node_incarnation(node.id)
                .ok_or_else(missing_incarnation)?;
            let core_node = core_node_id(node.id);
            let input = input_id(node.id);
            let mut ports = vec![input];
            inputs.insert(input, node.id);
            builder.insert_port(
                input,
                exec_port(
                    core_node,
                    PortKey::new("input"),
                    PortDirection::In,
                    PortCapacity::Multi,
                ),
            );
            for output in node.output_ports() {
                source
                    .output_incarnation(output)
                    .ok_or_else(missing_incarnation)?;
                let port = output_id(output);
                let key = match output {
                    OutputPort::Next { .. } => PortKey::new("next"),
                    OutputPort::Choice { choice, .. } => {
                        PortKey::new(format!("choice.{}", choice.0))
                    }
                };
                ports.push(port);
                outputs.insert(port, output);
                builder.insert_port(
                    port,
                    exec_port(core_node, key, PortDirection::Out, PortCapacity::Single),
                );
            }
            let kind = match node.body.as_ref() {
                NodeBody::Line { .. } => "nara.dialogue.line",
                NodeBody::Choice { .. } => "nara.dialogue.choice",
                NodeBody::End { .. } => "nara.dialogue.end",
            };
            let data = serde_json::to_value(node.body.as_ref()).map_err(|_| {
                error(
                    "dialogue.projection-data",
                    "The dialogue node body could not be projected",
                )
            })?;
            builder.insert_node(
                core_node,
                CoreNode {
                    kind: NodeKindKey::new(kind),
                    kind_version: 1,
                    pos: CanvasPoint {
                        x: node.position[0],
                        y: node.position[1],
                    },
                    origin: None,
                    selectable: None,
                    draggable: None,
                    connectable: None,
                    deletable: None,
                    parent: None,
                    extent: None,
                    expand_parent: None,
                    size: None,
                    hidden: false,
                    collapsed: false,
                    ports,
                    data,
                },
            );
            nodes.insert(core_node, node.id);
        }
        for link in source.links() {
            source
                .link_incarnation(link.from)
                .ok_or_else(missing_incarnation)?;
            let from = output_id(link.from);
            let edge = EdgeId(from.0);
            builder.insert_edge(edge, Edge::new(EdgeKind::Exec, from, input_id(link.to)));
            edges.insert(edge, link.from);
        }
        let graph = builder.build().map_err(|_| {
            error(
                "dialogue.projection-graph",
                "The dialogue projection does not satisfy core graph invariants",
            )
        })?;
        Ok(Self {
            basis: DialogueFactsBasis { document, revision },
            source,
            graph: Arc::new(graph),
            nodes,
            inputs,
            outputs,
            edges,
        })
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Retains this exact graph allocation and its original source-owned incarnation facts.
    ///
    /// Publishing these facts does not acknowledge an authored edit. The caller keeps the
    /// original Session basis and command result separately from the external View version.
    pub fn external_facts(&self) -> Result<ExternalGraphFacts, ExternalGraphError> {
        let graph = self.graph.graph_id();
        let nodes = self.nodes.keys().filter_map(|node| {
            self.node_incarnation(graph, *node)
                .map(|serial| (*node, serial))
        });
        let ports = self
            .inputs
            .keys()
            .filter_map(|port| {
                self.input_incarnation(graph, *port)
                    .map(|serial| (*port, serial))
            })
            .chain(self.outputs.keys().filter_map(|port| {
                self.output_incarnation(graph, *port)
                    .map(|serial| (*port, serial))
            }));
        let edges = self.edges.keys().filter_map(|edge| {
            self.link_incarnation(graph, *edge)
                .map(|serial| (*edge, serial))
        });
        ExternalGraphFacts::new(Arc::clone(&self.graph), nodes, ports, edges)
    }

    pub const fn source(&self) -> &Arc<DialogueView> {
        &self.source
    }

    pub const fn basis(&self) -> DialogueFactsBasis {
        self.basis
    }

    /// Resolves only nodes present in this exact capture and graph.
    pub fn domain_node(&self, graph: GraphId, node: CoreNodeId) -> Option<NodeId> {
        self.nodes
            .get(&node)
            .copied()
            .filter(|_| graph == self.graph.graph_id())
    }

    /// Resolves only actual input ports, including inputs with no incident links.
    pub fn input_node(&self, graph: GraphId, port: PortId) -> Option<NodeId> {
        self.inputs
            .get(&port)
            .copied()
            .filter(|_| graph == self.graph.graph_id())
    }

    /// Resolves only actual outputs, preserving their authored choice owner.
    pub fn output_port(&self, graph: GraphId, port: PortId) -> Option<OutputPort> {
        self.outputs
            .get(&port)
            .copied()
            .filter(|_| graph == self.graph.graph_id())
    }

    /// Resolves only links present in this capture, not merely a plausible output UUID.
    pub fn edge_output(&self, graph: GraphId, edge: EdgeId) -> Option<OutputPort> {
        self.edges
            .get(&edge)
            .copied()
            .filter(|_| graph == self.graph.graph_id())
    }

    pub fn node_incarnation(&self, graph: GraphId, node: CoreNodeId) -> Option<u64> {
        self.source.node_incarnation(self.domain_node(graph, node)?)
    }

    pub fn input_incarnation(&self, graph: GraphId, port: PortId) -> Option<u64> {
        self.source.node_incarnation(self.input_node(graph, port)?)
    }

    pub fn output_incarnation(&self, graph: GraphId, port: PortId) -> Option<u64> {
        self.source
            .output_incarnation(self.output_port(graph, port)?)
    }

    pub fn link_incarnation(&self, graph: GraphId, edge: EdgeId) -> Option<u64> {
        self.source.link_incarnation(self.edge_output(graph, edge)?)
    }
}

fn namespaced_id(namespace: u64, raw: u64) -> u128 {
    (u128::from(namespace) << 64) | u128::from(raw)
}

fn core_node_id(node: NodeId) -> CoreNodeId {
    CoreNodeId::from_u128(u128::from(node.0))
}

fn input_id(node: NodeId) -> PortId {
    PortId::from_u128(namespaced_id(INPUT_NAMESPACE, node.0))
}

fn output_id(port: OutputPort) -> PortId {
    let (namespace, raw) = match port {
        OutputPort::Next { node } => (NEXT_NAMESPACE, node.0),
        OutputPort::Choice { choice, .. } => (CHOICE_NAMESPACE, choice.0),
    };
    PortId::from_u128(namespaced_id(namespace, raw))
}

fn exec_port(node: CoreNodeId, key: PortKey, dir: PortDirection, capacity: PortCapacity) -> Port {
    Port {
        node,
        key,
        dir,
        kind: PortKind::Exec,
        capacity,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: Value::Null,
    }
}

fn missing_incarnation() -> EditorExtensionDocumentError {
    error(
        "dialogue.projection-lifetime",
        "The dialogue projection requires live source incarnations",
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nara_engine::tooling::EditorExtensionDocument;
    use serde_json::json;

    use super::*;
    use crate::dialogue::{ChoiceDraft, ChoiceId, DialogueCommand, DialogueGraph};

    fn story(node_ids: [u64; 3], choice_ids: [u64; 2]) -> DialogueGraph {
        let [line, choice, end] = node_ids;
        let [finish, again] = choice_ids;
        let source = json!({
            "schema_version": 1,
            "entry": line,
            "nodes": [
                {
                    "id": line,
                    "position": [10., 20.],
                    "body": { "kind": "line", "speaker": "Guide", "text": "Welcome." }
                },
                {
                    "id": choice,
                    "position": [210., 40.],
                    "body": {
                        "kind": "choice",
                        "prompt": "Where next?",
                        "choices": [
                            { "id": finish, "label": "Finish" },
                            { "id": again, "label": "Again" }
                        ]
                    }
                },
                {
                    "id": end,
                    "position": [410., 60.],
                    "body": { "kind": "end", "text": "Goodbye." }
                }
            ],
            "links": [
                { "from": { "kind": "next", "node": line }, "to": choice },
                {
                    "from": { "kind": "choice", "node": choice, "choice": finish },
                    "to": end
                },
                {
                    "from": { "kind": "choice", "node": choice, "choice": again },
                    "to": line
                }
            ]
        });
        DialogueGraph::decode(&serde_json::to_vec(&source).unwrap()).unwrap()
    }

    fn edge_from(facts: &DialogueFacts, port: PortId) -> EdgeId {
        facts
            .graph()
            .edges()
            .iter()
            .find_map(|(id, edge)| (edge.from == port).then_some(*id))
            .unwrap()
    }

    fn authored_graph_facts(graph: &Graph) -> Value {
        let nodes = graph
            .nodes()
            .iter()
            .map(|(id, node)| (*id, serde_json::to_value(node).unwrap()))
            .collect::<BTreeMap<_, _>>();
        let ports = graph
            .ports()
            .iter()
            .map(|(id, port)| (*id, serde_json::to_value(port).unwrap()))
            .collect::<BTreeMap<_, _>>();
        let edges = graph
            .edges()
            .iter()
            .map(|(id, edge)| (*id, serde_json::to_value(edge).unwrap()))
            .collect::<BTreeMap<_, _>>();
        serde_json::json!({ "nodes": nodes, "ports": ports, "edges": edges })
    }

    #[test]
    fn labels_reordering_and_retargeting_preserve_checked_port_and_edge_identity() {
        let mut owner = story([1, 2, 3], [1, 2]);
        let document = EditorDocumentId::from_raw(7);
        let revision = DocumentRevision::initial();
        let source = owner.view();
        let before = DialogueFacts::capture(document, revision, Arc::clone(&source)).unwrap();
        let graph = before.graph().graph_id();
        let choice_node = CoreNodeId::from_u128(2);
        let original_ports = before.graph().node(&choice_node).unwrap().ports.clone();
        let [input, finish, again] = original_ports.as_slice() else {
            panic!("one input and two authored choices");
        };
        let finish_edge = edge_from(&before, *finish);
        let again_edge = edge_from(&before, *again);
        owner
            .apply(&DialogueCommand::SetChoices {
                node: NodeId(2),
                prompt: "Choose again".to_owned(),
                choices: vec![
                    ChoiceDraft {
                        id: Some(ChoiceId(2)),
                        label: "Return to the guide".to_owned(),
                        target: Some(NodeId(1)),
                    },
                    ChoiceDraft {
                        id: Some(ChoiceId(1)),
                        label: "Stay here".to_owned(),
                        target: Some(NodeId(2)),
                    },
                ],
            })
            .unwrap();
        let after = DialogueFacts::capture(document, revision.next(), owner.view()).unwrap();

        assert!(Arc::ptr_eq(before.source(), &source));
        assert_eq!(before.basis(), DialogueFactsBasis { document, revision });
        assert_eq!(after.graph().graph_id(), graph);
        assert_eq!(
            after.graph().node(&choice_node).unwrap().ports,
            vec![*input, *again, *finish]
        );
        assert_eq!(
            before.graph().node(&choice_node).unwrap().ports,
            original_ports
        );
        assert_eq!(
            before.graph().node(&choice_node).unwrap().data["choices"][0]["label"],
            "Finish"
        );
        assert_eq!(
            after.graph().node(&choice_node).unwrap().data["choices"][1]["label"],
            "Stay here"
        );
        assert_eq!(after.domain_node(graph, choice_node), Some(NodeId(2)));
        assert_eq!(after.input_node(graph, *input), Some(NodeId(2)));
        for port in [*finish, *again] {
            assert_eq!(
                after.output_port(graph, port),
                before.output_port(graph, port)
            );
            assert_eq!(
                after.output_incarnation(graph, port),
                before.output_incarnation(graph, port)
            );
        }
        for edge in [finish_edge, again_edge] {
            assert_eq!(
                after.edge_output(graph, edge),
                before.edge_output(graph, edge)
            );
            assert_eq!(
                after.link_incarnation(graph, edge),
                before.link_incarnation(graph, edge)
            );
        }
        assert_eq!(
            before.input_node(graph, before.graph().edge(&finish_edge).unwrap().to),
            Some(NodeId(3))
        );
        assert_eq!(after.graph().edge(&finish_edge).unwrap().from, *finish);
        assert_eq!(after.graph().edge(&finish_edge).unwrap().to, *input);
        assert_eq!(
            after.graph().node(&choice_node).unwrap().data,
            serde_json::to_value(after.source().node(NodeId(2)).unwrap().body.as_ref()).unwrap(),
        );
    }

    #[test]
    fn coalesced_remove_and_undo_restore_source_ids_with_fresh_live_incarnations() {
        let mut owner = story([1, 2, 3], [1, 2]);
        let document = EditorDocumentId::from_raw(11);
        let revision = DocumentRevision::initial();
        let bytes = owner.encode().unwrap();
        let before = DialogueFacts::capture(document, revision, owner.view()).unwrap();
        let graph = before.graph().graph_id();
        let choice_node = CoreNodeId::from_u128(2);
        let ports = before.graph().node(&choice_node).unwrap().ports.clone();
        let edge = edge_from(&before, ports[1]);
        owner
            .apply(&DialogueCommand::Remove(vec![NodeId(2)]))
            .unwrap();
        owner.undo().unwrap();
        let after = DialogueFacts::capture(document, revision.next().next(), owner.view()).unwrap();

        assert_eq!(owner.encode().unwrap(), bytes);
        assert_eq!(before.graph().graph_id(), after.graph().graph_id());
        assert_eq!(
            authored_graph_facts(before.graph()),
            authored_graph_facts(after.graph()),
        );
        assert!(!Arc::ptr_eq(before.source(), after.source()));
        assert_ne!(before.basis(), after.basis());
        assert_eq!(
            before.basis().revision.source_id(),
            after.basis().revision.source_id()
        );
        assert_eq!(after.domain_node(graph, choice_node), Some(NodeId(2)));
        assert_ne!(
            before.node_incarnation(graph, choice_node),
            after.node_incarnation(graph, choice_node)
        );
        assert_ne!(
            before.input_incarnation(graph, ports[0]),
            after.input_incarnation(graph, ports[0])
        );
        assert_ne!(
            before.output_incarnation(graph, ports[1]),
            after.output_incarnation(graph, ports[1])
        );
        assert_ne!(
            before.link_incarnation(graph, edge),
            after.link_incarnation(graph, edge)
        );
        assert_eq!(
            after.node_incarnation(graph, CoreNodeId::from_u128(1)),
            before.node_incarnation(graph, CoreNodeId::from_u128(1)),
        );
        assert_eq!(
            before
                .source()
                .target(before.edge_output(graph, edge).unwrap()),
            Some(NodeId(3))
        );
        assert_eq!(
            after
                .source()
                .target(after.edge_output(graph, edge).unwrap()),
            Some(NodeId(3))
        );
    }

    #[test]
    fn high_u64_ids_have_disjoint_port_namespaces_and_refuse_foreign_or_absent_mappings() {
        let high = u64::MAX - 2;
        let mut owner = story([high, high + 1, high - 1], [high, high + 1]);
        let unlinked = OutputPort::Choice {
            node: NodeId(high + 1),
            choice: ChoiceId(high + 1),
        };
        owner
            .apply(&DialogueCommand::SetTarget {
                port: unlinked,
                target: None,
            })
            .unwrap();
        let document = EditorDocumentId::from_raw(u64::MAX);
        let revision = DocumentRevision::initial();
        let facts = DialogueFacts::capture(document, revision, owner.view()).unwrap();
        let other = DialogueFacts::capture(
            EditorDocumentId::from_raw(u64::MAX - 1),
            revision,
            owner.view(),
        )
        .unwrap();
        let graph = facts.graph().graph_id();
        let foreign = other.graph().graph_id();
        let line = CoreNodeId::from_u128(u128::from(high));
        let input = facts.graph().node(&line).unwrap().ports[0];
        let next = facts.graph().node(&line).unwrap().ports[1];
        let choice = facts
            .graph()
            .node(&CoreNodeId::from_u128(u128::from(high + 1)))
            .unwrap()
            .ports[1];
        let edge = edge_from(&facts, next);

        assert_ne!(graph, foreign);
        assert_eq!(graph.0.as_u128() >> 64, u128::from(GRAPH_NAMESPACE));
        assert_eq!(
            graph.0.as_u128() & u128::from(u64::MAX),
            u128::from(document.raw())
        );
        assert_eq!(line.0.as_u128(), u128::from(high));
        let uuids = [input.0.as_u128(), next.0.as_u128(), choice.0.as_u128()];
        assert_eq!(
            uuids
                .into_iter()
                .map(|id| id >> 64)
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
        for id in uuids {
            assert_eq!(id & u128::from(u64::MAX), u128::from(high));
        }
        assert_eq!(
            facts
                .graph()
                .ports()
                .keys()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            6
        );
        assert_eq!(facts.domain_node(graph, line), Some(NodeId(high)));
        assert_eq!(facts.input_node(graph, input), Some(NodeId(high)));
        assert_eq!(
            facts.output_port(graph, next),
            Some(OutputPort::Next { node: NodeId(high) })
        );
        assert_eq!(
            facts.output_port(graph, choice),
            Some(OutputPort::Choice {
                node: NodeId(high + 1),
                choice: ChoiceId(high),
            })
        );
        assert_eq!(
            facts.edge_output(graph, edge),
            Some(OutputPort::Next { node: NodeId(high) })
        );
        assert_eq!(facts.domain_node(foreign, line), None);
        assert_eq!(facts.input_node(foreign, input), None);
        assert_eq!(facts.output_port(foreign, next), None);
        assert_eq!(facts.edge_output(foreign, edge), None);
        assert_eq!(facts.node_incarnation(foreign, line), None);
        assert_eq!(facts.input_incarnation(foreign, input), None);
        assert_eq!(facts.output_incarnation(foreign, next), None);
        assert_eq!(facts.link_incarnation(foreign, edge), None);
        assert_eq!(facts.domain_node(graph, CoreNodeId::from_u128(1)), None);
        assert_eq!(facts.input_node(graph, PortId::from_u128(1)), None);
        assert_eq!(facts.output_port(graph, PortId::from_u128(1)), None);
        assert_eq!(facts.edge_output(graph, EdgeId::from_u128(1)), None);
        assert_eq!(facts.input_node(graph, next), None);
        assert_eq!(facts.output_port(graph, input), None);
        assert_eq!(
            facts.output_port(graph, output_id(unlinked)),
            Some(unlinked)
        );
        assert_eq!(
            facts.edge_output(graph, EdgeId(output_id(unlinked).0)),
            None
        );
        assert_eq!(other.domain_node(foreign, line), Some(NodeId(high)));
    }

    #[test]
    fn real_core_graph_accepts_cycles_real_port_ownership_and_immutable_intrinsic_geometry() {
        let mut owner = story([1, 2, 3], [1, 2]);
        owner
            .apply(&DialogueCommand::SetTarget {
                port: OutputPort::Choice {
                    node: NodeId(2),
                    choice: ChoiceId(1),
                },
                target: Some(NodeId(2)),
            })
            .unwrap();
        let source = owner.view();
        let bytes = owner.encode().unwrap();
        let history = owner.history_status();
        let facts = DialogueFacts::capture(
            EditorDocumentId::from_raw(21),
            DocumentRevision::initial(),
            Arc::clone(&source),
        )
        .unwrap();
        let graph = facts.graph().graph_id();
        assert_eq!(owner.encode().unwrap(), bytes);
        assert_eq!(owner.history_status(), history);
        assert!(Arc::ptr_eq(facts.source(), &source));
        assert_eq!(facts.graph().nodes().len(), 3);
        assert_eq!(facts.graph().ports().len(), 6);
        assert_eq!(facts.graph().edges().len(), 3);
        for (id, node) in facts.graph().nodes() {
            let domain = facts
                .source()
                .node(facts.domain_node(graph, *id).unwrap())
                .unwrap();
            assert_eq!(
                node.pos,
                CanvasPoint {
                    x: domain.position[0],
                    y: domain.position[1]
                }
            );
            assert_eq!(node.size, None);
            assert_eq!(
                node.data,
                serde_json::to_value(domain.body.as_ref()).unwrap()
            );
            assert_eq!(node.ports.len(), 1 + domain.output_ports().count());
            for (index, port_id) in node.ports.iter().enumerate() {
                let port = facts.graph().port(port_id).unwrap();
                assert_eq!(port.node, *id);
                assert_eq!(port.kind, PortKind::Exec);
                assert_eq!(
                    port.dir,
                    if index == 0 {
                        PortDirection::In
                    } else {
                        PortDirection::Out
                    }
                );
                assert_eq!(
                    port.capacity,
                    if index == 0 {
                        PortCapacity::Multi
                    } else {
                        PortCapacity::Single
                    }
                );
            }
        }
        for (id, edge) in facts.graph().edges() {
            assert_eq!(edge.kind, EdgeKind::Exec);
            let output = facts.output_port(graph, edge.from).unwrap();
            let target = facts.input_node(graph, edge.to).unwrap();
            assert_eq!(facts.edge_output(graph, *id), Some(output));
            assert_eq!(facts.source().target(output), Some(target));
        }
        let line = facts.graph().node(&CoreNodeId::from_u128(1)).unwrap();
        let choice = facts.graph().node(&CoreNodeId::from_u128(2)).unwrap();
        let end = facts.graph().node(&CoreNodeId::from_u128(3)).unwrap();
        assert_eq!(line.kind, NodeKindKey::new("nara.dialogue.line"));
        assert_eq!(choice.kind, NodeKindKey::new("nara.dialogue.choice"));
        assert_eq!(end.kind, NodeKindKey::new("nara.dialogue.end"));
        assert_eq!(end.ports.len(), 1);
        assert_eq!(
            facts
                .graph()
                .edge(&edge_from(&facts, line.ports[1]))
                .unwrap()
                .to,
            choice.ports[0]
        );
        assert_eq!(
            facts
                .graph()
                .edge(&edge_from(&facts, choice.ports[1]))
                .unwrap()
                .to,
            choice.ports[0]
        );
        assert_eq!(
            facts
                .graph()
                .edge(&edge_from(&facts, choice.ports[2]))
                .unwrap()
                .to,
            line.ports[0]
        );
        owner
            .apply(&DialogueCommand::SetLine {
                node: NodeId(1),
                speaker: "Other".to_owned(),
                text: "Later content".to_owned(),
            })
            .unwrap();
        drop(owner);
        assert_eq!(
            facts.graph().node(&CoreNodeId::from_u128(1)).unwrap().data["text"],
            "Welcome."
        );
        assert_eq!(
            facts.source().node(NodeId(1)).unwrap().body,
            source.node(NodeId(1)).unwrap().body
        );
    }
}
