//! Shared private field drafts for the Editor frontend.

pub(in crate::frontend) mod references;
pub(in crate::frontend) mod structure;

use std::collections::{HashMap, HashSet};

use nara::{
    reflect::{
        ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, ComponentValueShape,
    },
    scene::{AuthoringSchemaBasis, SceneEntityId},
    tooling::{
        EditorAction, EditorActionOutcome, EditorDocumentId, EditorFieldTarget, EditorFrameModel,
        EditorSceneModel, SceneInspectorComponentView, SceneInspectorFieldView,
    },
};

use super::authoring;
use super::value_draft::{self, RootDraft};
use crate::panels::{
    PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId, PanelSubmitError,
};

// Clean entries are reclaimable. Retained edits are admitted before any input is enabled.
const MAX_RETAINED_FIELD_DRAFTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::frontend) struct FieldKey {
    pub(in crate::frontend) document: EditorDocumentId,
    pub(in crate::frontend) entity: SceneEntityId,
    pub(in crate::frontend) component: ComponentTypeId,
    pub(in crate::frontend) field: ComponentFieldId,
}

impl FieldKey {
    pub(in crate::frontend) fn from_target(target: &EditorFieldTarget) -> Self {
        Self {
            document: target.document(),
            entity: target.entity().clone(),
            component: target.component().clone(),
            field: target.field().clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::frontend) struct FieldBasis {
    pub(in crate::frontend) target: EditorFieldTarget,
    pub(in crate::frontend) catalog: Option<AuthoringSchemaBasis>,
    pub(in crate::frontend) document_version: ComponentSchemaVersion,
    pub(in crate::frontend) field: SceneInspectorFieldView,
}

impl FieldBasis {
    pub(in crate::frontend) fn new(
        frame: &EditorFrameModel,
        scene: &EditorSceneModel,
        entity: &SceneEntityId,
        component: &SceneInspectorComponentView,
        field: &SceneInspectorFieldView,
    ) -> Self {
        Self {
            target: EditorFieldTarget::new(
                scene.document,
                scene.revision,
                entity.clone(),
                component.component.clone(),
                component.schema_version,
                field.id.clone(),
            ),
            catalog: frame
                .workspace()
                .document(scene.document)
                .and_then(|document| document.authoring_schema_basis.clone()),
            document_version: component.document_version,
            field: field.clone(),
        }
    }

    fn compatible(&self, current: &Self) -> bool {
        self.target.base_revision().source_id() == current.target.base_revision().source_id()
            && self.catalog == current.catalog
            && self.target.component_version() == current.target.component_version()
            && self.document_version == current.document_version
            && self.field.shape == current.field.shape
            && self.field.path == current.field.path
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::frontend) enum Candidate {
    Text(String),
    Bool(bool),
    Structured(RootDraft),
}

impl Candidate {
    fn from_field(field: &SceneInspectorFieldView) -> Self {
        match &field.shape {
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::Bool,
                ..
            } => Self::Bool(
                source(field)
                    .and_then(ComponentValue::as_bool)
                    .unwrap_or(false),
            ),
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::Null,
                ..
            } => Self::Structured(RootDraft::default()),
            ComponentValueShape::Scalar { .. }
            | ComponentValueShape::EntityRef
            | ComponentValueShape::AssetRef { .. } => {
                Self::Text(authoring::field_source_text(field))
            }
            _ => Self::Structured(RootDraft::default()),
        }
    }

    pub(in crate::frontend) fn value(
        &self,
        shape: &ComponentValueShape,
    ) -> Result<ComponentValue, String> {
        let value = match self {
            Self::Text(text) => value_draft::parse_scalar(shape, text)?,
            Self::Bool(value) => ComponentValue::Bool(*value),
            Self::Structured(root) => {
                return root
                    .node
                    .as_ref()
                    .ok_or("Open a value draft first")?
                    .value(shape);
            }
        };
        shape
            .validate_value(&value)
            .map_err(|error| error.to_string())?;
        Ok(value)
    }

    pub(in crate::frontend) fn copy_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Bool(value) => value.to_string(),
            Self::Structured(root) => root
                .node
                .as_ref()
                .map_or_else(String::new, |node| node.copy_text()),
        }
    }
}

pub(in crate::frontend) struct FieldDraft {
    pub(in crate::frontend) basis: FieldBasis,
    pub(in crate::frontend) candidate: Candidate,
    pub(in crate::frontend) dirty: bool,
    pub(in crate::frontend) conflict: bool,
    pub(in crate::frontend) incompatible: bool,
    pub(in crate::frontend) queued: bool,
    pub(in crate::frontend) pending: Option<PanelSubmissionId>,
    pub(in crate::frontend) error: Option<String>,
    pub(in crate::frontend) remove_intent: bool,
}

impl FieldDraft {
    pub(in crate::frontend) fn new(basis: FieldBasis) -> Self {
        Self {
            candidate: Candidate::from_field(&basis.field),
            basis,
            dirty: false,
            conflict: false,
            incompatible: false,
            queued: false,
            pending: None,
            error: None,
            remove_intent: false,
        }
    }

    pub(in crate::frontend) fn synchronize(&mut self, current: FieldBasis) {
        if !self.dirty && !self.in_flight() && self.basis != current {
            *self = Self::new(current);
        } else if !self.basis.compatible(&current) {
            self.incompatible = true;
        } else if self.basis.target.base_revision() != current.target.base_revision() {
            self.conflict = true;
        }
    }

