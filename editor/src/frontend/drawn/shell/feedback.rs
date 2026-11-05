use nara::tooling::{
    EditorActionAcknowledgement, EditorActionOutcome, EditorActionRejection,
    EditorDocumentOpenPhase, EditorDocumentOpenResult, EditorDocumentOpenView,
    EditorDocumentTabModel, EditorPersistenceFailureStage, EditorPersistenceOperation,
    EditorPersistenceRejection, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorPersistenceView, EditorProjectBrowserActionOutcome, EditorProjectBrowserStatus,
    EditorWorkspaceIntentPhase, EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
    EditorWorkspaceIntentResult, EditorWorkspaceIntentView,
};

use crate::{PanelActionDispatch, PanelActionNotDispatched};

pub(super) fn action_result(result: &PanelActionDispatch) -> (bool, String) {
    match result {
        PanelActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(result)) => {
            match result {
                EditorProjectBrowserActionOutcome::Preparing { request } => (
                    true,
                    format!("Source open request {} accepted", request.get()),
                ),
                EditorProjectBrowserActionOutcome::OpenCancellationRequested { request } => (
                    true,
                    format!(
                        "Source open request {} cancellation requested",
                        request.get()
                    ),
                ),
                EditorProjectBrowserActionOutcome::OpenAcknowledged { request } => (
                    true,
                    format!("Source open request {} acknowledged", request.get()),
                ),
                EditorProjectBrowserActionOutcome::Activated { document } => (
                    true,
                    format!("Source document #{} activated", document.raw()),
                ),
                EditorProjectBrowserActionOutcome::Rejected { .. } => (
                    false,
                    "Source action refused; review the current project source".into(),
                ),
                EditorProjectBrowserActionOutcome::RefreshRequested => {
                    (true, "Project source refresh requested".into())
                }
                EditorProjectBrowserActionOutcome::RefreshAlreadyPending => {
                    (true, "Project source refresh is already pending".into())
                }
            }
        }
        PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) => {
            let message = if report.applied {
                report.document.map_or_else(
                    || "Document action applied".into(),
                    |document| format!("Document #{} action applied", document.raw()),
                )
            } else if let Some(diagnostic) = report.diagnostics.iter().next() {
                format!(
                    "Action refused: {}",
                    super::bounded_text(diagnostic.summary().as_str(), 72)
                )
            } else {
                "The document action made no change".into()
            };
            (report.applied, message)
        }
        PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(result)) => match result {
            EditorPersistenceRequestResult::Accepted { attempt } => (
                true,
                format!("Persistence attempt {} accepted", attempt.get()),
            ),
            EditorPersistenceRequestResult::Acknowledged { attempt } => (
                true,
                format!("Persistence attempt {} acknowledged", attempt.get()),
            ),
            EditorPersistenceRequestResult::Rejected { attempt, reason } => (
                false,
                format!(
                    "Attempt {} refused: {}",
                    attempt.get(),
                    persistence_rejection(*reason)
                ),
            ),
            EditorPersistenceRequestResult::Refused(reason) => (
                false,
                format!("Persistence refused: {}", persistence_rejection(*reason)),
            ),
        },
        PanelActionDispatch::Dispatched(EditorActionOutcome::WorkspaceIntent(result)) => {
            match result {
                EditorWorkspaceIntentRequestResult::Accepted => {
                    (true, "Workspace close action accepted".into())
                }
                EditorWorkspaceIntentRequestResult::Rejected(reason) => (
                    false,
                    format!("Close refused: {}", workspace_rejection(*reason)),
                ),
            }
        }
        PanelActionDispatch::Dispatched(EditorActionOutcome::Acknowledged(
            EditorActionAcknowledgement::WorkspaceIntent(acknowledged),
        )) => (
            *acknowledged,
            if *acknowledged {
                "Workspace close result acknowledged".into()
            } else {
                "No workspace close result was acknowledged".into()
            },
        ),
        PanelActionDispatch::Dispatched(EditorActionOutcome::Rejected(reason)) => {
            (false, action_rejection(*reason).into())
        }
        PanelActionDispatch::NotDispatched(reason) => (
            false,
            match reason {
                PanelActionNotDispatched::Superseded => "The original panel was replaced",
                PanelActionNotDispatched::ProjectChanged => "The project changed before dispatch",
                PanelActionNotDispatched::Closing => "The workspace is closing",
                PanelActionNotDispatched::NoPublishedFrame => "No Editor frame is published",
                PanelActionNotDispatched::PresentationFailed => {
                    "The original controls were not presented"
                }
                PanelActionNotDispatched::BatchRejected(reason) => action_rejection(*reason),
            }
            .into(),
        ),
        _ => (
            false,
            "The toolbar received an unexpected action result".into(),
        ),
    }
}

