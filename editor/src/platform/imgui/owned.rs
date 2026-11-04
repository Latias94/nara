use std::sync::Arc;

use nara::{
    core::{Color, Mat3},
    ecs::Entity,
    render::{
        __private::{
            EditorRenderGeneration, build_editor_render_frame_packet, editor_viewport_image_target,
        },
        Extent2d, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, RenderViewId,
        RenderWindowPacket, ViewLoadOp, ViewMapping2d, ViewportRect,
    },
    render_wgpu::{
        WgpuImageCompositeReceipt, WgpuRenderHost, WgpuStockCapture, WgpuStockCaptureAdapter,
    },
};
use nara_text::FontFace;
use nara_ui_kernel::__private::UiFrameJournalAuthority;
use nara_ui_kernel::{
    UiDispatchBatch, UiDispatchOutcome, UiFrame, UiFrameJournal, UiInputEvent,
    UiInteractionSession, UiRouteStamp,
};
use nara_ui_render::UiPresentationPlanner;
use winit::{
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::ModifiersState,
    window::WindowId,
};

mod keyboard;
mod scene;
pub(super) use scene::OwnedPanelCandidate;
use scene::OwnedSceneSample;

use crate::{
    frontend::imgui::{ImguiFrontendOutput, ImguiImageInteraction, ImguiOwnedImage},
    workspace::EditorPanelInstanceId,
};

use super::{
    EditorRunError, ImguiRenderer, ImguiTarget, ImguiViewImage, NativeImgui, imgui_error,
    platform_gpu_error,
};

pub(super) fn font_policy() -> Result<Vec<FontFace>, EditorRunError> {
    let mut fonts = vec![nara_text::bundled_font_face().map_err(imgui_error)?];
    if let Some(path) = std::env::var_os("NARA_EDITOR_CJK_FONT") {
        let path = std::path::PathBuf::from(path);
        if std::fs::metadata(&path).map_err(imgui_error)?.len()
            > nara_text::MAX_FONT_CONTAINER_BYTES as u64
        {
            return Err(imgui_error(
                "Editor fallback font exceeds the container limit",
            ));
        }
        fonts.push(
            FontFace::from_bytes(std::fs::read(path).map_err(imgui_error)?, 0)
                .map_err(imgui_error)?,
        );
    }
    Ok(fonts)
}

#[derive(Default)]
pub(super) struct OwnedPanelInput {
    focus: Option<OwnedTextFocus>,
    keyboard: Option<(EditorPanelInstanceId, WindowId)>,
    cursor: Option<(WindowId, winit::dpi::PhysicalPosition<f64>)>,
    modifiers: ModifiersState,
    ime_ready: bool,
    dragging: bool,
    pointer_panel: Option<EditorPanelInstanceId>,
    scene_drag: Option<scene::OwnedSceneDrag>,
}

#[derive(Clone)]
struct OwnedTextFocus {
    panel: EditorPanelInstanceId,
    frame: Arc<UiFrame>,
    field: nara_ui_kernel::UiKey,
    token: nara_ui_widgets::UiTextInputToken,
    window: WindowId,
}

pub(super) struct OwnedPanelSurface {
    pub(super) target: ImguiTarget,
    generation: EditorRenderGeneration,
    image: nara::asset::Handle<RenderImage2d>,
    view: RenderViewId,
    planner: UiPresentationPlanner,
    policy: WgpuStockCaptureAdapter,
    pending: Option<Arc<UiFrame>>,
    presented: Option<Arc<UiFrame>>,
    journal: UiFrameJournal,
    authority: UiFrameJournalAuthority,
    interaction: UiInteractionSession,
    route: Option<UiRouteStamp>,
    mapping: Option<(WindowId, ImguiImageInteraction)>,
    scene: Option<OwnedSceneSample>,
}

impl OwnedPanelSurface {
    pub(super) fn new(fonts: Vec<FontFace>) -> Result<Self, EditorRunError> {
        let (journal, authority) =
            nara_ui_kernel::__private::new_hosted_frame_journal(Default::default())
                .map_err(imgui_error)?;
        Ok(Self {
            target: ImguiTarget::default(),
            generation: EditorRenderGeneration::try_acquire()
                .ok_or_else(|| imgui_error("Owned panel render generation exhausted"))?,
            image: RenderImage2d::allocate_handle().map_err(imgui_error)?,
            view: RenderViewId::try_acquire()
                .ok_or_else(|| imgui_error("Owned panel View identity exhausted"))?,
            planner: UiPresentationPlanner::with_fonts(fonts, Default::default())
                .map_err(imgui_error)?,
            policy: WgpuStockCaptureAdapter::stock().map_err(imgui_error)?,
            pending: None,
            presented: None,
            journal,
            authority,
            interaction: UiInteractionSession::default(),
            route: None,
            mapping: None,
            scene: None,
        })
    }

