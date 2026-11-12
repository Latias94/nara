//! Exact canvas requests, original Session commands, and sealed domain publications.

use std::{fmt, sync::Arc};

use jellyflow_core::{EdgeId, PortDirection, PortId, PortKind};
use jellyflow_runtime::{
    graph::{EdgeEndpoint, ExternalGraphDocument, ExternalGraphError, RuntimePatch},
    interaction::{
        ExternalConnectionIdentity, ExternalCreatedNodeIdentity, ExternalGraphCompletion,
        ExternalGraphCompletionError, ExternalGraphEffect, ExternalGraphRequest,
        GraphConnectionTarget, GraphMutationIntent,
    },
    schema::NodeCreation,
};
use nara_engine::tooling::{
    EditorActionOutcome, EditorActionRejection, EditorExtensionCommand,
    EditorExtensionCommandOutcome, EditorExtensionDocumentError,
};

use super::{DialogueFacts, core_node_id, input_id, output_id};
use crate::dialogue::{
    DialogueCommand, DialogueCommandOutcome, DialogueGraph, NodePosition, NodeRecipe, WireEndpoint,
};

/// A single original canvas operation lowered through its captured domain observation.
///
/// Clone `command()` only to carry the same payload through an original ordered Session slot.
/// Keep this request until that slot's outcome is known; never recreate or redispatch it to
/// repair a completion association.
#[derive(Debug)]
#[must_use = "retain the request until its original Session operation is settled"]
pub struct DialogueCanvasRequest {
    request: ExternalGraphRequest,
    command: EditorExtensionCommand,
}

/// An accepted original Session result paired with the publication made from its exact view.
///
/// Only `DialogueCanvasRequest::publish` constructs this receipt. An arbitrary graph patch,
/// current document observation, or equal authored values cannot substitute for it.
#[derive(Debug)]
#[must_use = "retain the original publication until the matching request is completed"]
pub struct DialogueCanvasPublication {
    outcome: EditorExtensionCommandOutcome,
    patch: RuntimePatch,
    facts: DialogueFacts,
}

#[derive(Debug)]
pub enum DialogueCanvasError {
    MissingBasis,
    ForeignFacts,
    MissingIdentity,
    UnsupportedGeometry,
    UnsupportedRecipe,
    InvalidCreationTarget,
    ResultMismatch,
    ExpectedRejection,
    Projection(EditorExtensionDocumentError),
    Publication(ExternalGraphError),
    Completion(ExternalGraphCompletionError),
}