pub(super) fn source_status(status: EditorProjectBrowserStatus) -> &'static str {
    match status {
        EditorProjectBrowserStatus::NotRequested => "Project sources not requested",
        EditorProjectBrowserStatus::Scanning => "Scanning project sources",
        EditorProjectBrowserStatus::Ready => "Project sources ready",
        EditorProjectBrowserStatus::ReadyWithDiagnostics => "Project sources need attention",
        EditorProjectBrowserStatus::Unavailable => "Project sources unavailable",
    }
}

pub(super) fn open_status(view: &EditorDocumentOpenView) -> String {
    if let Some((request, _, phase)) = view.pending() {
        return match phase {
            EditorDocumentOpenPhase::Preparing => {
                format!("Opening source (request {})", request.get())
            }
            EditorDocumentOpenPhase::Cancelling => {
                format!("Cancelling source open (request {})", request.get())
            }
        };
    }
    match view.result() {
        Some(EditorDocumentOpenResult::Opened { request, document }) => {
            format!(
                "Opened document #{} (request {})",
                document.raw(),
                request.get()
            )
        }
        Some(EditorDocumentOpenResult::Rejected { request, .. }) => {
            format!(
                "Source open refused (request {}); review the project source",
                request.get()
            )
        }
        Some(EditorDocumentOpenResult::Cancelled { request }) => {
            format!("Source open cancelled (request {})", request.get())
        }
        None => "Choose a source to open".into(),
    }
}

pub(super) fn persistence_status(view: EditorPersistenceView) -> String {
    match view {
        EditorPersistenceView::Idle => "Persistence idle".into(),
        EditorPersistenceView::Pending { attempt, operation } => match operation {
            EditorPersistenceOperation::Saving { document, .. } => {
                format!(
                    "Saving document #{} (attempt {})",
                    document.raw(),
                    attempt.get()
                )
            }
            EditorPersistenceOperation::Opening { document } => {
                format!(
                    "Reopening document #{} (attempt {})",
                    document.raw(),
                    attempt.get()
                )
            }
        },
        EditorPersistenceView::Terminal { attempt, result } => {
            let result = match result {
                EditorPersistenceResult::Saved { document, .. } => {
                    format!("Saved document #{}", document.raw())
                }
                EditorPersistenceResult::Opened { document, .. } => {
                    format!("Reopened document #{}", document.raw())
                }
                EditorPersistenceResult::Rejected { reason, .. } => {
                    format!("Persistence refused: {}", persistence_rejection(reason))
                }
                EditorPersistenceResult::Failed { stage, .. } => {
                    format!("Persistence failed: {}", persistence_stage(stage))
                }
                EditorPersistenceResult::PersistenceUncertain { document, .. } => {
                    format!(
                        "Save unconfirmed; reopen document #{} to reconcile",
                        document.raw()
                    )
                }
            };
            format!("{result} (attempt {})", attempt.get())
        }
    }
}

pub(super) fn workspace_status(
    view: &EditorWorkspaceIntentView,
    documents: &[EditorDocumentTabModel],
) -> String {
    if let Some(phase) = view.phase() {
        return match phase {
            EditorWorkspaceIntentPhase::AwaitingDecision => view.decision_prompt().map_or_else(
                || "Waiting for a close decision".into(),
                |prompt| {
                    let title = documents
                        .iter()
                        .find(|document| document.document == prompt.document())
                        .map_or_else(
                            || "Original document unavailable".into(),
                            |document| super::bounded_text(&document.title, 24),
                        );
                    format!("Save before closing #{} {title}?", prompt.document().raw())
                },
            ),
            EditorWorkspaceIntentPhase::Saving => "Saving before closing".into(),
            EditorWorkspaceIntentPhase::RetiringRuntime => {
                "Stopping the game before closing".into()
            }
            EditorWorkspaceIntentPhase::Prepared => {
                "Close prepared; waiting for the workspace owner".into()
            }
        };
    }
    match view.result() {
        Some(EditorWorkspaceIntentResult::Applied { .. }) => "Workspace close applied".into(),
        Some(EditorWorkspaceIntentResult::Cancelled { .. }) => "Workspace close cancelled".into(),
        Some(EditorWorkspaceIntentResult::Rejected { reason, .. }) => {
            format!("Close refused: {}", workspace_rejection(reason))
        }
        None => "Workspace ready".into(),
    }
}