    pub(super) fn prepare(
        &mut self,
        candidate: OwnedPanelCandidate,
        gpu: &mut WgpuRenderHost,
        renderer: &mut ImguiRenderer,
        window: RenderWindowPacket,
        frame_index: u64,
    ) -> Result<(), EditorRunError> {
        self.prepare_scene(candidate.scene, gpu, renderer)?;
        let frame = candidate.frame;
        if self
            .pending
            .as_ref()
            .or(self.presented.as_ref())
            .is_some_and(|current| Arc::ptr_eq(current, &frame))
        {
            return Ok(());
        }
        let environment = frame.environment();
        let logical = environment.logical_size();
        let pixels = environment.content_transform().physical_surface_size();
        let extent = Extent2d::new(pixels.width().ceil() as u32, pixels.height().ceil() as u32)
            .ok_or_else(|| imgui_error("Inspector physical extent is invalid"))?;
        let viewport = ViewportRect::from_extent(extent)
            .ok_or_else(|| imgui_error("Inspector viewport is invalid"))?;
        let target = RenderTarget::Image(self.image);
        let paint = self
            .planner
            .plan(Arc::clone(&frame), target, &Default::default())
            .map_err(imgui_error)?;
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: self.view,
            camera_entity: Entity::PLACEHOLDER,
            target,
            viewport,
            mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, logical.height(), viewport)
                .ok_or_else(|| imgui_error("Inspector view mapping is invalid"))?,
            order: 0,
            load: ViewLoadOp::Clear(Color::rgba(0.075, 0.085, 0.11, 1.0)),
        });
        let packet = build_editor_render_frame_packet(
            self.generation,
            frame_index,
            &views,
            editor_viewport_image_target(self.image, extent).map_err(imgui_error)?,
        )
        .map_err(imgui_error)?
        .ok_or_else(|| imgui_error("Inspector render View is absent"))?;
        let input = self
            .policy
            .capture(
                packet,
                WgpuStockCapture::new().with_ui(vec![paint], None, None),
            )
            .map_err(imgui_error)?;
        let nara::render::RenderFrameTarget::Image(image_target) = input.packet().target() else {
            return Err(imgui_error("Inspector render target is not an image"));
        };
        self.target.ensure(
            gpu,
            window,
            input.packet().source_generation(),
            image_target,
        )?;
        let producer = self
            .target
            .view
            .as_ref()
            .ok_or_else(|| imgui_error("Inspector producer is absent"))?;
        let output = gpu.submit(producer, &input).map_err(platform_gpu_error)?;
        if self
            .target
            .identity
            .is_none_or(|identity| !identity.matches_submission(output.receipt()))
        {
            return Err(imgui_error(
                "Inspector submission does not match its target",
            ));
        }
        let display = gpu.display_image(output).map_err(platform_gpu_error)?;
        self.target.select_display(renderer, display)?;
        if self.presented.is_none() && self.pending.is_none() {
            self.journal
                .install_surface(
                    environment.surface_id(),
                    environment.slot_generation().get(),
                )
                .map_err(imgui_error)?;
        }
        self.route = None;
        self.pending = Some(frame);
        Ok(())
    }

    pub(super) fn image(&self) -> Option<ImguiViewImage> {
        self.target.image()
    }

    fn displayed_frame(&self) -> Option<&Arc<UiFrame>> {
        self.pending.as_ref().or(self.presented.as_ref())
    }

    pub(super) fn promote(
        &mut self,
        receipts: impl IntoIterator<Item = WgpuImageCompositeReceipt>,
        frame_index: u64,
        invalidated_keys: &[nara_ui_kernel::UiKey],
    ) -> Result<Option<(Arc<UiFrame>, UiDispatchBatch)>, EditorRunError> {
        let matched = self.target.display.as_ref().is_some_and(|display| {
            receipts.into_iter().any(|receipt| {
                receipt.image() == display.receipt() && receipt.occurrence() == display.occurrence()
            })
        });
        if !matched {
            self.invalidate();
            return Ok(None);
        }
        if let Some(frame) = self.pending.take() {
            let receipt = self
                .authority
                .submit(&mut self.journal, Arc::clone(&frame), frame_index)
                .map_err(imgui_error)?;
            let dispatch = self
                .interaction
                .reconcile_latest_presentation(
                    &self.journal,
                    frame.environment().surface_id(),
                    invalidated_keys,
                )
                .map_err(imgui_error)?;
            self.route = Some(UiRouteStamp::from_receipt(receipt));
            self.presented = Some(frame);
            return Ok(dispatch.map(|dispatch| {
                (
                    Arc::clone(self.presented.as_ref().expect("Inspector was promoted")),
                    dispatch,
                )
            }));
        }
        if let Some(frame) = self.presented.as_ref() {
            self.route = Some(UiRouteStamp::from_receipt(
                self.journal
                    .latest(frame.environment().surface_id())
                    .map_err(imgui_error)?
                    .receipt(),
            ));
        }
        Ok(None)
    }

    pub(super) fn dispatch(
        &mut self,
        event: UiInputEvent,
    ) -> Result<Option<(Arc<UiFrame>, UiDispatchOutcome)>, EditorRunError> {
        let Some(route) = self.route else {
            return Ok(None);
        };
        let frame = self
            .presented
            .as_ref()
            .ok_or_else(|| imgui_error("Inspector presented geometry is absent"))?;
        let outcome = self
            .interaction
            .dispatch(&self.journal, route, event)
            .map_err(imgui_error)?;
        Ok(Some((Arc::clone(frame), outcome)))
    }

    pub(super) fn invalidate(&mut self) {
        self.route = None;
        self.interaction = UiInteractionSession::default();
    }
}

