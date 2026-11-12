//! A Dialogue Graph owned by the ordinary Editor document, with independent local previews.

mod codec;
#[cfg(feature = "jellyflow-trial")]
pub mod jellyflow;
pub mod preview;
mod validation;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use nara_core::{DocumentContentState, DocumentHistoryStatus, DocumentRevision};
use nara_engine::{
    diagnostic::{DiagnosticCode, SafeSummary},
    tooling::{EditorExtensionDocument, EditorExtensionDocumentError},
};
use serde::{Deserialize, Serialize};

const NODE_LIMIT: usize = 512;
const CHOICE_LIMIT: usize = 4096;
const TEXT_BYTES: usize = 4096;
const SPEAKER_BYTES: usize = 256;
const SOURCE_BYTES: usize = 2 * 1024 * 1024;
const HISTORY_LIMIT: usize = 64;
const POSITION_LIMIT: f32 = 1_000_000.;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChoiceId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: ChoiceId,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeBody {
    Line {
        speaker: String,
        text: String,
    },
    Choice {
        prompt: String,
        choices: Vec<Choice>,
    },
    End {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: NodeId,
    pub position: [f32; 2],
    pub body: Arc<NodeBody>,
}

impl Node {
    pub fn output_ports(&self) -> impl Iterator<Item = OutputPort> + '_ {
        let next = matches!(self.body.as_ref(), NodeBody::Line { .. })
            .then_some(OutputPort::Next { node: self.id });
        let choices = match self.body.as_ref() {
            NodeBody::Choice { choices, .. } => choices.as_slice(),
            _ => &[],
        };
        next.into_iter()
            .chain(choices.iter().map(|choice| OutputPort::Choice {
                node: self.id,
                choice: choice.id,
            }))
    }
}

/// Stable output identity; reordering a choice never changes its corresponding port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputPort {
    Next { node: NodeId },
    Choice { node: NodeId, choice: ChoiceId },
}

impl OutputPort {
    pub const fn node(self) -> NodeId {
        match self {
            Self::Next { node } | Self::Choice { node, .. } => node,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub from: OutputPort,
    pub to: NodeId,
}

/// Authored facts and live incarnations. Native geometry and transient interaction are absent.
///
/// Incarnations are scoped to this open document and excluded from its source. They preserve
/// retirement even when observers skip a deletion and subsequent Undo. Adapters still require
/// the original document/revision for admission; these numbers are not command capabilities.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueView {
    entry: Option<NodeId>,
    nodes: BTreeMap<NodeId, Node>,
    links: BTreeMap<OutputPort, NodeId>,
    node_incarnations: BTreeMap<NodeId, u64>,
    choice_incarnations: BTreeMap<(NodeId, ChoiceId), u64>,
    link_incarnations: BTreeMap<OutputPort, u64>,
}

impl DialogueView {
    fn authored(
        entry: Option<NodeId>,
        nodes: BTreeMap<NodeId, Node>,
        links: BTreeMap<OutputPort, NodeId>,
    ) -> Self {
        Self {
            entry,
            nodes,
            links,
            node_incarnations: BTreeMap::new(),
            choice_incarnations: BTreeMap::new(),
            link_incarnations: BTreeMap::new(),
        }
    }

    pub const fn entry(&self) -> Option<NodeId> {
        self.entry
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn nodes(&self) -> impl ExactSizeIterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn links(&self) -> impl ExactSizeIterator<Item = Link> + '_ {
        self.links.iter().map(|(from, to)| Link {
            from: *from,
            to: *to,
        })
    }

    pub fn target(&self, port: OutputPort) -> Option<NodeId> {
        self.links.get(&port).copied()
    }

    pub fn contains_output(&self, port: OutputPort) -> bool {
        self.node(port.node())
            .is_some_and(|node| node.output_ports().any(|candidate| candidate == port))
    }

    pub fn node_incarnation(&self, node: NodeId) -> Option<u64> {
        self.node_incarnations.get(&node).copied()
    }

