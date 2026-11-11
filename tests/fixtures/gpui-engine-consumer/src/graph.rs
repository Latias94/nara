//! A finite scalar/vector graph owned by the ordinary Editor extension document lifecycle.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nara_core::{DocumentContentState, DocumentHistoryStatus, DocumentRevision};
use nara_engine::{
    diagnostic::{DiagnosticCode, SafeSummary},
    tooling::{EditorExtensionDocument, EditorExtensionDocumentError},
};
use serde::{Deserialize, Serialize};

const NODE_LIMIT: usize = 64;
const LINK_LIMIT: usize = 128;
const SOURCE_LIMIT: usize = 64 * 1024;
const HISTORY_LIMIT: usize = 64;
const VALUE_LIMIT: f64 = 10_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Scalar(f64),
    Vector([f64; 2]),
    Add,
    Output,
}

/// The two concrete port types supported by this consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Scalar,
    Vector,
}

impl NodeKind {
    #[must_use]
    pub const fn input_type(&self, input: u8) -> Option<ValueType> {
        match (self, input) {
            (Self::Add, 0 | 1) | (Self::Output, 0) => Some(ValueType::Scalar),
            _ => None,
        }
    }

    #[must_use]
    pub const fn output_type(&self) -> Option<ValueType> {
        match self {
            Self::Scalar(_) | Self::Add => Some(ValueType::Scalar),
            Self::Vector(_) => Some(ValueType::Vector),
            Self::Output => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub position: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub from: NodeId,
    pub to: NodeId,
    pub input: u8,
}

/// Authored content only; at most one Output is admitted, and incomplete inputs are permitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphView {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
}

impl GraphView {
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphCommand {
    Add { kind: NodeKind, position: [f32; 2] },
    SetScalar { node: NodeId, value: f64 },
    Move { node: NodeId, position: [f32; 2] },
    Connect(Link),
    Disconnect { to: NodeId, input: u8 },
    Remove(NodeId),
}

type Snapshot = (DocumentContentState, GraphView);

/// Owns validated content and at most 64 historical snapshots across Undo and Redo.
///
/// Node IDs start at one and advance only on accepted Add commands. The allocator and forward
/// content clock never rewind with history. The maximum u32 ID is refused so every accepted
/// source can reconstruct its checked successor on decode. Reopen starts a fresh allocator
/// from the source's maximum ID and a fresh document incarnation with empty history.
pub struct ValueGraph {
    view: GraphView,
    next_id: u32,
    clock: DocumentRevision,
    state: DocumentContentState,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
}

impl ValueGraph {
    #[must_use]
    pub fn new() -> Self {
        let clock = DocumentRevision::initial();
        Self {
            view: GraphView {
                nodes: Vec::new(),
                links: Vec::new(),
            },
            next_id: 1,
            clock,
            state: clock.content_state(),
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }
}

impl Default for ValueGraph {
    fn default() -> Self {
        Self::new()
    }
}

fn error(code: &'static str, summary: &'static str) -> EditorExtensionDocumentError {
    EditorExtensionDocumentError::new(
        DiagnosticCode::new(code).expect("static graph diagnostic code"),
        SafeSummary::new(summary).expect("static graph safe summary"),
    )
}

fn missing_node() -> EditorExtensionDocumentError {
    error(
        "graph.node-missing",
        "The selected graph node does not exist",
    )
}

fn invalid_input() -> EditorExtensionDocumentError {
    error(
        "graph.input-invalid",
        "The selected graph input is not supported by this node",
    )
}

fn bounded(value: f64) -> bool {
    value.is_finite() && value.abs() <= VALUE_LIMIT
}

/// Validates the complete source independently of its node and link ordering.
fn validate(view: &GraphView) -> Result<Vec<usize>, EditorExtensionDocumentError> {
    if view.nodes.len() > NODE_LIMIT || view.links.len() > LINK_LIMIT {
        return Err(error(
            "graph.topology-limit",
            "The graph exceeds its node or link limit",
        ));
    }
    let mut indices = BTreeMap::new();
    let mut outputs = 0;
    for (index, node) in view.nodes.iter().enumerate() {
        if indices.insert(node.id, index).is_some() {
            return Err(error(
                "graph.node-duplicate",
                "Graph node identifiers must be unique",
            ));
        }
        let valid_value = match &node.kind {
            NodeKind::Scalar(value) => bounded(*value),
            NodeKind::Vector(values) => values.iter().all(|value| bounded(*value)),
            NodeKind::Add => true,
            NodeKind::Output => {
                outputs += 1;
                true
            }
        };
        if !valid_value || !node.position.iter().all(|value| bounded(f64::from(*value))) {
            return Err(error(
                "graph.value-invalid",
                "Graph values and positions must be finite and within the authored bounds",
            ));
        }
    }
    if outputs > 1 {
        return Err(error(
            "graph.output-limit",
            "The graph permits at most one Output node",
        ));
    }
    let mut occupied = BTreeSet::new();
    let mut indegrees = vec![0_usize; view.nodes.len()];
    let mut outgoing = vec![Vec::new(); view.nodes.len()];
    for link in &view.links {
        let from = *indices.get(&link.from).ok_or_else(missing_node)?;
        let to = *indices.get(&link.to).ok_or_else(missing_node)?;
        if from == to {
            return Err(error(
                "graph.self-link",
                "A graph node cannot connect to itself",
            ));
        }
        let input_type = view.nodes[to]
            .kind
            .input_type(link.input)
            .ok_or_else(invalid_input)?;
        let output_type = view.nodes[from].kind.output_type().ok_or_else(|| {
            error(
                "graph.output-invalid",
                "The selected graph node has no output",
            )
        })?;
        if input_type != output_type {
            return Err(error(
                "graph.type-mismatch",
                "Graph input and output types must match",
            ));
        }
        if !occupied.insert((link.to, link.input)) {
            return Err(error(
                "graph.input-occupied",
                "The graph input already has a connection",
            ));
        }
        indegrees[to] += 1;
        outgoing[from].push(to);
    }
    let mut ready: VecDeque<_> = indegrees
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(index))
        .collect();
    let mut order = Vec::with_capacity(view.nodes.len());
    while let Some(index) = ready.pop_front() {
        order.push(index);
        for &to in &outgoing[index] {
            indegrees[to] -= 1;
            if indegrees[to] == 0 {
                ready.push_back(to);
            }
        }
    }
    if order.len() != view.nodes.len() {
        return Err(error(
            "graph.cycle",
            "Graph connections must not contain a cycle",
        ));
    }
    Ok(order)
}

/// Evaluates the single Output in bounded topological order, without recursive traversal.
///
/// No Output or an incomplete dependency path produces None. Constants and positions have
/// authored bounds; sums may exceed those bounds but must remain finite. Invalid disconnected
/// topology also refuses, so an unused cycle cannot hide behind an otherwise valid Output.
pub fn evaluate(view: &GraphView) -> Result<Option<f64>, EditorExtensionDocumentError> {
    let order = validate(view)?;
    let mut values = BTreeMap::<NodeId, Option<f64>>::new();
    let mut output = None;
    for index in order {
        let node = &view.nodes[index];
        let input = |port| {
            view.links
                .iter()
                .find(|link| link.to == node.id && link.input == port)
                .and_then(|link| values.get(&link.from))
                .copied()
                .flatten()
        };
        let value = match &node.kind {
            NodeKind::Scalar(value) => Some(*value),
            NodeKind::Vector(_) => None,
            NodeKind::Add => input(0).zip(input(1)).map(|(left, right)| left + right),
            NodeKind::Output => input(0),
        };
        if value.is_some_and(|value| !value.is_finite()) {
            return Err(error(
                "graph.result-nonfinite",
                "Graph evaluation must produce finite values",
            ));
        }
        if matches!(node.kind, NodeKind::Output) {
            output = value;
        }
        values.insert(node.id, value);
    }
    Ok(output)
}

impl EditorExtensionDocument for ValueGraph {
    type Command = GraphCommand;
    type View = GraphView;
    const TYPE_ID: &'static str = "consumer.value-graph";

