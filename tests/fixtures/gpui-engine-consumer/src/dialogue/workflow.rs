//! View-local drafts and bounded preview over the original Session document.

use std::sync::Arc;

use nara_core::{DocumentHistoryStatus, DocumentRevision};
use nara_engine::{
    project_host::EditorProjectSession,
    tooling::{
        EditorActionOutcome, EditorDocumentId, EditorExtensionCommand, EditorPersistenceView,
    },
};

use super::{
    ChoiceDraft, DialogueCommand, DialogueGraph, DialogueView, NodeBody, NodeId, NodePosition,
    OutputPort,
    preview::{Conversation, ConversationStep},
};

mod merge;
pub use merge::{ConflictResolution, DraftConflict, DraftField};
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq)]
pub struct DialogueSnapshot {
    pub document: EditorDocumentId,
    pub revision: DocumentRevision,
    pub view: Arc<DialogueView>,
    pub dirty: bool,
    pub history: DocumentHistoryStatus,
    pub persistence_pending: bool,
}

impl DialogueSnapshot {
    pub fn capture(session: &EditorProjectSession, document: EditorDocumentId) -> Option<Self> {
        let frame = session.current_frame();
        let projection = frame.workspace().extension_document(document)?;
        let slot = frame.workspace().document(document)?;
        Some(Self {
            document,
            revision: projection.revision(),
            view: projection.get::<DialogueGraph>()?.clone(),
            dirty: slot.dirty,
            history: slot.history,
            persistence_pending: matches!(
                frame.persistence(),
                EditorPersistenceView::Pending { .. }
            ),
        })
    }

    pub fn command(&self, command: DialogueCommand) -> EditorExtensionCommand {
        EditorExtensionCommand::new::<DialogueGraph>(self.document, self.revision, command)
    }

