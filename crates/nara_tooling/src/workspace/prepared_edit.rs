//! Prepared edits preserve the document owner's candidate and publication authority.

use super::{
    EditorDocumentId, EditorDocumentKind, EditorDocumentSlot, EditorWorkspace,
    EditorWorkspaceCommandReport, WorkspaceDocumentContext, authoring_command_error_report,
    authoring_document_error_report,
    authoring_query::{AuthoringSource, EditorPreparedDocumentPreview, rejected_basis_report},
    report_for_document, report_for_document_patch, workspace_document_error_report,
    workspace_revision_error_report,
};
use crate::{
    EditorDocumentQueryBasis, EditorPrefabComponentEditKind,
    document_query::EditorDocumentQueryOwner,
};
use nara_scene::{
    __private::{
        ObservedPrefabComponentEdit, ObservedPrefabPropertyApply, ObservedPrefabPropertyWrite,
        SourceDocumentInput,
    },
    AuthoringPrefabPropertyClaimKind, AuthoringPropertyEdit, PrefabComponentEdit,
    PrefabComponentSetMode, PrefabDeclarationLimits, PrefabDocument, PrefabInstancePath,
    PrefabRevert, PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit, SceneAuthoringRevision,
    ScenePatchDocument,
};
use std::sync::Arc;

/// A move-only edit of one explicitly named open document; it owns no workspace or selection.
#[derive(Debug)]
pub struct PreparedEditorDocumentEdit {
    basis: EditorDocumentQueryBasis,
    edit: PreparedDocumentEdit,
    origin: PreparedEditOrigin,
    query_owner: EditorDocumentQueryOwner,
}

