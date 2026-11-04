use super::*;
use crate::frontend::imgui::SceneViewPlacement;
use nara::render::{DisplayMapping2d, DisplayRect};
use nara::render_wgpu::WgpuImageDisplay;

pub(in crate::platform::imgui) struct OwnedPanelCandidate {
    pub(super) frame: Arc<UiFrame>,
    pub(super) scene: Option<OwnedSceneSource>,
}

#[derive(Clone, PartialEq)]
pub(super) struct OwnedSceneSource {
    display: WgpuImageDisplay,
    placement: SceneViewPlacement,
}

impl OwnedSceneSource {
    pub(super) fn capture(
        placement: SceneViewPlacement,
        source: Option<&WgpuImageDisplay>,
    ) -> Option<Self> {
        let image = placement.image?;
        let source = source?;
        (source.occurrence().get() == image.occurrence && source.output().extent() == image.extent)
            .then(|| Self {
                display: source.clone(),
                placement,
            })
    }
}

#[derive(Clone, Copy)]
struct SceneDestination {
    native: WindowId,
    window: nara::window::WindowId,
    panel: ImguiImageInteraction,
    mapping: DisplayMapping2d,
    clip: DisplayRect,
}

impl PartialEq for SceneDestination {
    fn eq(&self, other: &Self) -> bool {
        self.native == other.native
            && self.window == other.window
            && self.panel.rect == other.panel.rect
            && self.panel.viewport == other.panel.viewport
            && self.panel.viewport_origin == other.panel.viewport_origin
            && self.panel.pixels_per_point == other.panel.pixels_per_point
            && self.panel.window == other.panel.window
            && self.mapping == other.mapping
            && self.clip == other.clip
    }
}

pub(super) struct OwnedSceneSample {
    source: OwnedSceneSource,
    target: ImguiTarget,
    destination: Option<SceneDestination>,
    confirmed: bool,
    rebind: bool,
}

pub(super) struct OwnedSceneDrag {
    pub(super) panel: EditorPanelInstanceId,
    window: WindowId,
    point: nara_ui_kernel::UiPoint,
    source: u64,
    rect: DisplayRect,
}

impl OwnedPanelSurface {
    pub(in crate::platform::imgui) fn scene_display(&self) -> Option<&WgpuImageDisplay> {
        self.scene.as_ref()?.target.display.as_ref()
    }

    pub(super) fn scene_image(&self) -> Option<ImguiViewImage> {
        self.scene.as_ref()?.target.image()
    }

    pub(super) fn scene_needs_rebind(&self) -> bool {
        self.scene.as_ref().is_some_and(|scene| scene.rebind)
    }

    pub(super) fn scene_point_is_visible(&self, point: nara_ui_kernel::UiPoint) -> bool {
        let Some(scene) = self
            .scene
            .as_ref()
            .filter(|scene| scene.confirmed && !scene.rebind)
        else {
            return false;
        };
        let Some(destination) = scene.destination else {
            return false;
        };
        let Some(frame) = self.presented.as_ref() else {
            return false;
        };
        let logical = frame.environment().logical_size();
        let panel = destination.panel;
        let global = [
            panel.viewport_origin[0]
                + panel.rect.min[0]
                + point.x() / logical.width() * (panel.rect.max[0] - panel.rect.min[0]),
            panel.viewport_origin[1]
                + panel.rect.min[1]
                + point.y() / logical.height() * (panel.rect.max[1] - panel.rect.min[1]),
        ];
        destination.clip.contains(global) && destination.mapping.content_rect().contains(global)
    }

    pub(super) fn prepare_scene(
        &mut self,
        source: Option<OwnedSceneSource>,
        gpu: &mut WgpuRenderHost,
        renderer: &mut ImguiRenderer,
    ) -> Result<(), EditorRunError> {
        let Some(source) = source else {
            if let Some(scene) = self.scene.as_mut() {
                scene.target.retire(gpu, renderer)?;
            }
            self.scene = None;
            return Ok(());
        };
        if let Some(scene) = self.scene.as_mut() {
            scene.target.release_obsolete(renderer)?;
            if scene.source == source && !scene.rebind {
                return Ok(());
            }
        }
        let display = gpu
            .display_image(source.display.output().clone())
            .map_err(platform_gpu_error)?;
        if let Some(scene) = self.scene.as_mut() {
            scene.target.select_display(renderer, display)?;
            scene.source = source;
            scene.destination = None;
            scene.confirmed = false;
            scene.rebind = false;
        } else {
            let mut target = ImguiTarget::default();
            target.select_display(renderer, display)?;
            self.scene = Some(OwnedSceneSample {
                source,
                target,
                destination: None,
                confirmed: false,
                rebind: false,
            });
        }
        Ok(())
    }