    pub(in crate::frontend) fn in_flight(&self) -> bool {
        self.queued || self.pending.is_some()
    }
}

#[derive(Default)]
pub(in crate::frontend) struct FieldEditors {
    pub(in crate::frontend) drafts: HashMap<FieldKey, FieldDraft>,
    pub(in crate::frontend) painted: HashSet<FieldKey>,
}

pub(in crate::frontend) struct PreparedFieldEdit {
    pub(in crate::frontend) key: FieldKey,
    pub(in crate::frontend) action: EditorAction,
}

impl FieldEditors {
    pub(in crate::frontend) fn retained_count(&self) -> usize {
        self.drafts
            .values()
            .filter(|draft| draft.dirty || draft.in_flight())
            .count()
    }

    pub(in crate::frontend) fn at_capacity(&self) -> bool {
        self.retained_count() >= MAX_RETAINED_FIELD_DRAFTS
    }

    pub(in crate::frontend) fn has_draft_capacity(&self, key: &FieldKey) -> bool {
        self.drafts
            .get(key)
            .is_some_and(|draft| draft.dirty || draft.in_flight())
            || !self.at_capacity()
    }

    pub(in crate::frontend) fn clear(&mut self) {
        self.drafts.clear();
        self.painted.clear();
    }

    pub(in crate::frontend) fn begin_paint(&mut self, frame: &EditorFrameModel) {
        // Keep the previous presentation's clean controls for stable active input. Clean controls
        // which stop being presented are reclaimable; dirty drafts never depend on paint coverage.
        self.drafts
            .retain(|key, draft| draft.dirty || draft.in_flight() || self.painted.contains(key));
        self.painted.clear();
        // A missing selected projection is no evidence of deletion. Only complete owner rows,
        // or the selected entity's complete component/field inventory, establish absence.
        self.drafts
            .retain(|key, _| frame.workspace().document(key.document).is_some());
        for (key, draft) in &mut self.drafts {
            if draft.dirty
                && let Some(document) = frame.workspace().document(key.document)
            {
                if draft.basis.catalog != document.authoring_schema_basis
                    || draft.basis.target.base_revision().source_id()
                        != document.revision.source_id()
                {
                    draft.incompatible = true;
                } else if draft.basis.target.base_revision() != document.revision {
                    draft.conflict = true;
                }
            }
            let Some(scene) = frame
                .workspace()
                .active_scene
                .as_ref()
                .filter(|scene| scene.document == key.document)
            else {
                continue;
            };
            if !scene
                .editor
                .inspector
                .entities
                .iter()
                .any(|entity| entity.id == key.entity)
            {
                if draft.dirty {
                    draft.incompatible = true;
                }
                continue;
            }
            let Some(entity) = scene
                .editor
                .inspector
                .selected_entity_view
                .as_ref()
                .filter(|entity| entity.id == key.entity)
            else {
                if draft.dirty && draft.basis.target.base_revision() != scene.revision {
                    if draft.basis.target.base_revision().source_id() != scene.revision.source_id()
                    {
                        draft.incompatible = true;
                    } else {
                        draft.conflict = true;
                    }
                }
                continue;
            };
            let current = entity
                .components
                .iter()
                .find(|component| component.component == key.component)
                .and_then(|component| {
                    component
                        .fields
                        .iter()
                        .find(|field| field.id == key.field)
                        .map(|field| FieldBasis::new(frame, scene, &entity.id, component, field))
                });
            if let Some(current) = current {
                draft.synchronize(current);
            } else if draft.dirty {
                draft.incompatible = true;
            }
        }
    }

    pub(in crate::frontend) fn admitted(
        &mut self,
        key: &FieldKey,
        result: Result<PanelSubmissionId, PanelSubmitError>,
    ) {
        let Some(draft) = self.drafts.get_mut(key) else {
            return;
        };
        draft.queued = false;
        match result {
            Ok(id) => {
                draft.pending = Some(id);
                draft.error = None;
            }
            Err(error) => {
                draft.error = Some(format!(
                    "The field edit was not queued: {error}. Apply it again when ready."
                ));
            }
        }
    }

    pub(in crate::frontend) fn action_finished(
        &mut self,
        id: PanelSubmissionId,
        result: &PanelActionDispatch,
    ) {
        let key = self
            .drafts
            .iter()
            .find(|(_, draft)| draft.pending == Some(id))
            .map(|(key, _)| key.clone());
        let Some(key) = key else {
            return;
        };
        if matches!(result, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied)
        {
            self.drafts.remove(&key);
        } else if let Some(draft) = self.drafts.get_mut(&key) {
            draft.pending = None;
            draft.error = Some(match result {
                PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded) =>
                    "This edit was not submitted. Your input is retained; apply it again.".to_owned(),
                PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing) =>
                    "Finish or cancel closing the document before applying this retained edit.".to_owned(),
                PanelActionDispatch::NotDispatched(_) =>
                    "The editing context changed before this edit was applied. Review the current value and try again.".to_owned(),
                PanelActionDispatch::Dispatched(_) =>
                    "The document rejected this edit. Your input is retained; check Diagnostics and correct or review it before retrying.".to_owned(),
            });
        }
    }
}

pub(in crate::frontend) fn source(field: &SceneInspectorFieldView) -> Option<&ComponentValue> {
    field.value.as_ref().or(field.default_value.as_ref())
}