impl fmt::Display for DialogueCanvasError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBasis => formatter.write_str("the request has no Dialogue domain basis"),
            Self::ForeignFacts => {
                formatter.write_str("the Dialogue basis is not the request's captured graph")
            }
            Self::MissingIdentity => {
                formatter.write_str("the original Dialogue basis does not contain this identity")
            }
            Self::UnsupportedGeometry => {
                formatter.write_str("Dialogue node size is View-local and cannot be authored")
            }
            Self::UnsupportedRecipe => {
                formatter.write_str("the proposed creation is not a Dialogue node recipe")
            }
            Self::InvalidCreationTarget => {
                formatter.write_str("the proposed port does not match the wire direction")
            }
            Self::ResultMismatch => {
                formatter.write_str("the result does not belong to the original Dialogue command")
            }
            Self::ExpectedRejection => {
                formatter.write_str("the original Session slot does not contain a command refusal")
            }
            Self::Projection(error) => fmt::Display::fmt(error, formatter),
            Self::Publication(error) => fmt::Display::fmt(error, formatter),
            Self::Completion(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for DialogueCanvasError {}

/// Lowering failed without consuming the original external request.
#[derive(Debug)]
pub struct DialogueCanvasLoweringFailure {
    request: ExternalGraphRequest,
    error: DialogueCanvasError,
}

impl DialogueCanvasLoweringFailure {
    pub fn error(&self) -> &DialogueCanvasError {
        &self.error
    }

    pub fn into_parts(self) -> (ExternalGraphRequest, DialogueCanvasError) {
        (self.request, self.error)
    }
}

impl fmt::Display for DialogueCanvasLoweringFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for DialogueCanvasLoweringFailure {}

/// An invalid or unpublishable result is returned intact; its request is still borrowed.
#[derive(Debug)]
pub struct DialogueCanvasPublicationFailure {
    outcome: EditorExtensionCommandOutcome,
    error: DialogueCanvasError,
}

impl DialogueCanvasPublicationFailure {
    pub fn error(&self) -> &DialogueCanvasError {
        &self.error
    }

    pub fn into_parts(self) -> (EditorExtensionCommandOutcome, DialogueCanvasError) {
        (self.outcome, self.error)
    }
}

impl fmt::Display for DialogueCanvasPublicationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for DialogueCanvasPublicationFailure {}

/// Failed settlement retains both original authorities for explicit evidence repair.
#[derive(Debug)]
pub struct DialogueCanvasCompletionFailure {
    pending: DialogueCanvasRequest,
    publication: DialogueCanvasPublication,
    error: DialogueCanvasError,
}

impl DialogueCanvasCompletionFailure {
    pub fn error(&self) -> &DialogueCanvasError {
        &self.error
    }

    pub fn into_parts(
        self,
    ) -> (
        DialogueCanvasRequest,
        DialogueCanvasPublication,
        DialogueCanvasError,
    ) {
        (self.pending, self.publication, self.error)
    }
}

impl fmt::Display for DialogueCanvasCompletionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for DialogueCanvasCompletionFailure {}

/// A mismatched refusal preserves the request, original submitted payload, and result slot.
#[derive(Debug)]
pub struct DialogueCanvasRejectionFailure {
    pending: DialogueCanvasRequest,
    command: EditorExtensionCommand,
    outcome: EditorActionOutcome,
    error: DialogueCanvasError,
}

impl DialogueCanvasRejectionFailure {
    pub fn error(&self) -> &DialogueCanvasError {
        &self.error
    }

    pub fn into_parts(
        self,
    ) -> (
        DialogueCanvasRequest,
        EditorExtensionCommand,
        EditorActionOutcome,
        DialogueCanvasError,
    ) {
        (self.pending, self.command, self.outcome, self.error)
    }
}

impl fmt::Display for DialogueCanvasRejectionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for DialogueCanvasRejectionFailure {}

impl DialogueCanvasRequest {
    pub fn lower(request: ExternalGraphRequest) -> Result<Self, DialogueCanvasLoweringFailure> {
        let lowered = (|| {
            let facts = request
                .owner_basis::<DialogueFacts>()
                .ok_or(DialogueCanvasError::MissingBasis)?;
            if !std::ptr::eq(facts.graph(), request.snapshot().graph()) {
                return Err(DialogueCanvasError::ForeignFacts);
            }
            let command = lower_command(&request, facts)?;
            Ok(EditorExtensionCommand::new::<DialogueGraph>(
                facts.basis().document,
                facts.basis().revision,
                command,
            ))
        })();
        match lowered {
            Ok(command) => Ok(Self { request, command }),
            Err(error) => Err(DialogueCanvasLoweringFailure { request, error }),
        }
    }

    pub fn request(&self) -> &ExternalGraphRequest {
        &self.request
    }

    pub fn command(&self) -> &EditorExtensionCommand {
        &self.command
    }

    /// Publishes the accepted command's retained view at this request's original predecessor.
    ///
    /// The result is validated before publication. A stale graph predecessor returns the exact
    /// result without publishing or rerunning its already accepted domain operation.
    pub fn publish(
        &self,
        external: &ExternalGraphDocument,
        outcome: EditorExtensionCommandOutcome,
    ) -> Result<DialogueCanvasPublication, DialogueCanvasPublicationFailure> {
        let prepared = (|| {
            if outcome.command() != &self.command
                || outcome.revision() != self.command.base_revision().next()
            {
                return Err(DialogueCanvasError::ResultMismatch);
            }
            let receipt = outcome
                .get::<DialogueGraph>()
                .ok_or(DialogueCanvasError::ResultMismatch)?;
            let facts = DialogueFacts::capture(
                self.command.document(),
                outcome.revision(),
                Arc::clone(receipt.view()),
            )
            .map_err(DialogueCanvasError::Projection)?;
            connection_identity(&self.request, &facts, receipt.outcome())?;
            let published = facts
                .external_facts()
                .map_err(DialogueCanvasError::Publication)?;
            let patch = external
                .publish(self.request.expected_version(), published)
                .map_err(DialogueCanvasError::Publication)?;
            Ok((patch, facts))
        })();
        match prepared {
            Ok((patch, facts)) => Ok(DialogueCanvasPublication {
                outcome,
                patch,
                facts,
            }),
            Err(error) => Err(DialogueCanvasPublicationFailure { outcome, error }),
        }
    }

    /// Completes only this original request, retaining the original publication on any failure.
    pub fn complete(
        self,
        publication: DialogueCanvasPublication,
    ) -> Result<ExternalGraphCompletion, DialogueCanvasCompletionFailure> {
        if publication.outcome.command() != &self.command {
            return Err(DialogueCanvasCompletionFailure {
                pending: self,
                publication,
                error: DialogueCanvasError::ResultMismatch,
            });
        }
        let Some(receipt) = publication.outcome.get::<DialogueGraph>() else {
            return Err(DialogueCanvasCompletionFailure {
                pending: self,
                publication,
                error: DialogueCanvasError::ResultMismatch,
            });
        };
        let effect = match receipt.outcome() {
            DialogueCommandOutcome::Noop => ExternalGraphEffect::Noop,
            _ => ExternalGraphEffect::Applied,
        };
        let connection =
            match connection_identity(&self.request, &publication.facts, receipt.outcome()) {
                Ok(identity) => identity,
                Err(error) => {
                    return Err(DialogueCanvasCompletionFailure {
                        pending: self,
                        publication,
                        error,
                    });
                }
            };
        let Self { request, command } = self;
        let DialogueCanvasPublication {
            outcome,
            patch,
            facts,
        } = publication;
        request
            .accept(patch, effect, connection)
            .map_err(|failure| {
                let (request, patch, _, _, error) = failure.into_parts();
                DialogueCanvasCompletionFailure {
                    pending: Self { request, command },
                    publication: DialogueCanvasPublication {
                        outcome,
                        patch,
                        facts,
                    },
                    error: DialogueCanvasError::Completion(error),
                }
            })
    }

    /// Rejects from the original ordered Session slot and its retained submitted command.
    ///
    /// The caller is responsible for retaining that slot association. Success, unrelated
    /// command payloads, and unrelated result kinds cannot be converted into a refusal.
    pub fn reject(
        self,
        command: EditorExtensionCommand,
        outcome: EditorActionOutcome,
    ) -> Result<ExternalGraphCompletion, DialogueCanvasRejectionFailure> {
        let error = if command != self.command {
            Some(DialogueCanvasError::ResultMismatch)
        } else {
            match &outcome {
                EditorActionOutcome::Rejected(EditorActionRejection::StaleDocumentFact {
                    document,
                    expected,
                    ..
                }) if *document == command.document() && *expected == command.base_revision() => {
                    None
                }
                EditorActionOutcome::Rejected(EditorActionRejection::DocumentNotInFrame {
                    document,
                }) if *document == command.document() => None,
                EditorActionOutcome::Rejected(
                    EditorActionRejection::StaleFrame { .. }
                    | EditorActionRejection::NoPublishedFrame,
                ) => None,
                EditorActionOutcome::Workspace(report)
                    if !report.applied
                        && report
                            .document
                            .is_none_or(|document| document == command.document())
                        && report.extension_outcome.is_none() =>
                {
                    None
                }
                _ => Some(DialogueCanvasError::ExpectedRejection),
            }
        };
        if let Some(error) = error {
            return Err(DialogueCanvasRejectionFailure {
                pending: self,
                command,
                outcome,
                error,
            });
        }
        Ok(self.request.reject(format!("{outcome:?}")))
    }
}

impl DialogueCanvasPublication {
    pub fn outcome(&self) -> &EditorExtensionCommandOutcome {
        &self.outcome
    }

    pub fn patch(&self) -> &RuntimePatch {
        &self.patch
    }

    pub fn facts(&self) -> &DialogueFacts {
        &self.facts
    }
}

fn lower_command(
    request: &ExternalGraphRequest,
    facts: &DialogueFacts,
) -> Result<DialogueCommand, DialogueCanvasError> {
    let graph = facts.graph().graph_id();
    let missing = || DialogueCanvasError::MissingIdentity;
    match request.intent() {
        GraphMutationIntent::MoveNodes(edits) => {
            let positions = edits
                .iter()
                .map(|edit| {
                    if edit.size().is_some() {
                        return Err(DialogueCanvasError::UnsupportedGeometry);
                    }
                    let node = facts.domain_node(graph, edit.node()).ok_or_else(missing)?;
                    let position = edit.position().map_or_else(
                        || facts.source().node(node).map(|node| node.position),
                        |position| Some([position.x, position.y]),
                    );
                    Ok(NodePosition {
                        node,
                        position: position.ok_or_else(missing)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DialogueCommand::Move(positions))
        }
        GraphMutationIntent::DeleteEdge(edge) => Ok(DialogueCommand::Remove {
            nodes: Vec::new(),
            links: vec![facts.edge_output(graph, edge.edge()).ok_or_else(missing)?],
        }),
        GraphMutationIntent::DeleteSelection { nodes, edges } => Ok(DialogueCommand::Remove {
            nodes: nodes
                .iter()
                .map(|node| facts.domain_node(graph, node.node()).ok_or_else(missing))
                .collect::<Result<_, _>>()?,
            links: edges
                .iter()
                .map(|edge| facts.edge_output(graph, edge.edge()).ok_or_else(missing))
                .collect::<Result<_, _>>()?,
        }),
        GraphMutationIntent::Connection { binding, target } => {
            let fixed = binding.fixed_port_instance().port();
            match target {
                GraphConnectionTarget::Existing(target) => {
                    let (output, input) = match binding.fixed_endpoint() {
                        EdgeEndpoint::From => (fixed, target.port()),
                        EdgeEndpoint::To => (target.port(), fixed),
                    };
                    let output = facts.output_port(graph, output).ok_or_else(missing)?;
                    let input = facts.input_node(graph, input).ok_or_else(missing)?;
                    match binding.reconnect_edge() {
                        Some(edge) => Ok(DialogueCommand::Reconnect {
                            from: facts.edge_output(graph, edge.edge()).ok_or_else(missing)?,
                            to: output,
                            target: input,
                        }),
                        None => Ok(DialogueCommand::SetTarget {
                            port: output,
                            target: Some(input),
                        }),
                    }
                }
                GraphConnectionTarget::CreateFromWire { target, creation } => {
                    let recipe = creation_recipe(creation)?;
                    let port = creation
                        .ports()
                        .iter()
                        .find_map(|(id, port)| (id == target).then_some(port))
                        .ok_or(DialogueCanvasError::InvalidCreationTarget)?;
                    let endpoint = match binding.fixed_endpoint() {
                        EdgeEndpoint::From if port.dir == PortDirection::In => {
                            WireEndpoint::Output(
                                facts.output_port(graph, fixed).ok_or_else(missing)?,
                            )
                        }
                        EdgeEndpoint::To if port.dir == PortDirection::Out => {
                            WireEndpoint::Input(facts.input_node(graph, fixed).ok_or_else(missing)?)
                        }
                        _ => return Err(DialogueCanvasError::InvalidCreationTarget),
                    };
                    let position = creation.node().pos;
                    Ok(DialogueCommand::CreateFromWire {
                        recipe,
                        position: [position.x, position.y],
                        endpoint,
                        reconnect: binding
                            .reconnect_edge()
                            .map(|edge| facts.edge_output(graph, edge.edge()).ok_or_else(missing))
                            .transpose()?,
                    })
                }
            }
        }
    }
}

fn connection_identity(
    request: &ExternalGraphRequest,
    after: &DialogueFacts,
    outcome: &DialogueCommandOutcome,
) -> Result<Option<ExternalConnectionIdentity>, DialogueCanvasError> {
    let GraphMutationIntent::Connection { binding, target } = request.intent() else {
        return Ok(None);
    };
    let before = request
        .owner_basis::<DialogueFacts>()
        .ok_or(DialogueCanvasError::MissingBasis)?;
    let graph = before.graph().graph_id();
    let fixed = binding.fixed_port_instance().port();
    let (output, opposite, created_node) = match target {
        GraphConnectionTarget::Existing(target) => {
            let output = match binding.fixed_endpoint() {
                EdgeEndpoint::From => before.output_port(graph, fixed),
                EdgeEndpoint::To => before.output_port(graph, target.port()),
            }
            .ok_or(DialogueCanvasError::MissingIdentity)?;
            (output, target.port(), None)
        }
        GraphConnectionTarget::CreateFromWire { target, creation } => {
            let DialogueCommandOutcome::Created {
                node,
                outputs,
                link: Some(link),
            } = outcome
            else {
                return Err(DialogueCanvasError::ResultMismatch);
            };
            creation_recipe(creation)?;
            let mut remaining_outputs = outputs.iter();
            let mut port_mapping = Vec::with_capacity(creation.ports().len());
            for (proposed, port) in creation.ports() {
                let canonical = match port.dir {
                    PortDirection::In => input_id(*node),
                    PortDirection::Out => output_id(
                        *remaining_outputs
                            .next()
                            .ok_or(DialogueCanvasError::ResultMismatch)?,
                    ),
                };
                port_mapping.push((*proposed, canonical));
            }
            if remaining_outputs.next().is_some() {
                return Err(DialogueCanvasError::ResultMismatch);
            }
            let opposite = port_mapping
                .iter()
                .find_map(|(proposed, canonical)| (proposed == target).then_some(*canonical))
                .ok_or(DialogueCanvasError::InvalidCreationTarget)?;
            (
                link.from,
                opposite,
                Some(ExternalCreatedNodeIdentity {
                    node: core_node_id(*node),
                    port_mapping,
                }),
            )
        }
    };
    let canonical = output_id(output);
    let edge = EdgeId(canonical.0);
    let after_graph = after.graph();
    let actual = after_graph
        .edge(&edge)
        .ok_or(DialogueCanvasError::MissingIdentity)?;
    let (expected_output, expected_input): (PortId, PortId) = match binding.fixed_endpoint() {
        EdgeEndpoint::From => (fixed, opposite),
        EdgeEndpoint::To => (opposite, fixed),
    };
    if actual.from != expected_output || actual.to != expected_input {
        return Err(DialogueCanvasError::ResultMismatch);
    }
    Ok(Some(ExternalConnectionIdentity {
        edge,
        target: opposite,
        created_node,
    }))
}

fn creation_recipe(creation: &NodeCreation) -> Result<NodeRecipe, DialogueCanvasError> {
    let recipe = match creation.node().kind.0.as_str() {
        "nara.dialogue.line" => NodeRecipe::Line,
        "nara.dialogue.choice" => NodeRecipe::Choice,
        "nara.dialogue.end" => NodeRecipe::End,
        _ => return Err(DialogueCanvasError::UnsupportedRecipe),
    };
    let inputs = creation
        .ports()
        .iter()
        .filter(|(_, port)| port.dir == PortDirection::In)
        .count();
    let outputs = creation.ports().len() - inputs;
    let expected_outputs = usize::from(recipe != NodeRecipe::End);
    if creation.node().kind_version != 1
        || inputs != 1
        || outputs != expected_outputs
        || creation
            .ports()
            .iter()
            .any(|(_, port)| port.kind != PortKind::Exec || port.ty.is_some())
    {
        return Err(DialogueCanvasError::UnsupportedRecipe);
    }
    Ok(recipe)
}
