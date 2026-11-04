use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use nara::{
    scene::{
        PrefabEntityTarget, SceneAuthoringSourceId, SceneEntityId, ScenePlacement,
        ScenePlacementParent, ScenePlacementSlot,
    },
    tooling::{
        EditorAction, EditorActionOutcome, EditorDocumentId, EditorDocumentKind,
        EditorDocumentQueryBasis, EditorFrameModel, EditorNavigationAction,
        SceneCompositionCapabilities, SceneCompositionCommand, SceneCompositionEntityKind,
    },
};
use nara_text::{FontFace, TextEdit, TextEditOutcome, TextEngine, TextEngineCreateError};
use nara_ui_kernel::{
    UiAuthoringError, UiAuthoringSnapshot, UiCompileError, UiCompiler, UiDispatchBatch,
    UiEnvironment, UiFrame, UiKey, UiKeyError, UiPoint, UiRect, UiRootKey, UiStyleError, UiTheme,
};
use nara_ui_widgets::{
    UiButton, UiButtonState, UiIntentBatch, UiIntentBatchError, UiTextField, UiTextFieldError,
    UiTextInputToken, UiWidgetError,
};
use thiserror::Error;

use super::{PanelActionDispatch, PanelSubmissionId, PanelSubmitError, authoring};

mod input;
mod presentation;
#[cfg(test)]
mod tests;

const RETAINED_FRAMES: usize = 3;
const TEXT_SIZE: f32 = 16.0;
const INSET: f32 = 12.0;
const SOURCE_ROW_HEIGHT: f32 = 68.0;
const CHOICE_HEIGHT: f32 = 30.0;
const FILTER_TEXT_BYTES: usize = 1024;

type DocumentScope = (EditorDocumentId, EditorDocumentKind, SceneAuthoringSourceId);

/// Exact source facts, including Schema admission identity rather than catalog equality.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HierarchyBasis {
    target: EditorDocumentQueryBasis,
    kind: EditorDocumentKind,
    selected: Option<SceneEntityId>,
    placement: Option<ScenePlacement>,
    capabilities: SceneCompositionCapabilities,
    editing: bool,
    navigation: bool,
    read_only: bool,
    removal_reason: Option<&'static str>,
}

impl HierarchyBasis {
    fn capture(model: &EditorFrameModel, read_only: bool) -> Option<Self> {
        let scene = model.workspace().active_scene.as_ref()?;
        if model.workspace().active_document != Some(scene.document)
            || !matches!(
                scene.kind,
                EditorDocumentKind::Scene | EditorDocumentKind::Prefab
            )
        {
            return None;
        }
        let inspector = &scene.editor.inspector;
        let selected = inspector
            .selected_entity
            .as_ref()
            .and_then(|id| inspector.entities.iter().find(|row| &row.id == id));
        Some(Self {
            target: EditorDocumentQueryBasis {
                document: scene.document,
                revision: scene.revision,
                schema: model
                    .workspace()
                    .document(scene.document)
                    .and_then(|tab| tab.authoring_schema_basis.clone()),
            },
            kind: scene.kind,
            selected: inspector.selected_entity.clone(),
            placement: selected.and_then(|row| row.placement.clone()),
            capabilities: inspector.composition.clone(),
            editing: !read_only && authoring::editing_enabled(model),
            navigation: !read_only && model.workspace_intent().intent().is_none(),
            read_only,
            removal_reason: authoring::entity_removal_unavailable_reason(model, scene),
        })
    }

