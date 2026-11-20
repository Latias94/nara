//! Public prepared authoring keeps document publication in its existing owner.

use nara_scene::{
    AuthoringConstructionDraft, AuthoringPropertyEdit, PrefabDeclarationLimits,
    PrefabInstanceObservation, PrefabInstancePath, PrefabPropertyIntent, PrefabRevert,
    ScenePatchDocument, ScenePatchOperation,
};
use nara_tooling::{
    CommittedEditorDocumentConstruction, EditorActionOutcome, EditorActionRejection,
    EditorDocumentId, EditorDocumentKind, EditorDocumentQueryBasis, EditorDocumentQueryRejection,
    EditorPrefabPropertyClaimTarget, EditorPropertyReadLimits, EditorWorkspaceCommandReport,
    PreparedEditorDocumentConstruction, PreparedEditorDocumentEdit,
};

use super::{EditorProjectSession, editor_workspace_command_rejected};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditorEditPreparationError {
    #[error("no Editor workspace is available")]
    WorkspaceUnavailable,
    #[error("the Editor is closing or resolving a workspace intent")]
    Closing,
    #[error("a whole-Session Schema adoption is pending")]
    Busy,
    #[error("the edit requires its specialized document-domain contract")]
    StructureContractMismatch { document: EditorDocumentId },
    #[error("the captured document-query basis is no longer admitted: {0}")]
    QueryBasis(EditorDocumentQueryRejection),
    #[error("the document owner rejected edit preparation")]
    Document(Box<EditorWorkspaceCommandReport>),
}

impl EditorProjectSession {
    /// Captures the exact basis for a named open Scene or Prefab without changing navigation.
    pub fn document_query_basis(
        &self,
        document: EditorDocumentId,
    ) -> Result<EditorDocumentQueryBasis, EditorDocumentQueryRejection> {
        self.workspace.document_query_basis(document)
    }

    /// Runs a bounded selection-independent entity query through the owning workspace.
    pub fn query_document_entities(
        &self,
        query: nara_tooling::EditorEntityQuery,
    ) -> Result<nara_tooling::EditorEntityPage, EditorDocumentQueryRejection> {
        self.workspace.query_document_entities(query)
    }

    /// Runs a bounded selection-independent property or component query through the owner.
    pub fn query_document_authoring(
        &self,
        query: nara_tooling::EditorAuthoringQuery,
    ) -> Result<nara_tooling::EditorAuthoringPage, EditorDocumentQueryRejection> {
        self.workspace.query_document_authoring(query)
    }

    /// Reads a named source property through the same owner and basis as action-based queries.
    /// This does not mutate navigation, reserve a revision or grant preparation authority.
    pub fn read_document_property(
        &self,
        request: nara_tooling::EditorPropertyRead,
    ) -> Result<nara_tooling::EditorPropertyObservation, EditorDocumentQueryRejection> {
        self.workspace.read_document_property(request)
    }