    pub(in crate::platform::imgui) fn hide_scene(&mut self) {
        if let Some(scene) = self.scene.as_mut() {
            scene.confirmed = false;
            scene.rebind |= scene.destination.is_some();
        }
    }

    pub(super) fn observe_scene(
        &mut self,
        output: &ImguiFrontendOutput,
        native: WindowId,
        window: nara::window::WindowId,
        panel: ImguiImageInteraction,
        receipts: impl IntoIterator<Item = WgpuImageCompositeReceipt>,
    ) {
        let Some(scene) = self.scene.as_mut() else {
            return;
        };
        let destination = output
            .scene_mapping
            .zip(output.scene_clip)
            .map(|(mapping, clip)| SceneDestination {
                native,
                window,
                panel,
                mapping,
                clip,
            });
        let matches = self.route.is_some()
            && scene.target.display.as_ref().is_some_and(|display| {
                Some(display.occurrence()) == output.authoring_occurrence
                    && receipts.into_iter().any(|receipt| {
                        receipt.image() == display.receipt()
                            && receipt.occurrence() == display.occurrence()
                            && receipt.window().window_id() == window
                    })
            });
        let Some(destination) = destination.filter(|_| matches) else {
            scene.confirmed = false;
            scene.rebind |= scene.destination.is_some();
            return;
        };
        if scene.destination.is_some_and(|old| old != destination) {
            scene.confirmed = false;
            scene.rebind = true;
            return;
        }
        scene.destination = Some(destination);
        scene.confirmed = !scene.rebind;
    }

    pub(in crate::platform::imgui) fn confirmed_scene(
        &self,
    ) -> Option<(&WgpuImageDisplay, dear_imgui_wgpu::ExternalTextureId)> {
        let scene = self
            .scene
            .as_ref()
            .filter(|scene| scene.confirmed && !scene.rebind)?;
        Some((scene.target.display.as_ref()?, scene.target.texture?))
    }

    pub(super) fn retire(
        &mut self,
        gpu: &mut WgpuRenderHost,
        renderer: &mut ImguiRenderer,
    ) -> Result<(), EditorRunError> {
        if let Some(scene) = self.scene.as_mut() {
            scene.target.retire(gpu, renderer)?;
        }
        self.scene = None;
        self.target.retire(gpu, renderer)
    }
}

impl NativeImgui {
    pub(in crate::platform::imgui) fn cancel_owned_scene_drag(&mut self) {
        self.owned_input.scene_drag = None;
    }

    #[cfg(test)]
    pub(in crate::platform::imgui) fn owned_scene_snapshot_for_test(
        &self,
    ) -> Option<(
        EditorPanelInstanceId,
        Arc<UiFrame>,
        WgpuImageDisplay,
        DisplayMapping2d,
        DisplayRect,
    )> {
        self.owned.iter().find_map(|(panel, surface)| {
            surface.route?;
            let scene = surface
                .scene
                .as_ref()
                .filter(|scene| scene.confirmed && !scene.rebind)?;
            let display = scene.target.display.as_ref()?;
            if !self
                .scene_presenter
                .matches_occurrence(Some(display.occurrence()))
            {
                return None;
            }
            let destination = scene.destination?;
            Some((
                *panel,
                Arc::clone(surface.presented.as_ref()?),
                display.clone(),
                destination.mapping,
                destination.clip,
            ))
        })
    }

