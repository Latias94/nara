use std::sync::Arc;

use dear_imgui_rs::Ui;
use nara::{
    project_host::EditorUiAuthoringFrame,
    tooling::{EditorAction, EditorFrameModel},
};
use nara_text::{FontFace, TextEdit, TextEditOutcome};
use nara_ui_kernel::{
    UiDispatchBatch, UiEnvironment, UiFrame, UiKey, UiPoint, UiRect, UiRootKey, UiSize,
    UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme,
};
use nara_ui_widgets::{UiIntentBatch, UiTextInputToken};

use super::owned_hierarchy::OwnedHierarchy;
use super::owned_ui_hierarchy::OwnedUiHierarchy;
use super::{ImguiEditorPanelContext, PanelActionDispatch, PanelSubmissionId, PanelSubmitError};

/// The source-declaration list and specialized UI tree have distinct domain projections.
/// Each live Hierarchy panel owns exactly one of them and shares the same native input bridge.
pub(super) enum HierarchyControls {
    Source(OwnedHierarchy),
    Ui(OwnedUiHierarchy),
}

pub(super) enum HierarchySubmission {
    Source(super::owned_hierarchy::HierarchySubmission),
    Ui(super::owned_ui_hierarchy::UiHierarchySubmission),
}

impl HierarchySubmission {
    fn into_action(
        self,
        model: &EditorFrameModel,
        ui: Option<&EditorUiAuthoringFrame>,
        read_only: bool,
    ) -> Result<EditorAction, String> {
        match self {
            Self::Source(submission) if ui.is_none() => submission
                .into_action(model, read_only)
                .map_err(|error| error.to_string()),
            Self::Ui(submission) => submission
                .into_action(
                    model,
                    ui.ok_or("The UI source is no longer active")?,
                    read_only,
                )
                .map_err(|error| error.to_string()),
            _ => Err("The Hierarchy projection changed before submission".into()),
        }
    }
}

impl HierarchyControls {
    fn new(root: UiRootKey, fonts: Vec<FontFace>, ui: bool) -> Result<Self, String> {
        if ui {
            OwnedUiHierarchy::new(root, fonts)
                .map(Self::Ui)
                .map_err(|error| error.to_string())
        } else {
            OwnedHierarchy::new(root, fonts)
                .map(Self::Source)
                .map_err(|error| error.to_string())
        }
    }

    pub(super) fn is_ui(&self) -> bool {
        matches!(self, Self::Ui(_))
    }

    pub(super) fn compiled_frame(&self) -> Option<&Arc<UiFrame>> {
        match self {
            Self::Source(owner) => owner.compiled_frame(),
            Self::Ui(owner) => owner.compiled_frame(),
        }
    }

    fn retains_frame(&self, frame: &Arc<UiFrame>) -> bool {
        match self {
            Self::Source(owner) => owner.retains_frame(frame),
            Self::Ui(owner) => owner.retains_frame(frame),
        }
    }

    fn compile(
        &mut self,
        model: &EditorFrameModel,
        ui: Option<&EditorUiAuthoringFrame>,
        environment: UiEnvironment,
        theme: UiTheme,
        read_only: bool,
    ) -> Result<Arc<UiFrame>, String> {
        match self {
            Self::Source(owner) if ui.is_none() => owner
                .compile(model, environment, theme, read_only)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .compile(
                    model,
                    ui.ok_or("The UI source is no longer active")?,
                    environment,
                    theme,
                    read_only,
                )
                .map_err(|error| error.to_string()),
            _ => Err("The Hierarchy projection changed".into()),
        }
    }

