//! Prepared edits preserve the document owner's candidate and publication authority.

use super::{
    EditorDocumentId, EditorDocumentKind, EditorDocumentSlot, EditorWorkspace,
    EditorWorkspaceCommandReport, WorkspaceDocumentContext, authoring_command_error_report,
    authoring_document_error_report,
    authoring_query::{PropertySource, read_property_page, rejected_basis_report},
    report_for_document, report_for_document_patch, workspace_document_error_report,
    workspace_revision_error_report,
};
use crate::{
    EditorDocumentQueryBasis, EditorDocumentQueryRejection, EditorPropertyObservation,
    EditorPropertyPage, EditorPropertyPageRead, EditorPropertyRead,
    authoring_query::PropertyValuePageOwner, document_query::EditorDocumentQueryOwner,
};
use nara_scene::{
    AuthoringEntityView, AuthoringPropertyEdit, AuthoringReadError, PrefabDeclarationLimits,
    PrefabInstancePath, PrefabRevert, PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit,
    SceneAuthoringRevision, SceneEntityId, ScenePatchDocument,
};

/// A move-only edit of one explicitly named open document; it owns no workspace or selection.
#[derive(Debug)]
pub struct PreparedEditorDocumentEdit {
    basis: EditorDocumentQueryBasis,
    edit: PreparedDocumentEdit,
    origin: PreparedEditOrigin,
    page_owner: EditorDocumentQueryOwner,
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
        self.basis.document
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
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.basis.revision
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

    /// Reads a proposed property from the exact candidate retained for commit.
    ///
    /// Use the original preparation basis in the request. The returned target refers to that
    /// basis and the value comes from this candidate. An intervening live edit does not change
    /// the preview or grant permission to commit it.
    pub fn preview_property(
        &self,
        request: EditorPropertyRead,
    ) -> Result<EditorPropertyObservation, EditorDocumentQueryRejection> {
        self.validate_preview_basis(&request.target.basis)?;
        PropertySource::Prepared(self).read_property(request)
    }

    /// Reads one bounded value page from the candidate. Continue on this same prepared edit.
    pub fn preview_property_page(
        &self,
        request: EditorPropertyPageRead,
    ) -> Result<EditorPropertyPage, EditorDocumentQueryRejection> {
        read_property_page(
            request,
            PropertyValuePageOwner::Prepared(self.page_owner.clone()),
            |basis, _| {
                self.validate_preview_basis(basis)?;
                Ok(PropertySource::Prepared(self))
            },
        )
    }

    fn validate_preview_basis(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<(), EditorDocumentQueryRejection> {
        if basis.document != self.document() {
            return Err(EditorDocumentQueryRejection::WrongPreparedEdit);
        }
        basis.require_exact_source(self.base_revision(), || self.basis.schema.clone())
    }

    pub(super) fn authoring_entity_view(
        &self,
        entity: &SceneEntityId,
    ) -> Result<AuthoringEntityView<'_>, AuthoringReadError> {
        match &self.edit {
            PreparedDocumentEdit::Scene(edit) => edit.authoring_entity_view(entity),
            PreparedDocumentEdit::Prefab(edit) => edit.authoring_entity_view(entity),
        }
    }
}

impl EditorWorkspace {
    /// Prepares an edit against the complete retained observation without changing navigation.
    pub fn prepare_authoring_edit(
        &self,
        basis: &EditorDocumentQueryBasis,
        patch: &ScenePatchDocument,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        self.prepare_edit(basis, EditRequest::Patch(patch))
    }

    /// Prepares ordered stable-property edits without making the requested document active.
    pub fn prepare_authoring_property_edit(
        &self,
        basis: &EditorDocumentQueryBasis,
        edits: &[AuthoringPropertyEdit],
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        self.prepare_edit(basis, EditRequest::Properties(edits))
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
        self.prepare_edit(
            basis,
            EditRequest::PrefabRevert {
                path,
                action,
                limits,
            },
        )
    }

    fn prepare_edit(
        &self,
        basis: &EditorDocumentQueryBasis,
        request: EditRequest<'_>,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        self.validate_document_query_basis(basis)
            .map_err(|error| rejected_basis_report(self, basis, error))?;
        let document = basis.document;
        let base_revision = basis.revision;
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
                basis: basis.clone(),
                edit,
                origin,
                page_owner: EditorDocumentQueryOwner::default(),
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
