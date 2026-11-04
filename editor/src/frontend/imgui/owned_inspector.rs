use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

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
use super::field_drafts::structure::{self, DraftAction, DraftInput};
use super::field_drafts::{
    Candidate, FieldBasis, FieldDraft, FieldEditors, FieldKey, PreparedFieldEdit, source,
};

const TEXT_SIZE: f32 = 16.0;
const PANEL_INSET: f32 = 12.0;
const RETAINED_FRAMES: usize = 3;

mod recovery;
mod references;
mod structured;
mod tools;
mod viewport;

use recovery::DisplayedRecovery;
pub(super) use references::ReferenceAction;
use references::{ReferenceBinding, ReferencePicker};
pub(super) use tools::ToolAction;
use tools::{DisplayedTool, ToolBasis, ToolControl};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InspectorOperation {
    Apply,
    Discard,
    Reapply,
    Reset,
    Toggle,
}

impl InspectorOperation {
    const FOOTER: [Self; 4] = [Self::Apply, Self::Discard, Self::Reapply, Self::Reset];
    const COUNT: usize = 5;

    const fn index(self) -> usize {
        match self {
            Self::Apply => 0,
            Self::Discard => 1,
            Self::Reapply => 2,
            Self::Reset => 3,
            Self::Toggle => 4,
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Apply => "Apply",
            Self::Discard => "Discard",
            Self::Reapply => "Reapply",
            Self::Reset => "Reset",
            Self::Toggle => "Toggle",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InspectorBinding {
    Field(FieldKey),
    Copy {
        field: UiKey,
    },
    DiscardRetainedDrafts,
    SourceTool {
        field: UiKey,
        action: ToolAction,
    },
    Operation {
        field: UiKey,
        operation: InspectorOperation,
    },
    Reference {
        field: UiKey,
        input: Option<DraftInput>,
        action: ReferenceAction,
    },
    Structure {
        field: UiKey,
        action: DraftAction,
    },
}

struct FieldControl {
    identity: u64,
    key: UiKey,
    basis: FieldBasis,
    had_draft: bool,
    input: FieldInput,
    unavailable: Option<String>,
    allowed: bool,
    host_enabled: bool,
    buttons: [UiButtonState; InspectorOperation::COUNT],
    reference: Option<ReferencePicker>,
    copy_button: UiButtonState,
    copy_feedback: Option<String>,
    tool: Option<ToolControl>,
}

struct ButtonControl {
    key: UiKey,
    state: UiButtonState,
}

impl FieldControl {
    fn reference(&self, input: Option<&DraftInput>) -> Option<&ReferencePicker> {
        match input {
            None => self.reference.as_ref(),
            Some(input) => match &self.input {
                FieldInput::Structured(controls) => controls.references.get(input),
                _ => None,
            },
        }
    }

    fn reference_mut(&mut self, input: Option<&DraftInput>) -> Option<&mut ReferencePicker> {
        match input {
            None => self.reference.as_mut(),
            Some(input) => match &mut self.input {
                FieldInput::Structured(controls) => controls.references.get_mut(input),
                _ => None,
            },
        }
    }

    fn input_key(&self) -> &UiKey {
        match &self.input {
            FieldInput::Text(text) => text.key(),
            FieldInput::Boolean(key) => key,
            FieldInput::Structured(_) | FieldInput::ReadOnly => &self.key,
        }
    }

    fn text(&self) -> Option<&UiTextField> {
        match &self.input {
            FieldInput::Text(text) => Some(text),
            _ => None,
        }
    }

    fn text_at(&self, input: &UiKey) -> Option<&UiTextField> {
        match &self.input {
            FieldInput::Text(text) if text.key() == input => Some(text),
            FieldInput::Structured(controls) => controls
                .inputs
                .values()
                .find(|text| text.key() == input)
                .map(Box::as_ref),
            _ => None,
        }
    }

    fn text_at_mut(&mut self, input: &UiKey) -> Option<&mut UiTextField> {
        match &mut self.input {
            FieldInput::Text(text) if text.key() == input => Some(text),
            FieldInput::Structured(controls) => controls
                .inputs
                .values_mut()
                .find(|text| text.key() == input)
                .map(Box::as_mut),
            _ => None,
        }
    }

    fn text_target(&self, input: &UiKey) -> Option<&DraftInput> {
        match &self.input {
            FieldInput::Structured(controls) => controls
                .inputs
                .iter()
                .find_map(|(target, text)| (text.key() == input).then_some(target)),
            _ => None,
        }
    }

    fn texts_mut(&mut self) -> impl Iterator<Item = &mut UiTextField> {
        let (single, children) = match &mut self.input {
            FieldInput::Text(text) => (Some(text.as_mut()), None),
            FieldInput::Structured(controls) => (None, Some(&mut controls.inputs)),
            _ => (None, None),
        };
        single.into_iter().chain(
            children
                .into_iter()
                .flat_map(|inputs| inputs.values_mut().map(Box::as_mut)),
        )
    }

    fn is_composing(&self) -> bool {
        match &self.input {
            FieldInput::Text(text) => text.editor().is_composing(),
            FieldInput::Structured(controls) => controls
                .inputs
                .values()
                .any(|text| text.editor().is_composing()),
            _ => false,
        }
    }
}

enum FieldInput {
    Text(Box<UiTextField>),
    Boolean(UiKey),
    Structured(Box<structured::StructuredControls>),
    ReadOnly,
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
    operations: [bool; InspectorOperation::COUNT],
    reference_views: HashMap<Option<DraftInput>, u64>,
    tool: Option<DisplayedTool>,
    width: f32,
}

struct DisplayedInput {
    field: UiKey,
    display_text: String,
    width: f32,
}

#[derive(Clone)]
struct DisplayedButton {
    field: UiKey,
    action: InspectorButtonAction,
    button: UiButton,
}

#[derive(Clone)]
enum InspectorButtonAction {
    Operation(InspectorOperation),
    Reference(ReferenceBinding),
    Structure(DraftAction),
    Copy,
    SourceTool(ToolAction),
}

struct CompiledInspector {
    frame: Arc<UiFrame>,
    snapshot: UiAuthoringSnapshot,
    theme: UiTheme,
    projected_fields: HashMap<UiKey, DisplayedField>,
    fields: HashMap<UiKey, DisplayedField>,
    inputs: HashMap<UiKey, DisplayedInput>,
    buttons: HashMap<UiKey, DisplayedButton>,
    projected_recovery: Option<DisplayedRecovery>,
    recovery: Option<DisplayedRecovery>,
}

/// Panel-local CPU controls. The Host owns presentation receipts and native input admission.
pub(super) struct OwnedInspector {
    root: UiRootKey,
    content_key: UiKey,
    scroll_pixels: f32,
    engine: TextEngine,
    compiler: UiCompiler,
    next_identity: u64,
    next_snapshot: u64,
    controls: HashMap<FieldKey, FieldControl>,
    frames: Vec<CompiledInspector>,
    read_only: bool,
    recovery_button: ButtonControl,
}

impl OwnedInspector {
    pub(super) fn new(root: UiRootKey, faces: Vec<FontFace>) -> Result<Self, OwnedInspectorError> {
        let engine = TextEngine::with_fonts(faces.clone(), Default::default())?;
        let compiler = UiCompiler::with_fonts(faces, Default::default())?;
        let content_key = UiKey::scoped(
            root.clone(),
            UiLocalKey::runtime("nara.inspector.content", "fields")?,
        )?;
        let recovery_key = UiKey::scoped(
            root.clone(),
            UiLocalKey::runtime("nara.inspector.recovery", "discard")?,
        )?;
        Ok(Self {
            root,
            content_key,
            scroll_pixels: 0.0,
            engine,
            compiler,
            next_identity: 1,
            next_snapshot: 1,
            controls: HashMap::new(),
            frames: Vec::new(),
            read_only: false,
            recovery_button: ButtonControl {
                key: recovery_key,
                state: UiButtonState::default(),
            },
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
        let mut invalidated = previous
            .buttons
            .iter()
            .filter(|(_, button)| {
                let before = previous.fields.get(&button.field);
                let after = candidate.fields.get(&button.field);
                if matches!(button.action, InspectorButtonAction::Copy) {
                    before.map(|field| (&field.key, &field.draft))
                        != after.map(|field| (&field.key, &field.draft))
                } else {
                    before != after
                }
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        if previous.recovery != candidate.recovery {
            invalidated.push(self.recovery_button.key.clone());
        }
        Ok(invalidated)
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
        tool_presentation: Option<&super::source_tool::OwnedSourceToolPresentation>,
    ) -> Result<Arc<UiFrame>, OwnedInspectorError> {
        self.read_only = read_only;
        let width = environment.logical_size().width() - 2.0 * PANEL_INSET;
        if width <= 16.0 {
            return Err(OwnedInspectorError::InvalidViewport);
        }
        let mut nodes = vec![self.label("heading", "Inspector")?];
        let mut displayed = HashMap::new();
        let mut inputs = HashMap::new();
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
                        let tool_basis = ToolBasis::capture(model, &key, &basis).filter(|_| {
                            component.access.is_editable()
                                && field.capabilities.contains(&ComponentCapability::Edit)
                        });
                        nodes.push(self.field_node(
                            key.clone(),
                            Some(basis),
                            editable,
                            width,
                            fields,
                            &mut displayed,
                            &mut inputs,
                            &mut buttons,
                            &scene.editor.inspector.entities,
                            tool_basis,
                            tool_presentation,
                            model,
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
            let mut retained_owner = None;
            for key in retained {
                let owner = (key.entity.clone(), key.component.clone());
                if retained_owner.as_ref() != Some(&owner) {
                    nodes.push(self.label(
                        &format!("retained.{}", visible.len()),
                        &format!(
                            "Retained input: {} / {}",
                            key.entity.as_str(),
                            key.component.as_str(),
                        ),
                    )?);
                    retained_owner = Some(owner);
                }
                nodes.push(self.field_node(
                    key.clone(),
                    None,
                    false,
                    width,
                    fields,
                    &mut displayed,
                    &mut inputs,
                    &mut buttons,
                    &scene.editor.inspector.entities,
                    None,
                    tool_presentation,
                    model,
                )?);
                visible.push(key);
            }
        } else {
            nodes.push(self.label("empty", "No active document")?);
        }
        self.controls.retain(|key, _| visible.contains(key));
        let projected_recovery = self.recovery_node(fields, width)?.map(|(node, recovery)| {
            nodes.insert(1, node);
            recovery
        });
        let retry_nodes = (self.scroll_pixels > 0.0).then(|| nodes.clone());
        let mut snapshot = self.viewport_snapshot(nodes)?;
        if let Some(previous) = self.frames.last()
            && previous.snapshot.roots() == snapshot.roots()
            && previous.frame.environment() == environment
            && previous.theme == theme
            && previous.projected_fields == displayed
            && previous.projected_recovery == projected_recovery
        {
            return Ok(Arc::clone(&previous.frame));
        }
        let next_snapshot = self
            .next_snapshot
            .checked_add(1)
            .ok_or(OwnedInspectorError::IdentityExhausted)?;
        let mut frame = self.compiler.compile(&snapshot, environment, theme)?;
        self.next_snapshot = next_snapshot;
        if let Some(nodes) = retry_nodes
            && self.clamp_scroll(&frame)
        {
            snapshot = self.viewport_snapshot(nodes)?;
            let next_snapshot = self
                .next_snapshot
                .checked_add(1)
                .ok_or(OwnedInspectorError::IdentityExhausted)?;
            frame = self.compiler.compile(&snapshot, environment, theme)?;
            self.next_snapshot = next_snapshot;
        }
        let projected_fields = displayed.clone();
        let mut recovery = projected_recovery.clone();
        if frame.root_outcomes().iter().any(|outcome| {
            outcome.key() == &self.root && outcome.status() == UiRootStatus::Published
        }) {
            let semantics = frame
                .semantics()
                .iter()
                .map(|node| (node.key(), node.enabled()))
                .collect::<HashMap<_, _>>();
            buttons.retain(|key, _| semantics.contains_key(key));
            inputs.retain(|key, _| semantics.contains_key(key));
            if !semantics.contains_key(&self.recovery_button.key) {
                recovery = None;
            }
            let button_fields = buttons
                .values()
                .map(|button| button.field.clone())
                .chain(
                    inputs
                        .values()
                        .map(|input: &DisplayedInput| input.field.clone()),
                )
                .collect::<HashSet<_>>();
            displayed.retain(|key, _| semantics.contains_key(key) || button_fields.contains(key));
            for field in displayed.values_mut() {
                field.operations = [false; InspectorOperation::COUNT];
            }
            for (key, button) in &buttons {
                if let Some(field) = displayed.get_mut(&button.field)
                    && let InspectorButtonAction::Operation(operation) = button.action
                {
                    field.operations[operation.index()] =
                        semantics.get(key).copied().unwrap_or(false);
                }
            }
        } else {
            displayed.clear();
            inputs.clear();
            buttons.clear();
            recovery = None;
            self.recovery_button.state = UiButtonState::default();
            for control in self.controls.values_mut() {
                for text in control.texts_mut() {
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
            inputs,
            buttons,
            projected_recovery,
            recovery,
        });
        Ok(frame)
    }

    pub(super) fn binding(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<InspectorBinding, OwnedInspectorError> {
        let compiled = self.retained(frame)?;
        if key == &self.recovery_button.key && compiled.recovery.is_some() {
            return Ok(InspectorBinding::DiscardRetainedDrafts);
        }
        if let Some(button) = compiled.buttons.get(key) {
            self.live_field(frame, &button.field)?;
            return Ok(match &button.action {
                InspectorButtonAction::Operation(operation) => InspectorBinding::Operation {
                    field: button.field.clone(),
                    operation: *operation,
                },
                InspectorButtonAction::Reference(reference) => InspectorBinding::Reference {
                    field: button.field.clone(),
                    input: reference.input.clone(),
                    action: reference.action.clone(),
                },
                InspectorButtonAction::Structure(action) => InspectorBinding::Structure {
                    field: button.field.clone(),
                    action: action.clone(),
                },
                InspectorButtonAction::Copy => InspectorBinding::Copy {
                    field: button.field.clone(),
                },
                InspectorButtonAction::SourceTool(action) => InspectorBinding::SourceTool {
                    field: button.field.clone(),
                    action: *action,
                },
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
        let key = self.live_input(frame, field)?.key.clone();
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text_at_mut(field)
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        let token = text.focus()?;
        for control in self.controls.values_mut() {
            for text in control.texts_mut() {
                if text.key() != field && text.input_token().is_some() {
                    text.blur(&mut self.engine)?;
                }
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
            .text_at_mut(field)
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
        control.host_enabled = enabled;
        let allowed = control.allowed;
        let text = control
            .text_at_mut(field)
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if !enabled {
            text.blur(&mut self.engine)?;
        }
        Ok(text.set_enabled(enabled && allowed, &mut self.engine)?)
    }

    pub(super) fn input_token(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<Option<UiTextInputToken>, OwnedInspectorError> {
        let displayed = self.live_input(frame, field)?;
        Ok(self.controls[&displayed.key]
            .text_at(field)
            .ok_or(OwnedInspectorError::ReadOnlyField)?
            .input_token())
    }

    pub(super) fn cancel_input(&mut self) -> Result<(), OwnedInspectorError> {
        let mut failure = None;
        for control in self.controls.values_mut() {
            for text in control.texts_mut() {
                if let Err(error) = text.blur(&mut self.engine) {
                    failure.get_or_insert(error);
                }
            }
            for state in &mut control.buttons {
                *state = UiButtonState::default();
            }
            control.copy_button = UiButtonState::default();
            if let Some(tool) = &mut control.tool {
                tool.buttons = [UiButtonState::default(); 2];
            }
            if let Some(reference) = &mut control.reference {
                reference.cancel_input();
            }
            if let FieldInput::Structured(controls) = &mut control.input {
                for button in controls.buttons.values_mut() {
                    button.state = UiButtonState::default();
                }
                for picker in controls.references.values_mut() {
                    picker.cancel_input();
                }
            }
        }
        self.recovery_button.state = UiButtonState::default();
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
        let displayed = self.live_input(frame, field)?;
        let text = self.controls[&displayed.key]
            .text_at(field)
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if text.input_token() != Some(token) {
            return Err(UiTextFieldError::StaleInput.into());
        }
        Ok(text.editor().is_composing())
    }

    pub(super) fn selected_text(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        fields: &FieldEditors,
    ) -> Result<Option<String>, OwnedInspectorError> {
        let displayed = self.live_input(frame, field)?;
        self.check_input(&displayed.key, field, fields)?;
        let text = self.controls[&displayed.key]
            .text_at(field)
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if text.input_token() != Some(token) {
            return Err(UiTextFieldError::StaleInput.into());
        }
        if text.editor().is_composing() {
            return Err(OwnedInspectorError::Composing);
        }
        let range = text.editor().selection_bytes();
        Ok((!range.is_empty()).then(|| text.editor().text()[range].to_owned()))
    }

    pub(super) fn input_error(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        fields: &mut FieldEditors,
        error: String,
    ) {
        if self.input_token(frame, field).ok().flatten() == Some(token)
            && let Ok(displayed) = self.live_field(frame, field)
        {
            let key = displayed.key.clone();
            if let Some(tool) = self
                .controls
                .get_mut(&key)
                .and_then(|control| control.tool.as_mut())
            {
                tool.error = Some(error);
            } else if let Some(draft) = fields.drafts.get_mut(&key) {
                draft.error = Some(error);
            }
        }
    }

    pub(super) fn edit(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
        fields: &mut FieldEditors,
    ) -> Result<TextEditOutcome, OwnedInspectorError> {
        let key = self.live_input(frame, field)?.key.clone();
        self.check_input(&key, field, fields)?;
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let target = control.text_target(field).cloned();
        let tool_parameter = control.tool.is_some();
        let text = control
            .text_at_mut(field)
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        let outcome = text.edit(token, edit, &mut self.engine)?;
        if outcome.text_changed {
            if tool_parameter {
                control.tool.as_mut().expect("tool parameter exists").error = None;
            } else {
                Self::adopt_text(fields, &key, target.as_ref(), text.editor().text())?;
                control.copy_feedback = None;
            }
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
        let displayed = self.live_input(frame, field)?.clone();
        self.check_input(&displayed.key, field, fields)?;
        let input = self
            .retained(frame)?
            .inputs
            .get(field)
            .ok_or(OwnedInspectorError::UnknownControl)?;
        let display_text = input.display_text.clone();
        let control = self
            .controls
            .get_mut(&displayed.key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let target = control.text_target(field).cloned();
        let tool_parameter = control.tool.is_some();
        let text = control
            .text_at_mut(field)
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if display_text != text.editor().display_text() {
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
            if tool_parameter {
                control.tool.as_mut().expect("tool parameter exists").error = None;
            } else {
                Self::adopt_text(
                    fields,
                    &displayed.key,
                    target.as_ref(),
                    text.editor().text(),
                )?;
                control.copy_feedback = None;
            }
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
        let displayed = self.live_input(frame, field)?;
        let text = self.controls[&displayed.key]
            .text_at(field)
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if text.input_token() != Some(token) {
            return Err(UiTextFieldError::StaleInput.into());
        }
        let input = self
            .retained(frame)?
            .inputs
            .get(field)
            .ok_or(OwnedInspectorError::UnknownControl)?;
        Ok(text.ime_area(input.width)?)
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
        let inputs = compiled.inputs.keys().cloned().collect::<HashSet<_>>();
        let buttons = compiled.buttons.values().cloned().collect::<Vec<_>>();
        let recovery = compiled.recovery.clone();
        for control in self.controls.values_mut() {
            for text in control.texts_mut() {
                if inputs.contains(text.key()) {
                    text.reduce_focus(dispatch, &mut self.engine)?;
                }
            }
        }
        let mut intents = UiIntentBatch::default();
        if let Some(recovery) = recovery {
            let button = self.recovery_control(&recovery)?;
            self.recovery_button
                .state
                .reduce_batch(&button, dispatch, &mut intents)?;
        }
        for button in buttons {
            let Ok(displayed) = self.live_field(frame, &button.field) else {
                continue;
            };
            let key = displayed.key.clone();
            let control = self
                .controls
                .get_mut(&key)
                .ok_or(OwnedInspectorError::RetiredField)?;
            let state = match &button.action {
                InspectorButtonAction::Operation(operation) => {
                    &mut control.buttons[operation.index()]
                }
                InspectorButtonAction::Reference(reference) => {
                    let Some(picker) = control.reference_mut(reference.input.as_ref()) else {
                        continue;
                    };
                    let stale = picker.revision != reference.revision;
                    let Some(reference) = picker
                        .buttons
                        .get_mut(&reference.action)
                        .filter(|reference| reference.key == *button.button.key())
                    else {
                        continue;
                    };
                    if stale {
                        reference.state.reduce_batch(
                            &button.button.clone().with_enabled(false),
                            dispatch,
                            &mut intents,
                        )?;
                        continue;
                    }
                    &mut reference.state
                }
                InspectorButtonAction::Structure(action) => {
                    let FieldInput::Structured(controls) = &mut control.input else {
                        continue;
                    };
                    let Some(button_state) = controls
                        .buttons
                        .get_mut(action)
                        .filter(|state| state.key == *button.button.key())
                    else {
                        continue;
                    };
                    &mut button_state.state
                }
                InspectorButtonAction::Copy => &mut control.copy_button,
                InspectorButtonAction::SourceTool(action) => {
                    &mut control
                        .tool
                        .as_mut()
                        .ok_or(OwnedInspectorError::ChangedProjection)?
                        .buttons[action.index()]
                }
            };
            state.reduce_batch(&button.button, dispatch, &mut intents)?;
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
        if button == &self.recovery_button.key {
            self.discard_retained(frame, fields)?;
            return Ok(None);
        }
        let binding = self
            .retained(frame)?
            .buttons
            .get(button)
            .ok_or(OwnedInspectorError::UnknownControl)?
            .clone();
        match binding.action {
            InspectorButtonAction::Operation(operation) => {
                self.operate(frame, &binding.field, operation, model, fields)
            }
            InspectorButtonAction::Reference(_) => {
                self.reference_action(frame, button, &binding, model, fields)?;
                Ok(None)
            }
            InspectorButtonAction::Structure(action) => {
                self.change_structure(frame, &binding.field, button, &action, model, fields)?;
                Ok(None)
            }
            InspectorButtonAction::Copy => Err(OwnedInspectorError::HostClipboardRequired),
            InspectorButtonAction::SourceTool(_) => Err(OwnedInspectorError::SourceToolRequired),
        }
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
        if operation != InspectorOperation::Discard && !fields.has_draft_capacity(key) {
            return Err(OwnedInspectorError::DraftCapacity);
        }
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
        if self.read_only || !editable || !control.host_enabled || !control.allowed {
            return Err(OwnedInspectorError::OperationDisabled(operation));
        }
        if control.is_composing() {
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
        if operation == InspectorOperation::Toggle {
            let Candidate::Bool(value) = &mut draft.candidate else {
                return Err(OwnedInspectorError::ReadOnlyField);
            };
            *value = !*value;
            draft.remove_intent = false;
            if let Some(control) = self.controls.get_mut(key) {
                control.copy_feedback = None;
            }
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
        let compiled = self.retained(frame)?;
        let owner = compiled
            .inputs
            .get(input)
            .map_or(input, |input| &input.field);
        let field = compiled
            .fields
            .get(owner)
            .ok_or(OwnedInspectorError::UnknownControl)?;
        let control = self
            .controls
            .get(&field.key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        if control.input_key() != owner || (input != owner && control.text_at(input).is_none()) {
            return Err(OwnedInspectorError::RetiredField);
        }
        Ok(field)
    }

    fn live_input(
        &self,
        frame: &Arc<UiFrame>,
        input: &UiKey,
    ) -> Result<&DisplayedField, OwnedInspectorError> {
        let field = self.live_field(frame, input)?;
        if !self.retained(frame)?.inputs.contains_key(input)
            || !frame
                .semantics()
                .iter()
                .any(|node| node.key() == input && node.role() == UiSemanticRole::TextInput)
        {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        Ok(field)
    }

    fn check_input(
        &self,
        key: &FieldKey,
        input: &UiKey,
        fields: &FieldEditors,
    ) -> Result<(), OwnedInspectorError> {
        let control = self
            .controls
            .get(key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let text = control
            .text_at(input)
            .ok_or(OwnedInspectorError::ReadOnlyField)?;
        if let Some(tool) = &control.tool {
            if tool.submitting {
                return Err(OwnedInspectorError::Pending);
            }
            return if control.allowed && control.host_enabled {
                Ok(())
            } else {
                Err(OwnedInspectorError::ReadOnlyField)
            };
        }
        if !fields.has_draft_capacity(key) {
            return Err(OwnedInspectorError::DraftCapacity);
        }
        let draft = fields
            .drafts
            .get(key)
            .ok_or(OwnedInspectorError::ChangedDraft)?;
        let candidate_text = match (&draft.candidate, control.text_target(input)) {
            (Candidate::Text(text), None) => Some(text.as_str()),
            (Candidate::Structured(root), Some(target)) => structure::text(root, target),
            _ => None,
        };
        if draft.basis != control.basis || candidate_text != Some(text.editor().text()) {
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
        target: Option<&DraftInput>,
        text: &str,
    ) -> Result<(), OwnedInspectorError> {
        let draft = fields
            .drafts
            .get_mut(key)
            .ok_or(OwnedInspectorError::ChangedDraft)?;
        match (&mut draft.candidate, target) {
            (Candidate::Text(value), None) => *value = text.to_owned(),
            (Candidate::Structured(root), Some(target)) => structure::set_text(root, target, text)
                .map_err(OwnedInspectorError::InvalidCandidate)?,
            _ => return Err(OwnedInspectorError::ChangedDraft),
        }
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
        inputs: &mut HashMap<UiKey, DisplayedInput>,
        buttons: &mut HashMap<UiKey, DisplayedButton>,
        entities: &[nara::tooling::SceneInspectorEntityRow],
        tool_basis: Option<ToolBasis>,
        tool_presentation: Option<&super::source_tool::OwnedSourceToolPresentation>,
        model: &EditorFrameModel,
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
            if tool_basis.is_none()
                && !fields.drafts.contains_key(&key)
                && supported_input(basis)
                && capacity
            {
                fields
                    .drafts
                    .insert(key.clone(), FieldDraft::new(basis.clone()));
            }
            if let Some(draft) = fields.drafts.get_mut(&key) {
                draft.synchronize(basis.clone());
            }
        }
        let draft = fields.drafts.get(&key);
        let basis = if tool_basis.is_some() {
            current.as_ref()
        } else {
            draft.map(|draft| &draft.basis).or(current.as_ref())
        }
        .ok_or(OwnedInspectorError::ChangedDraft)?;
        let replace = self.controls.get(&key).is_none_or(|control| {
            control.basis != *basis
                || control.tool.as_ref().map(|tool| &tool.basis) != tool_basis.as_ref()
                || control.had_draft != draft.is_some()
                || (tool_basis.is_none() && matches!((control.text(), draft), (Some(text), Some(draft))
                    if !matches!(&draft.candidate, Candidate::Text(value) if value == text.editor().text()))
                )
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
            let input = if let Some(tool) = &tool_basis {
                FieldInput::Text(Box::new(UiTextField::new(
                    self.root.clone(),
                    tool.label.clone(),
                    &tool.initial.to_string(),
                    TEXT_SIZE,
                    &mut self.engine,
                )?))
            } else if let Some(draft) = draft
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
                    Ok(text) => FieldInput::Text(Box::new(text)),
                    Err(error) => {
                        unavailable = Some(format!("Read-only: {error}"));
                        FieldInput::ReadOnly
                    }
                }
            } else if let Some(draft) = draft
                && scalar_input(&draft.basis)
                && matches!(draft.candidate, Candidate::Bool(_))
            {
                FieldInput::Boolean(UiKey::scoped(
                    self.root.clone(),
                    UiLocalKey::runtime("nara.inspector.action", format!("{identity}.Toggle"))?,
                )?)
            } else if let Some(draft) = draft
                && supported_input(&draft.basis)
                && matches!(draft.candidate, Candidate::Structured(_))
            {
                FieldInput::Structured(Box::default())
            } else {
                FieldInput::ReadOnly
            };
            self.controls.insert(
                key.clone(),
                FieldControl {
                    identity,
                    key: row_key,
                    basis: basis.clone(),
                    had_draft: draft.is_some(),
                    input,
                    unavailable,
                    allowed: false,
                    host_enabled: true,
                    buttons: [UiButtonState::default(); InspectorOperation::COUNT],
                    reference: (basis.field.shape == ComponentValueShape::EntityRef)
                        .then(ReferencePicker::default),
                    copy_button: UiButtonState::default(),
                    copy_feedback: None,
                    tool: tool_basis.map(ToolControl::new),
                },
            );
        }
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let identity = control.identity;
        let current_supported = current.as_ref().is_some_and(supported_input);
        control.allowed = if let Some(tool) = &mut control.tool {
            if tool_presentation.is_some_and(|presentation| presentation.enabled) {
                tool.submitting = false;
            }
            editable
                && tool_presentation.is_some_and(|presentation| presentation.enabled)
                && !tool.submitting
        } else {
            editable
                && current_supported
                && capacity
                && draft.is_some_and(|draft| !draft.incompatible && !draft.in_flight())
        };
        let enabled = control.allowed && control.host_enabled;
        for text in control.texts_mut() {
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
            .text()
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
        let boolean = matches!(&control.input, FieldInput::Boolean(_));
        let structured = matches!(&control.input, FieldInput::Structured(_));
        let has_input = !matches!(&control.input, FieldInput::ReadOnly);
        let tool_parameter = control.tool.is_some();
        let tool_label = control.tool.as_ref().map(|tool| tool.basis.label.clone());
        let has_text = control.text().is_some() && !tool_parameter;
        let composing = control.is_composing();
        let display_text = control
            .text()
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
        if let Some(label) = tool_label {
            children.push(self.label(&format!("{identity}.tool.label"), &label)?);
        }
        if let Some(node) = text_node {
            inputs.insert(
                input.clone(),
                DisplayedInput {
                    field: input.clone(),
                    display_text: display_text
                        .clone()
                        .ok_or(OwnedInspectorError::ReadOnlyField)?,
                    width,
                },
            );
            children.push(node);
        } else if !boolean {
            children.push(self.label(
                &format!("{identity}.value"),
                &authoring::value_label(source(&current.as_ref().unwrap_or(basis).field)),
            )?);
        }
        if structured && let Some(draft) = draft {
            children.extend(self.structured_nodes(&key, draft, width, inputs, buttons, entities)?);
        }
        let mut operations = [false; InspectorOperation::COUNT];
        let tool = self
            .tool_nodes(&key, width, tool_presentation, model, buttons)?
            .map(|(nodes, displayed)| {
                children.extend(nodes);
                displayed
            });
        if let Some(draft) = draft {
            let can_edit = editable
                && !tool_parameter
                && current_supported
                && capacity
                && has_input
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
            operations[InspectorOperation::Toggle.index()] = can_edit && boolean && !draft.conflict;
            if basis.field.shape == ComponentValueShape::EntityRef {
                let control = self
                    .controls
                    .get_mut(&key)
                    .ok_or(OwnedInspectorError::RetiredField)?;
                let picker = control
                    .reference
                    .as_mut()
                    .ok_or(OwnedInspectorError::ReadOnlyField)?;
                for (node, button) in references::reference_nodes(
                    &self.root,
                    &mut self.next_identity,
                    picker,
                    entities,
                    references::ReferenceContext {
                        field: &input,
                        input: None,
                        name: None,
                        enabled: can_edit,
                        width,
                    },
                )? {
                    children.push(node);
                    buttons.insert(button.button.key().clone(), button);
                }
            }
            if boolean && let Candidate::Bool(value) = &draft.candidate {
                let button = UiButton::new(
                    input.clone(),
                    if *value { "Set false" } else { "Set true" },
                    UiActionId::new(InspectorOperation::Toggle.label())?,
                )?
                .with_enabled(operations[InspectorOperation::Toggle.index()]);
                let role = button_color(states[InspectorOperation::Toggle.index()], &button);
                children.push(
                    button
                        .authored_node(
                            &self.root,
                            UiStyle::flex()
                                .with_width(UiDimension::Px(width))
                                .with_height(UiDimension::Px(36.0))
                                .with_flex_shrink(0.0),
                            UiVisual::role(role),
                            UiBrush::Role(UiColorRole::PrimaryText),
                            TEXT_SIZE,
                        )?
                        .with_semantics(
                            UiSemantics::new(UiSemanticRole::Button, button.label())?
                                .with_value(value.to_string())?
                                .with_action(button.action().clone())?,
                        ),
                );
                buttons.insert(
                    input.clone(),
                    DisplayedButton {
                        field: input.clone(),
                        action: InspectorButtonAction::Operation(InspectorOperation::Toggle),
                        button,
                    },
                );
            }
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
            if draft.conflict || draft.incompatible || (tool_parameter && draft.dirty) {
                if let Some(current) = &current {
                    children.push(self.label(
                        &format!("{identity}.current"),
                        &format!(
                            "Current: {}",
                            authoring::value_label(source(&current.field))
                        ),
                    )?);
                }
                if !has_text {
                    let retained = draft.candidate.copy_text();
                    let end = retained
                        .char_indices()
                        .nth(128)
                        .map_or(retained.len(), |(offset, _)| offset);
                    let suffix = if end < retained.len() { "…" } else { "" };
                    children.push(self.label(
                        &format!("{identity}.retained"),
                        &format!("Retained: {}{suffix}", &retained[..end]),
                    )?);
                }
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
            for operation in InspectorOperation::FOOTER {
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
                let role = button_color(states[operation.index()], &button);
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
                        action: InspectorButtonAction::Operation(operation),
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
            if draft.dirty || draft.incompatible {
                let button_key = UiKey::scoped(
                    self.root.clone(),
                    UiLocalKey::runtime("nara.inspector.copy", identity.to_string())?,
                )?;
                let button = UiButton::new(
                    button_key.clone(),
                    "Copy retained input",
                    UiActionId::new("Copy retained input")?,
                )?;
                let state = self.controls[&key].copy_button;
                children.push(
                    button.authored_node(
                        &self.root,
                        UiStyle::flex()
                            .with_width(UiDimension::Px(width))
                            .with_height(UiDimension::Px(30.0))
                            .with_flex_shrink(0.0),
                        UiVisual::role(button_color(state, &button)),
                        UiBrush::Role(UiColorRole::PrimaryText),
                        TEXT_SIZE,
                    )?,
                );
                buttons.insert(
                    button_key,
                    DisplayedButton {
                        field: input.clone(),
                        action: InspectorButtonAction::Copy,
                        button,
                    },
                );
                if let Some(feedback) = &self.controls[&key].copy_feedback {
                    children.push(self.label(&format!("{identity}.copy-feedback"), feedback)?);
                }
            }
        }
        if let Some(unavailable) = unavailable {
            children.push(self.label(&format!("{identity}.unavailable"), &unavailable)?);
        }
        if !current_supported {
            children.push(self.label(
                &format!("{identity}.readonly"),
                if current.is_some() {
                    "Read-only: this field shape is not yet supported by this panel"
                } else {
                    "This field is not currently projected. Its retained draft is unchanged."
                },
            )?);
        } else if !capacity && !tool_parameter {
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
        let control = &self.controls[&key];
        let mut reference_views = HashMap::new();
        if let Some(reference) = &control.reference {
            reference_views.insert(None, reference.revision);
        }
        if let FieldInput::Structured(controls) = &control.input {
            reference_views.extend(
                controls
                    .references
                    .iter()
                    .map(|(input, picker)| (Some(input.clone()), picker.revision)),
            );
        }
        displayed.insert(
            input,
            DisplayedField {
                key,
                current,
                draft: draft.map(DisplayedDraft::observe),
                display_text,
                operations,
                reference_views,
                tool,
                width,
            },
        );
        Ok(node)
    }
}

fn button_color(state: UiButtonState, button: &UiButton) -> UiColorRole {
    match state.visual_state(button) {
        UiButtonVisualState::Disabled => UiColorRole::Disabled,
        UiButtonVisualState::Pressed | UiButtonVisualState::Focused => UiColorRole::Focus,
        UiButtonVisualState::Hovered => UiColorRole::SecondaryText,
        UiButtonVisualState::Idle => UiColorRole::Panel,
    }
}

fn scalar_input(basis: &FieldBasis) -> bool {
    scalar_text(basis)
        || matches!(
            basis.field.shape,
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::Bool,
                ..
            }
        )
}

fn supported_input(basis: &FieldBasis) -> bool {
    authoring::value_draft::supported(&basis.field.shape)
}

fn scalar_text(basis: &FieldBasis) -> bool {
    matches!(
        basis.field.shape,
        ComponentValueShape::EntityRef
            | ComponentValueShape::AssetRef { .. }
            | ComponentValueShape::Scalar {
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
    #[error("Inspector scroll delta must be finite")]
    InvalidScroll,
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
    #[error("copying retained input requires the Host clipboard")]
    HostClipboardRequired,
    #[error("this control requires the source-tool submission owner")]
    SourceToolRequired,
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
