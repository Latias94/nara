//! Nominal source conversion candidates retain their dependency fence through Workspace routing.

use std::{error::Error, fmt};

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_core::ByteLimit;
use nara_scene::{
    LocalizedPrefabEntity, PrefabConversionError, PrefabConversionRequest, PrefabDocument,
    PrefabExpansionOptions, PrefabExpansionWorkingSet, PreparedPrefabConversion,
    PreparedScenePrefabConversion, ResolvedPrefabSource, SceneAuthoringRevision, SceneDocument,
    SceneEntityId, ScenePrefabConversionRequest,
};

use super::{
    EditorDocumentId, EditorDocumentKind, EditorDocumentSlot, EditorWorkspace,
    EditorWorkspaceCommandReport, authoring_command_error_report,
    authoring_query::rejected_basis_report, report_for_document_patch,
};
use crate::EditorDocumentQueryBasis;

/// An exact named source request that can move to a background task without the Workspace.
#[derive(Debug)]
pub struct EditorPrefabConversionRequest {
    basis: EditorDocumentQueryBasis,
    source: ConversionRequest,
}

#[derive(Debug)]
enum ConversionRequest {
    Scene(ScenePrefabConversionRequest),
    Prefab(PrefabConversionRequest),
}

impl EditorPrefabConversionRequest {
    #[must_use]
    pub fn checked_retained_bytes(&self) -> Option<usize> {
        let source = match &self.source {
            ConversionRequest::Scene(request) => request.checked_retained_bytes()?,
            ConversionRequest::Prefab(request) => request.checked_retained_bytes()?,
        };
        source.checked_add(std::mem::size_of::<Self>())
    }

    #[must_use]
    pub fn basis(&self) -> &EditorDocumentQueryBasis {
        &self.basis
    }

    #[must_use]
    pub fn document_kind(&self) -> EditorDocumentKind {
        match self.source {
            ConversionRequest::Scene(_) => EditorDocumentKind::Scene,
            ConversionRequest::Prefab(_) => EditorDocumentKind::Prefab,
        }
    }

    pub fn restrict_working_bytes(&mut self, maximum: ByteLimit) {
        match &mut self.source {
            ConversionRequest::Scene(request) => request.restrict_working_bytes(maximum),
            ConversionRequest::Prefab(request) => request.restrict_working_bytes(maximum),
        }
    }

    /// Keeps the original owner-created candidate and all actual semantic source snapshots.
    pub fn prepare_with_loader<F, E>(
        self,
        database: Option<&ProjectAssetDatabase>,
        loader: F,
    ) -> Result<PreparedEditorPrefabConversion, PrefabConversionError<E>>
    where
        F: FnMut(
            &AssetRef,
            &mut PrefabExpansionWorkingSet,
        ) -> Result<Option<ResolvedPrefabSource>, E>,
    {
        let source = match self.source {
            ConversionRequest::Scene(request) => {
                PreparedConversion::Scene(request.prepare_with_loader(database, loader)?)
            }
            ConversionRequest::Prefab(request) => {
                PreparedConversion::Prefab(request.prepare_with_loader(database, loader)?)
            }
        };
        Ok(PreparedEditorPrefabConversion {
            basis: self.basis,
            source,
        })
    }
}

/// A move-only candidate for one open document; its semantic fence cannot be extracted away.
///
/// A concrete Host additionally keeps current file, catalogue and working-copy evidence.
#[derive(Debug)]
pub struct PreparedEditorPrefabConversion {
    basis: EditorDocumentQueryBasis,
    source: PreparedConversion,
}

#[derive(Debug)]
enum PreparedConversion {
    Scene(PreparedScenePrefabConversion),
    Prefab(PreparedPrefabConversion),
}

impl PreparedEditorPrefabConversion {
    #[must_use]
    pub fn checked_retained_bytes(&self) -> Option<usize> {
        let source = match &self.source {
            PreparedConversion::Scene(prepared) => prepared.checked_retained_bytes()?,
            PreparedConversion::Prefab(prepared) => prepared.checked_retained_bytes()?,
        };
        source.checked_add(std::mem::size_of::<Self>())
    }

    #[must_use]
    pub fn basis(&self) -> &EditorDocumentQueryBasis {
        &self.basis
    }

    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.basis.document
    }

    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.basis.revision
    }

    #[must_use]
    pub fn document_kind(&self) -> EditorDocumentKind {
        match self.source {
            PreparedConversion::Scene(_) => EditorDocumentKind::Scene,
            PreparedConversion::Prefab(_) => EditorDocumentKind::Prefab,
        }
    }

    #[must_use]
    pub fn anchor(&self) -> &SceneEntityId {
        match &self.source {
            PreparedConversion::Scene(prepared) => prepared.anchor(),
            PreparedConversion::Prefab(prepared) => prepared.anchor(),
        }
    }

    #[must_use]
    pub fn entities(&self) -> &[LocalizedPrefabEntity] {
        match &self.source {
            PreparedConversion::Scene(prepared) => prepared.entities(),
            PreparedConversion::Prefab(prepared) => prepared.entities(),
        }
    }

    #[must_use]
    pub fn scene_document(&self) -> Option<&SceneDocument> {
        match &self.source {
            PreparedConversion::Scene(prepared) => Some(prepared.document()),
            PreparedConversion::Prefab(_) => None,
        }
    }

    #[must_use]
    pub fn prefab_document(&self) -> Option<&PrefabDocument> {
        match &self.source {
            PreparedConversion::Scene(_) => None,
            PreparedConversion::Prefab(prepared) => Some(prepared.document()),
        }
    }
}