    /// Reads one bounded page of a source property through the owning workspace.
    /// Returns `Closing` while the Session is closing or resolving a workspace intent.
    pub fn read_document_property_page(
        &self,
        request: nara_tooling::EditorPropertyPageRead,
    ) -> Result<nara_tooling::EditorPropertyPage, EditorDocumentQueryRejection> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorDocumentQueryRejection::Closing);
        }
        self.workspace.read_document_property_page(request)
    }

    /// Reads one bounded stored Prefab property claim at the original document basis.
    pub fn read_prefab_property_claim(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
        limits: EditorPropertyReadLimits,
    ) -> Result<PrefabPropertyIntent, EditorDocumentQueryRejection> {
        self.workspace.read_prefab_property_claim(claim, limits)
    }

    /// Prepares Revert of a discovered claim through its original source owner.
    pub fn prepare_prefab_property_revert(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        self.admit_authoring_preparation()?;
        self.workspace
            .validate_prefab_property_claim_target(claim)
            .map_err(EditorEditPreparationError::QueryBasis)?;
        self.workspace
            .prepare_prefab_property_revert(claim, limits)
            .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))
    }

    /// Reads stored instance intent without requiring a successful composition or selection.
    pub fn observe_prefab_instance(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        limits: PrefabDeclarationLimits,
    ) -> Result<PrefabInstanceObservation, EditorDocumentQueryRejection> {
        self.workspace.observe_prefab_instance(basis, path, limits)
    }

    /// Removes selected source intent through its existing prepared transaction owner.
    ///
    /// This narrowly admitted repair also works for UI Prefabs whose composition cannot be
    /// activated. The immutable candidate origin distinguishes it from arbitrary structural edits.
    pub fn prepare_prefab_revert(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        action: PrefabRevert,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        self.validate_authoring_query_basis(basis)?;
        self.workspace
            .prepare_prefab_revert(basis, path, action, limits)
            .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))
    }

    /// Validates a retained observation for tool reads or results without refreshing it.
    ///
    /// Preparation entries already perform this check. Concrete tools may also need it before
    /// interpreting external results or accepting a reviewed result; commit independently checks
    /// its actual candidate and current lifecycle.
    pub fn validate_authoring_query_basis(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<(), EditorEditPreparationError> {
        self.admit_authoring_preparation()?;
        self.workspace
            .validate_document_query_basis(basis)
            .map_err(EditorEditPreparationError::QueryBasis)
    }

    fn admit_authoring_preparation(&self) -> Result<(), EditorEditPreparationError> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorEditPreparationError::Closing);
        }
        if self.authoring_operation_fenced() {
            return Err(EditorEditPreparationError::Busy);
        }
        Ok(())
    }

    /// Captures intent at the complete retained observation; no IDs or state are published.
    pub fn begin_authoring_construction(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<AuthoringConstructionDraft, EditorEditPreparationError> {
        self.validate_authoring_query_basis(basis)?;
        self.workspace
            .begin_authoring_construction(basis)
            .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))
    }

    /// Keeps specialized Prefab admission after lowering, without a second transaction executor.
    pub fn prepare_authoring_construction(
        &self,
        basis: &EditorDocumentQueryBasis,
        draft: &AuthoringConstructionDraft,
    ) -> Result<PreparedEditorDocumentConstruction, EditorEditPreparationError> {
        self.validate_authoring_query_basis(basis)?;
        let document = basis.document;
        let prepared = self
            .workspace
            .prepare_authoring_construction(basis, draft)
            .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))?;
        self.verify_prepared_patch_contract(document, prepared.patch())
            .map_err(|_| EditorEditPreparationError::StructureContractMismatch { document })?;
        Ok(prepared)
    }

    /// A successful receipt is returned only after source publication; rejection retains its outcome.
    pub fn commit_authoring_construction(
        &mut self,
        prepared: PreparedEditorDocumentConstruction,
    ) -> Result<CommittedEditorDocumentConstruction, EditorActionOutcome> {
        if self.authoring_operation_fenced() {
            return Err(EditorActionOutcome::Workspace(
                editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.schema-adoption-busy",
                ),
            ));
        }
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorActionOutcome::Workspace(
                editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.workspace-intent-pending",
                ),
            ));
        }
        self.verify_prepared_patch_contract(prepared.document(), prepared.patch())
            .map_err(EditorActionOutcome::Rejected)?;
        match self.workspace.commit_authoring_construction(prepared) {
            Ok(committed) => {
                let _ = self
                    .diagnostics
                    .extend(committed.report().diagnostics.clone());
                Ok(committed)
            }
            Err(report) => {
                let _ = self.diagnostics.extend(report.diagnostics.clone());
                Err(EditorActionOutcome::Workspace(report))
            }
        }
    }

    /// Prepares properties against the retained source/Schema observation, without navigation.
    pub fn prepare_authoring_property_edit(
        &self,
        basis: &EditorDocumentQueryBasis,
        edits: &[AuthoringPropertyEdit],
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        #[cfg(feature = "editor-work-observation")]
        let work_span = tracing::info_span!(
            target: "nara::editor::work",
            "editor_property_edit_prepare",
            document = basis.document.raw(),
            source = tracing::field::Empty,
            revision = basis.revision.generation(),
            current_revision = tracing::field::Empty,
            outcome = "pending",
        )
        .entered();
        let result = (|| {
            self.validate_authoring_query_basis(basis)?;
            let document = basis.document;
            let prepared = self
                .workspace
                .prepare_authoring_property_edit(basis, edits)
                .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))?;
            self.verify_prepared_patch_contract(document, prepared.patch())
                .map_err(|_| EditorEditPreparationError::StructureContractMismatch { document })?;
            Ok(prepared)
        })();
        #[cfg(feature = "editor-work-observation")]
        work_span.record(
            "outcome",
            match &result {
                Ok(_) => "prepared",
                Err(error) => editor_edit_preparation_outcome(error),
            },
        );
        result
    }

    /// Prepares an edit from the complete original observation without refreshing its basis.
    pub fn prepare_authoring_edit(
        &self,
        basis: &EditorDocumentQueryBasis,
        patch: &ScenePatchDocument,
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        #[cfg(feature = "editor-work-observation")]
        let work_span = tracing::info_span!(
            target: "nara::editor::work",
            "editor_edit_prepare",
            document = basis.document.raw(),
            source = tracing::field::Empty,
            revision = basis.revision.generation(),
            current_revision = tracing::field::Empty,
            outcome = "pending",
        )
        .entered();
        let result = (|| {
            self.validate_authoring_query_basis(basis)?;
            let document = basis.document;
            self.verify_prepared_patch_contract(document, patch)
                .map_err(|_| EditorEditPreparationError::StructureContractMismatch { document })?;
            self.workspace
                .prepare_authoring_edit(basis, patch)
                .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))
        })();
        #[cfg(feature = "editor-work-observation")]
        work_span.record(
            "outcome",
            match &result {
                Ok(_) => "prepared",
                Err(error) => editor_edit_preparation_outcome(error),
            },
        );
        result
    }

    /// Consumes the accepted candidate; later project/domain changes cannot authorize it anew.
    pub fn commit_authoring_edit(
        &mut self,
        prepared: PreparedEditorDocumentEdit,
    ) -> EditorActionOutcome {
        #[cfg(feature = "editor-work-observation")]
        let base_revision = prepared.base_revision();
        #[cfg(feature = "editor-work-observation")]
        let work_span = tracing::info_span!(
            target: "nara::editor::work",
            "editor_edit_commit",
            document = prepared.document().raw(),
            source = tracing::field::Empty,
            revision = base_revision.generation(),
            current_revision = tracing::field::Empty,
            outcome = "pending",
        )
        .entered();
        if self.authoring_operation_fenced() {
            #[cfg(feature = "editor-work-observation")]
            work_span.record("outcome", "busy");
            return EditorActionOutcome::Workspace(editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.schema-adoption-busy",
            ));
        }
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            #[cfg(feature = "editor-work-observation")]
            work_span.record("outcome", "closing");
            return EditorActionOutcome::Workspace(editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.workspace-intent-pending",
            ));
        }
        if !prepared.is_prefab_revert()
            && let Err(rejection) =
                self.verify_prepared_patch_contract(prepared.document(), prepared.patch())
        {
            #[cfg(feature = "editor-work-observation")]
            work_span.record("outcome", "structure_mismatch");
            return EditorActionOutcome::Rejected(rejection);
        }
        let report = self.workspace.commit_authoring_edit(prepared);
        let _ = self.diagnostics.extend(report.diagnostics.clone());
        #[cfg(feature = "editor-work-observation")]
        {
            if let Some(revision) = report.revision {
                work_span.record("current_revision", revision.generation());
            }
            work_span.record(
                "outcome",
                editor_edit_commit_outcome(&report, base_revision),
            );
        }
        EditorActionOutcome::Workspace(report)
    }

    pub(super) fn verify_prepared_patch_contract(
        &self,
        document: EditorDocumentId,
        patch: &ScenePatchDocument,
    ) -> Result<(), EditorActionRejection> {
        if self.workspace.document_kind(document) == Some(EditorDocumentKind::Prefab)
            && patch.operations.iter().any(|operation| {
                !matches!(
                    operation,
                    ScenePatchOperation::SetField { .. }
                        | ScenePatchOperation::SetAssetRefField { .. }
                )
            })
            && (self.prefab_uses_ui_authoring_domain(document)
                || super::actions::prefab_patch_uses_ui_domain(patch))
        {
            return Err(EditorActionRejection::StructureContractMismatch { document });
        }
        Ok(())
    }
}