    pub(super) fn handle_owned_scene_event(
        &mut self,
        window: WindowId,
        event: &WindowEvent,
        application: &crate::EditorApplication,
    ) -> Result<Option<bool>, EditorRunError> {
        if matches!(
            event,
            WindowEvent::CursorLeft { .. }
                | WindowEvent::Focused(false)
                | WindowEvent::CloseRequested
                | WindowEvent::Destroyed
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
        ) && self
            .owned_input
            .scene_drag
            .as_ref()
            .is_some_and(|drag| drag.window == window)
        {
            self.owned_input.scene_drag = None;
        }
        if application.ordinary_input_locked() {
            self.owned_input.scene_drag = None;
            return Ok(None);
        }
        if matches!(event, WindowEvent::CursorMoved { .. })
            && let Some(mut drag) = self.owned_input.scene_drag.take()
        {
            let next = self.owned_cursor_point(drag.panel, window, true).zip(
                self.owned
                    .get(&drag.panel)
                    .and_then(|surface| surface.presented.as_ref().cloned()),
            );
            if let Some((point, frame)) = next
                && let Some(placement) = self.frontend.owned_scene_placement(drag.panel, &frame)
                && placement
                    .image
                    .is_some_and(|image| image.occurrence == drag.source)
                && placement
                    .mapping
                    .is_some_and(|mapping| mapping.panel_rect() == drag.rect)
                && placement.clip.contains([point.x(), point.y()])
                && self.owned[&drag.panel].scene_point_is_visible(point)
                && drag.window == window
            {
                let delta = [point.x() - drag.point.x(), point.y() - drag.point.y()];
                self.frontend
                    .set_owned_scene_pointer(drag.panel, &frame, Some(point));
                self.frontend
                    .pan_owned_scene(drag.panel, &frame, delta)
                    .map_err(imgui_error)?;
                drag.point = point;
                self.owned_input.scene_drag = Some(drag);
            }
            return Ok(Some(true));
        }
        if let WindowEvent::MouseInput {
            button: MouseButton::Middle,
            state: ElementState::Released,
            ..
        } = event
        {
            return Ok(self.owned_input.scene_drag.take().map(|_| true));
        }
        if !matches!(
            event,
            WindowEvent::MouseWheel { .. }
                | WindowEvent::MouseInput {
                    button: MouseButton::Middle,
                    state: ElementState::Pressed,
                    ..
                }
        ) {
            return Ok(None);
        }
        let hovered = self.owned.iter().find_map(|(panel, surface)| {
            let point = self.owned_cursor_point(*panel, window, true)?;
            let frame = surface.presented.as_ref()?;
            let placement = self.frontend.owned_scene_placement(*panel, frame)?;
            Some((*panel, Arc::clone(frame), placement, point))
        });
        let Some((panel, frame, placement, point)) = hovered else {
            return Ok(None);
        };
        let Some(mapping) = placement.mapping.filter(|mapping| {
            mapping.content_rect().contains([point.x(), point.y()])
                && placement.clip.contains([point.x(), point.y()])
        }) else {
            return Ok(Some(true));
        };
        if !self.owned[&panel].scene_point_is_visible(point) {
            return Ok(Some(true));
        }
        match event {
            WindowEvent::MouseWheel { delta, phase, .. } if *phase != TouchPhase::Cancelled => {
                let wheel = match delta {
                    MouseScrollDelta::LineDelta(_, vertical) => *vertical,
                    MouseScrollDelta::PixelDelta(point) => {
                        point.y as f32
                            / self.owned[&panel]
                                .mapping
                                .map_or(1.0, |(_, mapping)| mapping.pixels_per_point)
                            / 48.0
                    }
                };
                if self
                    .frontend
                    .zoom_owned_scene(panel, &frame, point, wheel)
                    .map_err(imgui_error)?
                {
                    self.cancel_owned_pointer(panel)?;
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Middle,
                state: ElementState::Pressed,
                ..
            } => {
                if let Some(image) = placement.image {
                    self.frontend
                        .set_owned_scene_pointer(panel, &frame, Some(point));
                    self.cancel_owned_focus()?;
                    self.owned_input.scene_drag = Some(OwnedSceneDrag {
                        panel,
                        window,
                        point,
                        source: image.occurrence,
                        rect: mapping.panel_rect(),
                    });
                }
            }
            _ => {}
        }
        Ok(Some(true))
    }
}
