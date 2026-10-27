//! Selection-independent access to declarations owned by an exact source session.

use nara_scene::{PrefabDeclarationLimits, PrefabInstanceObservation, PrefabInstancePath};

use super::{
    EditorDocumentSlot, EditorWorkspace, EditorWorkspaceCommandReport, WorkspaceDocumentContext,
    authoring_command_error_report, workspace_document_error_report, workspace_error_report,
};
use crate::{EditorDocumentQueryBasis, EditorDocumentQueryRejection};

impl EditorWorkspace {
    /// Observes an optional source declaration without changing navigation or selection.
    ///
    /// The exact query basis is checked before any selector or payload is read. The source
    /// session owns bounded copying; this does not resolve inherited entities or Prefab sources.
    pub fn observe_prefab_instance(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        limits: PrefabDeclarationLimits,
    ) -> Result<PrefabInstanceObservation, EditorDocumentQueryRejection> {
        let slot = self.resolve_document_query_slot(basis, None)?;
        let observation = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.observe_prefab_instance(path, limits),
            EditorDocumentSlot::Prefab(slot) => slot.session.observe_prefab_instance(path, limits),
            EditorDocumentSlot::Extension(_) => {
                return Err(EditorDocumentQueryRejection::UnsupportedDocumentKind {
                    kind: slot.kind(),
                });
            }
        };
        observation.map_err(EditorDocumentQueryRejection::PrefabDeclaration)
    }
}

pub(super) fn rejected_basis_report(
    workspace: &EditorWorkspace,
    basis: &EditorDocumentQueryBasis,
    rejection: EditorDocumentQueryRejection,
) -> EditorWorkspaceCommandReport {
    let context = WorkspaceDocumentContext::new(workspace.active_document, Some(basis.document));
    let (code, summary) = match rejection {
        EditorDocumentQueryRejection::DocumentNotOpen { document } => {
            return workspace_document_error_report(context, document);
        }
        EditorDocumentQueryRejection::UnsupportedDocumentKind { .. } => {
            return authoring_command_error_report(workspace.active_document, basis.document);
        }
        EditorDocumentQueryRejection::StaleIncarnation => (
            "tooling.workspace-stale-document-incarnation",
            "Prefab Revert targets a different source incarnation",
        ),
        EditorDocumentQueryRejection::StaleRevision => (
            "tooling.workspace-stale-document-revision",
            "Prefab Revert targets a stale document revision",
        ),
        EditorDocumentQueryRejection::StaleSchema => (
            "tooling.workspace-stale-schema-basis",
            "Prefab Revert requires the exact current Schema admission",
        ),
        _ => (
            "tooling.workspace-prefab-revert-basis-rejected",
            "Prefab Revert requires a current Scene or Prefab document query basis",
        ),
    };
    workspace_error_report(context, code, summary)
}
