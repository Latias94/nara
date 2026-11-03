use std::{collections::HashMap, sync::Arc};

use nara::{
    reflect::{
        ComponentCapability, ComponentFieldPathSegment, ComponentValueKind, ComponentValueShape,
    },
    tooling::{
        EditorAction, EditorDocumentMutation, EditorFrameModel, SceneInspectorCommand,
        SceneInspectorFieldState,
    },
};
use nara_text::{FontFace, TextEdit, TextEditOutcome, TextEngine, TextEngineCreateError};
use nara_ui_kernel::{
    UiActionId, UiAuthoringError, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiBrush,
    UiColorRole, UiCompileError, UiCompiler, UiDimension, UiDispatchBatch, UiEdges, UiEnvironment,
    UiFlexDirection, UiFrame, UiKey, UiKeyError, UiLayer, UiLocalKey, UiNode, UiOverflow, UiPoint,
    UiRect, UiRoot, UiRootKey, UiRootStatus, UiSemanticRole, UiSemantics, UiStyle, UiStyleError,
    UiText, UiTextOverflow, UiTheme, UiVisual,
};
use nara_ui_widgets::{
    UiButton, UiButtonState, UiButtonVisualState, UiIntentBatch, UiIntentBatchError, UiTextField,
    UiTextFieldError, UiTextInputToken, UiWidgetError,
};
use thiserror::Error;

use super::PanelSubmissionId;
use super::authoring;
use super::field_drafts::{
    Candidate, FieldBasis, FieldDraft, FieldEditors, FieldKey, PreparedFieldEdit, source,
};

const TEXT_SIZE: f32 = 16.0;
const PANEL_INSET: f32 = 12.0;
const RETAINED_FRAMES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InspectorOperation {
    Apply,
    Discard,
    Reapply,
    Reset,
}

impl InspectorOperation {
    const ALL: [Self; 4] = [Self::Apply, Self::Discard, Self::Reapply, Self::Reset];

