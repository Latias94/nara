//! The Dear ImGui adapter's private field controllers. Application projections remain read-only.

pub(super) mod entity_reference;

use std::collections::{HashMap, HashSet};

use dear_imgui_rs::{Key, Ui};
use nara::{
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPathSegment, ComponentSchemaVersion,
        ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    },
    scene::{AuthoringSchemaBasis, SceneEntityId},
    tooling::{
        EditorAction, EditorActionOutcome, EditorDocumentId, EditorDocumentMutation,
        EditorFieldTarget, EditorFrameModel, EditorSceneModel, SceneInspectorCommand,
        SceneInspectorComponentView, SceneInspectorFieldState, SceneInspectorFieldView,
    },
};

use super::authoring::{
    self, recursive_controls,
    value_draft::{self, RootDraft},
};
use super::{
    ImguiEditorPanelContext, ImguiTextOwner, PanelActionDispatch, PanelSubmissionId,
    PanelSubmitError, observe_text_owner,
};

// Clean entries are reclaimable. Retained edits are admitted before any input is enabled.
const MAX_RETAINED_FIELD_DRAFTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FieldKey {
    document: EditorDocumentId,
    entity: SceneEntityId,
    component: ComponentTypeId,
    field: ComponentFieldId,
}

impl FieldKey {
    fn from_target(target: &EditorFieldTarget) -> Self {
        Self {
            document: target.document(),
            entity: target.entity().clone(),
            component: target.component().clone(),
            field: target.field().clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FieldBasis {
    target: EditorFieldTarget,
    catalog: Option<AuthoringSchemaBasis>,
    document_version: ComponentSchemaVersion,
    field: SceneInspectorFieldView,
}

impl FieldBasis {
    fn new(
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
enum Candidate {
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
            | ComponentValueShape::AssetRef => Self::Text(authoring::field_source_text(field)),
            _ => Self::Structured(RootDraft::default()),
        }
    }

    fn value(&self, shape: &ComponentValueShape) -> Result<ComponentValue, String> {
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

    fn copy_text(&self) -> String {
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

struct FieldDraft {
    basis: FieldBasis,
    candidate: Candidate,
    dirty: bool,
    conflict: bool,
    incompatible: bool,
    pending: Option<PanelSubmissionId>,
    error: Option<String>,
    remove_intent: bool,
}

impl FieldDraft {
    fn new(basis: FieldBasis) -> Self {
        Self {
            candidate: Candidate::from_field(&basis.field),
            basis,
            dirty: false,
            conflict: false,
            incompatible: false,
            pending: None,
            error: None,
            remove_intent: false,
        }
    }

    fn synchronize(&mut self, current: FieldBasis) {
        if !self.dirty && self.pending.is_none() && self.basis != current {
            *self = Self::new(current);
        } else if !self.basis.compatible(&current) {
            self.incompatible = true;
        } else if self.basis.target.base_revision() != current.target.base_revision() {
            self.conflict = true;
        }
    }
}

#[derive(Default)]
pub(super) struct FieldEditors {
    drafts: HashMap<FieldKey, FieldDraft>,
    painted: HashSet<FieldKey>,
}

pub(super) struct PreparedFieldEdit {
    key: FieldKey,
    action: EditorAction,
}

impl FieldEditors {
    fn has_draft_capacity(&self, key: &FieldKey) -> bool {
        self.drafts
            .get(key)
            .is_some_and(|draft| draft.dirty || draft.pending.is_some())
            || self
                .drafts
                .values()
                .filter(|draft| draft.dirty || draft.pending.is_some())
                .count()
                < MAX_RETAINED_FIELD_DRAFTS
    }

    pub(super) fn clear(&mut self) {
        self.drafts.clear();
        self.painted.clear();
    }

    pub(super) fn begin_paint(&mut self, frame: &EditorFrameModel) {
        // Keep the previous presentation's clean controls for stable active input. Clean controls
        // which stop being presented are reclaimable; dirty drafts never depend on paint coverage.
        self.drafts.retain(|key, draft| {
            draft.dirty || draft.pending.is_some() || self.painted.contains(key)
        });
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn show(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        scene: &EditorSceneModel,
        entity: &SceneEntityId,
        component: &SceneInspectorComponentView,
        field: &SceneInspectorFieldView,
        editable: bool,
        text_owner: &mut Option<ImguiTextOwner>,
    ) -> Option<PreparedFieldEdit> {
        let basis = FieldBasis::new(frame, scene, entity, component, field);
        let key = FieldKey::from_target(&basis.target);
        let _id = ui.push_id(field.id.as_str());
        ui.text(format!(
            "{}{}",
            field.path,
            if field.required { " *" } else { "" }
        ));
        if !self.painted.insert(key.clone()) {
            ui.text_wrapped(authoring::value_label(source(field)));
            ui.text_disabled("This field already has an editor in this panel");
            return None;
        }
        if !value_draft::supported(&field.shape) {
            ui.text_wrapped(authoring::value_label(source(field)));
            ui.text_disabled("This field shape is read-only in the generic Inspector");
            self.show_requested_recovery(ui, &key);
            return None;
        }
        if !self.has_draft_capacity(&key) {
            ui.text_wrapped(authoring::value_label(source(field)));
            ui.text_disabled(
                "Field draft capacity reached. Discard retained edits to open another editor.",
            );
            if ui.button("Discard retained field drafts") {
                self.drafts.retain(|_, draft| draft.pending.is_some());
            }
            return None;
        }
        if !self.drafts.contains_key(&key) {
            self.drafts
                .insert(key.clone(), FieldDraft::new(basis.clone()));
        }
        let draft = self.drafts.get_mut(&key).expect("the field was admitted");
        draft.synchronize(basis.clone());
        let pending = draft.pending.is_some();
        let mut discard = false;
        let mut apply = false;
        let mut remove = false;
        let mut reapply = false;
        let mut converted = None;
        if draft.incompatible {
            ui.text_wrapped("The schema or source changed. Copy the retained input, then reopen the current field.");
            ui.text_wrapped(format!(
                "Current: {}",
                authoring::value_label(source(field))
            ));
            show_copy(ui, &draft.candidate);
            discard = button(ui, &key, "Discard draft and reopen", !pending);
        } else {
            if draft.conflict {
                ui.text_colored(
                    [1.0, 0.7, 0.2, 1.0],
                    "Conflict: review the accepted value before reapplying.",
                );
                ui.text_wrapped(format!(
                    "Current: {}",
                    authoring::value_label(source(field))
                ));
                ui.text("Retained draft:");
                if draft.remove_intent {
                    ui.text("Remove the authored override");
                }
            }
            let controls_enabled = editable && !pending;
            let before = draft.candidate.clone();
            match &mut draft.candidate {
                Candidate::Text(text) => {
                    let enter = if field.shape == ComponentValueShape::EntityRef {
                        ui.with_disabled_if(!controls_enabled, || {
                            entity_reference::show(ui, &scene.editor.inspector.entities, text);
                        });
                        false
                    } else {
                        let enter = ui.with_disabled_if(!controls_enabled, || {
                            ui.input_text("##value", text)
                                .enter_returns_true(true)
                                .build()
                        });
                        observe_text_owner(ui, text_owner);
                        if ui.is_item_active() && ui.is_key_pressed(Key::Escape) && !pending {
                            discard = true;
                        }
                        enter
                    };
                    record(
                        ui,
                        format!(
                            "field:{}:{}",
                            component.component.as_str(),
                            field.id.as_str()
                        ),
                    );
                    ui.same_line();
                    apply = (button(ui, &key, "Apply", controls_enabled && !draft.conflict)
                        || enter)
                        && controls_enabled
                        && !draft.conflict;
                }
                Candidate::Bool(value) => {
                    apply = ui
                        .with_disabled_if(!controls_enabled, || ui.checkbox("##value", value))
                        && !draft.conflict;
                    record(
                        ui,
                        format!(
                            "field:{}:{}",
                            component.component.as_str(),
                            field.id.as_str()
                        ),
                    );
                    if draft.dirty && !draft.conflict {
                        ui.same_line();
                        apply |= button(ui, &key, "Apply", controls_enabled);
                    }
                }
                Candidate::Structured(root) => {
                    apply = recursive_controls::show_root(
                        ui,
                        &basis.target,
                        &scene.editor.inspector.entities,
                        &field.shape,
                        source(field),
                        root,
                        controls_enabled,
                        !draft.conflict,
                        text_owner,
                    );
                }
            }
            if draft.candidate != before {
                draft.dirty = true;
                draft.error = None;
                draft.remove_intent = false;
            }
            if let Candidate::Structured(root) = &draft.candidate
                && root.node.is_none()
                && before != draft.candidate
            {
                discard = true;
            }
            if draft.dirty && !draft.remove_intent {
                let value = draft.candidate.value(&draft.basis.field.shape);
                if let Err(error) = &value {
                    draft.error = Some(error.clone());
                }
                converted = Some(value);
            }
            if draft.conflict && button(ui, &key, "Reapply draft", controls_enabled) {
                draft.basis = basis.clone();
                draft.conflict = false;
                apply = true;
                reapply = true;
            }
            if draft.dirty {
                ui.same_line();
                discard |= button(ui, &key, "Discard draft", !pending);
            }
            let remove_label = if field.default_value.is_some() {
                "Reset to default"
            } else {
                "Remove field"
            };
            remove = button(
                ui,
                &key,
                remove_label,
                controls_enabled
                    && !draft.conflict
                    && field.state == SceneInspectorFieldState::Present
                    && matches!(
                        field.path.segments().last(),
                        Some(ComponentFieldPathSegment::Field(_))
                    )
                    && (!field.required || field.default_value.is_some()),
            );
        }
        if pending {
            ui.text_disabled("Submitting this field edit...");
        }
        if let Some(error) = &draft.error {
            ui.text_colored([1.0, 0.4, 0.3, 1.0], error);
        }
        if let SceneInspectorFieldState::InvalidPath(reason) = &field.state {
            ui.text_wrapped(reason);
        }
        ui.separator();
        if discard {
            self.drafts.remove(&key);
            return None;
        }
        if pending || (!apply && !remove) {
            return None;
        }
        draft.dirty = true;
        if !reapply {
            draft.remove_intent = remove;
        }
        let target = &draft.basis.target;
        let command = if draft.remove_intent {
            SceneInspectorCommand::RemoveField {
                entity: target.entity().clone(),
                component: target.component().clone(),
                component_version: target.component_version(),
                field: target.field().clone(),
            }
        } else {
            match converted.unwrap_or_else(|| draft.candidate.value(&draft.basis.field.shape)) {
                Ok(value) => SceneInspectorCommand::SetField {
                    entity: target.entity().clone(),
                    component: target.component().clone(),
                    component_version: target.component_version(),
                    field: target.field().clone(),
                    value,
                },
                Err(error) => {
                    draft.error = Some(error);
                    return None;
                }
            }
        };
        Some(PreparedFieldEdit {
            key,
            action: EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
                document: target.document(),
                base_revision: target.base_revision(),
                command,
            }),
        })
    }

    fn show_requested_recovery(&mut self, ui: &Ui, key: &FieldKey) {
        let Some(draft) = self.drafts.get(key).filter(|draft| draft.dirty) else {
            return;
        };
        let _id = ui.push_id(&format!(
            "retained:{}:{}:{}",
            key.entity.as_str(),
            key.component.as_str(),
            key.field.as_str()
        ));
        ui.text_disabled(if draft.incompatible {
            "Retained incompatible draft"
        } else {
            "Retained draft (temporarily unavailable)"
        });
        show_copy(ui, &draft.candidate);
        if button(ui, key, "Discard retained draft", draft.pending.is_none()) {
            self.drafts.remove(key);
        }
    }

    fn show_recovery(&mut self, ui: &Ui, document: EditorDocumentId) {
        let mut discard = Vec::new();
        for (key, draft) in &self.drafts {
            if key.document != document
                || !draft.dirty
                || !draft.incompatible
                || self.painted.contains(key)
            {
                continue;
            }
            let _id = ui.push_id(&format!(
                "retained:{}:{}:{}",
                key.entity.as_str(),
                key.component.as_str(),
                key.field.as_str()
            ));
            ui.text_wrapped(format!(
                "Retained incompatible input: {} / {} / {}",
                key.entity.as_str(),
                key.component.as_str(),
                key.field.as_str()
            ));
            show_copy(ui, &draft.candidate);
            if button(
                ui,
                key,
                "Discard incompatible draft",
                draft.pending.is_none(),
            ) {
                discard.push(key.clone());
            }
        }
        for key in discard {
            self.drafts.remove(&key);
        }
    }

    #[cfg(test)]
    pub(super) fn retained_text(
        &self,
        component: &str,
        field: &str,
    ) -> Option<(String, bool, bool, bool)> {
        self.drafts
            .iter()
            .find(|(key, draft)| {
                key.component.as_str() == component && key.field.as_str() == field && draft.dirty
            })
            .map(|(_, draft)| {
                (
                    draft.candidate.copy_text(),
                    draft.conflict,
                    draft.incompatible,
                    draft.pending.is_some(),
                )
            })
    }

    fn admitted(&mut self, key: &FieldKey, result: Result<PanelSubmissionId, PanelSubmitError>) {
        let Some(draft) = self.drafts.get_mut(key) else {
            return;
        };
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

    fn action_finished(&mut self, id: PanelSubmissionId, result: &PanelActionDispatch) {
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
                PanelActionDispatch::NotDispatched(super::PanelActionNotDispatched::Superseded) =>
                    "This edit was not submitted. Your input is retained; apply it again.".to_owned(),
                PanelActionDispatch::NotDispatched(super::PanelActionNotDispatched::Closing) =>
                    "Finish or cancel closing the document before applying this retained edit.".to_owned(),
                PanelActionDispatch::NotDispatched(_) =>
                    "The editing context changed before this edit was applied. Review the current value and try again.".to_owned(),
                PanelActionDispatch::Dispatched(_) =>
                    "The document rejected this edit. Your input is retained; check Diagnostics and correct or review it before retrying.".to_owned(),
            });
        }
    }
}

#[cfg(test)]
impl super::ImguiEditorFrontend {
    pub(crate) fn retained_field_draft(
        &self,
        component: &str,
        field: &str,
    ) -> Option<(String, bool, bool, bool)> {
        self.presentation
            .widgets
            .values()
            .find_map(|widgets| widgets.authoring.fields.retained_text(component, field))
    }

    pub(crate) fn retained_field_error(&self, component: &str, field: &str) -> Option<String> {
        self.presentation.widgets.values().find_map(|widgets| {
            widgets
                .authoring
                .fields
                .drafts
                .iter()
                .find(|(key, draft)| {
                    key.component.as_str() == component
                        && key.field.as_str() == field
                        && draft.dirty
                })
                .and_then(|(_, draft)| draft.error.clone())
        })
    }
}

impl authoring::ImguiAuthoringPanels {
    pub(super) fn action_finished(&mut self, id: PanelSubmissionId, result: &PanelActionDispatch) {
        self.fields.action_finished(id, result);
    }
}

impl ImguiEditorPanelContext<'_, '_> {
    /// Edits one stable root field in this panel's resolved document.
    ///
    /// Drafts, validation, exact submission results and conflict recovery are retained privately
    /// for this panel instance. An unprojected target is shown as unavailable without changing
    /// selection. Present a field at most once per panel paint.
    pub fn edit_field(
        &mut self,
        ui: &Ui,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        field: &ComponentFieldId,
    ) {
        let Some(document) = self.model.document() else {
            ui.text_disabled("This panel has no authoring document");
            return;
        };
        let frame = self.model.frame();
        let key = FieldKey {
            document: document.document,
            entity: entity.clone(),
            component: component.clone(),
            field: field.clone(),
        };
        let Some(scene) = frame
            .workspace()
            .active_scene
            .as_ref()
            .filter(|scene| scene.document == document.document)
        else {
            ui.text_disabled("The panel's document is not currently projected");
            self.widgets
                .authoring
                .fields
                .show_requested_recovery(ui, &key);
            return;
        };
        let Some(owner) = scene
            .editor
            .inspector
            .selected_entity_view
            .as_ref()
            .filter(|owner| &owner.id == entity)
        else {
            ui.text_disabled("The requested entity is not currently projected");
            self.widgets
                .authoring
                .fields
                .show_requested_recovery(ui, &key);
            return;
        };
        let Some(component) = owner
            .components
            .iter()
            .find(|candidate| &candidate.component == component)
        else {
            ui.text_disabled("The requested component is unavailable");
            self.widgets
                .authoring
                .fields
                .show_requested_recovery(ui, &key);
            return;
        };
        let Some(field) = component
            .fields
            .iter()
            .find(|candidate| &candidate.id == field)
        else {
            ui.text_disabled("The requested field is unavailable");
            self.widgets
                .authoring
                .fields
                .show_requested_recovery(ui, &key);
            return;
        };
        let _scope = ui.push_id(&format!(
            "document:{}:entity:{}:component:{}",
            document.document.raw(),
            entity.as_str(),
            component.component.as_str()
        ));
        let editable = !self.input.read_only
            && authoring::editing_enabled(frame)
            && component.access.is_editable()
            && field.capabilities.contains(&ComponentCapability::Edit);
        if let Some(prepared) = self.widgets.authoring.fields.show(
            ui,
            frame,
            scene,
            entity,
            component,
            field,
            editable,
            &mut self.output.text_owner,
        ) {
            self.submit_field(prepared);
        }
    }

    pub(super) fn show_scene_inspector(&mut self, ui: &Ui) {
        let mut prepared = Vec::new();
        self.widgets.authoring.show_inspector(
            ui,
            self.input.frame,
            self.actions,
            &mut self.output.text_owner,
            &mut prepared,
        );
        if let Some(document) = self.model.document() {
            self.widgets
                .authoring
                .fields
                .show_recovery(ui, document.document);
        }
        for edit in prepared {
            self.submit_field(edit);
        }
    }

    fn submit_field(&mut self, prepared: PreparedFieldEdit) {
        let result = self.submit_authoring(prepared.action);
        self.widgets
            .authoring
            .fields
            .admitted(&prepared.key, result);
    }
}

fn source(field: &SceneInspectorFieldView) -> Option<&ComponentValue> {
    field.value.as_ref().or(field.default_value.as_ref())
}

fn show_copy(ui: &Ui, candidate: &Candidate) {
    ui.text_wrapped(candidate.copy_text());
    if ui.button("Copy retained input") {
        let text = std::ffi::CString::new(candidate.copy_text().replace('\0', "?"))
            .expect("interior NUL characters were sanitized");
        ui.with_bound_context(|| {
            // SAFETY: Ui binds its live Context on this thread. Dear ImGui consumes the
            // NUL-terminated string synchronously through the installed clipboard backend.
            unsafe {
                dear_imgui_rs::sys::igSetClipboardText(text.as_ptr());
            }
        });
    }
}

fn button(ui: &Ui, key: &FieldKey, label: &str, enabled: bool) -> bool {
    let clicked = ui.with_disabled_if(!enabled, || ui.button(label));
    record(
        ui,
        format!(
            "field-action:{}:{}:{label}",
            key.component.as_str(),
            key.field.as_str()
        ),
    );
    if label == "Apply" {
        record(ui, "Apply".to_owned());
    }
    clicked
}

fn record(_ui: &Ui, _name: String) {
    #[cfg(test)]
    super::interaction_tests::record_control(_ui, _name);
}

#[cfg(test)]
mod tests;