/// Successful source publication with the same fixed identity mapping reviewed by the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct CommittedEditorPrefabConversion {
    entities: Vec<LocalizedPrefabEntity>,
    report: EditorWorkspaceCommandReport,
}

impl CommittedEditorPrefabConversion {
    #[must_use]
    pub fn entities(&self) -> &[LocalizedPrefabEntity] {
        &self.entities
    }

    #[must_use]
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<LocalizedPrefabEntity>, EditorWorkspaceCommandReport) {
        (self.entities, self.report)
    }
}

#[derive(Debug)]
pub enum EditorPrefabConversionCommitError<E> {
    Document(Box<EditorWorkspaceCommandReport>),
    Loader(E),
}

impl<E: fmt::Display> fmt::Display for EditorPrefabConversionCommitError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(_) => {
                formatter.write_str("Prefab conversion source publication was refused")
            }
            Self::Loader(error) => write!(
                formatter,
                "Prefab conversion source validation failed: {error}"
            ),
        }
    }
}

impl<E: Error + 'static> Error for EditorPrefabConversionCommitError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Document(_) => None,
            Self::Loader(error) => Some(error),
        }
    }
}

impl EditorWorkspace {
    /// Captures an exact direct-anchor request without changing active document or selection.
    pub fn request_complete_prefab_conversion(
        &self,
        basis: &EditorDocumentQueryBasis,
        anchor: &SceneEntityId,
        options: PrefabExpansionOptions,
    ) -> Result<EditorPrefabConversionRequest, EditorWorkspaceCommandReport> {
        self.validate_document_query_basis(basis)
            .map_err(|error| rejected_basis_report(self, basis, error))?;
        let slot = self.resolve_authoring_preparation_slot(basis.document, basis.revision)?;
        let source = match slot {
            EditorDocumentSlot::Scene(slot) => slot
                .session
                .request_complete_prefab_conversion(basis.revision, anchor, options)
                .map(ConversionRequest::Scene),
            EditorDocumentSlot::Prefab(slot) => slot
                .session
                .request_complete_prefab_conversion(basis.revision, anchor, options)
                .map(ConversionRequest::Prefab),
            EditorDocumentSlot::Extension(_) => {
                return Err(authoring_command_error_report(
                    self.active_document,
                    basis.document,
                ));
            }
        }
        .map_err(|report| self.authoring_preparation_error(basis.document, slot, report))?;
        Ok(EditorPrefabConversionRequest {
            basis: basis.clone(),
            source,
        })
    }

    /// Routes the original nominal candidate to the same source owner after exact basis checks.
    pub fn commit_complete_prefab_conversion_with_loader<F, E>(
        &mut self,
        prepared: PreparedEditorPrefabConversion,
        loader: F,
    ) -> Result<CommittedEditorPrefabConversion, EditorPrefabConversionCommitError<E>>
    where
        F: FnMut(&AssetRef) -> Result<Option<ResolvedPrefabSource>, E>,
    {
        self.validate_document_query_basis(prepared.basis())
            .map_err(|error| {
                EditorPrefabConversionCommitError::Document(Box::new(rejected_basis_report(
                    self,
                    prepared.basis(),
                    error,
                )))
            })?;
        let document = prepared.document();
        let (active, slot) = self
            .resolve_exact_authoring_document_mutation_slot(document, prepared.base_revision())
            .map_err(|report| EditorPrefabConversionCommitError::Document(Box::new(report)))?;
        let result = match (&mut *slot, prepared.source) {
            (EditorDocumentSlot::Scene(slot), PreparedConversion::Scene(prepared)) => slot
                .session
                .commit_complete_prefab_conversion_with_loader(prepared, loader),
            (EditorDocumentSlot::Prefab(slot), PreparedConversion::Prefab(prepared)) => slot
                .session
                .commit_complete_prefab_conversion_with_loader(prepared, loader),
            _ => {
                return Err(EditorPrefabConversionCommitError::Document(Box::new(
                    authoring_command_error_report(active, document),
                )));
            }
        };
        match result {
            Ok(receipt) => {
                let (entities, patch_report) = receipt.into_parts();
                let report = report_for_document_patch(document, active, slot, patch_report);
                Ok(CommittedEditorPrefabConversion { entities, report })
            }
            Err(PrefabConversionError::Source(report)) => {
                Err(EditorPrefabConversionCommitError::Document(Box::new(
                    report_for_document_patch(document, active, slot, report),
                )))
            }
            Err(PrefabConversionError::Loader(error)) => {
                Err(EditorPrefabConversionCommitError::Loader(error))
            }
        }
    }
}
