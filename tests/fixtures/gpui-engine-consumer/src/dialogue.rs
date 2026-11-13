//! A Dialogue Graph owned by the ordinary Editor document, with independent local previews.

mod codec;
#[cfg(feature = "jellyflow-trial")]
pub mod jellyflow;
pub mod preview;
mod validation;
pub mod workflow;

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

/// An existing endpoint to connect atomically to a newly allocated node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireEndpoint {
    Output(OutputPort),
    Input(NodeId),
}

/// A successful command's domain result, including identities allocated by its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogueCommandOutcome {
    Noop,
    Changed,
    Created {
        node: NodeId,
        outputs: Vec<OutputPort>,
        link: Option<Link>,
    },
    /// Choice identities in their complete authored order after the edit.
    ChoicesChanged {
        node: NodeId,
        choices: Vec<ChoiceId>,
    },
}

/// A successful command's effect and the exact immutable view installed by that command.
///
/// The retained facts remain readable after later edits or document retirement. They do not
/// authorize another command against the current document.
#[derive(Debug, Clone)]
pub struct DialogueCommandReceipt {
    outcome: DialogueCommandOutcome,
    view: Arc<DialogueView>,
}

impl DialogueCommandReceipt {
    pub fn outcome(&self) -> &DialogueCommandOutcome {
        &self.outcome
    }