    pub fn output_incarnation(&self, port: OutputPort) -> Option<u64> {
        if !self.contains_output(port) {
            return None;
        }
        match port {
            OutputPort::Next { node } => self.node_incarnation(node),
            OutputPort::Choice { node, choice } => {
                self.choice_incarnations.get(&(node, choice)).copied()
            }
        }
    }

    pub fn link_incarnation(&self, port: OutputPort) -> Option<u64> {
        self.link_incarnations.get(&port).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRecipe {
    Line,
    Choice,
    End,
}

/// Existing IDs preserve identity; the document allocates an ID for each new choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceDraft {
    pub id: Option<ChoiceId>,
    pub label: String,
    pub target: Option<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodePosition {
    pub node: NodeId,
    pub position: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub enum DialogueCommand {
    Add {
        recipe: NodeRecipe,
        position: [f32; 2],
    },
    SetEntry(Option<NodeId>),
    SetLine {
        node: NodeId,
        speaker: String,
        text: String,
    },
    SetChoices {
        node: NodeId,
        prompt: String,
        choices: Vec<ChoiceDraft>,
    },
    SetEnd {
        node: NodeId,
        text: String,
    },
    SetTarget {
        port: OutputPort,
        target: Option<NodeId>,
    },
    Move(Vec<NodePosition>),
    Remove(Vec<NodeId>),
}

type Snapshot = (DocumentContentState, Arc<DialogueView>);

/// Owns source/history; snapshots share unchanged node contents across positions and Views.
pub struct DialogueGraph {
    view: Arc<DialogueView>,
    next_node: u64,
    next_choice: u64,
    next_incarnation: u64,
    clock: DocumentRevision,
    state: DocumentContentState,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
}

impl DialogueGraph {
    pub fn new() -> Self {
        let clock = DocumentRevision::initial();
        Self {
            view: Arc::new(DialogueView::authored(
                None,
                BTreeMap::new(),
                BTreeMap::new(),
            )),
            next_node: 1,
            next_choice: 1,
            next_incarnation: 1,
            clock,
            state: clock.content_state(),
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }

    fn from_decoded(
        mut view: DialogueView,
        next_node: u64,
        next_choice: u64,
    ) -> Result<Self, EditorExtensionDocumentError> {
        let mut owner = Self::new();
        owner.next_incarnation = owner.prepare_incarnations(&mut view)?;
        owner.view = Arc::new(view);
        owner.next_node = next_node;
        owner.next_choice = next_choice;
        Ok(owner)
    }

    fn prepare_incarnations(
        &self,
        view: &mut DialogueView,
    ) -> Result<u64, EditorExtensionDocumentError> {
        let mut next = self.next_incarnation;
        let mut node_incarnations = BTreeMap::new();
        let mut choice_incarnations = BTreeMap::new();
        let mut link_incarnations = BTreeMap::new();
        for node in view.nodes.values() {
            let incarnation = match self.view.node_incarnations.get(&node.id) {
                Some(incarnation) => *incarnation,
                None => allocate(&mut next)?,
            };
            node_incarnations.insert(node.id, incarnation);
            if let NodeBody::Choice { choices, .. } = node.body.as_ref() {
                for choice in choices {
                    let identity = (node.id, choice.id);
                    let incarnation = match self.view.choice_incarnations.get(&identity) {
                        Some(incarnation) => *incarnation,
                        None => allocate(&mut next)?,
                    };
                    choice_incarnations.insert(identity, incarnation);
                }
            }
        }
        for port in view.links.keys() {
            let incarnation = match self.view.link_incarnations.get(port) {
                Some(incarnation) => *incarnation,
                None => allocate(&mut next)?,
            };
            link_incarnations.insert(*port, incarnation);
        }
        view.node_incarnations = node_incarnations;
        view.choice_incarnations = choice_incarnations;
        view.link_incarnations = link_incarnations;
        Ok(next)
    }
}

impl Default for DialogueGraph {
    fn default() -> Self {
        Self::new()
    }
}

fn error(code: &'static str, summary: &'static str) -> EditorExtensionDocumentError {
    EditorExtensionDocumentError::new(
        DiagnosticCode::new(code).expect("static dialogue diagnostic code"),
        SafeSummary::new(summary).expect("static dialogue safe summary"),
    )
}

fn missing_node() -> EditorExtensionDocumentError {
    error(
        "dialogue.node-missing",
        "The selected dialogue node does not exist",
    )
}

fn wrong_kind() -> EditorExtensionDocumentError {
    error(
        "dialogue.node-kind",
        "The dialogue edit does not match the selected node kind",
    )
}

fn allocate(next: &mut u64) -> Result<u64, EditorExtensionDocumentError> {
    let id = *next;
    *next = next.checked_add(1).ok_or_else(|| {
        error(
            "dialogue.id-exhausted",
            "The dialogue identifier space is exhausted",
        )
    })?;
    Ok(id)
}

fn replace_choices(
    view: &mut DialogueView,
    node: NodeId,
    prompt: &str,
    drafts: &[ChoiceDraft],
    next_choice: &mut u64,
) -> Result<(), EditorExtensionDocumentError> {
    let current = view.node(node).ok_or_else(missing_node)?;
    let NodeBody::Choice { choices, .. } = current.body.as_ref() else {
        return Err(wrong_kind());
    };
    let owned = choices
        .iter()
        .map(|choice| choice.id)
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut choices = Vec::with_capacity(drafts.len());
    let mut links = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let id = match draft.id {
            Some(id) if owned.contains(&id) => id,
            Some(_) => {
                return Err(error(
                    "dialogue.choice-owner",
                    "An existing choice must belong to the selected dialogue node",
                ));
            }
            None => ChoiceId(allocate(next_choice)?),
        };
        if !seen.insert(id) {
            return Err(error(
                "dialogue.choice-duplicate",
                "A dialogue choice may appear only once",
            ));
        }
        if let Some(target) = draft.target {
            view.node(target).ok_or_else(missing_node)?;
            links.push((OutputPort::Choice { node, choice: id }, target));
        }
        choices.push(Choice {
            id,
            label: draft.label.clone(),
        });
    }
    view.links.retain(|port, _| port.node() != node);
    view.links.extend(links);
    view.nodes.get_mut(&node).ok_or_else(missing_node)?.body = Arc::new(NodeBody::Choice {
        prompt: prompt.to_owned(),
        choices,
    });
    Ok(())
}

impl EditorExtensionDocument for DialogueGraph {
    type Command = DialogueCommand;
    type View = Arc<DialogueView>;
    const TYPE_ID: &'static str = "consumer.dialogue-graph";

    fn title(&self) -> &str {
        "Dialogue Graph"
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

    fn view(&self) -> Self::View {
        self.view.clone()
    }

    fn apply(&mut self, command: &Self::Command) -> Result<(), EditorExtensionDocumentError> {
        let mut candidate = self.view.as_ref().clone();
        let mut next_node = self.next_node;
        let mut next_choice = self.next_choice;
        match command {
            DialogueCommand::Add { recipe, position } => {
                let id = NodeId(allocate(&mut next_node)?);
                let body = match recipe {
                    NodeRecipe::Line => NodeBody::Line {
                        speaker: "Guide".to_owned(),
                        text: "New line".to_owned(),
                    },
                    NodeRecipe::Choice => NodeBody::Choice {
                        prompt: "Choose a response".to_owned(),
                        choices: vec![Choice {
                            id: ChoiceId(allocate(&mut next_choice)?),
                            label: "Continue".to_owned(),
                        }],
                    },
                    NodeRecipe::End => NodeBody::End {
                        text: "End".to_owned(),
                    },
                };
                candidate.nodes.insert(
                    id,
                    Node {
                        id,
                        position: *position,
                        body: Arc::new(body),
                    },
                );
                if candidate.entry.is_none() {
                    candidate.entry = Some(id);
                }
            }
            DialogueCommand::SetEntry(entry) => {
                if let Some(node) = entry {
                    candidate.node(*node).ok_or_else(missing_node)?;
                }
                candidate.entry = *entry;
            }
            DialogueCommand::SetLine {
                node,
                speaker,
                text,
            } => {
                let selected = candidate.nodes.get_mut(node).ok_or_else(missing_node)?;
                if !matches!(selected.body.as_ref(), NodeBody::Line { .. }) {
                    return Err(wrong_kind());
                }
                selected.body = Arc::new(NodeBody::Line {
                    speaker: speaker.clone(),
                    text: text.clone(),
                });
            }
            DialogueCommand::SetChoices {
                node,
                prompt,
                choices,
            } => {
                replace_choices(&mut candidate, *node, prompt, choices, &mut next_choice)?;
            }
            DialogueCommand::SetEnd { node, text } => {
                let selected = candidate.nodes.get_mut(node).ok_or_else(missing_node)?;
                if !matches!(selected.body.as_ref(), NodeBody::End { .. }) {
                    return Err(wrong_kind());
                }
                selected.body = Arc::new(NodeBody::End { text: text.clone() });
            }
            DialogueCommand::SetTarget { port, target } => {
                if !candidate.contains_output(*port) {
                    return Err(error(
                        "dialogue.port-missing",
                        "The selected dialogue output does not exist",
                    ));
                }
                if let Some(target) = target {
                    candidate.node(*target).ok_or_else(missing_node)?;
                    candidate.links.insert(*port, *target);
                } else {
                    candidate.links.remove(port);
                }
            }
            DialogueCommand::Move(positions) => {
                let mut seen = BTreeSet::new();
                for change in positions {
                    if !seen.insert(change.node) {
                        return Err(error(
                            "dialogue.move-duplicate",
                            "A dialogue move may name each node only once",
                        ));
                    }
                    candidate
                        .nodes
                        .get_mut(&change.node)
                        .ok_or_else(missing_node)?
                        .position = change.position;
                }
            }
            DialogueCommand::Remove(nodes) => {
                let mut removed = BTreeSet::new();
                for node in nodes {
                    candidate.node(*node).ok_or_else(missing_node)?;
                    if !removed.insert(*node) {
                        return Err(error(
                            "dialogue.remove-duplicate",
                            "A dialogue removal may name each node only once",
                        ));
                    }
                }
                candidate.nodes.retain(|node, _| !removed.contains(node));
                candidate.links.retain(|port, target| {
                    !removed.contains(&port.node()) && !removed.contains(target)
                });
                if candidate.entry.is_some_and(|node| removed.contains(&node)) {
                    candidate.entry = None;
                }
            }
        }
        let _ = codec::encode(&candidate)?;
        if candidate == *self.view {
            return Ok(());
        }
        if self.clock.generation() == u64::MAX {
            return Err(error(
                "dialogue.state-exhausted",
                "The dialogue content identity space is exhausted",
            ));
        }
        let next_incarnation = self.prepare_incarnations(&mut candidate)?;
        let clock = self.clock.next();
        let previous = std::mem::replace(&mut self.view, Arc::new(candidate));
        self.undo.push_back((self.state, previous));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.next_node = next_node;
        self.next_choice = next_choice;
        self.next_incarnation = next_incarnation;
        self.clock = clock;
        self.state = clock.content_state();
        Ok(())
    }

    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let (state, view) = self
            .undo
            .back()
            .ok_or_else(|| error("dialogue.undo-empty", "There is no dialogue edit to undo"))?;
        let state = *state;
        let mut candidate = view.as_ref().clone();
        let next_incarnation = self.prepare_incarnations(&mut candidate)?;
        let _ = self.undo.pop_back();
        self.redo.push((
            self.state,
            std::mem::replace(&mut self.view, Arc::new(candidate)),
        ));
        self.state = state;
        self.next_incarnation = next_incarnation;
        Ok(())
    }

    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let (state, view) = self
            .redo
            .last()
            .ok_or_else(|| error("dialogue.redo-empty", "There is no dialogue edit to redo"))?;
        let state = *state;
        let mut candidate = view.as_ref().clone();
        let next_incarnation = self.prepare_incarnations(&mut candidate)?;
        let _ = self.redo.pop();
        self.undo.push_back((
            self.state,
            std::mem::replace(&mut self.view, Arc::new(candidate)),
        ));
        self.state = state;
        self.next_incarnation = next_incarnation;
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        codec::encode(&self.view)
    }

    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        codec::decode(bytes)
    }
}

#[cfg(test)]
mod tests;