    pub fn same_source(&self, other: &Self) -> bool {
        self.document == other.document && self.revision.source_id() == other.revision.source_id()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeDraft {
    pub speaker: String,
    pub text: String,
    pub choices: Vec<ChoiceDraft>,
    pub position: [String; 2],
}

impl NodeDraft {
    fn capture(snapshot: &DialogueSnapshot, node: NodeId) -> Option<Self> {
        let record = snapshot.view.node(node)?;
        let (speaker, text, choices) = match record.body.as_ref() {
            NodeBody::Line { speaker, text } => (speaker.clone(), text.clone(), Vec::new()),
            NodeBody::Choice { prompt, choices } => (
                String::new(),
                prompt.clone(),
                choices
                    .iter()
                    .map(|choice| ChoiceDraft {
                        id: Some(choice.id),
                        label: choice.label.clone(),
                        target: snapshot.view.target(OutputPort::Choice {
                            node,
                            choice: choice.id,
                        }),
                    })
                    .collect(),
            ),
            NodeBody::End { text } => (String::new(), text.clone(), Vec::new()),
        };
        Some(Self {
            speaker,
            text,
            choices,
            position: record.position.map(|value| value.to_string()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftState {
    Clean,
    Editing,
    Pending,
    Accepted,
    Rejected,
    Conflict,
    Retired,
}

pub struct DialogueDraft {
    basis: DialogueSnapshot,
    node: NodeId,
    original: NodeDraft,
    value: NodeDraft,
    pending: Option<EditorExtensionCommand>,
    pending_position: bool,
    state: DraftState,
    read_only: bool,
    error: Option<String>,
    conflicts: Vec<DraftField>,
}

impl DialogueDraft {
    pub fn new(basis: DialogueSnapshot, node: NodeId) -> Option<Self> {
        let value = NodeDraft::capture(&basis, node)?;
        Some(Self {
            basis,
            node,
            original: value.clone(),
            value,
            pending: None,
            pending_position: false,
            state: DraftState::Clean,
            read_only: false,
            error: None,
            conflicts: Vec::new(),
        })
    }

    pub const fn node(&self) -> NodeId {
        self.node
    }

    pub fn value(&self) -> &NodeDraft {
        &self.value
    }

    pub fn basis(&self) -> &DialogueSnapshot {
        &self.basis
    }

    pub const fn state(&self) -> DraftState {
        self.state
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn conflicts(&self) -> Vec<DraftConflict> {
        self.conflicts
            .iter()
            .map(|field| DraftConflict {
                field: *field,
                current: field.display(&self.original),
                draft: field.display(&self.value),
            })
            .collect()
    }

    pub fn has_live_node(&self, current: &DialogueSnapshot) -> bool {
        self.state != DraftState::Retired
            && self.basis.same_source(current)
            && current.view.node(self.node).is_some()
            && self.basis.view.node_incarnation(self.node)
                == current.view.node_incarnation(self.node)
    }

    pub const fn read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn editable(&self) -> bool {
        !self.read_only && !matches!(self.state, DraftState::Pending | DraftState::Retired)
    }

    pub fn edit(&mut self, update: impl FnOnce(&mut NodeDraft)) -> bool {
        if !self.editable() {
            return false;
        }
        update(&mut self.value);
        self.error = None;
        if self.state != DraftState::Conflict {
            self.state = if self.value == self.original {
                DraftState::Clean
            } else {
                DraftState::Editing
            };
        }
        true
    }

    pub fn observe(&mut self, current: &DialogueSnapshot) {
        let same_node = self.basis.same_source(current)
            && self.basis.view.node_incarnation(self.node)
                == current.view.node_incarnation(self.node);
        if !same_node || current.view.node(self.node).is_none() {
            self.state = DraftState::Retired;
            self.error = Some("Source replaced or node retired; select a current node".into());
            return;
        }
        if self.pending.is_some() || self.state == DraftState::Retired {
            return;
        }
        if self.basis.revision == current.revision {
            return;
        }
        if self.value == self.original && self.state != DraftState::Conflict {
            self.cancel(current);
        } else {
            self.state = DraftState::Conflict;
            self.error = Some("Source changed; Cancel or Review draft before Apply".into());
        }
    }

    pub fn cancel(&mut self, current: &DialogueSnapshot) -> bool {
        if self.pending.is_some()
            || self.state == DraftState::Retired
            || !self.basis.same_source(current)
            || self.basis.view.node_incarnation(self.node)
                != current.view.node_incarnation(self.node)
        {
            return false;
        }
        let Some(value) = NodeDraft::capture(current, self.node) else {
            return false;
        };
        self.basis = current.clone();
        self.original = value.clone();
        self.value = value;
        self.state = DraftState::Clean;
        self.error = None;
        self.conflicts.clear();
        true
    }

    pub fn review(&mut self, current: &DialogueSnapshot) -> bool {
        if self.pending.is_some()
            || self.state == DraftState::Retired
            || !self.basis.same_source(current)
            || self.basis.view.node_incarnation(self.node)
                != current.view.node_incarnation(self.node)
        {
            return false;
        }
        let Some(original) = NodeDraft::capture(current, self.node) else {
            return false;
        };
        self.value
            .merge(&self.original, &original, &mut self.conflicts);
        self.basis = current.clone();
        self.original = original;
        self.conflicts
            .retain(|field| !field.equal(&self.original, &self.value));
        self.update_review_state();
        true
    }

    pub fn resolve(
        &mut self,
        field: DraftField,
        resolution: ConflictResolution,
        current: &DialogueSnapshot,
    ) -> bool {
        self.observe(current);
        if !self.editable()
            || self.basis.revision != current.revision
            || !self.conflicts.contains(&field)
        {
            return false;
        }
        if !self.value.resolve(field, resolution, &self.original) {
            return false;
        }
        self.conflicts
            .retain(|conflict| *conflict != field && !conflict.equal(&self.original, &self.value));
        self.update_review_state();
        true
    }

    fn update_review_state(&mut self) {
        if self.conflicts.is_empty() {
            self.state = if self.value == self.original {
                DraftState::Clean
            } else {
                DraftState::Editing
            };
            self.error = None;
        } else {
            self.state = DraftState::Conflict;
            self.error =
                Some("Choose Current or Draft for each conflicting field before Apply".into());
        }
    }

    pub fn prepare(
        &mut self,
        current: &DialogueSnapshot,
        position_only: bool,
    ) -> Result<EditorExtensionCommand, String> {
        self.observe(current);
        let prepared: Result<EditorExtensionCommand, String> = (|| {
            if !self.editable() || self.state == DraftState::Conflict {
                return Err("Review the current source before Apply".into());
            }
            let command = if position_only {
                let mut position = [0.; 2];
                for (result, text) in position.iter_mut().zip(&self.value.position) {
                    *result = text
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite() && value.abs() <= super::POSITION_LIMIT)
                        .ok_or("Enter a finite position within the document bounds")?;
                }
                DialogueCommand::Move(vec![NodePosition {
                    node: self.node,
                    position,
                }])
            } else {
                if self.value.text.len() > super::TEXT_BYTES
                    || self.value.speaker.len() > super::SPEAKER_BYTES
                    || self
                        .value
                        .choices
                        .iter()
                        .any(|choice| choice.label.len() > super::TEXT_BYTES)
                {
                    return Err("The draft exceeds the document UTF-8 byte limit".into());
                }
                match self
                    .basis
                    .view
                    .node(self.node)
                    .map(|node| node.body.as_ref())
                {
                    Some(NodeBody::Line { .. }) => DialogueCommand::SetLine {
                        node: self.node,
                        speaker: self.value.speaker.clone(),
                        text: self.value.text.clone(),
                    },
                    Some(NodeBody::Choice { .. }) => DialogueCommand::SetChoices {
                        node: self.node,
                        prompt: self.value.text.clone(),
                        choices: self.value.choices.clone(),
                    },
                    Some(NodeBody::End { .. }) => DialogueCommand::SetEnd {
                        node: self.node,
                        text: self.value.text.clone(),
                    },
                    None => return Err("The selected node is unavailable".into()),
                }
            };
            Ok(self.basis.command(command))
        })();
        match prepared {
            Ok(command) => {
                self.pending = Some(command.clone());
                self.pending_position = position_only;
                self.state = DraftState::Pending;
                self.error = None;
                Ok(command)
            }
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn settle(
        &mut self,
        command: &EditorExtensionCommand,
        outcome: &EditorActionOutcome,
        current: &DialogueSnapshot,
    ) -> Result<(), &'static str> {
        if self.pending.as_ref() != Some(command) {
            return Err("The result does not belong to this draft's original command");
        }
        let accepted = match outcome {
            EditorActionOutcome::Workspace(report) if report.applied => {
                let receipt = report
                    .extension_outcome
                    .as_ref()
                    .ok_or("Missing exact outcome")?;
                if report.document != Some(command.document())
                    || receipt.command() != command
                    || receipt.revision() != command.base_revision().next()
                    || receipt.get::<DialogueGraph>().is_none()
                {
                    return Err("The outcome does not match the original command");
                }
                Some(receipt)
            }
            EditorActionOutcome::Workspace(report) if !report.applied => None,
            EditorActionOutcome::Rejected(_) => None,
            _ => return Err("The original slot does not contain a document result"),
        };
        self.pending = None;
        if !self.basis.same_source(current) || self.state == DraftState::Retired {
            self.state = DraftState::Retired;
            return Ok(());
        }
        if let Some(accepted) = accepted {
            let mut captured = current.clone();
            captured.revision = accepted.revision();
            captured.view = accepted
                .get::<DialogueGraph>()
                .ok_or("Foreign outcome")?
                .view()
                .clone();
            let canonical =
                NodeDraft::capture(&captured, self.node).ok_or("Accepted node is missing")?;
            if self.pending_position {
                self.value.position = canonical.position.clone();
            } else {
                self.value.speaker = canonical.speaker.clone();
                self.value.text = canonical.text.clone();
                self.value.choices = canonical.choices.clone();
            }
            self.basis = captured;
            self.original = canonical;
            self.error = None;
            self.state = if self.value == self.original {
                DraftState::Accepted
            } else {
                DraftState::Editing
            };
            self.observe(current);
        } else {
            self.state = DraftState::Rejected;
            self.error = Some("The document refused this edit; review the current source".into());
            self.observe(current);
        }
        Ok(())
    }
}

pub struct DialoguePreview {
    basis: DialogueSnapshot,
    conversation: Conversation,
}

impl DialoguePreview {
    pub fn new(basis: DialogueSnapshot) -> Self {
        Self {
            conversation: Conversation::new(basis.view.clone()),
            basis,
        }
    }

    fn validate(&self, current: &DialogueSnapshot) -> Result<(), String> {
        if self.basis.document != current.document || self.basis.revision != current.revision {
            return Err("Source changed; Restart preview".into());
        }
        Ok(())
    }

    pub fn step<'view>(
        &'view self,
        current: &DialogueSnapshot,
    ) -> Result<ConversationStep<'view>, String> {
        self.validate(current)?;
        self.conversation.step().map_err(|error| error.to_string())
    }

    pub fn advance(&mut self, current: &DialogueSnapshot) -> Result<NodeId, String> {
        self.validate(current)?;
        self.conversation
            .advance()
            .map_err(|error| error.to_string())
    }

    pub fn choose(
        &mut self,
        current: &DialogueSnapshot,
        choice: super::ChoiceId,
    ) -> Result<NodeId, String> {
        self.validate(current)?;
        self.conversation
            .choose(choice)
            .map_err(|error| error.to_string())
    }
}
