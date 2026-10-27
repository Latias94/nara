//! Prepared edits preserve the document owner's candidate and publication authority.

use super::{
    EditorDocumentId, EditorDocumentKind, EditorDocumentSlot, EditorWorkspace,
    EditorWorkspaceCommandReport, WorkspaceDocumentContext, authoring_command_error_report,
    authoring_document_error_report, prefab_declarations::rejected_basis_report,
    report_for_document, report_for_document_patch, workspace_document_error_report,
    workspace_revision_error_report,
};
use crate::EditorDocumentQueryBasis;
use nara_scene::{
    AuthoringPropertyEdit, PrefabDeclarationLimits, PrefabInstancePath, PrefabRevert,
    PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit, SceneAuthoringRevision,
    ScenePatchDocument,
};

/// A move-only edit of one explicitly named open document; it owns no workspace or selection.
#[derive(Debug)]
pub struct PreparedEditorDocumentEdit {
    document: EditorDocumentId,
    edit: PreparedDocumentEdit,
    origin: PreparedEditOrigin,
}

#[derive(Debug)]
enum PreparedEditOrigin {
    Authoring,
    PrefabRevert,
}

#[derive(Debug)]
enum PreparedDocumentEdit {
    Scene(PreparedSceneAuthoringEdit),
    Prefab(PreparedPrefabAuthoringEdit),
}

enum EditRequest<'a> {
    Patch(&'a ScenePatchDocument),
    Properties(&'a [AuthoringPropertyEdit]),
    PrefabRevert {
        path: &'a PrefabInstancePath,
        action: PrefabRevert,
        limits: PrefabDeclarationLimits,
    },
}

impl PreparedEditorDocumentEdit {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }

    /// Identifies a Revert lowered by the source owner's dedicated preparation API.
    ///
    /// A generic patch with identical operations does not carry this origin.
    #[must_use]
    pub const fn is_prefab_revert(&self) -> bool {
        matches!(self.origin, PreparedEditOrigin::PrefabRevert)
    }

    #[must_use]
    pub fn document_kind(&self) -> EditorDocumentKind {
        match self.edit {
            PreparedDocumentEdit::Scene(_) => EditorDocumentKind::Scene,
            PreparedDocumentEdit::Prefab(_) => EditorDocumentKind::Prefab,
        }
    }

    #[must_use]
    pub fn base_revision(&self) -> SceneAuthoringRevision {
        match &self.edit {
            PreparedDocumentEdit::Scene(edit) => edit.base_revision(),
            PreparedDocumentEdit::Prefab(edit) => edit.base_revision(),
        }
    }

    #[must_use]
    pub fn patch(&self) -> &ScenePatchDocument {
        match &self.edit {
            PreparedDocumentEdit::Scene(edit) => edit.patch(),
            PreparedDocumentEdit::Prefab(edit) => edit.patch(),
        }
    }

    #[must_use]
    pub fn inverse(&self) -> &ScenePatchDocument {
        match &self.edit {
            PreparedDocumentEdit::Scene(edit) => edit.inverse(),
            PreparedDocumentEdit::Prefab(edit) => edit.inverse(),
        }
    }
}

impl EditorWorkspace {
    /// Prepares a data-authoring edit without changing navigation, selection or saved state.
    pub fn prepare_authoring_edit(
        &self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        patch: &ScenePatchDocument,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        self.prepare_edit(document, base_revision, EditRequest::Patch(patch))
    }

    /// Prepares ordered stable-property edits without making the requested document active.
    pub fn prepare_authoring_property_edit(
        &self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        edits: &[AuthoringPropertyEdit],
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        self.prepare_edit(document, base_revision, EditRequest::Properties(edits))
    }

    /// Prepares one source-owned Revert without activating its document or changing selection.
    ///
    /// Snapshot selectors require the exact incarnation, revision and Schema admission.
    /// The source owner lowers the Revert and retains the candidate and inverse consumed by
    /// [`Self::commit_authoring_edit`]; this method adds no publication or history authority.
    pub fn prepare_prefab_revert(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        action: PrefabRevert,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        self.validate_document_query_basis(basis)
            .map_err(|error| rejected_basis_report(self, basis, error))?;
        self.prepare_edit(
            basis.document,
            basis.revision,
            EditRequest::PrefabRevert {
                path,
                action,
                limits,
            },
        )
    }

