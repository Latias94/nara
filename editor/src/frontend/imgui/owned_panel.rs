use std::{collections::BTreeMap, sync::Arc};

use dear_imgui_rs::Ui;
use nara::tooling::{EditorFrameModel, EditorSourceToolAction};
use nara_text::{FontFace, TextEdit};
use nara_ui_kernel::{
    UiDispatchBatch, UiEnvironment, UiFrame, UiKey, UiPoint, UiRootKey, UiSize,
    UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme,
};
use nara_ui_widgets::UiTextInputToken;

use crate::workspace::EditorPanelInstanceId;

use super::owned_inspector::{InspectorBinding, InspectorOperation, OwnedInspector};
use super::{ImguiEditorFrontend, ImguiEditorPanelContext, ImguiViewImage};

pub(super) enum OwnedSubmission {
    Field(super::field_drafts::PreparedFieldEdit),
    SourceTool(EditorSourceToolAction),
}

#[derive(Clone)]
pub(crate) struct ImguiOwnedImage {
    pub(crate) frame: Arc<UiFrame>,
    pub(crate) image: ImguiViewImage,
}

impl ImguiEditorFrontend {
    pub(crate) fn scroll_owned_panel(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        delta: f32,
    ) -> Result<bool, String> {
        self.presentation
            .widgets
            .get_mut(&panel)
            .and_then(|widgets| widgets.owned_inspector.as_mut())
            .ok_or("Inspector panel is retired")?
            .scroll(frame, delta)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn set_owned_images(
        &mut self,
        fonts: Vec<FontFace>,
        images: BTreeMap<EditorPanelInstanceId, ImguiOwnedImage>,
    ) {
        if !self
            .owned_fonts
            .iter()
            .map(FontFace::id)
            .eq(fonts.iter().map(FontFace::id))
        {
            for widgets in self.presentation.widgets.values_mut() {
                widgets.owned_inspector = None;
                widgets.owned_revision = 0;
            }
        }
        self.owned_fonts = fonts;
        self.owned_images = images;
    }

    pub(crate) fn reduce_owned_input(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        dispatch: &UiDispatchBatch,
        model: &EditorFrameModel,
        clipboard: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        let widgets = self
            .presentation
            .widgets
            .get_mut(&panel)
            .ok_or("Inspector panel is retired")?;
        let inspector = widgets
            .owned_inspector
            .as_mut()
            .ok_or("Inspector controls are absent")?;
        let intents = inspector
            .dispatch(frame, dispatch)
            .map_err(|error| error.to_string())?;
        for intent in intents.intents() {
            match inspector
                .binding(frame, intent.target())
                .map_err(|error| error.to_string())?
            {
                InspectorBinding::Copy { .. } => {
                    let text = inspector
                        .copy_draft(frame, intent.target(), &widgets.authoring.fields)
                        .map_err(|error| error.to_string())?;
                    inspector
                        .copy_finished(frame, intent.target(), clipboard(&text))
                        .map_err(|error| error.to_string())?;
                    continue;
                }
                InspectorBinding::DiscardRetainedDrafts => {
                    inspector
                        .activate(frame, intent.target(), model, &mut widgets.authoring.fields)
                        .map_err(|error| error.to_string())?;
                    continue;
                }
                InspectorBinding::SourceTool { field, .. } => {
                    if widgets.owned_pending.len() == 64 {
                        break;
                    }
                    let action = inspector
                        .source_tool_action(frame, intent.target(), model)
                        .map_err(|error| error.to_string())?;
                    if let Some(action) = widgets.authoring.source_tool.offer_owned(action) {
                        inspector
                            .tool_queued(frame, &field)
                            .map_err(|error| error.to_string())?;
                        widgets
                            .owned_pending
                            .push(OwnedSubmission::SourceTool(action));
                    }
                    continue;
                }
                _ => {}
            }
            if widgets.owned_pending.len() == 64 {
                break;
            }
            if let Some(edit) = inspector
                .activate(frame, intent.target(), model, &mut widgets.authoring.fields)
                .map_err(|error| error.to_string())?
            {
                widgets.owned_pending.push(OwnedSubmission::Field(edit));
            }
        }
        Ok(())
    }

    pub(crate) fn reconcile_owned_input(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        dispatch: &UiDispatchBatch,
    ) -> Result<(), String> {
        let inspector = self
            .presentation
            .widgets
            .get_mut(&panel)
            .and_then(|widgets| widgets.owned_inspector.as_mut())
            .ok_or("Inspector panel is retired")?;
        let intents = inspector
            .dispatch(frame, dispatch)
            .map_err(|error| error.to_string())?;
        if !intents.intents().is_empty() {
            return Err("Inspector cleanup cannot activate an operation".into());
        }
        Ok(())
    }

    pub(crate) fn owned_invalidated_controls(
        &self,
        panel: EditorPanelInstanceId,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Vec<UiKey> {
        self.presentation
            .widgets
            .get(&panel)
            .and_then(|widgets| widgets.owned_inspector.as_ref())
            .and_then(|inspector| inspector.invalidated_controls(previous, candidate).ok())
            .unwrap_or_else(|| {
                previous
                    .hit_regions()
                    .iter()
                    .map(|region| region.key().clone())
                    .collect()
            })
    }

    pub(crate) fn cancel_owned_panel_input(
        &mut self,
        panel: EditorPanelInstanceId,
    ) -> Result<(), String> {
        if let Some(inspector) = self
            .presentation
            .widgets
            .get_mut(&panel)
            .and_then(|widgets| widgets.owned_inspector.as_mut())
        {
            inspector
                .cancel_input()
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(crate) fn owned_field_token(
        &self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Option<UiTextInputToken> {
        self.presentation
            .widgets
            .get(&panel)?
            .owned_inspector
            .as_ref()?
            .input_token(frame, field)
            .ok()
            .flatten()
    }

    pub(crate) fn owned_blur(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<(), String> {
        self.presentation
            .widgets
            .get_mut(&panel)
            .ok_or("Inspector panel is retired")?
            .owned_inspector
            .as_mut()
            .ok_or("Inspector controls are absent")?
            .blur(frame, field)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn owned_point(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        point: UiPoint,
        extend: bool,
    ) -> Result<(), String> {
        let widgets = self
            .presentation
            .widgets
            .get_mut(&panel)
            .ok_or("Inspector panel is retired")?;
        widgets
            .owned_inspector
            .as_mut()
            .ok_or("Inspector controls are absent")?
            .point(
                frame,
                field,
                token,
                point,
                extend,
                &mut widgets.authoring.fields,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn owned_edit(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
    ) -> Result<(), String> {
        let widgets = self
            .presentation
            .widgets
            .get_mut(&panel)
            .ok_or("Inspector panel is retired")?;
        widgets
            .owned_inspector
            .as_mut()
            .ok_or("Inspector controls are absent")?
            .edit(frame, field, token, edit, &mut widgets.authoring.fields)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn owned_selected_text(
        &self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<Option<String>, String> {
        let widgets = self
            .presentation
            .widgets
            .get(&panel)
            .ok_or("Inspector panel is retired")?;
        widgets
            .owned_inspector
            .as_ref()
            .ok_or("Inspector controls are absent")?
            .selected_text(frame, field, token, &widgets.authoring.fields)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn owned_input_error(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        error: String,
    ) {
        if let Some(widgets) = self.presentation.widgets.get_mut(&panel)
            && let Some(inspector) = widgets.owned_inspector.as_mut()
        {
            inspector.input_error(frame, field, token, &mut widgets.authoring.fields, error);
        }
    }

    pub(crate) fn owned_composing(
        &self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> bool {
        self.presentation
            .widgets
            .get(&panel)
            .and_then(|widgets| widgets.owned_inspector.as_ref())
            .is_some_and(|inspector| inspector.is_composing(frame, field, token).unwrap_or(false))
    }

    pub(crate) fn owned_ime_area(
        &self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Option<nara_ui_kernel::UiRect> {
        self.presentation
            .widgets
            .get(&panel)?
            .owned_inspector
            .as_ref()?
            .ime_area(frame, field, token)
            .ok()
    }

    pub(crate) fn cancel_owned_input(&mut self) {
        for widgets in self.presentation.widgets.values_mut() {
            if let Some(inspector) = widgets.owned_inspector.as_mut() {
                let _ = inspector.cancel_input();
            }
        }
    }

    pub(crate) fn owned_operation(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        operation: InspectorOperation,
        model: &EditorFrameModel,
    ) -> Result<(), String> {
        let widgets = self
            .presentation
            .widgets
            .get_mut(&panel)
            .ok_or("Inspector panel is retired")?;
        if widgets.owned_pending.len() == 64 {
            return Err("Inspector submission capacity reached".into());
        }
        let inspector = widgets
            .owned_inspector
            .as_mut()
            .ok_or("Inspector controls are absent")?;
        if inspector.is_tool_input(frame, field) {
            if operation == InspectorOperation::Discard {
                return inspector
                    .discard_tool_input(frame, field)
                    .map_err(|error| error.to_string());
            }
            if operation != InspectorOperation::Apply {
                return Err("The tool parameter requires an explicit proposal".into());
            }
            let action = inspector
                .prepare_tool_input(frame, field, model)
                .map_err(|error| error.to_string())?;
            if let Some(action) = widgets.authoring.source_tool.offer_owned(action) {
                inspector
                    .tool_queued(frame, field)
                    .map_err(|error| error.to_string())?;
                widgets
                    .owned_pending
                    .push(OwnedSubmission::SourceTool(action));
            }
            return Ok(());
        }
        let edit = widgets
            .owned_inspector
            .as_mut()
            .ok_or("Inspector controls are absent")?
            .operate(
                frame,
                field,
                operation,
                model,
                &mut widgets.authoring.fields,
            )
            .map_err(|error| error.to_string())?;
        if let Some(edit) = edit {
            widgets.owned_pending.push(OwnedSubmission::Field(edit));
        }
        Ok(())
    }

    pub(crate) fn owned_field_at(
        &self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> bool {
        self.presentation
            .widgets
            .get(&panel)
            .and_then(|widgets| widgets.owned_inspector.as_ref())
            .is_some_and(|inspector| {
                matches!(
                    inspector.binding(frame, field),
                    Ok(InspectorBinding::Field(_))
                )
            })
    }
}

#[cfg(test)]
mod tests;

pub(super) fn paint(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    let panel = context.model.instance().id();
    let serial = context.widgets.incarnation.serial;
    if context.widgets.owned_inspector.is_none() {
        let fonts = if context.input.owned_fonts.is_empty() {
            match nara_text::bundled_font_face() {
                Ok(face) => vec![face],
                Err(error) => {
                    ui.text(error.to_string());
                    return;
                }
            }
        } else {
            context.input.owned_fonts.to_vec()
        };
        let inspector = UiRootKey::runtime("nara.editor.inspector", serial.to_string())
            .map_err(|error| error.to_string())
            .and_then(|root| OwnedInspector::new(root, fonts).map_err(|error| error.to_string()));
        match inspector {
            Ok(inspector) => context.widgets.owned_inspector = Some(inspector),
            Err(error) => {
                ui.text_wrapped(error);
                return;
            }
        }
    }
    let available = ui.content_region_avail();
    let size = [available[0].max(60.0), available[1].max(60.0)];
    let scale = super::framebuffer_scale(ui)[0];
    let Some(surface) = serial.checked_add(1).and_then(UiSurfaceId::new) else {
        ui.text("Inspector identity exhausted");
        return;
    };
    let previous = context
        .widgets
        .owned_inspector
        .as_ref()
        .and_then(OwnedInspector::compiled_frame);
    let changed = previous.is_none_or(|frame| {
        let environment = frame.environment();
        environment.logical_size().width() != size[0]
            || environment.logical_size().height() != size[1]
            || environment.scale_factor() != scale
    });
    if changed {
        let Some(revision) = context.widgets.owned_revision.checked_add(1) else {
            ui.text("Inspector geometry identity exhausted");
            return;
        };
        context.widgets.owned_revision = revision;
    }
    let revision = context.widgets.owned_revision;
    let environment = (|| {
        let size = UiSize::new(size[0], size[1]).ok_or("Inspector logical size is invalid")?;
        let transform =
            UiSurfaceContentTransform::direct(size, scale).map_err(|error| error.to_string())?;
        UiEnvironment::for_surface(
            surface,
            transform,
            UiSurfaceGenerations::new(serial + 1, 1, revision, revision)
                .map_err(|error| error.to_string())?,
            1,
            1,
        )
        .map_err(|error| error.to_string())
    })();
    let candidate = environment.and_then(|environment| {
        let review = context
            .preparation
            .as_ref()
            .ok()
            .and_then(|session| session.source_tool_review());
        let tools = context.widgets.authoring.source_tool.owned_presentation(
            context.input.frame,
            review.as_ref(),
            !context.input.read_only && super::authoring::editing_enabled(context.input.frame),
        );
        context
            .widgets
            .owned_inspector
            .as_mut()
            .expect("Inspector initialized")
            .compile(
                context.input.frame,
                &mut context.widgets.authoring.fields,
                environment,
                UiTheme::default(),
                context.input.read_only,
                Some(&tools),
            )
            .map_err(|error| error.to_string())
    });
    match candidate {
        Ok(frame) => context.output.owned_candidates.push((panel, frame)),
        Err(error) => {
            ui.text_wrapped(error);
            return;
        }
    }
    if let Some(display) = context.input.owned_images.get(&panel).filter(|display| {
        display.frame.environment().surface_id() == surface
            && display.frame.environment().slot_generation().get() == serial + 1
    }) {
        ui.image_config(display.image.texture, size).build();
        context.output.owned_displays.push((
            panel,
            super::image_interaction(ui, display.image.occurrence),
        ));
    } else {
        ui.text_disabled("Preparing Nara Inspector");
    }
}