#[cfg(feature = "editor-work-observation")]
fn editor_edit_preparation_outcome(error: &EditorEditPreparationError) -> &'static str {
    match error {
        EditorEditPreparationError::WorkspaceUnavailable => "unavailable",
        EditorEditPreparationError::Closing => "closing",
        EditorEditPreparationError::Busy => "busy",
        EditorEditPreparationError::StructureContractMismatch { .. } => "structure_mismatch",
        EditorEditPreparationError::QueryBasis(rejection) => match rejection {
            EditorDocumentQueryRejection::WorkspaceUnavailable
            | EditorDocumentQueryRejection::DocumentNotOpen { .. } => "unavailable",
            EditorDocumentQueryRejection::Closing => "closing",
            EditorDocumentQueryRejection::StaleRevision => "stale_revision",
            EditorDocumentQueryRejection::StaleIncarnation => "stale_source",
            EditorDocumentQueryRejection::WrongWorkspace
            | EditorDocumentQueryRejection::StaleSchema => "workspace_fenced",
            EditorDocumentQueryRejection::UnsupportedDocumentKind { .. }
            | EditorDocumentQueryRejection::DocumentKindChanged { .. } => "structure_mismatch",
            EditorDocumentQueryRejection::Source(_)
            | EditorDocumentQueryRejection::PrefabDeclaration(_) => "domain_rejected",
            _ => "rejected",
        },
        EditorEditPreparationError::Document(report) => {
            if report.patch_report.is_some() {
                "domain_rejected"
            } else {
                "rejected"
            }
        }
    }
}

#[cfg(feature = "editor-work-observation")]
fn editor_edit_commit_outcome(
    report: &EditorWorkspaceCommandReport,
    base_revision: nara_scene::SceneAuthoringRevision,
) -> &'static str {
    if report.applied {
        return if report.revision == Some(base_revision) {
            "unchanged"
        } else {
            "committed"
        };
    }
    if let Some(revision) = report.revision {
        if revision.source_id() != base_revision.source_id() {
            return "stale_source";
        }
        if revision != base_revision {
            return "stale_revision";
        }
    }
    match &report.patch_report {
        Some(patch)
            if patch.inverse.is_some()
                && !patch.diagnostics.has_errors()
                && !report.diagnostics.has_errors() =>
        {
            "unchanged"
        }
        Some(_) => "domain_rejected",
        None => "rejected",
    }
}
