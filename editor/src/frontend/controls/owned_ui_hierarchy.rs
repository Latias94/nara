use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use nara::{
    project_host::{EditorUiAuthoringEditError, EditorUiAuthoringFrame},
    scene::{
        PrefabEntityOrigin, SceneAuthoringSourceId, SceneEntityId, ScenePlacement,
        ScenePlacementSlot,
    },
    tooling::{
        EditorAction, EditorActionOutcome, EditorDocumentId, EditorDocumentKind,
        EditorDocumentQueryBasis, EditorDocumentSourceBinding, EditorFrameModel,
        EditorNavigationAction, EditorSelectionSet, SceneCompositionCapabilities,
    },
    ui::__private::{
        UiAuthoringProjection, UiAuthoringProjectionError, UiAuthoringStructuralCapability,
        UiCatalogNodeKind, UiCreateParameters, UiImageChoiceGeneration, UiImageChoiceSelection,
        UiImageChoiceSet, UiPlacementCandidate, UiStructureIntent, UiStructureRejection,
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

use crate::panels::{PanelActionDispatch, PanelSubmissionId, PanelSubmitError};

use super::{editing_enabled, placement_intent_label};

mod input;
mod presentation;
#[cfg(all(test, feature = "imgui-reference"))]
mod tests;

const RETAINED_FRAMES: usize = 3;
const TEXT_SIZE: f32 = 16.0;
const INSET: f32 = 12.0;
const ROW_HEIGHT: f32 = 30.0;
const SOURCE_ROW_HEIGHT: f32 = 52.0;
// Match the authoring owner's creation-text contract; paint windows budget visible glyphs.
const FIELD_TEXT_BYTES: usize = 4096;

type DocumentScope = (EditorDocumentId, SceneAuthoringSourceId);

/// Exact semantic facts, including nominal Schema admission and the complete source projection.
#[derive(Clone, Debug, PartialEq)]
struct UiHierarchyBasis {
    target: EditorDocumentQueryBasis,
    source: EditorDocumentSourceBinding,
    selection: EditorSelectionSet,
    selected: Option<SceneEntityId>,
    capabilities: SceneCompositionCapabilities,
    projection: Result<UiAuthoringProjection, UiAuthoringProjectionError>,
    images: Result<UiImageChoiceSet, UiStructureRejection>,
    image_generation: UiImageChoiceGeneration,
    structure: bool,
    deletion_unavailable_reason: Option<&'static str>,
    editing: bool,
    navigation: bool,
    read_only: bool,
}

impl UiHierarchyBasis {
    fn capture(
        model: &EditorFrameModel,
        ui: &EditorUiAuthoringFrame,
        read_only: bool,
    ) -> Option<Self> {
        let scene = model.workspace().active_scene.as_ref()?;
        let tab = model.workspace().document(ui.document())?;
        if model.workspace().active_document != Some(ui.document())
            || scene.document != ui.document()
            || scene.kind != EditorDocumentKind::Prefab
            || tab.kind != EditorDocumentKind::Prefab
            || scene.revision != ui.revision()
            || tab.revision != ui.revision()
            || tab.source_binding.as_ref() != Some(ui.source_binding())
            || &scene.selection != ui.selection()
            || scene.editor.inspector.selected_entity.as_ref() != ui.selection().top_entity()
        {
            return None;
        }
        Some(Self {
            target: EditorDocumentQueryBasis {
                document: ui.document(),
                revision: ui.revision(),
                schema: tab.authoring_schema_basis.clone(),
            },
            source: ui.source_binding().clone(),
            selection: ui.selection().clone(),
            selected: scene.editor.inspector.selected_entity.clone(),
            capabilities: scene.editor.inspector.composition.clone(),
            projection: ui.projection().cloned(),
            images: ui.image_choices().cloned(),
            image_generation: ui.image_choice_generation(),
            structure: ui.structure_editing_enabled(),
            deletion_unavailable_reason: ui.structure_deletion_unavailable_reason(),
            editing: !read_only && editing_enabled(model),
            navigation: !read_only && model.workspace_intent().intent().is_none(),
            read_only,
        })
    }

    fn scope(&self) -> DocumentScope {
        (self.target.document, self.target.revision.source_id())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PlacementSlot {
    First,
    Last,
    Before(SceneEntityId),
    After(SceneEntityId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PlacementKey {
    parent: SceneEntityId,
    slot: PlacementSlot,
}

impl PlacementKey {
    fn of(candidate: &UiPlacementCandidate) -> Result<Self, OwnedUiHierarchyError> {
        let anchor = |target: &nara::scene::PrefabEntityTarget| match &target.entity {
            PrefabEntityOrigin::SourceEntity(entity) if target.through.is_empty() => {
                Ok(entity.clone())
            }
            _ => Err(OwnedUiHierarchyError::ChangedProjection),
        };
        let slot = match &candidate.placement().slot {
            ScenePlacementSlot::First => PlacementSlot::First,
            ScenePlacementSlot::Last => PlacementSlot::Last,
            ScenePlacementSlot::Before(target) => PlacementSlot::Before(anchor(target)?),
            ScenePlacementSlot::After(target) => PlacementSlot::After(anchor(target)?),
        };
        Ok(Self {
            parent: candidate.parent().clone(),
            slot,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Control {
    Select(SceneEntityId),
    Create,
    Move,
    Delete,
    Kind(UiCatalogNodeKind),
    ChoosePlacement,
    Placement(PlacementKey),
    ChooseImage,
    Image { source: String, path: String },
    CloseChoices,
    Apply,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chooser {
    Placement,
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Text,
    Label,
    Action,
}

struct FormField {
    kind: Field,
    input: UiTextField,
}

enum StructureDialog {
    Create {
        kind: UiCatalogNodeKind,
        placement: ScenePlacement,
        image: Option<UiImageChoiceSelection>,
    },
    Move {
        target: SceneEntityId,
        placement: ScenePlacement,
    },
    Delete {
        target: SceneEntityId,
    },
}

struct StructureForm {
    identity: u64,
    basis: UiHierarchyBasis,
    observation: Arc<AtomicBool>,
    dialog: StructureDialog,
    fields: Vec<FormField>,
}

impl StructureForm {
    fn moving(&self) -> Option<&SceneEntityId> {
        match &self.dialog {
            StructureDialog::Move { target, .. } => Some(target),
            _ => None,
        }
    }

    fn value(&self, kind: Field) -> &str {
        self.fields
            .iter()
            .find(|field| field.kind == kind)
            .map_or("", |field| field.input.editor().text())
    }

    fn intent(&self) -> Result<UiStructureIntent, OwnedUiHierarchyError> {
        if self
            .fields
            .iter()
            .any(|field| field.input.editor().is_composing())
        {
            return Err(OwnedUiHierarchyError::Composing);
        }
        Ok(match &self.dialog {
            StructureDialog::Create {
                kind,
                placement,
                image,
            } => UiStructureIntent::Create {
                parameters: match kind {
                    UiCatalogNodeKind::Container => UiCreateParameters::Container,
                    UiCatalogNodeKind::Text => UiCreateParameters::Text {
                        content: self.value(Field::Text).to_owned(),
                    },
                    UiCatalogNodeKind::Image => UiCreateParameters::Image {
                        selection: image.clone(),
                    },
                    UiCatalogNodeKind::Button => UiCreateParameters::Button {
                        label: self.value(Field::Label).to_owned(),
                        action: Some(nara_ui_kernel::UiActionId::new(
                            self.value(Field::Action).trim(),
                        )?),
                    },
                },
                placement: placement.clone(),
            },
            StructureDialog::Move { target, placement } => UiStructureIntent::Move {
                target: target.clone(),
                placement: placement.clone(),
            },
            StructureDialog::Delete { target } => UiStructureIntent::Delete {
                target: target.clone(),
            },
        })
    }

    fn prepare(&self, ui: &EditorUiAuthoringFrame) -> Result<EditorAction, OwnedUiHierarchyError> {
        let intent = self.intent()?;
        ui.projection()?
            .prepare_structure(&intent, ui.image_choices().ok())?;
        Ok(EditorAction::Mutate(ui.lower_structure_intent(&intent)?))
    }
}

struct ButtonControl {
    key: UiKey,
    state: UiButtonState,
}

#[derive(Clone)]
struct DisplayedButton {
    control: Control,
    button: UiButton,
}

#[derive(Clone)]
struct DisplayedInput {
    key: UiKey,
    form: u64,
    kind: Field,
    width: f32,
    display_text: String,
}

struct CompiledHierarchy {
    frame: Arc<UiFrame>,
    snapshot: UiAuthoringSnapshot,
    theme: UiTheme,
    basis: Option<UiHierarchyBasis>,
    form: Option<u64>,
    observation: Arc<AtomicBool>,
    viewport_observation: Arc<()>,
    buttons: HashMap<UiKey, DisplayedButton>,
    inputs: Vec<DisplayedInput>,
    scroll_limit: f32,
}

#[derive(Clone, Copy)]
struct SubmissionContext {
    scope: DocumentScope,
    form: Option<u64>,
}

/// Compiled specialized UI tree/forms. Source mutations, history and persistence stay in Session.
pub(in crate::frontend) struct OwnedUiHierarchy {
    root: UiRootKey,
    engine: TextEngine,
    compiler: UiCompiler,
    basis: Option<UiHierarchyBasis>,
    observation: Arc<AtomicBool>,
    viewport_observation: Arc<()>,
    identities: HashMap<Control, u64>,
    controls: HashMap<Control, ButtonControl>,
    next_control: u64,
    next_form: u64,
    next_snapshot: u64,
    frames: Vec<CompiledHierarchy>,
    form: Option<StructureForm>,
    chooser: Option<Chooser>,
    source_scroll: f32,
    form_scroll: f32,
    choice_scroll: f32,
    queued: Option<SubmissionContext>,
    pending: Option<(PanelSubmissionId, SubmissionContext)>,
    error: Option<String>,
    input_error: Option<String>,
}

/// Move-only action prepared by the UI owner, never rebased to a new form or active source.
pub(in crate::frontend) struct UiHierarchySubmission {
    basis: UiHierarchyBasis,
    observation: Arc<AtomicBool>,
    form_observation: Option<Arc<AtomicBool>>,
    action: EditorAction,
}

impl UiHierarchySubmission {
    pub(super) fn into_action(
        self,
        model: &EditorFrameModel,
        ui: &EditorUiAuthoringFrame,
        read_only: bool,
    ) -> Result<EditorAction, OwnedUiHierarchyError> {
        if !self.observation.load(Ordering::Relaxed)
            || self
                .form_observation
                .as_ref()
                .is_some_and(|live| !live.load(Ordering::Relaxed))
            || UiHierarchyBasis::capture(model, ui, read_only).as_ref() != Some(&self.basis)
        {
            return Err(OwnedUiHierarchyError::ChangedProjection);
        }
        Ok(self.action)
    }
}

impl OwnedUiHierarchy {
    pub(super) fn new(
        root: UiRootKey,
        fonts: Vec<FontFace>,
    ) -> Result<Self, OwnedUiHierarchyError> {
        Ok(Self {
            root,
            engine: TextEngine::with_fonts(
                fonts.clone(),
                nara_text::TextLimits {
                    max_text_bytes: FIELD_TEXT_BYTES,
                    max_glyphs: FIELD_TEXT_BYTES,
                    ..Default::default()
                },
            )?,
            compiler: UiCompiler::with_fonts(fonts, Default::default())?,
            basis: None,
            observation: Arc::new(AtomicBool::new(true)),
            viewport_observation: Arc::new(()),
            identities: HashMap::new(),
            controls: HashMap::new(),
            next_control: 1,
            next_form: 1,
            next_snapshot: 1,
            frames: Vec::new(),
            form: None,
            chooser: None,
            source_scroll: 0.0,
            form_scroll: 0.0,
            choice_scroll: 0.0,
            queued: None,
            pending: None,
            error: None,
            input_error: None,
        })
    }

    pub(super) fn compiled_frame(&self) -> Option<&Arc<UiFrame>> {
        self.frames.last().map(|compiled| &compiled.frame)
    }

    pub(super) fn retains_frame(&self, frame: &Arc<UiFrame>) -> bool {
        self.frames
            .iter()
            .any(|compiled| Arc::ptr_eq(&compiled.frame, frame))
    }

    fn retained(&self, frame: &Arc<UiFrame>) -> Result<&CompiledHierarchy, OwnedUiHierarchyError> {
        self.frames
            .iter()
            .find(|compiled| Arc::ptr_eq(&compiled.frame, frame))
            .ok_or(OwnedUiHierarchyError::RetiredFrame)
    }

    fn busy(&self) -> bool {
        self.queued.is_some() || self.pending.is_some()
    }

    fn invalidate_buttons(&mut self) {
        self.observation.store(false, Ordering::Relaxed);
        self.observation = Arc::new(AtomicBool::new(true));
        for control in self.controls.values_mut() {
            control.state.cancel_pointer_interaction();
        }
    }

    fn invalidate_actions(&mut self) {
        self.viewport_observation = Arc::new(());
        self.invalidate_buttons();
    }

    fn live_button(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<&DisplayedButton, OwnedUiHierarchyError> {
        let compiled = self.retained(frame)?;
        let button = compiled
            .buttons
            .get(key)
            .ok_or(OwnedUiHierarchyError::UnknownControl)?;
        let latest = self
            .frames
            .last()
            .ok_or(OwnedUiHierarchyError::RetiredFrame)?;
        if !compiled.observation.load(Ordering::Relaxed)
            || !Arc::ptr_eq(&compiled.observation, &self.observation)
            || compiled.basis != latest.basis
            || compiled.form != self.form.as_ref().map(|form| form.identity)
            || latest
                .buttons
                .get(key)
                .is_none_or(|next| next.button != button.button)
            || self
                .controls
                .get(&button.control)
                .is_none_or(|control| control.key != *key)
        {
            return Err(OwnedUiHierarchyError::ChangedProjection);
        }
        Ok(button)
    }

    pub(super) fn invalidated_controls(
        &self,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Result<Vec<UiKey>, OwnedUiHierarchyError> {
        let previous = self.retained(previous)?;
        let candidate = self.retained(candidate)?;
        let mut keys = previous
            .buttons
            .iter()
            .filter(|(key, button)| {
                !Arc::ptr_eq(&previous.observation, &candidate.observation)
                    || previous.basis != candidate.basis
                    || previous.form != candidate.form
                    || candidate
                        .buttons
                        .get(*key)
                        .is_none_or(|next| next.button != button.button)
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        keys.extend(
            previous
                .inputs
                .iter()
                .filter(|input| {
                    previous.basis != candidate.basis
                        || !candidate
                            .inputs
                            .iter()
                            .any(|next| next.key == input.key && next.form == input.form)
                })
                .map(|input| input.key.clone()),
        );
        Ok(keys)
    }

    pub(super) fn dispatch(
        &mut self,
        frame: &Arc<UiFrame>,
        batch: &UiDispatchBatch,
    ) -> Result<UiIntentBatch, OwnedUiHierarchyError> {
        let compiled = self.retained(frame)?;
        if batch.frame_revision() != frame.revision() {
            return Err(OwnedUiHierarchyError::ForeignDispatch);
        }
        let inputs = compiled
            .inputs
            .iter()
            .filter(|input| self.live_input(frame, &input.key).is_ok())
            .map(|input| input.kind)
            .collect::<Vec<_>>();
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
        for kind in inputs {
            let (field, engine) = self.field_mut(kind)?;
            field.reduce_focus(batch, engine)?;
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
        key: &UiKey,
        model: &EditorFrameModel,
        ui: &EditorUiAuthoringFrame,
        read_only: bool,
    ) -> Result<Option<UiHierarchySubmission>, OwnedUiHierarchyError> {
        let result = self.activate_inner(frame, key, model, ui, read_only);
        if let Err(error) = &result {
            self.error.get_or_insert_with(|| error.to_string());
        }
        result
    }

    fn activate_inner(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        model: &EditorFrameModel,
        ui: &EditorUiAuthoringFrame,
        read_only: bool,
    ) -> Result<Option<UiHierarchySubmission>, OwnedUiHierarchyError> {
        let displayed = self.live_button(frame, key)?.clone();
        let basis = self
            .retained(frame)?
            .basis
            .clone()
            .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
        if UiHierarchyBasis::capture(model, ui, read_only).as_ref() != Some(&basis)
            || self.form.as_ref().is_some_and(|form| {
                form.basis != basis || !form.observation.load(Ordering::Relaxed)
            })
        {
            return Err(OwnedUiHierarchyError::ChangedProjection);
        }
        if !displayed.button.enabled() {
            return Err(OwnedUiHierarchyError::Disabled);
        }
        if self.busy() {
            return Err(OwnedUiHierarchyError::Pending);
        }
        let projection = ui.projection()?;
        match displayed.control {
            Control::Create | Control::Move | Control::Delete => {
                let dialog = match displayed.control {
                    Control::Create => {
                        let candidates = projection.structure_placement_candidates(None)?;
                        StructureDialog::Create {
                            kind: UiCatalogNodeKind::Container,
                            placement: candidates
                                .first()
                                .ok_or(OwnedUiHierarchyError::NoPlacement)?
                                .placement()
                                .clone(),
                            image: None,
                        }
                    }
                    Control::Move => {
                        let target = basis
                            .selected
                            .as_ref()
                            .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
                        let candidates = projection.structure_placement_candidates(Some(target))?;
                        let placement = candidates
                            .iter()
                            .find(|candidate| candidate.is_current())
                            .or_else(|| candidates.first())
                            .ok_or(OwnedUiHierarchyError::NoPlacement)?
                            .placement()
                            .clone();
                        StructureDialog::Move {
                            target: target.clone(),
                            placement,
                        }
                    }
                    _ => {
                        let target = basis
                            .selected
                            .clone()
                            .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
                        projection.prepare_structure(
                            &UiStructureIntent::Delete {
                                target: target.clone(),
                            },
                            ui.image_choices().ok(),
                        )?;
                        StructureDialog::Delete { target }
                    }
                };
                self.open_form(basis, dialog)?;
                return Ok(None);
            }
            Control::Cancel => {
                self.cancel_form()?;
                self.error = None;
                return Ok(None);
            }
            Control::CloseChoices => {
                self.chooser = None;
                self.choice_scroll = 0.0;
                self.invalidate_actions();
                return Ok(None);
            }
            Control::ChoosePlacement | Control::ChooseImage => {
                self.blur_fields()?;
                self.chooser = Some(if displayed.control == Control::ChoosePlacement {
                    Chooser::Placement
                } else {
                    Chooser::Image
                });
                self.choice_scroll = 0.0;
                self.invalidate_actions();
                return Ok(None);
            }
            Control::Kind(kind) => {
                self.blur_fields()?;
                let form = self
                    .form
                    .as_mut()
                    .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
                let StructureDialog::Create { kind: current, .. } = &mut form.dialog else {
                    return Err(OwnedUiHierarchyError::UnknownControl);
                };
                *current = kind;
                self.form_scroll = 0.0;
                self.input_error = None;
                self.invalidate_actions();
                return Ok(None);
            }
            Control::Placement(key) => {
                let form = self
                    .form
                    .as_ref()
                    .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
                if self.chooser != Some(Chooser::Placement) {
                    return Err(OwnedUiHierarchyError::ChangedProjection);
                }
                let placement = projection
                    .structure_placement_candidates(form.moving())?
                    .iter()
                    .find(|candidate| {
                        PlacementKey::of(candidate).is_ok_and(|candidate_key| candidate_key == key)
                    })
                    .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                    .placement()
                    .clone();
                match &mut self
                    .form
                    .as_mut()
                    .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                    .dialog
                {
                    StructureDialog::Create {
                        placement: current, ..
                    }
                    | StructureDialog::Move {
                        placement: current, ..
                    } => *current = placement,
                    _ => return Err(OwnedUiHierarchyError::UnknownControl),
                }
                self.chooser = None;
                self.choice_scroll = 0.0;
                self.invalidate_actions();
                return Ok(None);
            }
            Control::Image { source, path } => {
                if self.chooser != Some(Chooser::Image) {
                    return Err(OwnedUiHierarchyError::ChangedProjection);
                }
                let choices = ui.image_choices()?;
                let selection = choices
                    .choices()
                    .iter()
                    .find(|choice| {
                        choice.source_identity() == source && choice.project_path() == path
                    })
                    .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                    .selection(choices.generation());
                let StructureDialog::Create {
                    kind: UiCatalogNodeKind::Image,
                    image,
                    ..
                } = &mut self
                    .form
                    .as_mut()
                    .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                    .dialog
                else {
                    return Err(OwnedUiHierarchyError::UnknownControl);
                };
                *image = Some(selection);
                self.chooser = None;
                self.choice_scroll = 0.0;
                self.invalidate_actions();
                return Ok(None);
            }
            Control::Apply | Control::Select(_) => {}
        }
        let action = match displayed.control {
            Control::Select(entity) => {
                if !projection
                    .hierarchy()
                    .iter()
                    .any(|node| node.entity() == &entity)
                {
                    return Err(OwnedUiHierarchyError::ChangedProjection);
                }
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: basis.target.document,
                    entity: Some(entity),
                })
            }
            Control::Apply => self
                .form
                .as_ref()
                .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                .prepare(ui)?,
            _ => return Err(OwnedUiHierarchyError::UnknownControl),
        };
        self.blur_fields()?;
        self.invalidate_actions();
        self.queued = Some(SubmissionContext {
            scope: basis.scope(),
            form: self.form.as_ref().map(|form| form.identity),
        });
        Ok(Some(UiHierarchySubmission {
            basis,
            observation: Arc::clone(&self.observation),
            form_observation: self.form.as_ref().map(|form| Arc::clone(&form.observation)),
            action,
        }))
    }

    fn open_form(
        &mut self,
        basis: UiHierarchyBasis,
        dialog: StructureDialog,
    ) -> Result<(), OwnedUiHierarchyError> {
        self.cancel_form()?;
        let identity = self.next_form;
        self.next_form = identity
            .checked_add(1)
            .ok_or(OwnedUiHierarchyError::IdentityExhausted)?;
        let fields = if matches!(dialog, StructureDialog::Create { .. }) {
            [
                (Field::Text, "Text", "Text"),
                (Field::Label, "Label", "Button"),
                (Field::Action, "Action", ""),
            ]
            .into_iter()
            .map(|(kind, label, value)| {
                Ok(FormField {
                    kind,
                    input: UiTextField::new(
                        self.root.clone(),
                        label,
                        value,
                        TEXT_SIZE,
                        &mut self.engine,
                    )?,
                })
            })
            .collect::<Result<Vec<_>, OwnedUiHierarchyError>>()?
        } else {
            Vec::new()
        };
        self.form = Some(StructureForm {
            identity,
            basis,
            observation: Arc::new(AtomicBool::new(true)),
            dialog,
            fields,
        });
        self.error = None;
        self.input_error = None;
        self.invalidate_actions();
        Ok(())
    }

    fn cancel_form(&mut self) -> Result<(), OwnedUiHierarchyError> {
        self.blur_fields()?;
        if let Some(form) = self.form.take() {
            form.observation.store(false, Ordering::Relaxed);
        }
        self.chooser = None;
        self.form_scroll = 0.0;
        self.choice_scroll = 0.0;
        self.input_error = None;
        self.invalidate_actions();
        Ok(())
    }

    pub(super) fn escape(&mut self, frame: &Arc<UiFrame>) -> Result<bool, OwnedUiHierarchyError> {
        let compiled = self.retained(frame)?;
        if compiled.form != self.form.as_ref().map(|form| form.identity) {
            return Err(OwnedUiHierarchyError::ChangedProjection);
        }
        if self.busy() {
            return Ok(self.form.is_some());
        }
        if self.chooser.take().is_some() {
            self.choice_scroll = 0.0;
            self.invalidate_actions();
            return Ok(true);
        }
        if self.form.is_some() {
            self.cancel_form()?;
            self.error = None;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn admitted(&mut self, result: Result<PanelSubmissionId, PanelSubmitError>) {
        let Some(context) = self.queued.take() else {
            return;
        };
        match result {
            Ok(id) => self.pending = Some((id, context)),
            Err(error) => {
                self.error = Some(format!(
                    "UI structure action was not queued: {error}. Review the form and retry Apply."
                ))
            }
        }
        self.invalidate_actions();
    }

    pub(super) fn action_finished(&mut self, id: PanelSubmissionId, result: &PanelActionDispatch) {
        let Some((expected, context)) = self.pending else {
            return;
        };
        if expected != id {
            return;
        }
        self.pending = None;
        if self.basis.as_ref().map(UiHierarchyBasis::scope) != Some(context.scope) {
            self.invalidate_actions();
            return;
        }
        match result {
            PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
                if report.applied =>
            {
                if self.form.as_ref().map(|form| form.identity) == context.form {
                    // The input was already blurred when preparing the submission.
                    if let Some(form) = self.form.take() {
                        form.observation.store(false, Ordering::Relaxed);
                    }
                    self.chooser = None;
                    self.form_scroll = 0.0;
                    self.choice_scroll = 0.0;
                }
                self.error = None;
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
                self.error = Some(format!(
                    "UI structure change was refused: {diagnostics}. Review the form and retry Apply."
                ));
            }
            PanelActionDispatch::NotDispatched(reason) => {
                use crate::panels::PanelActionNotDispatched;
                let reason = match reason {
                    PanelActionNotDispatched::Superseded => {
                        "a later submission replaced the prepared batch"
                    }
                    PanelActionNotDispatched::ProjectChanged => {
                        "the project changed before dispatch"
                    }
                    PanelActionNotDispatched::Closing => "the Editor was closing",
                    PanelActionNotDispatched::NoPublishedFrame => {
                        "no current Editor frame was available"
                    }
                    PanelActionNotDispatched::PresentationFailed => {
                        "the prepared presentation failed"
                    }
                    PanelActionNotDispatched::BatchRejected(_) => {
                        "the application refused the prepared batch"
                    }
                };
                self.error = Some(format!(
                    "UI structure change was not dispatched: {reason}. Review the form and retry Apply."
                ));
            }
            PanelActionDispatch::Dispatched(_) => {
                self.error = Some("The source owner did not apply the UI structure change. Review the original form and retry Apply.".to_owned());
            }
        }
        self.invalidate_actions();
    }

    pub(super) fn submission_failed(&mut self, error: String) {
        if self.pending.is_none() {
            self.queued = None;
            self.error = Some(error);
            self.invalidate_actions();
        }
    }

    pub(super) fn scroll(
        &mut self,
        frame: &Arc<UiFrame>,
        delta: f32,
    ) -> Result<bool, OwnedUiHierarchyError> {
        let compiled = self.retained(frame)?;
        if !delta.is_finite() {
            return Err(OwnedUiHierarchyError::InvalidScroll);
        }
        if !Arc::ptr_eq(&compiled.viewport_observation, &self.viewport_observation) {
            return Err(OwnedUiHierarchyError::ChangedProjection);
        }
        let limit = compiled.scroll_limit;
        let offset = if self.chooser.is_some() {
            &mut self.choice_scroll
        } else if self.form.is_some() {
            &mut self.form_scroll
        } else {
            &mut self.source_scroll
        };
        let next = (*offset + delta.clamp(-limit, limit)).clamp(0.0, limit);
        let changed = next != *offset;
        *offset = next;
        if changed {
            self.invalidate_buttons();
        }
        Ok(changed)
    }
}

impl Drop for OwnedUiHierarchy {
    fn drop(&mut self) {
        self.observation.store(false, Ordering::Relaxed);
        if let Some(form) = &self.form {
            form.observation.store(false, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum OwnedUiHierarchyError {
    #[error(transparent)]
    Engine(#[from] TextEngineCreateError),
    #[error(transparent)]
    Compile(#[from] UiCompileError),
    #[error(transparent)]
    Authoring(#[from] UiAuthoringError),
    #[error(transparent)]
    Projection(#[from] UiAuthoringProjectionError),
    #[error(transparent)]
    Structure(#[from] UiStructureRejection),
    #[error(transparent)]
    Lowering(#[from] EditorUiAuthoringEditError),
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
    #[error("UI Hierarchy identity is exhausted")]
    IdentityExhausted,
    #[error("UI Hierarchy viewport has no positive content area")]
    InvalidViewport,
    #[error("UI Hierarchy scroll delta must be finite")]
    InvalidScroll,
    #[error("the exact compiled UI Hierarchy frame is no longer retained")]
    RetiredFrame,
    #[error("the UI key is not a live control in this compiled UI Hierarchy frame")]
    UnknownControl,
    #[error("the dispatch belongs to another compiled frame")]
    ForeignDispatch,
    #[error(
        "the original UI source, revision, selection, Schema, images, permissions or form observation changed; open a new form"
    )]
    ChangedProjection,
    #[error("the displayed UI Hierarchy action is disabled")]
    Disabled,
    #[error("a UI Hierarchy action is awaiting its exact submission result")]
    Pending,
    #[error(
        "finish or cancel text composition before using its selection, geometry or structure change"
    )]
    Composing,
    #[error("no exact source-local placement is available")]
    NoPlacement,
    #[error("the UI Hierarchy root did not publish; its controls have no current authority")]
    Unpublished,
}