#[derive(Debug)]
enum PreparedEditOrigin {
    Authoring,
    PrefabRevert,
    PrefabPropertyWrite(AuthoringPrefabPropertyClaimKind),
    PrefabComponentEdit(EditorPrefabComponentEditKind),
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
        action: PrefabRevert<'a>,
        limits: PrefabDeclarationLimits,
    },
    PrefabPropertyWrite(&'a ObservedPrefabPropertyWrite),
    PrefabComponentEdit {
        edit: &'a ObservedPrefabComponentEdit,
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

    #[doc(hidden)]
    #[must_use]
    pub const fn prefab_property_write_kind(&self) -> Option<AuthoringPrefabPropertyClaimKind> {
        match &self.origin {
            PreparedEditOrigin::PrefabPropertyWrite(kind) => Some(*kind),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub const fn prefab_component_edit_kind(&self) -> Option<EditorPrefabComponentEditKind> {
        match &self.origin {
            PreparedEditOrigin::PrefabComponentEdit(kind) => Some(*kind),
            _ => None,
        }
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

    /// Borrows the exact candidate retained for commit, including fields and stored claims.
    #[must_use]
    pub fn preview(&self) -> EditorPreparedDocumentPreview<'_> {
        let source = match &self.edit {
            PreparedDocumentEdit::Scene(edit) => AuthoringSource::PreparedScene(edit),
            PreparedDocumentEdit::Prefab(edit) => AuthoringSource::PreparedPrefab(edit),
        };
        EditorPreparedDocumentPreview::new(&self.basis, &self.query_owner, source)
    }

    #[doc(hidden)]
    pub fn source_input(&self) -> SourceDocumentInput<'_> {
        match &self.edit {
            PreparedDocumentEdit::Scene(edit) => SourceDocumentInput::Scene(edit.document()),
            PreparedDocumentEdit::Prefab(edit) => SourceDocumentInput::Prefab(edit.document()),
        }
    }

    #[doc(hidden)]
    pub fn shared_prefab_document(&self) -> Option<Arc<PrefabDocument>> {
        match &self.edit {
            PreparedDocumentEdit::Scene(_) => None,
            PreparedDocumentEdit::Prefab(edit) => Some(edit.shared_document()),
        }
    }

    #[doc(hidden)]
    pub fn checked_retained_bytes(&self) -> Option<usize> {
        let edit_bytes = match &self.edit {
            PreparedDocumentEdit::Scene(edit) => edit.checked_retained_bytes(),
            PreparedDocumentEdit::Prefab(edit) => edit.checked_retained_bytes(),
        }?;
        edit_bytes
            .checked_add(std::mem::size_of::<Self>() - std::mem::size_of::<PreparedDocumentEdit>())
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
    /// Selected property and component declarations must come from the original live observation.
    /// The source owner lowers the Revert and retains the candidate and inverse consumed by
    /// [`Self::commit_authoring_edit`]; this method adds no publication or history authority.
    pub fn prepare_prefab_revert(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        action: PrefabRevert<'_>,
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

    #[doc(hidden)]
    pub fn prepare_prefab_property_write(
        &self,
        basis: &EditorDocumentQueryBasis,
        edit: &ObservedPrefabPropertyWrite,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        self.prepare_edit(basis, EditRequest::PrefabPropertyWrite(edit))
    }

    #[doc(hidden)]
    pub fn prepare_prefab_component_edit(
        &self,
        basis: &EditorDocumentQueryBasis,
        edit: &ObservedPrefabComponentEdit,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        self.prepare_edit(basis, EditRequest::PrefabComponentEdit { edit, limits })
    }

    #[doc(hidden)]
    pub fn prepare_prefab_property_apply(
        &self,
        basis: &EditorDocumentQueryBasis,
        apply: &ObservedPrefabPropertyApply,
    ) -> Result<Option<PreparedEditorDocumentEdit>, EditorWorkspaceCommandReport> {
        self.validate_document_query_basis(basis)
            .map_err(|error| rejected_basis_report(self, basis, error))?;
        let slot = self.resolve_authoring_preparation_slot(basis.document, basis.revision)?;
        let EditorDocumentSlot::Prefab(prefab) = slot else {
            return Err(authoring_command_error_report(
                self.active_document,
                basis.document,
            ));
        };
        prefab
            .session
            .prepare_prefab_property_apply(basis.revision, apply)
            .map(|edit| {
                edit.map(|edit| PreparedEditorDocumentEdit {
                    basis: basis.clone(),
                    edit: PreparedDocumentEdit::Prefab(edit),
                    origin: PreparedEditOrigin::Authoring,
                    query_owner: EditorDocumentQueryOwner::prepared(),
                })
            })
            .map_err(|report| self.authoring_preparation_error(basis.document, slot, report))
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
            EditRequest::PrefabPropertyWrite(edit) => {
                PreparedEditOrigin::PrefabPropertyWrite(edit.kind())
            }
            EditRequest::PrefabComponentEdit { edit, .. } => {
                PreparedEditOrigin::PrefabComponentEdit(match edit.operation() {
                    PrefabComponentEdit::Set {
                        mode: PrefabComponentSetMode::Add,
                        ..
                    } => EditorPrefabComponentEditKind::Added,
                    PrefabComponentEdit::Set {
                        mode: PrefabComponentSetMode::Replace,
                        ..
                    } => EditorPrefabComponentEditKind::Replaced,
                    PrefabComponentEdit::Suppress => EditorPrefabComponentEditKind::Suppressed,
                    PrefabComponentEdit::Revert => EditorPrefabComponentEditKind::Reverted,
                })
            }
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
                EditRequest::PrefabPropertyWrite(edit) => slot
                    .session
                    .prepare_prefab_property_write(base_revision, edit),
                EditRequest::PrefabComponentEdit { edit, limits } => slot
                    .session
                    .prepare_prefab_component_edit(base_revision, edit, limits),
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
                EditRequest::PrefabPropertyWrite(edit) => slot
                    .session
                    .prepare_prefab_property_write(base_revision, edit),
                EditRequest::PrefabComponentEdit { edit, limits } => slot
                    .session
                    .prepare_prefab_component_edit(base_revision, edit, limits),
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
                query_owner: EditorDocumentQueryOwner::prepared(),
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
