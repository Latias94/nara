use nara_reflect::{ComponentCapability, ComponentPropertyPath};
use nara_scene::{
    AuthoringConstructionDraft, ConstructionEntityMap, LocalConstructionEntity,
    MAX_AUTHORING_EDIT_OPERATIONS, MAX_AUTHORING_FIELD_LOGICAL_BYTES,
    MAX_AUTHORING_FIELD_VALUE_NODES, PreparedPrefabAuthoringConstruction,
    PreparedSceneAuthoringConstruction, SceneAuthoringRevision, SceneEntityId, ScenePatchDocument,
    ScenePlacement,
};

use super::{
    EditorDocumentId, EditorDocumentKind, EditorDocumentSlot, EditorWorkspace,
    EditorWorkspaceCommandReport, authoring_command_error_report,
    authoring_query::rejected_basis_report, report_for_document_patch,
};
use crate::EditorDocumentQueryBasis;
use crate::composition::{
    PrefabCompositionEntityDraft, PrefabCompositionOutcome, SceneCompositionEntityKind,
    scene_entity_component_ids,
};

#[cfg(test)]
mod source_composition_tests;

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
    pub(super) fn apply_authoring_entity_creation(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        kind: SceneCompositionEntityKind,
        placement: ScenePlacement,
    ) -> Result<EditorWorkspaceCommandReport, EditorWorkspaceCommandReport> {
        let basis = self
            .document_query_basis(document)
            .map_err(|_| authoring_command_error_report(self.active_document, document))?;
        let mut draft = self.begin_authoring_construction(&basis)?;
        let slot = self.resolve_authoring_preparation_slot(document, base_revision)?;
        let error_report = |report| self.authoring_preparation_error(document, slot, report);
        let local = draft.create_entity(placement).map_err(error_report)?;
        for component in scene_entity_component_ids(kind) {
            draft
                .add_component(&local, nara_reflect::ComponentTypeId::new(*component))
                .map_err(error_report)?;
        }
        let prepared = self.prepare_authoring_construction(&basis, &draft)?;
        self.commit_selected_construction(prepared, &local)
    }

    pub(super) fn apply_authoring_prefab_creation(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        recipe: &PrefabCompositionEntityDraft,
        placement: ScenePlacement,
    ) -> Result<EditorWorkspaceCommandReport, EditorWorkspaceCommandReport> {
        let basis = self
            .document_query_basis(document)
            .map_err(|_| authoring_command_error_report(self.active_document, document))?;
        let slot = self.resolve_authoring_preparation_slot(document, base_revision)?;
        let EditorDocumentSlot::Prefab(prefab) = slot else {
            return Err(authoring_command_error_report(
                self.active_document,
                document,
            ));
        };
        let schemas = prefab
            .session
            .authoring_schema_basis()
            .expect("source construction retains its admitted Schema basis");
        let reject = |summary| {
            let mut report = authoring_command_error_report(self.active_document, document);
            report.diagnostics.push(crate::diagnostic::error(
                "tooling.workspace-authoring-recipe-rejected",
                summary,
            ));
            report
        };
        let error_report = |report| self.authoring_preparation_error(document, slot, report);
        let mut draft = self.begin_authoring_construction(&basis)?;
        let local = draft.create_entity(placement).map_err(error_report)?;
        let mut writes = 0_usize;
        let mut nodes = 0_usize;
        let mut bytes = 0_usize;
        for (component, record) in recipe.components() {
            let schema = schemas
                .schema(component)
                .ok_or_else(|| reject("The recipe component has no admitted source Schema"))?;
            let template = schema
                .authoring_initial_value()
                .ok_or_else(|| reject("The recipe component has no admitted creation template"))?;
            if record.version != schema.version() {
                return Err(reject(
                    "The recipe component version differs from the source Schema",
                ));
            }
            draft
                .add_component(&local, component.clone())
                .map_err(error_report)?;
            for field in schema.fields().iter().filter(|field| {
                field.has_capability(ComponentCapability::Scene)
                    && field.has_capability(ComponentCapability::Edit)
            }) {
                let desired = record.value.get_path(field.path()).ok();
                if desired == template.get_path(field.path()).ok() {
                    continue;
                }
                // Construction changes declared properties of the real source template.
                // Missing or additional payload cannot be silently dropped to make a recipe fit.
                let desired = desired
                    .ok_or_else(|| reject("The recipe omits a declared construction property"))?;
                if writes == MAX_AUTHORING_EDIT_OPERATIONS {
                    return Err(reject(
                        "The recipe exceeds the construction property-write limit",
                    ));
                }
                let cost = desired
                    .cost_up_to(
                        MAX_AUTHORING_FIELD_VALUE_NODES.saturating_sub(nodes),
                        MAX_AUTHORING_FIELD_LOGICAL_BYTES.saturating_sub(bytes),
                    )
                    .ok_or_else(|| {
                        reject("The recipe exceeds the construction value-copy limit")
                    })?;
                writes += 1;
                nodes += cost.nodes();
                bytes += cost.logical_bytes();
                draft
                    .set_property(
                        &local,
                        component.clone(),
                        ComponentPropertyPath::new(field.id().clone()),
                        desired.clone(),
                    )
                    .map_err(error_report)?;
            }
        }
        let prepared = self.prepare_authoring_construction(&basis, &draft)?;
        let PreparedDocumentConstruction::Prefab(candidate) = &prepared.construction else {
            unreachable!("a Prefab construction retains its document kind");
        };
        let entity = candidate
            .entity(&local)
            .expect("prepared construction resolves its local");
        let actual = candidate
            .document()
            .entities
            .iter()
            .find(|record| &record.id == entity)
            .expect("the prepared source contains its created entity");
        if actual.components.len() != recipe.components().len()
            || recipe.components().iter().any(|(component, desired)| {
                actual.components.get(component).is_none_or(|value| {
                    value.version != desired.version
                        || value.value != desired.value
                        || value.contextual_defaults != desired.contextual_defaults
                        || desired
                            .owner()
                            .is_some_and(|owner| value.owner() != Some(owner))
                })
            })
        {
            return Err(reject(
                "The recipe does not match the complete source construction result",
            ));
        }
        self.commit_selected_construction(prepared, &local)
    }

    fn commit_selected_construction(
        &mut self,
        prepared: PreparedEditorDocumentConstruction,
        local: &LocalConstructionEntity,
    ) -> Result<EditorWorkspaceCommandReport, EditorWorkspaceCommandReport> {
        let document = prepared.document();
        let committed = self.commit_authoring_construction(prepared)?;
        let entity = committed
            .entity(local)
            .cloned()
            .expect("successful construction retains its exact local mapping");
        let (_, mut report) = committed.into_parts();
        let slot = self
            .documents
            .get_mut(&document)
            .expect("the committed source remains open");
        slot.selection_mut().select_entity(Some(entity.clone()));
        if slot.kind() == EditorDocumentKind::Prefab {
            report.prefab_composition_outcome = Some(PrefabCompositionOutcome::Created { entity });
        }
        Ok(report)
    }

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