    const fn index(self) -> usize {
        match self {
            Self::Apply => 0,
            Self::Discard => 1,
            Self::Reapply => 2,
            Self::Reset => 3,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Apply => "Apply",
            Self::Discard => "Discard",
            Self::Reapply => "Reapply",
            Self::Reset => "Reset",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InspectorBinding {
    Field(FieldKey),
    Operation {
        field: UiKey,
        operation: InspectorOperation,
    },
}

struct FieldControl {
    identity: u64,
    key: UiKey,
    basis: FieldBasis,
    had_draft: bool,
    text: Option<UiTextField>,
    unavailable: Option<String>,
    allowed: bool,
    host_enabled: bool,
    buttons: [UiButtonState; 4],
}

impl FieldControl {
    fn input_key(&self) -> &UiKey {
        self.text.as_ref().map_or(&self.key, UiTextField::key)
    }
}

#[derive(Clone, PartialEq)]
struct DisplayedDraft {
    basis: FieldBasis,
    candidate: Candidate,
    dirty: bool,
    conflict: bool,
    incompatible: bool,
    queued: bool,
    pending: Option<PanelSubmissionId>,
    remove_intent: bool,
}

impl DisplayedDraft {
    fn observe(draft: &FieldDraft) -> Self {
        Self {
            basis: draft.basis.clone(),
            candidate: draft.candidate.clone(),
            dirty: draft.dirty,
            conflict: draft.conflict,
            incompatible: draft.incompatible,
            queued: draft.queued,
            pending: draft.pending,
            remove_intent: draft.remove_intent,
        }
    }
}

#[derive(Clone, PartialEq)]
struct DisplayedField {
    key: FieldKey,
    current: Option<FieldBasis>,
    draft: Option<DisplayedDraft>,
    display_text: Option<String>,
    operations: [bool; 4],
    width: f32,
}

#[derive(Clone)]
struct DisplayedButton {
    field: UiKey,
    operation: InspectorOperation,
    button: UiButton,
}

struct CompiledInspector {
    frame: Arc<UiFrame>,
    snapshot: UiAuthoringSnapshot,
    theme: UiTheme,
    projected_fields: HashMap<UiKey, DisplayedField>,
    fields: HashMap<UiKey, DisplayedField>,
    buttons: HashMap<UiKey, DisplayedButton>,
}

/// Panel-local CPU controls. The Host owns presentation receipts and native input admission.
pub(super) struct OwnedInspector {
    root: UiRootKey,
    engine: TextEngine,
    compiler: UiCompiler,
    next_identity: u64,
    next_snapshot: u64,
    controls: HashMap<FieldKey, FieldControl>,
    frames: Vec<CompiledInspector>,
    read_only: bool,
}

impl OwnedInspector {
    pub(super) fn new(root: UiRootKey, faces: Vec<FontFace>) -> Result<Self, OwnedInspectorError> {
        let engine = TextEngine::with_fonts(faces.clone(), Default::default())?;
        let compiler = UiCompiler::with_fonts(faces, Default::default())?;
        Ok(Self {
            root,
            engine,
            compiler,
            next_identity: 1,
            next_snapshot: 1,
            controls: HashMap::new(),
            frames: Vec::new(),
            read_only: false,
        })
    }

    #[cfg(test)]
    pub(super) fn root_key(&self) -> &UiRootKey {
        &self.root
    }

    pub(super) fn compiled_frame(&self) -> Option<&Arc<UiFrame>> {
        self.frames.last().map(|compiled| &compiled.frame)
    }

    pub(super) fn invalidated_controls(
        &self,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Result<Vec<UiKey>, OwnedInspectorError> {
        let previous = self.retained(previous)?;
        let candidate = self.retained(candidate)?;
        Ok(previous
            .buttons
            .iter()
            .filter(|(_, button)| {
                previous.fields.get(&button.field) != candidate.fields.get(&button.field)
            })
            .map(|(key, _)| key.clone())
            .collect())
    }

    #[cfg(test)]
    pub(super) fn authoring_snapshot(&self) -> Option<&UiAuthoringSnapshot> {
        self.frames.last().map(|compiled| &compiled.snapshot)
    }

    /// Call the existing FieldEditors::begin_paint once before painting this panel's fields.
    pub(super) fn compile(
        &mut self,
        model: &EditorFrameModel,
        fields: &mut FieldEditors,
        environment: UiEnvironment,
        theme: UiTheme,
        read_only: bool,
    ) -> Result<Arc<UiFrame>, OwnedInspectorError> {
        self.read_only = read_only;
        let width = environment.logical_size().width() - 2.0 * PANEL_INSET;
        if width <= 16.0 {
            return Err(OwnedInspectorError::InvalidViewport);
        }
        let mut nodes = vec![self.label("heading", "Inspector")?];
        let mut displayed = HashMap::new();
        let mut buttons = HashMap::new();
        let mut visible = Vec::new();
        if let Some(scene) = model.workspace().active_scene.as_ref() {
            if let Some(entity) = scene.editor.inspector.selected_entity_view.as_ref() {
                nodes.push(self.label("selection", entity.id.as_str())?);
                for component in &entity.components {
                    nodes.push(
                        UiNode::new(UiStyle::flex())
                            .with_key(UiLocalKey::runtime(
                                "nara.inspector.component",
                                component.component.as_str(),
                            )?)
                            .with_text(UiText::new(component.component.as_str(), TEXT_SIZE)?),
                    );
                    for field in &component.fields {
                        let basis = FieldBasis::new(model, scene, &entity.id, component, field);
                        let key = FieldKey::from_target(&basis.target);
                        if !fields.painted.insert(key.clone()) {
                            nodes.push(self.label(
                                &format!("duplicate.{}", visible.len()),
                                "This field already has an editor in this panel",
                            )?);
                            continue;
                        }
                        let editable = !read_only
                            && authoring::editing_enabled(model)
                            && component.access.is_editable()
                            && field.capabilities.contains(&ComponentCapability::Edit);
                        nodes.push(self.field_node(
                            key.clone(),
                            Some(basis),
                            editable,
                            width,
                            fields,
                            &mut displayed,
                            &mut buttons,
                        )?);
                        visible.push(key);
                    }
                }
                if entity.components.is_empty() {
                    nodes.push(self.label("empty", "No components")?);
                }
            } else {
                nodes.push(self.label("empty", "Select an entity")?);
            }
            let mut retained = fields
                .drafts
                .iter()
                .filter(|(key, draft)| {
                    key.document == scene.document && draft.dirty && !fields.painted.contains(*key)
                })
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            retained.sort_by(|left, right| {
                (
                    left.entity.as_str(),
                    left.component.as_str(),
                    left.field.as_str(),
                )
                    .cmp(&(
                        right.entity.as_str(),
                        right.component.as_str(),
                        right.field.as_str(),
                    ))
            });
            for key in retained {
                nodes.push(self.field_node(
                    key.clone(),
                    None,
                    false,
                    width,
                    fields,
                    &mut displayed,
                    &mut buttons,
                )?);
                visible.push(key);
            }
        } else {
            nodes.push(self.label("empty", "No active document")?);
        }
        self.controls.retain(|key, _| visible.contains(key));
        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill()
                .with_padding(UiEdges::all(PANEL_INSET)?)
                .with_gap(8.0)
                .with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::role(UiColorRole::Panel))
        .with_children(nodes);
        let snapshot = UiAuthoringSnapshotBuilder::new(self.next_snapshot)
            .with_root(root)
            .build()?;
        if let Some(previous) = self.frames.last()
            && previous.snapshot.roots() == snapshot.roots()
            && previous.frame.environment() == environment
            && previous.theme == theme
            && previous.projected_fields == displayed
        {
            return Ok(Arc::clone(&previous.frame));
        }
        let next_snapshot = self
            .next_snapshot
            .checked_add(1)
            .ok_or(OwnedInspectorError::IdentityExhausted)?;
        let frame = self.compiler.compile(&snapshot, environment, theme)?;
        self.next_snapshot = next_snapshot;
        let projected_fields = displayed.clone();
        if frame.root_outcomes().iter().any(|outcome| {
            outcome.key() == &self.root && outcome.status() == UiRootStatus::Published
        }) {
            let semantics = frame
                .semantics()
                .iter()
                .map(|node| (node.key(), node.enabled()))
                .collect::<HashMap<_, _>>();
            displayed.retain(|key, _| semantics.contains_key(key));
            buttons.retain(|key, _| semantics.contains_key(key));
            for field in displayed.values_mut() {
                field.operations = [false; 4];
            }
            for (key, button) in &buttons {
                if let Some(field) = displayed.get_mut(&button.field) {
                    field.operations[button.operation.index()] =
                        semantics.get(key).copied().unwrap_or(false);
                }
            }
        } else {
            displayed.clear();
            buttons.clear();
            for control in self.controls.values_mut() {
                if let Some(text) = &mut control.text {
                    text.blur(&mut self.engine)?;
                    text.set_enabled(false, &mut self.engine)?;
                }
            }
        }
        if self.frames.len() == RETAINED_FRAMES {
            self.frames.remove(0);
        }
        self.frames.push(CompiledInspector {
            frame: Arc::clone(&frame),
            snapshot,
            theme,
            projected_fields,
            fields: displayed,
            buttons,
        });
        Ok(frame)
    }

    pub(super) fn binding(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<InspectorBinding, OwnedInspectorError> {
        let compiled = self.retained(frame)?;
        if let Some(button) = compiled.buttons.get(key) {
            self.live_field(frame, &button.field)?;
            return Ok(InspectorBinding::Operation {
                field: button.field.clone(),
                operation: button.operation,
            });
        }
        let field = self.live_field(frame, key)?;
        Ok(InspectorBinding::Field(field.key.clone()))
    }

    #[cfg(test)]
    pub(super) fn focus(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<UiTextInputToken, OwnedInspectorError> {
        let key = self.live_field(frame, field)?.key.clone();
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text
            .as_mut()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        let token = text.focus()?;
        for (other, control) in &mut self.controls {
            if other != &key
                && let Some(text) = &mut control.text
                && text.input_token().is_some()
            {
                text.blur(&mut self.engine)?;
            }
        }
        Ok(token)
    }

    pub(super) fn blur(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<(), OwnedInspectorError> {
        let key = self.live_field(frame, field)?.key.clone();
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text
            .as_mut()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        Ok(text.blur(&mut self.engine)?)
    }

    #[cfg(test)]
    pub(super) fn set_enabled(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        enabled: bool,
    ) -> Result<(), OwnedInspectorError> {
        let key = self.live_field(frame, field)?.key.clone();
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text
            .as_mut()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        control.host_enabled = enabled;
        if !enabled {
            text.blur(&mut self.engine)?;
        }
        Ok(text.set_enabled(enabled && control.allowed, &mut self.engine)?)
    }

    pub(super) fn input_token(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<Option<UiTextInputToken>, OwnedInspectorError> {
        let displayed = self.live_field(frame, field)?;
        Ok(self.controls[&displayed.key]
            .text
            .as_ref()
            .ok_or(OwnedInspectorError::ReadOnlyField)?
            .input_token())
    }

    pub(super) fn cancel_input(&mut self) -> Result<(), OwnedInspectorError> {
        let mut failure = None;
        for control in self.controls.values_mut() {
            if let Some(text) = &mut control.text
                && let Err(error) = text.blur(&mut self.engine)
            {
                failure.get_or_insert(error);
            }
            for state in &mut control.buttons {
                *state = UiButtonState::default();
            }
        }
        match failure {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }

    pub(super) fn is_composing(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<bool, OwnedInspectorError> {
        let displayed = self.live_field(frame, field)?;
        let text = self.controls[&displayed.key]
            .text
            .as_ref()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if text.input_token() != Some(token) {
            return Err(UiTextFieldError::StaleInput.into());
        }
        Ok(text.editor().is_composing())
    }

    pub(super) fn edit(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
        fields: &mut FieldEditors,
    ) -> Result<TextEditOutcome, OwnedInspectorError> {
        let key = self.live_field(frame, field)?.key.clone();
        self.check_input(&key, fields)?;
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text
            .as_mut()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        let outcome = text.edit(token, edit, &mut self.engine)?;
        if outcome.text_changed {
            Self::adopt_text(fields, &key, text.editor().text())?;
        }
        Ok(outcome)
    }

    pub(super) fn point(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        position: UiPoint,
        extend: bool,
        fields: &mut FieldEditors,
    ) -> Result<TextEditOutcome, OwnedInspectorError> {
        let displayed = self.live_field(frame, field)?.clone();
        self.check_input(&displayed.key, fields)?;
        let control = self
            .controls
            .get_mut(&displayed.key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text
            .as_mut()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if displayed.display_text.as_deref() != Some(text.editor().display_text()) {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        if text.editor().is_composing() {
            return Err(OwnedInspectorError::Composing);
        }
        let field_rect = frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(field))
            .ok_or(OwnedInspectorError::ChangedProjection)?
            .rect();
        let text_key = frame
            .layouts()
            .iter()
            .find(|layout| layout.key().is_some() && layout.route_parent() == Some(field))
            .and_then(|layout| layout.key())
            .ok_or(OwnedInspectorError::ChangedProjection)?;
        let origin = frame
            .paint()
            .iter()
            .find_map(|primitive| match primitive {
                nara_ui_kernel::UiPaintPrimitive::Text {
                    key: Some(key),
                    rect,
                    ..
                } if key == text_key => Some(rect.origin()),
                _ => None,
            })
            .ok_or(OwnedInspectorError::ChangedProjection)?;
        let outcome = text.edit(
            token,
            &TextEdit::Point {
                x: field_rect.origin().x() + position.x() - origin.x(),
                y: field_rect.origin().y() + position.y() - origin.y(),
                extend,
            },
            &mut self.engine,
        )?;
        if outcome.text_changed {
            Self::adopt_text(fields, &displayed.key, text.editor().text())?;
        }
        Ok(outcome)
    }

    /// Returns field-local geometry; the Host maps it through the actual displayed image.
    pub(super) fn ime_area(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<UiRect, OwnedInspectorError> {
        let displayed = self.live_field(frame, field)?;
        let text = self.controls[&displayed.key]
            .text
            .as_ref()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if text.input_token() != Some(token) {
            return Err(UiTextFieldError::StaleInput.into());
        }
        Ok(text.ime_area(displayed.width)?)
    }

    /// Reduces Host-admitted routing facts. Returned intents still require explicit activation.
    pub(super) fn dispatch(
        &mut self,
        frame: &Arc<UiFrame>,
        dispatch: &UiDispatchBatch,
    ) -> Result<UiIntentBatch, OwnedInspectorError> {
        let compiled = self.retained(frame)?;
        if dispatch.frame_revision() != frame.revision() {
            return Err(OwnedInspectorError::ForeignDispatch);
        }
        let fields = compiled
            .fields
            .iter()
            .map(|(key, field)| (key.clone(), field.key.clone()))
            .collect::<Vec<_>>();
        let buttons = compiled.buttons.values().cloned().collect::<Vec<_>>();
        for (input, key) in fields {
            if let Some(control) = self.controls.get_mut(&key)
                && control.input_key() == &input
                && let Some(text) = &mut control.text
            {
                text.reduce_focus(dispatch, &mut self.engine)?;
            }
        }
        let mut intents = UiIntentBatch::default();
        for button in buttons {
            let Ok(displayed) = self.live_field(frame, &button.field) else {
                continue;
            };
            let key = displayed.key.clone();
            let control = self
                .controls
                .get_mut(&key)
                .ok_or(OwnedInspectorError::RetiredField)?;
            control.buttons[button.operation.index()].reduce_batch(
                &button.button,
                dispatch,
                &mut intents,
            )?;
        }
        Ok(intents)
    }

    pub(super) fn activate(
        &mut self,
        frame: &Arc<UiFrame>,
        button: &UiKey,
        model: &EditorFrameModel,
        fields: &mut FieldEditors,
    ) -> Result<Option<PreparedFieldEdit>, OwnedInspectorError> {
        let binding = self
            .retained(frame)?
            .buttons
            .get(button)
            .ok_or(OwnedInspectorError::UnknownControl)?
            .clone();
        self.operate(frame, &binding.field, binding.operation, model, fields)
    }

    pub(super) fn operate(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        operation: InspectorOperation,
        model: &EditorFrameModel,
        fields: &mut FieldEditors,
    ) -> Result<Option<PreparedFieldEdit>, OwnedInspectorError> {
        let displayed = self.live_field(frame, field)?.clone();
        if !displayed.operations[operation.index()] {
            return Err(OwnedInspectorError::OperationDisabled(operation));
        }
        let key = &displayed.key;
        let current = current_basis(model, key);
        let draft = fields
            .drafts
            .get_mut(key)
            .ok_or(OwnedInspectorError::ChangedDraft)?;
        if let Some((basis, _)) = &current {
            draft.synchronize(basis.clone());
        }
        if displayed.draft.as_ref() != Some(&DisplayedDraft::observe(draft)) {
            return Err(OwnedInspectorError::ChangedDraft);
        }
        if draft.in_flight() {
            return Err(OwnedInspectorError::Pending);
        }
        if operation == InspectorOperation::Discard {
            fields.drafts.remove(key);
            self.controls.remove(key);
            return Ok(None);
        }
        let (basis, editable) = current.ok_or(OwnedInspectorError::ChangedProjection)?;
        if displayed.current.as_ref() != Some(&basis) {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        let control = self
            .controls
            .get(key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text
            .as_ref()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if self.read_only || !editable || !control.host_enabled || !text.enabled() {
            return Err(OwnedInspectorError::OperationDisabled(operation));
        }
        if text.editor().is_composing() {
            return Err(OwnedInspectorError::Composing);
        }
        if draft.incompatible || (draft.conflict && operation != InspectorOperation::Reapply) {
            return Err(OwnedInspectorError::OperationDisabled(operation));
        }
        if operation == InspectorOperation::Reapply {
            draft.basis = basis;
            draft.conflict = false;
        }
        if operation == InspectorOperation::Reset {
            if !can_reset(&draft.basis) {
                return Err(OwnedInspectorError::OperationDisabled(operation));
            }
            draft.remove_intent = true;
        }
        draft.dirty = true;
        let target = &draft.basis.target;
        let command = if draft.remove_intent {
            SceneInspectorCommand::RemoveField {
                entity: target.entity().clone(),
                component: target.component().clone(),
                component_version: target.component_version(),
                field: target.field().clone(),
            }
        } else {
            let value = match draft.candidate.value(&draft.basis.field.shape) {
                Ok(value) => value,
                Err(error) => {
                    draft.error = Some(error.clone());
                    return Err(OwnedInspectorError::InvalidCandidate(error));
                }
            };
            SceneInspectorCommand::SetField {
                entity: target.entity().clone(),
                component: target.component().clone(),
                component_version: target.component_version(),
                field: target.field().clone(),
                value,
            }
        };
        draft.error = None;
        draft.queued = true;
        Ok(Some(PreparedFieldEdit {
            key: key.clone(),
            action: EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
                document: target.document(),
                base_revision: target.base_revision(),
                command,
            }),
        }))
    }

    fn retained(&self, frame: &Arc<UiFrame>) -> Result<&CompiledInspector, OwnedInspectorError> {
        self.frames
            .iter()
            .find(|compiled| Arc::ptr_eq(&compiled.frame, frame))
            .ok_or(OwnedInspectorError::RetiredFrame)
    }

    fn live_field(
        &self,
        frame: &Arc<UiFrame>,
        input: &UiKey,
    ) -> Result<&DisplayedField, OwnedInspectorError> {
        let field = self
            .retained(frame)?
            .fields
            .get(input)
            .ok_or(OwnedInspectorError::UnknownControl)?;
        let control = self
            .controls
            .get(&field.key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        if control.input_key() != input {
            return Err(OwnedInspectorError::RetiredField);
        }
        Ok(field)
    }

    fn check_input(
        &self,
        key: &FieldKey,
        fields: &FieldEditors,
    ) -> Result<(), OwnedInspectorError> {
        if !fields.has_draft_capacity(key) {
            return Err(OwnedInspectorError::DraftCapacity);
        }
        let draft = fields
            .drafts
            .get(key)
            .ok_or(OwnedInspectorError::ChangedDraft)?;
        let control = self
            .controls
            .get(key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text
            .as_ref()
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if draft.basis != control.basis
            || !matches!(&draft.candidate, Candidate::Text(value) if value == text.editor().text())
        {
            return Err(OwnedInspectorError::ChangedDraft);
        }
        if draft.in_flight() {
            return Err(OwnedInspectorError::Pending);
        }
        if draft.incompatible || !control.allowed || !control.host_enabled {
            return Err(OwnedInspectorError::ReadOnlyField);
        }
        Ok(())
    }

    fn adopt_text(
        fields: &mut FieldEditors,
        key: &FieldKey,
        text: &str,
    ) -> Result<(), OwnedInspectorError> {
        let draft = fields
            .drafts
            .get_mut(key)
            .ok_or(OwnedInspectorError::ChangedDraft)?;
        draft.candidate = Candidate::Text(text.to_owned());
        draft.dirty = true;
        draft.remove_intent = false;
        draft.error = draft.candidate.value(&draft.basis.field.shape).err();
        Ok(())
    }

    fn label(&self, identity: &str, text: &str) -> Result<UiNode, OwnedInspectorError> {
        Ok(UiNode::new(UiStyle::flex().with_flex_shrink(0.0))
            .with_key(UiLocalKey::runtime("nara.inspector.label", identity)?)
            .with_text(UiText::new(text, TEXT_SIZE)?.with_overflow(UiTextOverflow::Clip)))
    }

    #[allow(clippy::too_many_arguments)]
    fn field_node(
        &mut self,
        key: FieldKey,
        current: Option<FieldBasis>,
        editable: bool,
        width: f32,
        fields: &mut FieldEditors,
        displayed: &mut HashMap<UiKey, DisplayedField>,
        buttons: &mut HashMap<UiKey, DisplayedButton>,
    ) -> Result<UiNode, OwnedInspectorError> {
        if !fields.drafts.contains_key(&key)
            && self
                .controls
                .get(&key)
                .is_some_and(|control| control.had_draft)
        {
            self.controls.remove(&key);
        }
        let capacity = fields.has_draft_capacity(&key);
        if let Some(basis) = &current {
            if !fields.drafts.contains_key(&key) && scalar_text(basis) && capacity {
                fields
                    .drafts
                    .insert(key.clone(), FieldDraft::new(basis.clone()));
            }
            if let Some(draft) = fields.drafts.get_mut(&key) {
                draft.synchronize(basis.clone());
            }
        }
        let draft = fields.drafts.get(&key);
        let basis = draft
            .map(|draft| &draft.basis)
            .or(current.as_ref())
            .ok_or(OwnedInspectorError::ChangedDraft)?;
        let replace = self.controls.get(&key).is_none_or(|control| {
            control.basis != *basis
                || control.had_draft != draft.is_some()
                || matches!((&control.text, draft), (Some(text), Some(draft))
                    if !matches!(&draft.candidate, Candidate::Text(value) if value == text.editor().text()))
        });
        if replace {
            let identity = self.next_identity;
            self.next_identity = identity
                .checked_add(1)
                .ok_or(OwnedInspectorError::IdentityExhausted)?;
            let row_key = UiKey::scoped(
                self.root.clone(),
                UiLocalKey::runtime("nara.inspector.field", identity.to_string())?,
            )?;
            let mut unavailable = None;
            let text = if let Some(draft) = draft
                && scalar_text(&draft.basis)
                && let Candidate::Text(value) = &draft.candidate
            {
                match UiTextField::new(
                    self.root.clone(),
                    draft.basis.field.path.to_string(),
                    value,
                    TEXT_SIZE,
                    &mut self.engine,
                ) {
                    Ok(text) => Some(text),
                    Err(error) => {
                        unavailable = Some(format!("Read-only: {error}"));
                        None
                    }
                }
            } else {
                None
            };
            self.controls.insert(
                key.clone(),
                FieldControl {
                    identity,
                    key: row_key,
                    basis: basis.clone(),
                    had_draft: draft.is_some(),
                    text,
                    unavailable,
                    allowed: false,
                    host_enabled: true,
                    buttons: [UiButtonState::default(); 4],
                },
            );
        }
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let identity = control.identity;
        let current_scalar = current.as_ref().is_some_and(scalar_text);
        control.allowed = editable
            && current_scalar
            && capacity
            && draft.is_some_and(|draft| !draft.incompatible && !draft.in_flight());
        if let Some(text) = &mut control.text {
            let enabled = control.allowed && control.host_enabled;
            if !enabled {
                text.blur(&mut self.engine)?;
            }
            text.set_enabled(enabled, &mut self.engine)?;
        }
        let input = control.input_key().clone();
        let control_key = control
            .key
            .local_key()
            .cloned()
            .ok_or(OwnedInspectorError::UnknownControl)?;
        let text_node = control
            .text
            .as_ref()
            .map(|text| {
                text.authored_node(
                    &self.root,
                    UiStyle::flex()
                        .with_width(UiDimension::Px(width))
                        .with_height(UiDimension::Px(36.0))
                        .with_flex_shrink(0.0),
                    width,
                )
            })
            .transpose()?;
        let unavailable = control.unavailable.clone();
        let host_enabled = control.host_enabled;
        let has_text = control.text.is_some();
        let composing = control
            .text
            .as_ref()
            .is_some_and(|text| text.editor().is_composing());
        let display_text = control
            .text
            .as_ref()
            .map(|text| text.editor().display_text().to_owned());
        let states = control.buttons;
        let mut children = vec![self.label(
            &format!("{identity}.path"),
            &format!(
                "{}{}",
                basis.field.path,
                if basis.field.required { " *" } else { "" }
            ),
        )?];
        if let Some(node) = text_node {
            children.push(node);
        } else {
            children.push(self.label(
                &format!("{identity}.value"),
                &authoring::value_label(source(&current.as_ref().unwrap_or(basis).field)),
            )?);
        }
        let mut operations = [false; 4];
        if let Some(draft) = draft {
            let can_edit = editable
                && current_scalar
                && capacity
                && has_text
                && !composing
                && host_enabled
                && !draft.incompatible
                && !draft.in_flight();
            operations[InspectorOperation::Apply.index()] =
                can_edit && draft.dirty && !draft.conflict;
            operations[InspectorOperation::Discard.index()] =
                !draft.in_flight() && (draft.dirty || draft.incompatible);
            operations[InspectorOperation::Reapply.index()] =
                can_edit && draft.dirty && draft.conflict;
            operations[InspectorOperation::Reset.index()] =
                can_edit && !draft.conflict && current.as_ref().is_some_and(can_reset);
            if draft.incompatible {
                children.push(self.label(
                    &format!("{identity}.incompatible"),
                    "The schema or source changed. Retained input requires Discard before reopening.",
                )?);
            } else if draft.conflict {
                children.push(self.label(
                    &format!("{identity}.conflict"),
                    "Conflict: review the current value before choosing Reapply.",
                )?);
            }
            if draft.conflict || draft.incompatible {
                if let Some(current) = &current {
                    children.push(self.label(
                        &format!("{identity}.current"),
                        &format!(
                            "Current: {}",
                            authoring::value_label(source(&current.field))
                        ),
                    )?);
                }
                children.push(self.label(
                    &format!("{identity}.retained"),
                    &format!("Retained: {}", draft.candidate.copy_text()),
                )?);
            }
            if draft.remove_intent {
                children.push(self.label(
                    &format!("{identity}.remove"),
                    "Retained intent: remove the authored override",
                )?);
            }
            if draft.in_flight() {
                children.push(self.label(
                    &format!("{identity}.pending"),
                    "Submitting this field edit...",
                )?);
            }
            if let Some(error) = &draft.error {
                children.push(self.label(&format!("{identity}.error"), error)?);
            }
            let mut actions = Vec::new();
            for operation in InspectorOperation::ALL {
                let button_key = UiKey::scoped(
                    self.root.clone(),
                    UiLocalKey::runtime(
                        "nara.inspector.action",
                        format!("{identity}.{}", operation.label()),
                    )?,
                )?;
                let button = UiButton::new(
                    button_key.clone(),
                    operation.label(),
                    UiActionId::new(operation.label())?,
                )?
                .with_enabled(operations[operation.index()]);
                let role = match states[operation.index()].visual_state(&button) {
                    UiButtonVisualState::Disabled => UiColorRole::Disabled,
                    UiButtonVisualState::Pressed | UiButtonVisualState::Focused => {
                        UiColorRole::Focus
                    }
                    UiButtonVisualState::Hovered => UiColorRole::SecondaryText,
                    UiButtonVisualState::Idle => UiColorRole::Panel,
                };
                actions.push(
                    button.authored_node(
                        &self.root,
                        UiStyle::flex()
                            .with_width(UiDimension::Px((width - 12.0) / 4.0))
                            .with_height(UiDimension::Px(30.0))
                            .with_flex_shrink(0.0),
                        UiVisual::role(role),
                        UiBrush::Role(UiColorRole::PrimaryText),
                        TEXT_SIZE,
                    )?,
                );
                buttons.insert(
                    button_key,
                    DisplayedButton {
                        field: input.clone(),
                        operation,
                        button,
                    },
                );
            }
            children.push(
                UiNode::new(
                    UiStyle::flex()
                        .with_flex_direction(UiFlexDirection::Row)
                        .with_gap(4.0),
                )
                .with_children(actions),
            );
        }
        if let Some(unavailable) = unavailable {
            children.push(self.label(&format!("{identity}.unavailable"), &unavailable)?);
        }
        if !current_scalar {
            children.push(self.label(
                &format!("{identity}.readonly"),
                if current.is_some() {
                    "Read-only: this panel edits only scalar strings and numbers"
                } else {
                    "This field is not currently projected. Its retained draft is unchanged."
                },
            )?);
        } else if !capacity {
            children.push(self.label(
                &format!("{identity}.capacity"),
                "Field draft capacity reached. Discard retained edits to enable another field.",
            )?);
        } else if !editable {
            children.push(self.label(
                &format!("{identity}.readonly"),
                "Read-only in the current editing context",
            )?);
        }
        if let Some(current) = &current
            && let SceneInspectorFieldState::InvalidPath(reason) = &current.field.state
        {
            children.push(self.label(&format!("{identity}.path-error"), reason)?);
        }
        let node = UiNode::new(UiStyle::flex().with_gap(4.0).with_flex_shrink(0.0))
            .with_key(control_key)
            .with_semantics(UiSemantics::new(
                UiSemanticRole::Group,
                basis.field.path.to_string(),
            )?)
            .with_children(children);
        displayed.insert(
            input,
            DisplayedField {
                key,
                current,
                draft: draft.map(DisplayedDraft::observe),
                display_text,
                operations,
                width,
            },
        );
        Ok(node)
    }
}

fn scalar_text(basis: &FieldBasis) -> bool {
    matches!(
        basis.field.shape,
        ComponentValueShape::Scalar {
            kind: ComponentValueKind::String
                | ComponentValueKind::I64
                | ComponentValueKind::U64
                | ComponentValueKind::F64,
            ..
        }
    )
}

fn can_reset(basis: &FieldBasis) -> bool {
    basis.field.state == SceneInspectorFieldState::Present
        && matches!(
            basis.field.path.segments().last(),
            Some(ComponentFieldPathSegment::Field(_))
        )
        && (!basis.field.required || basis.field.default_value.is_some())
}

fn current_basis(model: &EditorFrameModel, key: &FieldKey) -> Option<(FieldBasis, bool)> {
    let scene = model
        .workspace()
        .active_scene
        .as_ref()
        .filter(|scene| scene.document == key.document)?;
    let entity = scene
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .filter(|entity| entity.id == key.entity)?;
    let component = entity
        .components
        .iter()
        .find(|component| component.component == key.component)?;
    let field = component
        .fields
        .iter()
        .find(|field| field.id == key.field)?;
    let editable = authoring::editing_enabled(model)
        && component.access.is_editable()
        && field.capabilities.contains(&ComponentCapability::Edit);
    Some((
        FieldBasis::new(model, scene, &entity.id, component, field),
        editable,
    ))
}

#[derive(Debug, Error)]
pub(super) enum OwnedInspectorError {
    #[error(transparent)]
    Engine(#[from] TextEngineCreateError),
    #[error(transparent)]
    Compile(#[from] UiCompileError),
    #[error(transparent)]
    Authoring(#[from] UiAuthoringError),
    #[error(transparent)]
    Key(#[from] UiKeyError),
    #[error(transparent)]
    Style(#[from] UiStyleError),
    #[error(transparent)]
    TextField(#[from] UiTextFieldError),
    #[error(transparent)]
    Button(#[from] UiWidgetError),
    #[error(transparent)]
    Intents(#[from] UiIntentBatchError),
    #[error("Inspector identity is exhausted")]
    IdentityExhausted,
    #[error("Inspector viewport has no positive field content width")]
    InvalidViewport,
    #[error("the exact compiled Inspector frame is no longer retained")]
    RetiredFrame,
    #[error("the UI key is not a control in this compiled Inspector frame")]
    UnknownControl,
    #[error("the compiled field incarnation has retired")]
    RetiredField,
    #[error("the dispatch belongs to another compiled frame")]
    ForeignDispatch,
    #[error("the field draft changed after this frame was compiled")]
    ChangedDraft,
    #[error("the current field projection differs from the compiled action basis")]
    ChangedProjection,
    #[error("this field is read-only")]
    ReadOnlyField,
    #[error("the retained field draft budget is exhausted")]
    DraftCapacity,
    #[error("this field already has an exact pending submission")]
    Pending,
    #[error("finish or cancel composition before applying this field")]
    Composing,
    #[error("the compiled field does not enable {0:?}")]
    OperationDisabled(InspectorOperation),
    #[error("the retained field candidate is invalid: {0}")]
    InvalidCandidate(String),
}

#[cfg(test)]
mod tests;