impl NativeImgui {
    #[cfg(test)]
    pub(super) fn owned_frames_for_test(
        &self,
    ) -> Vec<(EditorPanelInstanceId, Arc<UiFrame>, ImguiImageInteraction)> {
        self.owned
            .iter()
            .filter_map(|(panel, surface)| {
                surface.route?;
                Some((
                    *panel,
                    Arc::clone(surface.presented.as_ref()?),
                    surface.mapping?.1,
                ))
            })
            .collect()
    }

    #[cfg(test)]
    pub(super) fn owned_text_focus_for_test(&self) -> bool {
        self.owned_input.focus.as_ref().is_some_and(|focus| {
            self.frontend
                .owned_field_token(focus.panel, &focus.frame, &focus.field)
                == Some(focus.token)
        })
    }

    #[cfg(test)]
    pub(in crate::platform::imgui) fn owned_pending_frame_for_test(
        &self,
        panel: EditorPanelInstanceId,
    ) -> Option<Arc<UiFrame>> {
        self.owned_pending
            .get(&panel)
            .map(|candidate| Arc::clone(&candidate.frame))
    }
    fn cancel_owned_focus(&mut self) -> Result<(), EditorRunError> {
        self.release_owned_focus(None)
    }

    fn release_owned_focus(
        &mut self,
        preserve_panel: Option<EditorPanelInstanceId>,
    ) -> Result<(), EditorRunError> {
        if let Some((panel, _)) = self.owned_input.keyboard
            && preserve_panel != Some(panel)
        {
            if let Some(surface) = self.owned.get_mut(&panel) {
                surface.interaction = UiInteractionSession::default();
            }
            self.frontend
                .cancel_owned_panel_input(panel)
                .map_err(imgui_error)?;
            self.owned_input.keyboard = None;
        }
        if let Some(focus) = self.owned_input.focus.take() {
            let _ = self
                .frontend
                .owned_blur(focus.panel, &focus.frame, &focus.field);
            self.platform.set_ime_allowed(false).map_err(imgui_error)?;
            self.platform.set_ime_auto_management(true);
        }
        self.owned_input.ime_ready = false;
        self.owned_input.dragging = false;
        Ok(())
    }