    fn prepare_edit(
        &self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        request: EditRequest<'_>,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        let slot = self.resolve_authoring_preparation_slot(document, base_revision)?;
        let origin = match &request {
            EditRequest::PrefabRevert { .. } => PreparedEditOrigin::PrefabRevert,
            EditRequest::Patch(_) | EditRequest::Properties(_) => PreparedEditOrigin::Authoring,
        };
        let prepared = match slot {
            EditorDocumentSlot::Scene(slot) => match request {
                EditRequest::Patch(patch) => {
                    slot.session.prepare_authoring_edit(base_revision, patch)
                }
                EditRequest::Properties(edits) => slot
                    .session
                    .prepare_authoring_property_edit(base_revision, edits),
                EditRequest::PrefabRevert {
                    path,
                    action,
                    limits,
                } => slot
                    .session
                    .prepare_prefab_revert(base_revision, path, action, limits),
            }
            .map(PreparedDocumentEdit::Scene),
            EditorDocumentSlot::Prefab(slot) => match request {
                EditRequest::Patch(patch) => {
                    slot.session.prepare_authoring_edit(base_revision, patch)
                }
                EditRequest::Properties(edits) => slot
                    .session
                    .prepare_authoring_property_edit(base_revision, edits),
                EditRequest::PrefabRevert {
                    path,
                    action,
                    limits,
                } => slot
                    .session
                    .prepare_prefab_revert(base_revision, path, action, limits),
            }
            .map(PreparedDocumentEdit::Prefab),
            EditorDocumentSlot::Extension(_) => {
                return Err(authoring_command_error_report(
                    self.active_document,
                    document,
                ));
            }
        };
        prepared
            .map(|edit| PreparedEditorDocumentEdit {
                document,
                edit,
                origin,
            })
            .map_err(|patch_report| self.authoring_preparation_error(document, slot, patch_report))
    }

    pub(super) fn resolve_authoring_preparation_slot(
        &self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> Result<&EditorDocumentSlot, EditorWorkspaceCommandReport> {
        let context = WorkspaceDocumentContext::new(self.active_document, Some(document));
        let Some(slot) = self.documents.get(&document) else {
            return Err(workspace_document_error_report(context, document));
        };
        if slot.revision() != base_revision {
            return Err(workspace_revision_error_report(
                context,
                document,
                base_revision,
                slot.revision(),
            ));
        }
        if slot.authoring_readiness().is_none() {
            return Err(authoring_document_error_report(
                self.active_document,
                document,
            ));
        }
        Ok(slot)
    }

    pub(super) fn authoring_preparation_error(
        &self,
        document: EditorDocumentId,
        slot: &EditorDocumentSlot,
        patch_report: nara_scene::ScenePatchReport,
    ) -> EditorWorkspaceCommandReport {
        let mut report = report_for_document(document, self.active_document, slot);
        report.applied = false;
        let _ = report.diagnostics.extend(patch_report.diagnostics.clone());
        report.patch_report = Some(patch_report);
        report
    }

    /// Commits only to the original document/session and retains ordinary selection repair.
    pub fn commit_authoring_edit(
        &mut self,
        prepared: PreparedEditorDocumentEdit,
    ) -> EditorWorkspaceCommandReport {
        let document = prepared.document();
        let (active, slot) = match self
            .resolve_exact_authoring_document_mutation_slot(document, prepared.base_revision())
        {
            Ok(exact) => exact,
            Err(report) => return report,
        };
        let report = match (&mut *slot, prepared.edit) {
            (EditorDocumentSlot::Scene(slot), PreparedDocumentEdit::Scene(edit)) => {
                slot.session.commit_authoring_edit(edit)
            }
            (EditorDocumentSlot::Prefab(slot), PreparedDocumentEdit::Prefab(edit)) => {
                slot.session.commit_authoring_edit(edit)
            }
            _ => return authoring_command_error_report(active, document),
        };
        report_for_document_patch(document, active, slot, report)
    }
}
