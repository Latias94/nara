//! Typed extension edits and persistence through the original Editor project session.

use super::*;
use nara_engine::tooling::{
    EditorExtensionCommand, EditorExtensionDocument, EditorPersistenceAction,
    EditorPersistenceResult, EditorPersistenceView, EditorReopenTarget,
};
use nara_gpui_engine_consumer::graph::{GraphView, ValueGraph};

use super::super::graph_panel::{GraphIntent, GraphSnapshot};

const SOURCE: &str = "graphs/value-graph.json";

impl Engine {
    pub fn seed_graph_views(&mut self) -> Result<()> {
        use nara_gpui_engine_consumer::graph::{GraphCommand, Link, NodeId, NodeKind};
        for command in [
            GraphCommand::Add {
                kind: NodeKind::Scalar(2.),
                position: [24., 24.],
            },
            GraphCommand::Add {
                kind: NodeKind::Scalar(3.),
                position: [24., 144.],
            },
            GraphCommand::Add {
                kind: NodeKind::Add,
                position: [208., 24.],
            },
            GraphCommand::Add {
                kind: NodeKind::Output,
                position: [392., 24.],
            },
            GraphCommand::Connect(Link {
                from: NodeId(1),
                to: NodeId(3),
                input: 0,
            }),
            GraphCommand::Connect(Link {
                from: NodeId(2),
                to: NodeId(3),
                input: 1,
            }),
            GraphCommand::Connect(Link {
                from: NodeId(3),
                to: NodeId(4),
                input: 0,
            }),
        ] {
            let snapshot = self.graph_snapshot()?;
            self.graph_intent(GraphIntent::Edit {
                document: snapshot.document,
                revision: snapshot.revision,
                command,
            })?;
        }
        Ok(())
    }