    pub(super) fn handle_owned_event(
        &mut self,
        window: WindowId,
        event: &WindowEvent,
        application: &crate::EditorApplication,
    ) -> Result<bool, EditorRunError> {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.owned_input.modifiers = modifiers.state()
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.owned_input.cursor = Some((window, *position))
            }
            WindowEvent::CursorLeft { .. } => {
                if self
                    .owned_input
                    .cursor
                    .is_some_and(|(owner, _)| owner == window)
                {
                    self.owned_input.cursor = None;
                }
                if self.owned_input.pointer_panel.is_some_and(|panel| {
                    self.owned.get(&panel).is_some_and(|surface| {
                        surface.mapping.is_some_and(|(owner, _)| owner == window)
                    })
                }) {
                    if let Some(panel) = self.owned_input.pointer_panel {
                        self.cancel_owned_pointer(panel)?;
                    }
                }
            }
            WindowEvent::Focused(false)
            | WindowEvent::CloseRequested
            | WindowEvent::Destroyed
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. } => {
                if self.owned.values().any(|surface| {
                    surface.scene_display().is_some()
                        && surface.mapping.is_some_and(|(owner, _)| owner == window)
                }) {
                    self.scene_presenter.hide();
                }
                if self
                    .owned_input
                    .keyboard
                    .is_some_and(|(_, owner)| owner == window)
                {
                    self.cancel_owned_focus()?;
                    self.owned_input.modifiers = ModifiersState::empty();
                }
                if self.owned_input.pointer_panel.is_some_and(|panel| {
                    self.owned.get(&panel).is_some_and(|surface| {
                        surface.mapping.is_some_and(|(owner, _)| owner == window)
                    })
                }) {
                    self.owned_input.pointer_panel = None;
                    self.owned_input.dragging = false;
                }
                for (panel, surface) in self.owned.iter_mut().filter(|(_, surface)| {
                    surface.mapping.is_some_and(|(owner, _)| owner == window)
                }) {
                    self.frontend
                        .cancel_owned_panel_input(*panel)
                        .map_err(imgui_error)?;
                    surface.interaction = UiInteractionSession::default();
                    surface.invalidate();
                    surface.hide_scene();
                }
            }
            _ => {}
        }
        if let Some(consumed) = self.handle_owned_scene_event(window, event, application)? {
            return Ok(consumed);
        }
        if let WindowEvent::MouseWheel { delta, phase, .. } = event
            && let Some(panel) = self
                .owned
                .keys()
                .copied()
                .find(|panel| self.owned_cursor_point(*panel, window, true).is_some())
        {
            if !application.ordinary_input_locked() && *phase != TouchPhase::Cancelled {
                let surface = &self.owned[&panel];
                if let (Some(frame), Some((_, mapping))) = (&surface.presented, surface.mapping) {
                    let amount = match delta {
                        MouseScrollDelta::LineDelta(_, vertical) => -*vertical * 48.0,
                        MouseScrollDelta::PixelDelta(position) => {
                            -position.y as f32 / mapping.pixels_per_point
                                * frame.environment().logical_size().height()
                                / (mapping.rect.max[1] - mapping.rect.min[1])
                        }
                    };
                    if self
                        .frontend
                        .scroll_owned_panel(panel, frame, amount)
                        .unwrap_or(false)
                    {
                        self.cancel_owned_pointer(panel)?;
                    }
                }
            }
            return Ok(true);
        }
        if matches!(event, WindowEvent::CursorMoved { .. }) {
            let hovered = self
                .owned
                .keys()
                .copied()
                .filter_map(|panel| {
                    self.owned_cursor_point(panel, window, true)
                        .map(|point| (panel, point))
                })
                .collect::<Vec<_>>();
            for (panel, point) in hovered {
                if let Some((frame, outcome)) = self
                    .owned
                    .get_mut(&panel)
                    .expect("Inspector exists")
                    .dispatch(UiInputEvent::PointerMoved {
                        seat: nara_ui_kernel::UiSeatId::PRIMARY,
                        pointer: nara_ui_kernel::UiPointerId::PRIMARY,
                        position: point,
                    })?
                    && let Some(batch) = outcome.consumed()
                    && let Some(model) = application.current_frame()
                {
                    let admitted = self.owned[&panel]
                        .scene_point_is_visible(point)
                        .then_some(point);
                    self.frontend
                        .set_owned_scene_pointer(panel, &frame, admitted);
                    let clipboard = self.clipboard.clone();
                    let _ = self.frontend.reduce_owned_input(
                        panel,
                        &frame,
                        batch,
                        &model,
                        application.current_ui_authoring_frame().as_deref(),
                        &mut |text| clipboard.write(text),
                    );
                }
            }
            if self.owned_input.dragging
                && let Some(focus) = self
                    .owned_input
                    .focus
                    .as_ref()
                    .filter(|focus| focus.window == window)
                && let Some(point) = self.owned_cursor_point(focus.panel, window, false)
                && let Some(field) = focus
                    .frame
                    .layouts()
                    .iter()
                    .find(|layout| layout.key() == Some(&focus.field))
            {
                let local = nara_ui_kernel::UiPoint::new(
                    point.x() - field.rect().origin().x(),
                    point.y() - field.rect().origin().y(),
                );
                let _ = self.frontend.owned_point(
                    focus.panel,
                    &focus.frame,
                    &focus.field,
                    focus.token,
                    local,
                    true,
                );
            }
        }
        if let WindowEvent::MouseInput {
            button: MouseButton::Left,
            state,
            ..
        } = event
        {
            let mut pointer_consumed = false;
            let candidate = if *state == ElementState::Released {
                self.owned_input.pointer_panel.and_then(|panel| {
                    self.owned_cursor_point(panel, window, false)
                        .map(|point| (panel, point))
                })
            } else {
                self.owned.keys().find_map(|panel| {
                    self.owned_cursor_point(*panel, window, true)
                        .map(|point| (*panel, point))
                })
            };
            if let Some((panel, point)) = candidate {
                if *state == ElementState::Pressed {
                    self.owned_input.pointer_panel = Some(panel);
                }
                let event = UiInputEvent::PointerButton {
                    seat: nara_ui_kernel::UiSeatId::PRIMARY,
                    pointer: nara_ui_kernel::UiPointerId::PRIMARY,
                    button: nara_ui_kernel::UiPointerButton::Primary,
                    phase: if *state == ElementState::Pressed {
                        nara_ui_kernel::UiButtonPhase::Pressed
                    } else {
                        nara_ui_kernel::UiButtonPhase::Released
                    },
                    position: Some(point),
                };
                if let Some((frame, outcome)) = self
                    .owned
                    .get_mut(&panel)
                    .expect("Inspector surface exists")
                    .dispatch(event)?
                    && let Some(batch) = outcome.consumed()
                    && let Some(model) = application.current_frame()
                {
                    pointer_consumed = true;
                    let admitted = self.owned[&panel]
                        .scene_point_is_visible(point)
                        .then_some(point);
                    self.frontend
                        .set_owned_scene_pointer(panel, &frame, admitted);
                    let clipboard = self.clipboard.clone();
                    let _ = self.frontend.reduce_owned_input(
                        panel,
                        &frame,
                        batch,
                        &model,
                        application.current_ui_authoring_frame().as_deref(),
                        &mut |text| clipboard.write(text),
                    );
                    if *state == ElementState::Pressed
                        && let Some(hit) = frame.hit_test(point)
                        && self.frontend.owned_field_at(panel, &frame, hit.key())
                    {
                        self.sync_owned_keyboard_focus(panel, window)?;
                        let field = hit.key().clone();
                        let local = nara_ui_kernel::UiPoint::new(
                            point.x() - hit.rect().origin().x(),
                            point.y() - hit.rect().origin().y(),
                        );
                        if let Some(focus) = self
                            .owned_input
                            .focus
                            .as_ref()
                            .filter(|focus| focus.field == field)
                        {
                            let _ = self.frontend.owned_point(
                                panel,
                                &frame,
                                &field,
                                focus.token,
                                local,
                                self.owned_input.modifiers.shift_key(),
                            );
                            self.owned_input.dragging = true;
                        }
                    } else if *state == ElementState::Pressed {
                        self.sync_owned_keyboard_focus(panel, window)?;
                    }
                }
            } else if *state == ElementState::Pressed {
                self.cancel_owned_focus()?;
            } else if let Some(panel) = self.owned_input.pointer_panel {
                if let Some(surface) = self.owned.get_mut(&panel) {
                    surface.interaction = UiInteractionSession::default();
                }
                self.frontend
                    .cancel_owned_panel_input(panel)
                    .map_err(imgui_error)?;
                if self
                    .owned_input
                    .keyboard
                    .is_some_and(|(owner, _)| owner == panel)
                {
                    self.cancel_owned_focus()?;
                }
            }
            if *state == ElementState::Released {
                self.owned_input.dragging = false;
                self.owned_input.pointer_panel = None;
            }
            if pointer_consumed {
                return Ok(true);
            }
        }
        if let WindowEvent::KeyboardInput { event, .. } = event {
            return self.owned_key(
                window,
                event.logical_key.as_ref(),
                event.text.as_deref(),
                event.state,
                event.repeat,
                application,
            );
        }
        let Some(focus) = self
            .owned_input
            .focus
            .as_ref()
            .filter(|focus| focus.window == window)
        else {
            return Ok(false);
        };
        if application.ordinary_input_locked()
            || self
                .owned
                .get(&focus.panel)
                .is_none_or(|surface| surface.route.is_none())
            || self
                .frontend
                .owned_field_token(focus.panel, &focus.frame, &focus.field)
                != Some(focus.token)
        {
            let was_text_event = matches!(
                event,
                WindowEvent::Ime(_) | WindowEvent::KeyboardInput { .. }
            );
            self.cancel_owned_focus()?;
            return Ok(was_text_event);
        }
        let action = match event {
            WindowEvent::Ime(Ime::Enabled) => {
                self.owned_input.ime_ready = true;
                None
            }
            WindowEvent::Ime(Ime::Disabled) => {
                self.owned_input.ime_ready = false;
                Some(nara_text::TextEdit::CancelComposition)
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) if self.owned_input.ime_ready => {
                Some(nara_text::TextEdit::Preedit {
                    text: text.clone(),
                    cursor: *cursor,
                })
            }
            WindowEvent::Ime(Ime::Commit(text)) if self.owned_input.ime_ready => {
                Some(nara_text::TextEdit::Commit(text.clone()))
            }
            _ => None,
        };
        if let Some(action) = action {
            let focus = focus.clone();
            self.edit_owned_text(&focus, &action);
        }
        self.update_owned_ime_area()?;
        Ok(matches!(
            event,
            WindowEvent::Ime(_) | WindowEvent::KeyboardInput { .. }
        ))
    }

    fn cancel_owned_pointer(&mut self, panel: EditorPanelInstanceId) -> Result<(), EditorRunError> {
        if self.owned_input.pointer_panel != Some(panel) {
            return Ok(());
        }
        self.owned_input.pointer_panel = None;
        self.owned_input.dragging = false;
        if let Some(surface) = self.owned.get_mut(&panel) {
            if let Some((frame, outcome)) = surface.dispatch(UiInputEvent::PointerCancelled {
                seat: nara_ui_kernel::UiSeatId::PRIMARY,
                pointer: nara_ui_kernel::UiPointerId::PRIMARY,
            })? && let Some(batch) = outcome.consumed()
            {
                self.frontend
                    .reconcile_owned_input(panel, &frame, batch)
                    .map_err(imgui_error)?;
            } else {
                surface.interaction = UiInteractionSession::default();
                self.frontend
                    .cancel_owned_panel_input(panel)
                    .map_err(imgui_error)?;
            }
        }
        Ok(())
    }

    fn owned_cursor_point(
        &self,
        panel: EditorPanelInstanceId,
        window: WindowId,
        require_ownership: bool,
    ) -> Option<nara_ui_kernel::UiPoint> {
        let (cursor_window, cursor) = self.owned_input.cursor?;
        let surface = self.owned.get(&panel)?;
        let (owner, mapping) = surface.mapping?;
        if owner != window || cursor_window != window || surface.route.is_none() {
            return None;
        }
        let global = [
            cursor.x as f32 / mapping.pixels_per_point + mapping.viewport_origin[0],
            cursor.y as f32 / mapping.pixels_per_point + mapping.viewport_origin[1],
        ];
        let owns_pointer = mapping.owns_native_pointer(&self.context, global);
        if require_ownership && !owns_pointer {
            return None;
        }
        if !require_ownership
            && !owns_pointer
            && mapping.rect.contains([
                global[0] - mapping.viewport_origin[0],
                global[1] - mapping.viewport_origin[1],
            ])
        {
            return Some(nara_ui_kernel::UiPoint::new(-1.0, -1.0));
        }
        let logical = surface.presented.as_ref()?.environment().logical_size();
        Some(nara_ui_kernel::UiPoint::new(
            (global[0] - mapping.viewport_origin[0] - mapping.rect.min[0])
                / (mapping.rect.max[0] - mapping.rect.min[0])
                * logical.width(),
            (global[1] - mapping.viewport_origin[1] - mapping.rect.min[1])
                / (mapping.rect.max[1] - mapping.rect.min[1])
                * logical.height(),
        ))
    }

    fn update_owned_ime_area(&self) -> Result<(), EditorRunError> {
        let Some(focus) = &self.owned_input.focus else {
            return Ok(());
        };
        let Some(surface) = self.owned.get(&focus.panel) else {
            return Ok(());
        };
        let Some((_, mapping)) = surface.mapping else {
            return Ok(());
        };
        let Some(area) =
            self.frontend
                .owned_ime_area(focus.panel, &focus.frame, &focus.field, focus.token)
        else {
            return Ok(());
        };
        let Some(field) = focus
            .frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&focus.field))
        else {
            return Ok(());
        };
        let logical = focus.frame.environment().logical_size();
        let scale_x = (mapping.rect.max[0] - mapping.rect.min[0]) / logical.width();
        let scale_y = (mapping.rect.max[1] - mapping.rect.min[1]) / logical.height();
        let position = winit::dpi::LogicalPosition::new(
            f64::from(
                mapping.rect.min[0] + (field.rect().origin().x() + area.origin().x()) * scale_x,
            ),
            f64::from(
                mapping.rect.min[1] + (field.rect().origin().y() + area.origin().y()) * scale_y,
            ),
        );
        let size = winit::dpi::LogicalSize::new(
            f64::from(area.size().width() * scale_x),
            f64::from(area.size().height() * scale_y),
        );
        if let Some(snapshot) = self
            .platform
            .window_snapshot(focus.window)
            .map_err(imgui_error)?
        {
            self.platform
                .with_window(snapshot.target.instance_id, |window| {
                    window.set_ime_cursor_area(position, size)
                })
                .map_err(imgui_error)?;
        }
        Ok(())
    }

    pub(super) fn prepare_owned_panels(
        &mut self,
        window: RenderWindowPacket,
    ) -> Result<(), EditorRunError> {
        let retired = self
            .owned
            .keys()
            .filter(|key| !self.owned_pending.contains_key(key))
            .copied()
            .collect::<Vec<_>>();
        for panel in retired {
            if self
                .owned_input
                .scene_drag
                .as_ref()
                .is_some_and(|drag| drag.panel == panel)
            {
                self.owned_input.scene_drag = None;
            }
            if self
                .owned_input
                .keyboard
                .is_some_and(|(owner, _)| owner == panel)
            {
                self.cancel_owned_focus()?;
            }
            if let Some(mut surface) = self.owned.remove(&panel) {
                surface.retire(&mut self.gpu, &mut self.renderer)?;
            }
        }
        for (panel, candidate) in std::mem::take(&mut self.owned_pending) {
            if self
                .owned
                .get(&panel)
                .and_then(OwnedPanelSurface::displayed_frame)
                .is_some_and(|old| {
                    old.environment().surface_id() != candidate.frame.environment().surface_id()
                })
            {
                let mut surface = self
                    .owned
                    .remove(&panel)
                    .expect("replaced Inspector is retained");
                surface.retire(&mut self.gpu, &mut self.renderer)?;
            }
            if !self.owned.contains_key(&panel) {
                self.owned
                    .insert(panel, OwnedPanelSurface::new(self.owned_fonts.clone())?);
            }
            let surface = self
                .owned
                .get_mut(&panel)
                .expect("Inspector surface was created");
            surface.target.release_obsolete(&mut self.renderer)?;
            surface.prepare(
                candidate,
                &mut self.gpu,
                &mut self.renderer,
                window,
                self.frame_index,
            )?;
        }
        let images = self
            .owned
            .iter()
            .filter_map(|(panel, surface)| {
                Some((
                    *panel,
                    ImguiOwnedImage {
                        frame: Arc::clone(surface.displayed_frame()?),
                        image: surface.image()?,
                        scene: surface.scene_image(),
                    },
                ))
            })
            .collect();
        self.frontend
            .set_owned_images(self.owned_fonts.clone(), images);
        Ok(())
    }

    pub(super) fn observe_owned_panels(
        &mut self,
        output: &mut ImguiFrontendOutput,
        primary: &nara::render_wgpu::WgpuWindowFrameOutcome,
        secondary: &[nara::render_wgpu::WgpuWindowFrameOutcome],
        primary_viewport: dear_imgui_rs::Id,
        secondary_windows: &[(dear_imgui_rs::Id, WindowId, nara::window::WindowId)],
    ) -> Result<bool, EditorRunError> {
        self.owned_pending = std::mem::take(&mut output.owned_candidates)
            .into_iter()
            .map(|(panel, frame)| {
                let scene =
                    self.frontend
                        .owned_scene_placement(panel, &frame)
                        .and_then(|placement| {
                            scene::OwnedSceneSource::capture(placement, self.scene.display.as_ref())
                        });
                (panel, OwnedPanelCandidate { frame, scene })
            })
            .collect();
        let mut cancel_focus = false;
        for (panel, surface) in &mut self.owned {
            let mapping = output
                .owned_displays
                .iter()
                .find(|(id, _)| id == panel)
                .and_then(|(_, mapping)| {
                    let (native, window) = super::input::native_window(
                        mapping.viewport,
                        self.window.id(),
                        primary_viewport,
                        secondary_windows,
                    )?;
                    Some((native, window, *mapping))
                });
            let Some((native, window, mapping)) = mapping else {
                surface.invalidate();
                surface.hide_scene();
                surface.mapping = None;
                self.frontend
                    .cancel_owned_panel_input(*panel)
                    .map_err(imgui_error)?;
                if self.owned_input.pointer_panel == Some(*panel) {
                    self.owned_input.pointer_panel = None;
                    self.owned_input.dragging = false;
                }
                continue;
            };
            let invalidated_keys = match (surface.presented.as_ref(), surface.pending.as_ref()) {
                (Some(previous), Some(candidate)) => self
                    .frontend
                    .owned_invalidated_controls(*panel, previous, candidate),
                _ => Vec::new(),
            };
            if let Some((frame, dispatch)) = surface.promote(
                primary
                    .composites()
                    .iter()
                    .chain(secondary.iter().flat_map(|outcome| outcome.composites()))
                    .copied()
                    .filter(|receipt| {
                        receipt.window().window_id() == window
                            && Some(receipt.occurrence()) == mapping.occurrence
                    }),
                self.frame_index,
                &invalidated_keys,
            )? {
                self.frontend
                    .reconcile_owned_input(*panel, &frame, &dispatch)
                    .map_err(imgui_error)?;
            }
            let mapping_changed = surface.mapping.is_some_and(|(old_window, old)| {
                old_window != native
                    || old.rect != mapping.rect
                    || old.viewport != mapping.viewport
                    || old.window != mapping.window
                    || old.viewport_origin != mapping.viewport_origin
                    || old.pixels_per_point != mapping.pixels_per_point
            });
            surface.observe_scene(
                output,
                native,
                window,
                mapping,
                primary
                    .composites()
                    .iter()
                    .chain(secondary.iter().flat_map(|outcome| outcome.composites()))
                    .copied(),
            );
            if surface.route.is_none() {
                self.frontend
                    .cancel_owned_panel_input(*panel)
                    .map_err(imgui_error)?;
            } else if mapping_changed
                && let Some((frame, outcome)) =
                    surface.dispatch(UiInputEvent::PointerCancelled {
                        seat: nara_ui_kernel::UiSeatId::PRIMARY,
                        pointer: nara_ui_kernel::UiPointerId::PRIMARY,
                    })?
                && let Some(batch) = outcome.consumed()
            {
                self.frontend
                    .reconcile_owned_input(*panel, &frame, batch)
                    .map_err(imgui_error)?;
            }
            if (mapping_changed || surface.route.is_none())
                && self.owned_input.pointer_panel == Some(*panel)
            {
                self.owned_input.pointer_panel = None;
                self.owned_input.dragging = false;
            }
            surface.mapping = Some((native, mapping));
            if let Some(focus) = self
                .owned_input
                .focus
                .as_mut()
                .filter(|focus| focus.panel == *panel)
                && let Some(frame) = surface.presented.as_ref()
            {
                if native == focus.window
                    && self.frontend.owned_field_token(*panel, frame, &focus.field)
                        == Some(focus.token)
                {
                    focus.frame = Arc::clone(frame);
                } else {
                    cancel_focus = true;
                }
            }
        }
        cancel_focus |= self.owned_input.focus.as_ref().is_some_and(|focus| {
            self.owned
                .get(&focus.panel)
                .is_none_or(|surface| surface.route.is_none())
        });
        cancel_focus |= self.owned_input.keyboard.is_some_and(|(panel, window)| {
            self.owned.get(&panel).is_none_or(|surface| {
                surface.route.is_none() || surface.mapping.is_none_or(|(owner, _)| owner != window)
            })
        });
        if cancel_focus {
            self.cancel_owned_focus()?;
        }
        self.update_owned_ime_area()?;
        Ok(self.owned_pending.iter().any(|(panel, candidate)| {
            self.owned
                .get(panel)
                .and_then(OwnedPanelSurface::displayed_frame)
                .is_none_or(|current| !Arc::ptr_eq(current, &candidate.frame))
                || self
                    .owned
                    .get(panel)
                    .is_some_and(OwnedPanelSurface::scene_needs_rebind)
        }) || self
            .owned
            .keys()
            .any(|panel| !self.owned_pending.contains_key(panel)))
    }
}
