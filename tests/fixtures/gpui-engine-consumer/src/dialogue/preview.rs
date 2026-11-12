//! A bounded single transition over an immutable authored dialogue snapshot.

use std::{fmt, sync::Arc};

use super::{Choice, ChoiceId, DialogueView, NodeBody, NodeId, OutputPort};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationStep<'view> {
    Line {
        node: NodeId,
        speaker: &'view str,
        text: &'view str,
        next: Option<NodeId>,
    },
    Choice {
        node: NodeId,
        prompt: &'view str,
        choices: &'view [Choice],
    },
    End {
        node: NodeId,
        text: &'view str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewError {
    EntryMissing,
    NodeMissing(NodeId),
    NotALine(NodeId),
    NotAChoice(NodeId),
    ChoiceMissing { node: NodeId, choice: ChoiceId },
    TargetMissing(OutputPort),
}

impl fmt::Display for PreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntryMissing => formatter.write_str("Choose a dialogue entry node"),
            Self::NodeMissing(node) => write!(formatter, "Dialogue node {} is unavailable", node.0),
            Self::NotALine(node) => write!(formatter, "Dialogue node {} is not a line", node.0),
            Self::NotAChoice(node) => write!(formatter, "Dialogue node {} has no choices", node.0),
            Self::ChoiceMissing { node, choice } => {
                write!(
                    formatter,
                    "Dialogue choice {} does not belong to node {}",
                    choice.0, node.0
                )
            }
            Self::TargetMissing(_) => formatter.write_str("Connect the selected dialogue output"),
        }
    }
}

impl std::error::Error for PreviewError {}

/// A local cursor over one captured source; each explicit operation follows one link.
///
/// Cycles require no recursive traversal. An Editor adapter keeps this captured source's
/// document/revision and admits preview operations against its own current presentation.
pub struct Conversation {
    view: Arc<DialogueView>,
    current: Option<NodeId>,
}

impl Conversation {
    pub fn new(view: Arc<DialogueView>) -> Self {
        Self {
            current: view.entry(),
            view,
        }
    }

    pub const fn current(&self) -> Option<NodeId> {
        self.current
    }

    pub fn step(&self) -> Result<ConversationStep<'_>, PreviewError> {
        let id = self.current.ok_or(PreviewError::EntryMissing)?;
        let node = self.view.node(id).ok_or(PreviewError::NodeMissing(id))?;
        Ok(match node.body.as_ref() {
            NodeBody::Line { speaker, text } => ConversationStep::Line {
                node: id,
                speaker,
                text,
                next: self.view.target(OutputPort::Next { node: id }),
            },
            NodeBody::Choice { prompt, choices } => ConversationStep::Choice {
                node: id,
                prompt,
                choices,
            },
            NodeBody::End { text } => ConversationStep::End { node: id, text },
        })
    }

    pub fn advance(&mut self) -> Result<NodeId, PreviewError> {
        let id = self.current.ok_or(PreviewError::EntryMissing)?;
        let node = self.view.node(id).ok_or(PreviewError::NodeMissing(id))?;
        if !matches!(node.body.as_ref(), NodeBody::Line { .. }) {
            return Err(PreviewError::NotALine(id));
        }
        self.follow(OutputPort::Next { node: id })
    }

    pub fn choose(&mut self, choice: ChoiceId) -> Result<NodeId, PreviewError> {
        let id = self.current.ok_or(PreviewError::EntryMissing)?;
        let node = self.view.node(id).ok_or(PreviewError::NodeMissing(id))?;
        let NodeBody::Choice { choices, .. } = node.body.as_ref() else {
            return Err(PreviewError::NotAChoice(id));
        };
        if !choices.iter().any(|candidate| candidate.id == choice) {
            return Err(PreviewError::ChoiceMissing { node: id, choice });
        }
        self.follow(OutputPort::Choice { node: id, choice })
    }

    fn follow(&mut self, port: OutputPort) -> Result<NodeId, PreviewError> {
        let target = self
            .view
            .target(port)
            .ok_or(PreviewError::TargetMissing(port))?;
        self.view
            .node(target)
            .ok_or(PreviewError::NodeMissing(target))?;
        self.current = Some(target);
        Ok(target)
    }
}
