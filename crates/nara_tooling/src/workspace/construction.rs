use nara_scene::{
    AuthoringConstructionDraft, ConstructionEntityMap, LocalConstructionEntity,
    PreparedPrefabAuthoringConstruction, PreparedSceneAuthoringConstruction,
    SceneAuthoringRevision, SceneEntityId, ScenePatchDocument,
};

use super::{
    EditorDocumentId, EditorDocumentKind, EditorDocumentSlot, EditorWorkspace,
    EditorWorkspaceCommandReport, authoring_command_error_report,
    authoring_query::rejected_basis_report, report_for_document_patch,
};
use crate::EditorDocumentQueryBasis;

/// One source-owned candidate and fixed local mapping, without selection or Workspace custody.
#[derive(Debug)]
pub struct PreparedEditorDocumentConstruction {
    document: EditorDocumentId,
    construction: PreparedDocumentConstruction,
}

#[derive(Debug)]
enum PreparedDocumentConstruction {
    Scene(PreparedSceneAuthoringConstruction),
    Prefab(PreparedPrefabAuthoringConstruction),
}

impl PreparedEditorDocumentConstruction {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub fn document_kind(&self) -> EditorDocumentKind {
        match self.construction {
            PreparedDocumentConstruction::Scene(_) => EditorDocumentKind::Scene,
            PreparedDocumentConstruction::Prefab(_) => EditorDocumentKind::Prefab,
        }
    }

    #[must_use]
    pub fn base_revision(&self) -> SceneAuthoringRevision {
        match &self.construction {
            PreparedDocumentConstruction::Scene(prepared) => prepared.base_revision(),
            PreparedDocumentConstruction::Prefab(prepared) => prepared.base_revision(),
        }
    }

    #[must_use]
    pub fn patch(&self) -> &ScenePatchDocument {
        match &self.construction {
            PreparedDocumentConstruction::Scene(prepared) => prepared.patch(),
            PreparedDocumentConstruction::Prefab(prepared) => prepared.patch(),
        }
    }

    #[must_use]
    pub fn inverse(&self) -> &ScenePatchDocument {
        match &self.construction {
            PreparedDocumentConstruction::Scene(prepared) => prepared.inverse(),
            PreparedDocumentConstruction::Prefab(prepared) => prepared.inverse(),
        }
    }

    #[must_use]
    pub fn entity(&self, local: &LocalConstructionEntity) -> Option<&SceneEntityId> {
        match &self.construction {
            PreparedDocumentConstruction::Scene(prepared) => prepared.entity(local),
            PreparedDocumentConstruction::Prefab(prepared) => prepared.entity(local),
        }
    }
}

/// Exact successful publication; the mapping is historical result data, not current existence.
#[derive(Debug, Clone, PartialEq)]
pub struct CommittedEditorDocumentConstruction {
    entities: ConstructionEntityMap,
    report: EditorWorkspaceCommandReport,
}

impl CommittedEditorDocumentConstruction {
    #[must_use]
    pub fn entity(&self, local: &LocalConstructionEntity) -> Option<&SceneEntityId> {
        self.entities.entity(local)
    }

    #[must_use]
    pub const fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    #[must_use]
    pub fn into_parts(self) -> (ConstructionEntityMap, EditorWorkspaceCommandReport) {
        (self.entities, self.report)
    }
}

impl EditorWorkspace {
    /// Starts bounded intent at the exact source basis without publishing or changing navigation.
    pub fn begin_authoring_construction(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<AuthoringConstructionDraft, EditorWorkspaceCommandReport> {
        self.validate_document_query_basis(basis)
            .map_err(|error| rejected_basis_report(self, basis, error))?;
        let document = basis.document;
        let base_revision = basis.revision;
        let slot = self.resolve_authoring_preparation_slot(document, base_revision)?;
        let result = match slot {
            EditorDocumentSlot::Scene(slot) => {
                slot.session.begin_authoring_construction(base_revision)
            }
            EditorDocumentSlot::Prefab(slot) => {
                slot.session.begin_authoring_construction(base_revision)
            }
            EditorDocumentSlot::Extension(_) => {
                return Err(authoring_command_error_report(
                    self.active_document,
                    document,
                ));
            }
        };
        result.map_err(|report| self.authoring_preparation_error(document, slot, report))
    }

    /// Resolves locals once using the original document's templates, candidate and inverse owner.
    pub fn prepare_authoring_construction(
        &self,
        basis: &EditorDocumentQueryBasis,
        draft: &AuthoringConstructionDraft,
    ) -> Result<PreparedEditorDocumentConstruction, EditorWorkspaceCommandReport> {
        self.validate_document_query_basis(basis)
            .map_err(|error| rejected_basis_report(self, basis, error))?;
        let document = basis.document;
        let slot = self.resolve_authoring_preparation_slot(document, draft.base_revision())?;
        let result = match slot {
            EditorDocumentSlot::Scene(slot) => slot
                .session
                .prepare_authoring_construction(draft)
                .map(PreparedDocumentConstruction::Scene),
            EditorDocumentSlot::Prefab(slot) => slot
                .session
                .prepare_authoring_construction(draft)
                .map(PreparedDocumentConstruction::Prefab),
            EditorDocumentSlot::Extension(_) => {
                return Err(authoring_command_error_report(
                    self.active_document,
                    document,
                ));
            }
        };
        result
            .map(|construction| PreparedEditorDocumentConstruction {
                document,
                construction,
            })
            .map_err(|report| self.authoring_preparation_error(document, slot, report))
    }

    /// Returns a mapping only when the original document accepts its consuming publication.
    pub fn commit_authoring_construction(
        &mut self,
        prepared: PreparedEditorDocumentConstruction,
    ) -> Result<CommittedEditorDocumentConstruction, EditorWorkspaceCommandReport> {
        let document = prepared.document();
        let (active, slot) = self
            .resolve_exact_authoring_document_mutation_slot(document, prepared.base_revision())?;
        let result = match (&mut *slot, prepared.construction) {
            (EditorDocumentSlot::Scene(slot), PreparedDocumentConstruction::Scene(prepared)) => {
                slot.session.commit_authoring_construction(prepared)
            }
            (EditorDocumentSlot::Prefab(slot), PreparedDocumentConstruction::Prefab(prepared)) => {
                slot.session.commit_authoring_construction(prepared)
            }
            _ => return Err(authoring_command_error_report(active, document)),
        };
        match result {
            Ok(committed) => {
                let (entities, patch_report) = committed.into_parts();
                let report = report_for_document_patch(document, active, slot, patch_report);
                Ok(CommittedEditorDocumentConstruction { entities, report })
            }
            Err(report) => Err(report_for_document_patch(document, active, slot, report)),
        }
    }
}