    pub fn increment_graph(&mut self) -> Result<()> {
        use nara_gpui_engine_consumer::graph::{GraphCommand, NodeKind};
        let snapshot = self.graph_snapshot()?;
        let node = snapshot
            .view
            .nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::Scalar(_)))
            .context("the graph has no scalar")?;
        let NodeKind::Scalar(value) = node.kind else {
            unreachable!()
        };
        self.graph_intent(GraphIntent::Edit {
            document: snapshot.document,
            revision: snapshot.revision,
            command: GraphCommand::SetScalar {
                node: node.id,
                value: value + 1.,
            },
        })
    }

    pub fn close_graph_document(&mut self) -> Result<()> {
        use nara_engine::tooling::{
            EditorCloseDecision, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
            EditorWorkspaceIntentRequestResult,
        };
        let document = self.graph_document.context("the graph is not open")?;
        let report = dispatch_without_drive(
            &mut self.session,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                EditorWorkspaceIntent::CloseDocument { document },
            )),
        )?;
        ensure!(
            matches!(
                report.outcomes().first(),
                Some(EditorActionOutcome::WorkspaceIntent(
                    EditorWorkspaceIntentRequestResult::Accepted
                ))
            ),
            "the original Host refused graph close"
        );
        self.session.drive_editor_frame(Duration::ZERO);
        if let Some(prompt) = self
            .session
            .current_frame()
            .workspace_intent()
            .decision_prompt()
        {
            ensure!(
                prompt.document() == document,
                "another document owns the close prompt"
            );
            let action =
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard));
            dispatch_without_drive(&mut self.session, action)?;
        }
        self.poll_graph()
    }

    pub fn open_graph(&mut self) -> Result<()> {
        ensure!(self.graph_document.is_none(), "the graph is already open");
        let directory = self._project.path().join("graphs");
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join("value-graph.json"),
            br#"{"nodes":[],"links":[]}"#,
        )?;
        self.session.register_extension_document::<ValueGraph>()?;
        self.graph_document = Some(self.session.open_extension_document::<ValueGraph>(SOURCE)?);
        self.session.drive_editor_frame(Duration::ZERO);
        Ok(())
    }

    pub fn graph_snapshot(&self) -> Result<GraphSnapshot> {
        let document = self.graph_document.context("the graph is not open")?;
        let frame = self.session.current_frame();
        let projection = frame
            .workspace()
            .extension_document(document)
            .context("the graph owner is unavailable")?;
        let view = projection
            .get::<ValueGraph>()
            .context("the graph owner changed")?
            .clone();
        let slot = frame
            .workspace()
            .document(document)
            .context("the graph tab is unavailable")?;
        Ok(GraphSnapshot {
            document,
            revision: projection.revision(),
            view,
            dirty: slot.dirty,
            status: self.graph_status.clone(),
        })
    }

    pub fn graph_intent(&mut self, intent: GraphIntent) -> Result<()> {
        let action = match intent {
            GraphIntent::Edit {
                document,
                revision,
                command,
            } => EditorAction::Extension(EditorExtensionCommand::new::<ValueGraph>(
                document, revision, command,
            )),
            GraphIntent::Undo { document, revision } => {
                EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document,
                    base_revision: revision,
                })
            }
            GraphIntent::Redo { document, revision } => {
                EditorAction::Mutate(EditorDocumentMutation::Redo {
                    document,
                    base_revision: revision,
                })
            }
            GraphIntent::Save { document, revision } => {
                EditorAction::Persistence(EditorPersistenceAction::Save {
                    document,
                    base_revision: revision,
                })
            }
            GraphIntent::Reopen { document, revision } => {
                EditorAction::Persistence(EditorPersistenceAction::Reopen {
                    target: EditorReopenTarget::OpenDocument {
                        document,
                        base_revision: revision,
                    },
                })
            }
        };
        let report = dispatch_without_drive(&mut self.session, action)?;
        self.session.drive_editor_frame(Duration::ZERO);
        let accepted = match report.outcomes().first() {
            Some(EditorActionOutcome::Workspace(report)) => report.applied,
            Some(EditorActionOutcome::Persistence(result)) => {
                if let nara_engine::tooling::EditorPersistenceRequestResult::Accepted { attempt }
                | nara_engine::tooling::EditorPersistenceRequestResult::Rejected {
                    attempt,
                    ..
                } = result
                {
                    self.graph_persistence = Some(*attempt);
                }
                matches!(
                    result,
                    nara_engine::tooling::EditorPersistenceRequestResult::Accepted { .. }
                )
            }
            _ => false,
        };
        if !accepted {
            self.graph_refusals += 1;
            self.graph_status =
                Some("The graph command was refused; review the current document".into());
        } else {
            self.graph_status = None;
        }
        Ok(())
    }

    pub fn poll_graph(&mut self) -> Result<()> {
        if self.graph_document.is_none() {
            return Ok(());
        }
        let frame = self.session.drive_editor_frame(Duration::ZERO);
        if self
            .graph_document
            .is_some_and(|document| frame.workspace().document(document).is_none())
        {
            self.graph_document = None;
            self.graph_status = Some("Graph closed".into());
            return Ok(());
        }
        if let EditorPersistenceView::Terminal { attempt, result } = frame.persistence()
            && self.graph_persistence == Some(attempt)
        {
            self.graph_status = Some(
                match result {
                    EditorPersistenceResult::Saved { .. } => "Graph saved",
                    EditorPersistenceResult::Opened { .. } => "Graph reopened",
                    _ => "Graph persistence did not complete; review the current document",
                }
                .into(),
            );
            dispatch_without_drive(
                &mut self.session,
                EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
            )?;
            self.session.drive_editor_frame(Duration::ZERO);
            self.graph_persistence = None;
        }
        Ok(())
    }

    pub fn graph_persistence_pending(&self) -> bool {
        matches!(
            self.session.current_frame().persistence(),
            EditorPersistenceView::Pending { .. }
        )
    }

    pub fn graph_saved_view(&self) -> Result<GraphView> {
        let bytes = fs::read(self._project.path().join(SOURCE))?;
        Ok(ValueGraph::decode(&bytes)?.view())
    }
}