    pub fn view(&self) -> &Arc<DialogueView> {
        &self.view
    }
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
    CreateFromWire {
        recipe: NodeRecipe,
        position: [f32; 2],
        endpoint: WireEndpoint,
        reconnect: Option<OutputPort>,
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
    Reconnect {
        from: OutputPort,
        to: OutputPort,
        target: NodeId,
    },
    Move(Vec<NodePosition>),
    Remove {
        nodes: Vec<NodeId>,
        links: Vec<OutputPort>,
    },
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

fn create_node(
    view: &mut DialogueView,
    recipe: NodeRecipe,
    position: [f32; 2],
    next_node: &mut u64,
    next_choice: &mut u64,
) -> Result<(NodeId, Vec<OutputPort>), EditorExtensionDocumentError> {
    let id = NodeId(allocate(next_node)?);
    let body = match recipe {
        NodeRecipe::Line => NodeBody::Line {
            speaker: "Guide".to_owned(),
            text: "New line".to_owned(),
        },
        NodeRecipe::Choice => NodeBody::Choice {
            prompt: "Choose a response".to_owned(),
            choices: vec![Choice {
                id: ChoiceId(allocate(next_choice)?),
                label: "Continue".to_owned(),
            }],
        },
        NodeRecipe::End => NodeBody::End {
            text: "End".to_owned(),
        },
    };
    let node = Node {
        id,
        position,
        body: Arc::new(body),
    };
    let outputs = node.output_ports().collect();
    view.nodes.insert(id, node);
    if view.entry.is_none() {
        view.entry = Some(id);
    }
    Ok((id, outputs))
}

fn validate_output(
    view: &DialogueView,
    port: OutputPort,
) -> Result<(), EditorExtensionDocumentError> {
    if !view.contains_output(port) {
        return Err(error(
            "dialogue.port-missing",
            "The selected dialogue output does not exist",
        ));
    }
    Ok(())
}

fn set_target(
    view: &mut DialogueView,
    port: OutputPort,
    target: Option<NodeId>,
) -> Result<(), EditorExtensionDocumentError> {
    validate_output(view, port)?;
    if let Some(target) = target {
        view.node(target).ok_or_else(missing_node)?;
        view.links.insert(port, target);
    } else {
        view.links.remove(&port);
    }
    Ok(())
}

fn remove_selection(
    view: &mut DialogueView,
    nodes: &[NodeId],
    links: &[OutputPort],
) -> Result<(), EditorExtensionDocumentError> {
    let mut removed_nodes = BTreeSet::new();
    for node in nodes {
        view.node(*node).ok_or_else(missing_node)?;
        if !removed_nodes.insert(*node) {
            return Err(error(
                "dialogue.remove-duplicate",
                "A dialogue removal may name each node only once",
            ));
        }
    }
    let mut removed_links = BTreeSet::new();
    for port in links {
        if !view.links.contains_key(port) {
            return Err(error(
                "dialogue.link-missing",
                "The selected dialogue link does not exist",
            ));
        }
        if !removed_links.insert(*port) {
            return Err(error(
                "dialogue.remove-duplicate",
                "A dialogue removal may name each link only once",
            ));
        }
    }
    view.nodes.retain(|node, _| !removed_nodes.contains(node));
    view.links.retain(|port, target| {
        !removed_nodes.contains(&port.node())
            && !removed_nodes.contains(target)
            && !removed_links.contains(port)
    });
    if view.entry.is_some_and(|node| removed_nodes.contains(&node)) {
        view.entry = None;
    }
    Ok(())
}

fn replace_choices(
    view: &mut DialogueView,
    node: NodeId,
    prompt: &str,
    drafts: &[ChoiceDraft],
    next_choice: &mut u64,
) -> Result<Vec<ChoiceId>, EditorExtensionDocumentError> {
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
    let ids = choices.iter().map(|choice| choice.id).collect();
    view.nodes.get_mut(&node).ok_or_else(missing_node)?.body = Arc::new(NodeBody::Choice {
        prompt: prompt.to_owned(),
        choices,
    });
    Ok(ids)
}

impl EditorExtensionDocument for DialogueGraph {
    type Command = DialogueCommand;
    type Outcome = DialogueCommandReceipt;
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

    fn apply(
        &mut self,
        command: &Self::Command,
    ) -> Result<Self::Outcome, EditorExtensionDocumentError> {
        let mut candidate = self.view.as_ref().clone();
        let mut next_node = self.next_node;
        let mut next_choice = self.next_choice;
        let outcome = match command {
            DialogueCommand::Add { recipe, position } => {
                let (node, outputs) = create_node(
                    &mut candidate,
                    *recipe,
                    *position,
                    &mut next_node,
                    &mut next_choice,
                )?;
                DialogueCommandOutcome::Created {
                    node,
                    outputs,
                    link: None,
                }
            }
            DialogueCommand::CreateFromWire {
                recipe,
                position,
                endpoint,
                reconnect,
            } => {
                match endpoint {
                    WireEndpoint::Output(port) => validate_output(&candidate, *port)?,
                    WireEndpoint::Input(target) => {
                        candidate.node(*target).ok_or_else(missing_node)?;
                        if *recipe == NodeRecipe::End {
                            return Err(error(
                                "dialogue.wire-output",
                                "A dialogue node created from an input wire must have an output",
                            ));
                        }
                    }
                }
                if let Some(original) = reconnect {
                    validate_output(&candidate, *original)?;
                    let previous_target = candidate.target(*original).ok_or_else(|| {
                        error(
                            "dialogue.link-missing",
                            "The selected dialogue link does not exist",
                        )
                    })?;
                    let matches = match endpoint {
                        WireEndpoint::Output(port) => port == original,
                        WireEndpoint::Input(target) => *target == previous_target,
                    };
                    if !matches {
                        return Err(error(
                            "dialogue.wire-reconnect",
                            "The original dialogue link does not match the fixed wire endpoint",
                        ));
                    }
                }
                let (node, outputs) = create_node(
                    &mut candidate,
                    *recipe,
                    *position,
                    &mut next_node,
                    &mut next_choice,
                )?;
                let link = match endpoint {
                    WireEndpoint::Output(port) => Link {
                        from: *port,
                        to: node,
                    },
                    WireEndpoint::Input(target) => Link {
                        from: outputs.first().copied().ok_or_else(wrong_kind)?,
                        to: *target,
                    },
                };
                if let Some(original) = reconnect
                    && *original != link.from
                {
                    candidate.links.remove(original);
                }
                set_target(&mut candidate, link.from, Some(link.to))?;
                DialogueCommandOutcome::Created {
                    node,
                    outputs,
                    link: Some(link),
                }
            }
            DialogueCommand::SetEntry(entry) => {
                if let Some(node) = entry {
                    candidate.node(*node).ok_or_else(missing_node)?;
                }
                candidate.entry = *entry;
                DialogueCommandOutcome::Changed
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
                DialogueCommandOutcome::Changed
            }
            DialogueCommand::SetChoices {
                node,
                prompt,
                choices,
            } => {
                let choices =
                    replace_choices(&mut candidate, *node, prompt, choices, &mut next_choice)?;
                DialogueCommandOutcome::ChoicesChanged {
                    node: *node,
                    choices,
                }
            }
            DialogueCommand::SetEnd { node, text } => {
                let selected = candidate.nodes.get_mut(node).ok_or_else(missing_node)?;
                if !matches!(selected.body.as_ref(), NodeBody::End { .. }) {
                    return Err(wrong_kind());
                }
                selected.body = Arc::new(NodeBody::End { text: text.clone() });
                DialogueCommandOutcome::Changed
            }
            DialogueCommand::SetTarget { port, target } => {
                set_target(&mut candidate, *port, *target)?;
                DialogueCommandOutcome::Changed
            }
            DialogueCommand::Reconnect { from, to, target } => {
                if !candidate.links.contains_key(from) {
                    return Err(error(
                        "dialogue.link-missing",
                        "The selected dialogue link does not exist",
                    ));
                }
                validate_output(&candidate, *to)?;
                candidate.node(*target).ok_or_else(missing_node)?;
                if from != to && candidate.links.contains_key(to) {
                    return Err(error(
                        "dialogue.output-occupied",
                        "The destination dialogue output already has a link",
                    ));
                }
                candidate.links.remove(from);
                set_target(&mut candidate, *to, Some(*target))?;
                DialogueCommandOutcome::Changed
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
                DialogueCommandOutcome::Changed
            }
            DialogueCommand::Remove { nodes, links } => {
                remove_selection(&mut candidate, nodes, links)?;
                DialogueCommandOutcome::Changed
            }
        };
        let _ = codec::encode(&candidate)?;
        if candidate == *self.view {
            return Ok(DialogueCommandReceipt {
                outcome: DialogueCommandOutcome::Noop,
                view: Arc::clone(&self.view),
            });
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
        Ok(DialogueCommandReceipt {
            outcome,
            view: Arc::clone(&self.view),
        })
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
mod atomic_tests;
#[cfg(test)]
mod tests;