    fn title(&self) -> &str {
        "Value Graph"
    }

    fn content_state(&self) -> DocumentContentState {
        self.state
    }

    fn history_status(&self) -> DocumentHistoryStatus {
        DocumentHistoryStatus {
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
        }
    }

    fn view(&self) -> GraphView {
        self.view.clone()
    }

    fn apply(&mut self, command: &GraphCommand) -> Result<(), EditorExtensionDocumentError> {
        let mut candidate = self.view.clone();
        let mut next_id = self.next_id;
        match command {
            GraphCommand::Add { kind, position } => {
                next_id = next_id.checked_add(1).ok_or_else(|| {
                    error(
                        "graph.id-exhausted",
                        "The graph node identifier space is exhausted",
                    )
                })?;
                candidate.nodes.push(Node {
                    id: NodeId(self.next_id),
                    kind: kind.clone(),
                    position: *position,
                });
            }
            GraphCommand::SetScalar { node, value } => {
                let selected = candidate
                    .nodes
                    .iter_mut()
                    .find(|entry| entry.id == *node)
                    .ok_or_else(missing_node)?;
                let NodeKind::Scalar(current) = &mut selected.kind else {
                    return Err(error(
                        "graph.scalar-required",
                        "The selected node is not a scalar constant",
                    ));
                };
                *current = *value;
            }
            GraphCommand::Move { node, position } => {
                candidate
                    .nodes
                    .iter_mut()
                    .find(|entry| entry.id == *node)
                    .ok_or_else(missing_node)?
                    .position = *position;
            }
            GraphCommand::Connect(link) => candidate.links.push(*link),
            GraphCommand::Disconnect { to, input } => {
                let node = candidate.node(*to).ok_or_else(missing_node)?;
                node.kind.input_type(*input).ok_or_else(invalid_input)?;
                let previous = candidate.links.len();
                candidate
                    .links
                    .retain(|link| link.to != *to || link.input != *input);
                if previous == candidate.links.len() {
                    return Err(error(
                        "graph.link-missing",
                        "The graph input has no connection to disconnect",
                    ));
                }
            }
            GraphCommand::Remove(id) => {
                candidate.node(*id).ok_or_else(missing_node)?;
                candidate.nodes.retain(|node| node.id != *id);
                candidate
                    .links
                    .retain(|link| link.from != *id && link.to != *id);
            }
        }
        validate(&candidate)?;
        if self.clock.generation() == u64::MAX {
            return Err(error(
                "graph.state-exhausted",
                "The graph content identity space is exhausted",
            ));
        }
        let clock = self.clock.next();
        self.undo
            .push_back((self.state, std::mem::replace(&mut self.view, candidate)));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.next_id = next_id;
        self.clock = clock;
        self.state = clock.content_state();
        Ok(())
    }

    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let (state, view) = self
            .undo
            .pop_back()
            .ok_or_else(|| error("graph.undo-empty", "There is no graph edit to undo"))?;
        self.redo
            .push((self.state, std::mem::replace(&mut self.view, view)));
        self.state = state;
        Ok(())
    }

    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let (state, view) = self
            .redo
            .pop()
            .ok_or_else(|| error("graph.redo-empty", "There is no graph edit to redo"))?;
        self.undo
            .push_back((self.state, std::mem::replace(&mut self.view, view)));
        self.state = state;
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        let bytes = serde_json::to_vec(&self.view)
            .map_err(|_| error("graph.encode", "Graph content could not be encoded"))?;
        if bytes.len() > SOURCE_LIMIT {
            return Err(error(
                "graph.source-limit",
                "Graph source exceeds its byte limit",
            ));
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        if bytes.len() > SOURCE_LIMIT {
            return Err(error(
                "graph.source-limit",
                "Graph source exceeds its byte limit",
            ));
        }
        let view: GraphView = serde_json::from_slice(bytes).map_err(|_| {
            error(
                "graph.decode",
                "Graph source must be a valid typed JSON graph",
            )
        })?;
        validate(&view)?;
        let next_id = view
            .nodes
            .iter()
            .map(|node| node.id.0)
            .max()
            .map_or(Some(1), |maximum| maximum.checked_add(1))
            .ok_or_else(|| {
                error(
                    "graph.id-exhausted",
                    "The graph node identifier space is exhausted",
                )
            })?;
        Ok(Self {
            view,
            next_id,
            ..Self::new()
        })
    }
}

#[cfg(test)]
mod tests;