fn persistence_rejection(reason: EditorPersistenceRejection) -> &'static str {
    match reason {
        EditorPersistenceRejection::Busy => "another persistence operation is active",
        EditorPersistenceRejection::ResultPending => "acknowledge the previous persistence result",
        EditorPersistenceRejection::ResultMismatch => "the persistence attempt changed",
        EditorPersistenceRejection::AttemptIdExhausted => {
            "persistence attempt identities are exhausted"
        }
        EditorPersistenceRejection::NoActiveDocument => "select a document first",
        EditorPersistenceRejection::MissingDocument => "the original document is no longer open",
        EditorPersistenceRejection::DocumentKindMismatch => "the document kind changed",
        EditorPersistenceRejection::SourceBindingMismatch => "the source binding changed",
        EditorPersistenceRejection::NotDirty => "the document has no unsaved changes",
        EditorPersistenceRejection::StaleRevision => {
            "the document changed; use the current controls"
        }
        EditorPersistenceRejection::TargetChanged => "the saved source changed outside the Editor",
        EditorPersistenceRejection::TargetDeleted => "the saved source was deleted",
        EditorPersistenceRejection::LockUnavailable => "the source lock is unavailable",
        EditorPersistenceRejection::UnsupportedGuarantee => {
            "the source cannot provide the required write guarantees"
        }
        EditorPersistenceRejection::RequiresReconcile => {
            "reopen the original document to reconcile its source"
        }
        EditorPersistenceRejection::RuntimeActive => {
            "stop the game before persisting this document"
        }
        EditorPersistenceRejection::PreservationMismatch => {
            "source preservation could not be confirmed"
        }
        EditorPersistenceRejection::CleanupPending => "source cleanup is still pending",
    }
}

fn persistence_stage(stage: EditorPersistenceFailureStage) -> &'static str {
    match stage {
        EditorPersistenceFailureStage::Encode => "could not encode the document",
        EditorPersistenceFailureStage::OpenTarget => "could not open the source",
        EditorPersistenceFailureStage::ReadTarget => "could not read the source",
        EditorPersistenceFailureStage::CreateTemporary => "could not create the temporary source",
        EditorPersistenceFailureStage::WriteTemporary => "could not write the temporary source",
        EditorPersistenceFailureStage::SyncTemporary => {
            "could not synchronize the temporary source"
        }
        EditorPersistenceFailureStage::ReplaceTarget => "could not replace the source",
        EditorPersistenceFailureStage::Decode => "could not decode the source",
        EditorPersistenceFailureStage::Validate => "source validation failed",
    }
}

fn workspace_rejection(reason: EditorWorkspaceIntentRejection) -> &'static str {
    match reason {
        EditorWorkspaceIntentRejection::Busy => "another workspace operation is active",
        EditorWorkspaceIntentRejection::ResultPending => "acknowledge the previous close result",
        EditorWorkspaceIntentRejection::NoActiveDocument => "select a document first",
        EditorWorkspaceIntentRejection::MissingDocument => {
            "the original document is no longer open"
        }
        EditorWorkspaceIntentRejection::DecisionNotRequired => "no close decision is required",
        EditorWorkspaceIntentRejection::StalePrompt => "the original close decision changed",
        EditorWorkspaceIntentRejection::PersistenceRejected(reason) => {
            persistence_rejection(reason)
        }
    }
}

fn action_rejection(reason: EditorActionRejection) -> &'static str {
    match reason {
        EditorActionRejection::NoPublishedFrame => "No Editor frame is published",
        EditorActionRejection::StaleFrame { .. } => "The Editor frame changed before dispatch",
        EditorActionRejection::DocumentNotInFrame { .. } => {
            "The original document is absent from this frame"
        }
        EditorActionRejection::StaleDocumentFact { .. } => "The original document revision changed",
        EditorActionRejection::ActiveDocumentChanged { .. } => "The active document changed",
        EditorActionRejection::FieldContractMismatch { .. } => {
            "The original property contract changed"
        }
        EditorActionRejection::StructureContractMismatch { .. } => {
            "The original document structure changed"
        }
        EditorActionRejection::RuntimeGenerationChanged { .. } => "The game instance changed",
        EditorActionRejection::RuntimeDocumentChanged { .. } => "The game document changed",
        EditorActionRejection::InvalidReopenTarget => "The original reopen target is unavailable",
    }
}