    pub(super) fn dispatch(
        &mut self,
        frame: &Arc<UiFrame>,
        batch: &UiDispatchBatch,
    ) -> Result<UiIntentBatch, String> {
        match self {
            Self::Source(owner) => owner
                .dispatch(frame, batch)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .dispatch(frame, batch)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn activate(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        model: &EditorFrameModel,
        ui: Option<&EditorUiAuthoringFrame>,
        read_only: bool,
    ) -> Result<Option<HierarchySubmission>, String> {
        match self {
            Self::Source(owner) if ui.is_none() => owner
                .activate(frame, key, model, read_only)
                .map(|submission| submission.map(HierarchySubmission::Source))
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .activate(
                    frame,
                    key,
                    model,
                    ui.ok_or("The UI source is no longer active")?,
                    read_only,
                )
                .map(|submission| submission.map(HierarchySubmission::Ui))
                .map_err(|error| error.to_string()),
            _ => Err("The Hierarchy projection changed".into()),
        }
    }

    pub(super) fn admitted(&mut self, result: Result<PanelSubmissionId, PanelSubmitError>) {
        match self {
            Self::Source(owner) => owner.admitted(result),
            Self::Ui(owner) => owner.admitted(result),
        }
    }

    pub(super) fn action_finished(&mut self, id: PanelSubmissionId, result: &PanelActionDispatch) {
        match self {
            Self::Source(owner) => owner.action_finished(id, result),
            Self::Ui(owner) => owner.action_finished(id, result),
        }
    }

    fn submission_failed(&mut self, error: String) {
        match self {
            Self::Source(owner) => owner.submission_failed(error),
            Self::Ui(owner) => owner.submission_failed(error),
        }
    }

    pub(super) fn invalidated_controls(
        &self,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Result<Vec<UiKey>, String> {
        match self {
            Self::Source(owner) => owner
                .invalidated_controls(previous, candidate)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .invalidated_controls(previous, candidate)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn scroll(&mut self, frame: &Arc<UiFrame>, delta: f32) -> Result<bool, String> {
        match self {
            Self::Source(owner) => owner
                .scroll(frame, delta)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .scroll(frame, delta)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn input_token(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<Option<UiTextInputToken>, String> {
        match self {
            Self::Source(owner) => owner
                .input_token(frame, key)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .input_token(frame, key)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn blur(&mut self, frame: &Arc<UiFrame>, key: &UiKey) -> Result<(), String> {
        match self {
            Self::Source(owner) => owner.blur(frame, key).map_err(|error| error.to_string()),
            Self::Ui(owner) => owner.blur(frame, key).map_err(|error| error.to_string()),
        }
    }

    pub(super) fn cancel_input(&mut self) -> Result<(), String> {
        match self {
            Self::Source(owner) => owner.cancel_input().map_err(|error| error.to_string()),
            Self::Ui(owner) => owner.cancel_input().map_err(|error| error.to_string()),
        }
    }

    pub(super) fn edit(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
    ) -> Result<TextEditOutcome, String> {
        match self {
            Self::Source(owner) => owner
                .edit(frame, key, token, edit)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .edit(frame, key, token, edit)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn point(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
        point: UiPoint,
        extend: bool,
    ) -> Result<TextEditOutcome, String> {
        match self {
            Self::Source(owner) => owner
                .point(frame, key, token, point, extend)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .point(frame, key, token, point, extend)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn selected_text(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
    ) -> Result<Option<String>, String> {
        match self {
            Self::Source(owner) => owner
                .selected_text(frame, key, token)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .selected_text(frame, key, token)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn is_composing(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
    ) -> Result<bool, String> {
        match self {
            Self::Source(owner) => owner
                .is_composing(frame, key, token)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .is_composing(frame, key, token)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn ime_area(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
    ) -> Result<UiRect, String> {
        match self {
            Self::Source(owner) => owner
                .ime_area(frame, key, token)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .ime_area(frame, key, token)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn input_error(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
        error: String,
    ) {
        match self {
            Self::Source(owner) => owner.input_error(frame, key, token, error),
            Self::Ui(owner) => owner.input_error(frame, key, token, error),
        }
    }

    pub(super) fn finish_input(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        discard: bool,
    ) -> Result<(), String> {
        match self {
            Self::Source(owner) => owner
                .finish_input(frame, key, discard)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .finish_input(frame, key, discard)
                .map_err(|error| error.to_string()),
        }
    }

    pub(super) fn escape(&mut self, frame: &Arc<UiFrame>) -> Result<bool, String> {
        match self {
            Self::Source(_) => Ok(false),
            Self::Ui(owner) => owner.escape(frame).map_err(|error| error.to_string()),
        }
    }
}

pub(super) fn paint(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    let specialized = context.input.ui_authoring.is_some();
    if context
        .widgets
        .owned_hierarchy
        .as_ref()
        .is_some_and(|owner| owner.is_ui() != specialized)
    {
        if let Some(mut owner) = context.widgets.owned_hierarchy.take() {
            let _ = owner.cancel_input();
        }
        context.widgets.hierarchy_pending.clear();
    }
    for submission in std::mem::take(&mut context.widgets.hierarchy_pending) {
        match submission.into_action(
            context.input.frame,
            context.input.ui_authoring,
            context.input.read_only,
        ) {
            Ok(action) => {
                let result = context.submit_hierarchy(action);
                if let Some(hierarchy) = context.widgets.owned_hierarchy.as_mut() {
                    hierarchy.admitted(result);
                }
            }
            Err(error) => {
                if let Some(hierarchy) = context.widgets.owned_hierarchy.as_mut() {
                    hierarchy.submission_failed(format!(
                        "The source changed before this action was queued: {error}. Review it and retry."
                    ));
                }
            }
        }
    }
    if let Err(error) = paint_panel(ui, context) {
        ui.text_wrapped(error);
    }
}

fn paint_panel(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) -> Result<(), String> {
    let panel = context.model.instance().id();
    let serial = context.widgets.incarnation.serial;
    if context.widgets.owned_hierarchy.is_none() {
        let fonts = if context.input.owned_fonts.is_empty() {
            vec![nara_text::bundled_font_face().map_err(|error| error.to_string())?]
        } else {
            context.input.owned_fonts.to_vec()
        };
        let root = UiRootKey::runtime("nara.editor.hierarchy", serial.to_string())
            .map_err(|error| error.to_string())?;
        context.widgets.owned_hierarchy = Some(HierarchyControls::new(
            root,
            fonts,
            context.input.ui_authoring.is_some(),
        )?);
    }
    context.widgets.hierarchy_read_only = context.input.read_only;
    let available = ui.content_region_avail();
    let size = [available[0].max(60.0), available[1].max(60.0)];
    let scale = super::framebuffer_scale(ui)[0];
    let hierarchy = context
        .widgets
        .owned_hierarchy
        .as_mut()
        .ok_or("Hierarchy controls are absent")?;
    let changed = hierarchy.compiled_frame().is_none_or(|frame| {
        let environment = frame.environment();
        environment.logical_size().width() != size[0]
            || environment.logical_size().height() != size[1]
            || environment.scale_factor() != scale
    });
    if changed {
        context.widgets.owned_revision = context
            .widgets
            .owned_revision
            .checked_add(1)
            .ok_or("Hierarchy geometry identity exhausted")?;
    }
    let slot = serial
        .checked_add(1)
        .ok_or("Hierarchy identity exhausted")?;
    let surface = UiSurfaceId::new(slot).ok_or("Hierarchy surface is invalid")?;
    let logical = UiSize::new(size[0], size[1]).ok_or("Hierarchy logical size is invalid")?;
    let transform =
        UiSurfaceContentTransform::direct(logical, scale).map_err(|error| error.to_string())?;
    let revision = context.widgets.owned_revision;
    let environment = UiEnvironment::for_surface(
        surface,
        transform,
        UiSurfaceGenerations::new(slot, 1, revision, revision)
            .map_err(|error| error.to_string())?,
        1,
        1,
    )
    .map_err(|error| error.to_string())?;
    let candidate = hierarchy
        .compile(
            context.input.frame,
            context.input.ui_authoring,
            environment,
            UiTheme::default(),
            context.input.read_only,
        )
        .map_err(|error| error.to_string())?;
    context.output.owned_candidates.push((panel, candidate));
    if let Some(display) = context.input.owned_images.get(&panel).filter(|display| {
        hierarchy.retains_frame(&display.frame)
            && display.frame.environment().surface_id() == surface
            && display.frame.environment().slot_generation().get() == slot
            && display.frame.environment().logical_size() == logical
            && display.frame.environment().scale_factor() == scale
    }) {
        ui.image_config(display.image.texture, size).build();
        context.output.owned_displays.push((
            panel,
            super::image_interaction(ui, display.image.occurrence),
        ));
    } else {
        ui.text_disabled("Preparing Nara Hierarchy");
    }
    Ok(())
}