    fn scope(&self) -> DocumentScope {
        (
            self.target.document,
            self.kind,
            self.target.revision.source_id(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Control {
    Root,
    Child,
    Select(SceneEntityId),
    Parent,
    Order,
    ParentRoot,
    ParentEntity(SceneEntityId),
    First,
    Last,
    Before(SceneEntityId),
    After(SceneEntityId),
    Remove,
    ConfirmRemoval,
    CancelRemoval,
    CloseChoices,
    ClearFilter,
    ShowSelection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    Parent,
    Order,
}

struct Choices {
    kind: MenuKind,
    basis: HierarchyBasis,
}

struct ButtonControl {
    identity: u64,
    key: UiKey,
    state: UiButtonState,
}

#[derive(Clone)]
struct DisplayedButton {
    control: Control,
    button: UiButton,
}

struct DisplayedInput {
    key: UiKey,
    scope: Option<DocumentScope>,
    width: f32,
    display_text: String,
}

struct CompiledHierarchy {
    frame: Arc<UiFrame>,
    snapshot: UiAuthoringSnapshot,
    theme: UiTheme,
    basis: Option<HierarchyBasis>,
    observation: Arc<AtomicBool>,
    viewport_observation: Arc<()>,
    buttons: HashMap<UiKey, DisplayedButton>,
    input: Option<DisplayedInput>,
    source_limit: f32,
    choice_limit: f32,
}

/// Private source-declaration panel. Session owns selection, commands, history and persistence.
pub(super) struct OwnedHierarchy {
    root: UiRootKey,
    engine: TextEngine,
    compiler: UiCompiler,
    filter: UiTextField,
    scope: Option<DocumentScope>,
    basis: Option<HierarchyBasis>,
    observation: Arc<AtomicBool>,
    viewport_observation: Arc<()>,
    identities: HashMap<Control, u64>,
    controls: HashMap<Control, ButtonControl>,
    next_control: u64,
    next_snapshot: u64,
    frames: Vec<CompiledHierarchy>,
    choices: Option<Choices>,
    removal: Option<HierarchyBasis>,
    source_scroll: f32,
    choice_scroll: f32,
    queued: bool,
    pending: Option<PanelSubmissionId>,
    error: Option<String>,
    input_error: Option<String>,
}

/// Move-only preparation. It grants no authority until the original basis is checked again.
pub(in crate::frontend::imgui) struct HierarchySubmission {
    basis: HierarchyBasis,
    observation: Arc<AtomicBool>,
    action: EditorAction,
}

impl HierarchySubmission {
    pub(in crate::frontend::imgui) fn into_action(
        self,
        model: &EditorFrameModel,
        read_only: bool,
    ) -> Result<EditorAction, OwnedHierarchyError> {
        if !self.observation.load(Ordering::Relaxed)
            || HierarchyBasis::capture(model, read_only).as_ref() != Some(&self.basis)
        {
            return Err(OwnedHierarchyError::ChangedProjection);
        }
        Ok(self.action)
    }
}

impl OwnedHierarchy {
    pub(super) fn new(root: UiRootKey, fonts: Vec<FontFace>) -> Result<Self, OwnedHierarchyError> {
        let mut engine = TextEngine::with_fonts(
            fonts.clone(),
            nara_text::TextLimits {
                max_text_bytes: FILTER_TEXT_BYTES,
                max_glyphs: FILTER_TEXT_BYTES,
                ..Default::default()
            },
        )?;
        let compiler = UiCompiler::with_fonts(fonts, Default::default())?;
        let filter = UiTextField::new(root.clone(), "Filter entities", "", TEXT_SIZE, &mut engine)?;
        Ok(Self {
            root,
            engine,
            compiler,
            filter,
            scope: None,
            basis: None,
            observation: Arc::new(AtomicBool::new(true)),
            viewport_observation: Arc::new(()),
            identities: HashMap::new(),
            controls: HashMap::new(),
            next_control: 1,
            next_snapshot: 1,
            frames: Vec::new(),
            choices: None,
            removal: None,
            source_scroll: 0.0,
            choice_scroll: 0.0,
            queued: false,
            pending: None,
            error: None,
            input_error: None,
        })
    }

    pub(super) fn compiled_frame(&self) -> Option<&Arc<UiFrame>> {
        self.frames.last().map(|compiled| &compiled.frame)
    }

    fn retained(&self, frame: &Arc<UiFrame>) -> Result<&CompiledHierarchy, OwnedHierarchyError> {
        self.frames
            .iter()
            .find(|compiled| Arc::ptr_eq(&compiled.frame, frame))
            .ok_or(OwnedHierarchyError::RetiredFrame)
    }

    fn invalidate_actions(&mut self) {
        self.viewport_observation = Arc::new(());
        self.invalidate_buttons();
    }

    fn invalidate_buttons(&mut self) {
        self.observation.store(false, Ordering::Relaxed);
        self.observation = Arc::new(AtomicBool::new(true));
        for control in self.controls.values_mut() {
            control.state.cancel_pointer_interaction();
        }
    }

    fn live_button(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<&DisplayedButton, OwnedHierarchyError> {
        let compiled = self.retained(frame)?;
        let button = compiled
            .buttons
            .get(key)
            .ok_or(OwnedHierarchyError::UnknownControl)?;
        let latest = self
            .frames
            .last()
            .ok_or(OwnedHierarchyError::RetiredFrame)?;
        if !compiled.observation.load(Ordering::Relaxed)
            || !Arc::ptr_eq(&compiled.observation, &self.observation)
            || latest.basis != compiled.basis
            || !latest.buttons.contains_key(key)
            || self
                .controls
                .get(&button.control)
                .is_none_or(|control| control.key != *key)
        {
            return Err(OwnedHierarchyError::ChangedProjection);
        }
        Ok(button)
    }

    pub(super) fn invalidated_controls(
        &self,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Result<Vec<UiKey>, OwnedHierarchyError> {
        let previous = self.retained(previous)?;
        let candidate = self.retained(candidate)?;
        let mut keys = previous
            .buttons
            .iter()
            .filter(|(key, button)| {
                !Arc::ptr_eq(&previous.observation, &candidate.observation)
                    || previous.basis != candidate.basis
                    || candidate
                        .buttons
                        .get(*key)
                        .is_none_or(|next| next.button != button.button)
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        if let Some(input) = &previous.input
            && candidate
                .input
                .as_ref()
                .is_none_or(|next| next.key != input.key || next.scope != input.scope)
        {
            keys.push(input.key.clone());
        }
        Ok(keys)
    }

    /// Only reduce admitted interaction here; activation remains an explicit owner operation.
    pub(super) fn dispatch(
        &mut self,
        frame: &Arc<UiFrame>,
        batch: &UiDispatchBatch,
    ) -> Result<UiIntentBatch, OwnedHierarchyError> {
        let compiled = self.retained(frame)?;
        if batch.frame_revision() != frame.revision() {
            return Err(OwnedHierarchyError::ForeignDispatch);
        }
        let live_input = compiled
            .input
            .as_ref()
            .is_some_and(|input| input.key == *self.filter.key() && input.scope == self.scope);
        let buttons = compiled
            .buttons
            .iter()
            .map(|(key, button)| {
                let mut button = button.clone();
                if self.live_button(frame, key).is_err() {
                    button.button = button.button.with_enabled(false);
                }
                button
            })
            .collect::<Vec<_>>();
        if live_input {
            self.filter.reduce_focus(batch, &mut self.engine)?;
        }
        let mut intents = UiIntentBatch::default();
        for button in buttons {
            if let Some(control) = self.controls.get_mut(&button.control)
                && &control.key == button.button.key()
            {
                control
                    .state
                    .reduce_batch(&button.button, batch, &mut intents)?;
            }
        }
        Ok(intents)
    }

    pub(super) fn activate(
        &mut self,
        frame: &Arc<UiFrame>,
        button: &UiKey,
        model: &EditorFrameModel,
        read_only: bool,
    ) -> Result<Option<HierarchySubmission>, OwnedHierarchyError> {
        let result = self.activate_inner(frame, button, model, read_only);
        if let Err(error) = &result {
            self.error.get_or_insert_with(|| error.to_string());
        }
        result
    }

    fn activate_inner(
        &mut self,
        frame: &Arc<UiFrame>,
        button: &UiKey,
        model: &EditorFrameModel,
        read_only: bool,
    ) -> Result<Option<HierarchySubmission>, OwnedHierarchyError> {
        let displayed = self.live_button(frame, button)?.clone();
        let basis = self.retained(frame)?.basis.clone();
        if HierarchyBasis::capture(model, read_only) != basis {
            return Err(OwnedHierarchyError::ChangedProjection);
        }
        if !displayed.button.enabled() {
            return Err(OwnedHierarchyError::Disabled);
        }
        match displayed.control {
            Control::ClearFilter => {
                self.clear_filter()?;
                return Ok(None);
            }
            Control::ShowSelection => {
                self.clear_filter()?;
                if let Some(scene) = model.workspace().active_scene.as_ref()
                    && let Some(index) = scene.editor.inspector.entities.iter().position(|row| {
                        Some(&row.id) == scene.editor.inspector.selected_entity.as_ref()
                    })
                {
                    self.source_scroll = index as f32 * SOURCE_ROW_HEIGHT;
                }
                self.invalidate_actions();
                return Ok(None);
            }
            Control::CloseChoices => {
                self.choices = None;
                self.choice_scroll = 0.0;
                self.invalidate_actions();
                return Ok(None);
            }
            Control::CancelRemoval => {
                self.removal = None;
                self.invalidate_actions();
                return Ok(None);
            }
            _ => {}
        }
        let basis = basis.ok_or(OwnedHierarchyError::ChangedProjection)?;
        let scene = model
            .workspace()
            .active_scene
            .as_ref()
            .ok_or(OwnedHierarchyError::ChangedProjection)?;
        match displayed.control {
            Control::Parent | Control::Order => {
                let kind = if displayed.control == Control::Parent {
                    MenuKind::Parent
                } else {
                    MenuKind::Order
                };
                self.choices = if self
                    .choices
                    .as_ref()
                    .is_some_and(|choices| choices.kind == kind)
                {
                    None
                } else {
                    Some(Choices { kind, basis })
                };
                self.choice_scroll = 0.0;
                self.invalidate_actions();
                return Ok(None);
            }
            Control::Remove => {
                self.removal = Some(basis);
                self.choices = None;
                self.invalidate_actions();
                return Ok(None);
            }
            _ => {}
        }
        if self.queued || self.pending.is_some() {
            return Err(OwnedHierarchyError::Pending);
        }
        let selected = || {
            basis
                .selected
                .clone()
                .ok_or(OwnedHierarchyError::ChangedProjection)
        };
        let action = match displayed.control {
            Control::Select(entity) => {
                if !scene
                    .editor
                    .inspector
                    .entities
                    .iter()
                    .any(|row| row.id == entity)
                {
                    return Err(OwnedHierarchyError::ChangedProjection);
                }
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: basis.target.document,
                    entity: Some(entity),
                })
            }
            Control::Root | Control::Child => {
                let parent = if displayed.control == Control::Child {
                    ScenePlacementParent::Entity(PrefabEntityTarget::source(selected()?))
                } else {
                    ScenePlacementParent::Root
                };
                authoring::compose_action(
                    scene,
                    SceneCompositionCommand::CreateEntity {
                        kind: if basis.capabilities.generic_authoring {
                            SceneCompositionEntityKind::Empty
                        } else {
                            SceneCompositionEntityKind::ColoredSprite
                        },
                        placement: ScenePlacement::last(parent),
                    },
                )
            }
            Control::ConfirmRemoval => {
                if self.removal.as_ref() != Some(&basis) {
                    return Err(OwnedHierarchyError::ChangedConfirmation);
                }
                authoring::compose_action(
                    scene,
                    SceneCompositionCommand::RemoveEntity {
                        entity: selected()?,
                    },
                )
            }
            Control::ParentRoot
            | Control::ParentEntity(_)
            | Control::First
            | Control::Last
            | Control::Before(_)
            | Control::After(_) => {
                let choices = self
                    .choices
                    .as_ref()
                    .ok_or(OwnedHierarchyError::ChangedProjection)?;
                if choices.basis != basis {
                    return Err(OwnedHierarchyError::ChangedProjection);
                }
                let to = match displayed.control {
                    Control::ParentRoot => ScenePlacement::last(ScenePlacementParent::Root),
                    Control::ParentEntity(entity) => ScenePlacement::last(
                        ScenePlacementParent::Entity(PrefabEntityTarget::source(entity)),
                    ),
                    order => {
                        let placement = basis
                            .placement
                            .as_ref()
                            .ok_or(OwnedHierarchyError::ChangedProjection)?;
                        let slot = match order {
                            Control::First => ScenePlacementSlot::First,
                            Control::Last => ScenePlacementSlot::Last,
                            Control::Before(entity) => {
                                ScenePlacementSlot::Before(PrefabEntityTarget::source(entity))
                            }
                            Control::After(entity) => {
                                ScenePlacementSlot::After(PrefabEntityTarget::source(entity))
                            }
                            _ => return Err(OwnedHierarchyError::UnknownControl),
                        };
                        ScenePlacement {
                            parent: placement.parent.clone(),
                            slot,
                        }
                    }
                };
                authoring::compose_action(
                    scene,
                    SceneCompositionCommand::MoveEntity {
                        target: PrefabEntityTarget::source(selected()?),
                        to,
                    },
                )
            }
            _ => return Err(OwnedHierarchyError::UnknownControl),
        };
        // Spend the displayed operation before exposing a move-only queued submission.
        self.invalidate_actions();
        self.queued = true;
        self.choices = None;
        self.removal = None;
        Ok(Some(HierarchySubmission {
            basis,
            observation: Arc::clone(&self.observation),
            action,
        }))
    }

    pub(super) fn admitted(&mut self, result: Result<PanelSubmissionId, PanelSubmitError>) {
        if !self.queued || self.pending.is_some() {
            return;
        }
        self.queued = false;
        match result {
            Ok(id) => self.pending = Some(id),
            Err(error) => self.error = Some(format!("Hierarchy action was not queued: {error}")),
        }
        self.invalidate_actions();
    }

    pub(super) fn action_finished(&mut self, id: PanelSubmissionId, result: &PanelActionDispatch) {
        if self.pending != Some(id) {
            return;
        }
        self.pending = None;
        self.error = match result {
            PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
                if report.applied =>
            {
                None
            }
            PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) => {
                let diagnostics = report
                    .diagnostics
                    .iter()
                    .map(|diagnostic| {
                        format!(
                            "{}: {}",
                            diagnostic.code().as_str(),
                            diagnostic.summary().as_str()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                Some(if diagnostics.is_empty() {
                    format!("Hierarchy action was refused: {report:?}")
                } else {
                    format!("Hierarchy action was refused: {diagnostics}")
                })
            }
            other => Some(format!("Hierarchy action did not apply: {other:?}")),
        };
        self.invalidate_actions();
    }

    pub(super) fn submission_failed(&mut self, error: String) {
        if self.pending.is_none() {
            self.queued = false;
            self.error = Some(error);
            self.invalidate_actions();
        }
    }

    pub(super) fn scroll(
        &mut self,
        frame: &Arc<UiFrame>,
        delta: f32,
    ) -> Result<bool, OwnedHierarchyError> {
        let compiled = self.retained(frame)?;
        if !delta.is_finite() {
            return Err(OwnedHierarchyError::InvalidScroll);
        }
        if !Arc::ptr_eq(&compiled.viewport_observation, &self.viewport_observation) {
            return Err(OwnedHierarchyError::ChangedProjection);
        }
        let source_limit = compiled.source_limit;
        let choice_limit = compiled.choice_limit;
        let (offset, limit) = if self.choices.is_some() {
            (&mut self.choice_scroll, choice_limit)
        } else {
            (&mut self.source_scroll, source_limit)
        };
        let next = (*offset + delta.clamp(-limit, limit)).clamp(0.0, limit);
        let changed = next != *offset;
        *offset = next;
        if changed {
            // Relative deltas share the presented viewport; its old buttons lose authority.
            self.invalidate_buttons();
        }
        Ok(changed)
    }
}

impl Drop for OwnedHierarchy {
    fn drop(&mut self) {
        self.observation.store(false, Ordering::Relaxed);
    }
}

#[derive(Debug, Error)]
pub(super) enum OwnedHierarchyError {
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
    #[error("Hierarchy identity is exhausted")]
    IdentityExhausted,
    #[error("Hierarchy viewport has no positive content area")]
    InvalidViewport,
    #[error("Hierarchy scroll delta must be finite")]
    InvalidScroll,
    #[error("the exact compiled Hierarchy frame is no longer retained")]
    RetiredFrame,
    #[error("the UI key is not a live control in this compiled Hierarchy frame")]
    UnknownControl,
    #[error("the dispatch belongs to another compiled frame")]
    ForeignDispatch,
    #[error(
        "the source, selection, Schema, permissions or local Hierarchy observation changed; review the original action before retrying"
    )]
    ChangedProjection,
    #[error(
        "the removal confirmation belongs to the original selection and source; cancel it before choosing another entity"
    )]
    ChangedConfirmation,
    #[error("the displayed Hierarchy action is disabled")]
    Disabled,
    #[error("a Hierarchy action is already awaiting its exact submission result")]
    Pending,
    #[error("finish or cancel filter composition before using its selection or geometry")]
    Composing,
    #[error("the Hierarchy root did not publish; its controls have no current authority")]
    Unpublished,
}
