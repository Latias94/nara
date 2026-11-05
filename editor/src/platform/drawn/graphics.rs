use std::{collections::BTreeMap, sync::Arc};

use nara::{
    core::{Color, Mat3},
    ecs::Entity,
    render::{
        DisplayRect, Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration,
        RenderImage2d, RenderImagePacket, RenderTarget, RenderViewId, RenderWindowPacket,
        ViewLoadOp, ViewMapping2d, ViewportRect, build_image_render_frame_packet,
    },
    window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider},
};
use nara_render_wgpu::{
    WgpuImageDisplay, WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuRenderHost,
    WgpuRenderProducer, WgpuStockCapture, WgpuStockPolicy, WgpuSubmissionPrefix,
    WgpuUiImageSubmission, WgpuUiSubmission, WgpuWindowFrameOutcome,
};
use nara_text::FontFace;
use nara_ui_kernel::{UiFrame, UiFrameJournal, UiSurfaceId};
use nara_ui_render::{
    UiPreparedImages, UiSurfacePresentationEvent, UiSurfacePresenter,
    advanced::UiCompletedBackendOutcome,
};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::{native::EditorPanelImageFrame, workspace::EditorPanelInstanceId};

#[cfg(all(test, target_os = "windows", not(feature = "imgui-reference")))]
mod capture_tests;

pub(super) struct Graphics {
    host: WgpuRenderHost,
    policy: WgpuPolicyInstallation<WgpuStockPolicy>,
    painter: ImageBlitter,
    fonts: Vec<FontFace>,
    panels: BTreeMap<EditorPanelInstanceId, PanelSurface>,
    retiring: Vec<WgpuSubmissionPrefix>,
    close: GraphicsCloseState,
    pub(super) frame: u64,
}

#[must_use]
pub(super) struct GraphicsCloseReport {
    pub(super) completed: bool,
    pub(super) errors: Vec<String>,
}

#[derive(Default)]
struct GraphicsCloseState {
    started: bool,
    completed: bool,
    reported: Vec<String>,
    truncated: bool,
    #[cfg(test)]
    held_polls: usize,
    #[cfg(test)]
    fail_next_poll: bool,
}

impl GraphicsCloseState {
    fn record(&mut self, error: String, errors: &mut Vec<String>) {
        const ERROR_LIMIT: usize = 16;
        if self.reported.contains(&error) {
            return;
        }
        if self.reported.len() < ERROR_LIMIT {
            self.reported.push(error.clone());
            errors.push(error);
        } else if !self.truncated {
            self.truncated = true;
            errors.push(format!(
                "Further distinct Editor graphics retirement errors were omitted after {ERROR_LIMIT} observations"
            ));
        }
    }
}

struct PanelSurface {
    surface: UiSurfaceId,
    slot_generation: u64,
    producer: WgpuRenderProducer,
    target: RenderImagePacket,
    source: RenderFrameSourceGeneration,
    view: RenderViewId,
    presenter: UiSurfacePresenter,
    current: Option<(Arc<UiFrame>, WgpuImageDisplay)>,
    pending: Option<WgpuUiImageSubmission>,
    retry: Option<Arc<UiFrame>>,
    returned: Vec<UiCompletedBackendOutcome>,
}

impl Graphics {
    pub(super) fn new(window: &Arc<Window>, fonts: Vec<FontFace>) -> Result<Self, String> {
        let policy = WgpuPolicyDefinition::new(WgpuStockPolicy::new())
            .install()
            .map_err(|error| error.to_string())?;
        let mut host = WgpuRenderHost::new(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::clone(window)),
        )
        .map_err(|error| error.to_string())?;
        let painter = host
            .with_device(window_packet(window), |scope| {
                ImageBlitter::new(scope.device(), scope.surface_format(), scope.device_epoch())
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            host,
            policy,
            painter,
            fonts,
            panels: BTreeMap::new(),
            retiring: Vec::new(),
            close: GraphicsCloseState::default(),
            frame: 0,
        })
    }

    pub(super) fn begin(&mut self, window: &Window, frame: u64) -> Result<(), String> {
        self.check_open()?;
        self.poll_retired()?;
        if frame <= self.frame {
            return Err("Editor display frame identity must advance".to_owned());
        }
        let packet = window_packet(window);
        self.host
            .begin_window_frame(packet, frame)
            .map_err(|error| error.to_string())?;
        self.host
            .with_device(packet, |scope| {
                if self.painter.format != scope.surface_format()
                    || self.painter.device_epoch != scope.device_epoch()
                {
                    self.painter = ImageBlitter::new(
                        scope.device(),
                        scope.surface_format(),
                        scope.device_epoch(),
                    );
                }
            })
            .map_err(|error| error.to_string())?;
        self.frame = frame;
        Ok(())
    }

    pub(super) fn render(
        &mut self,
        slot: EditorPanelInstanceId,
        frame: Arc<UiFrame>,
    ) -> Result<(WgpuImageDisplay, Arc<UiFrame>), String> {
        self.check_open()?;
        if self.frame == 0 {
            return Err(
                "Begin the original Editor window frame before rendering panels".to_owned(),
            );
        }
        let environment = frame.environment();
        let physical = environment.content_transform().physical_surface_size();
        let extent = Extent2d::new(
            physical.width().ceil() as u32,
            physical.height().ceil() as u32,
        )
        .ok_or_else(|| "Editor panel physical target extent is invalid".to_owned())?;
        let replace = self.panels.get(&slot).is_some_and(|panel| {
            panel.surface != environment.surface_id()
                || panel.slot_generation != environment.slot_generation().get()
                || panel.target.extent() != extent
                || !self
                    .host
                    .image_target_matches(&panel.producer, panel.source, panel.target)
        });
        if replace {
            self.retire_panel(slot)?;
        }
        if !self.panels.contains_key(&slot) {
            let panel = self.create_panel(&frame, extent)?;
            self.panels.insert(slot, panel);
        }
        let panel = self
            .panels
            .get_mut(&slot)
            .ok_or_else(|| "Editor panel graphics lifetime is absent".to_owned())?;
        if let Some((current, display)) = &panel.current
            && (Arc::ptr_eq(current, &frame) || panel.pending.is_some())
            && display.output().is_current()
            && panel.retry.is_none()
        {
            return Ok((display.clone(), Arc::clone(current)));
        }
        if panel.pending.is_some() {
            return Err(format!(
                "Editor panel {slot:?} pending UI image has no current original output"
            ));
        }
        if !panel.returned.is_empty() {
            panel.settle(Vec::new())?;
        }
        let frame = panel.retry.as_ref().map_or(frame, Arc::clone);
        let environment = frame.environment();
        let viewport = ViewportRect::from_extent(extent)
            .ok_or_else(|| "Editor panel viewport is invalid".to_owned())?;
        let mapping = ViewMapping2d::orthographic(
            Mat3::IDENTITY,
            environment.logical_size().height(),
            viewport,
        )
        .ok_or_else(|| "Editor panel view mapping is invalid".to_owned())?;
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: panel.view,
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(panel.target.image()),
            viewport,
            mapping,
            order: 0,
            load: ViewLoadOp::Clear(Color::rgba(0.0, 0.0, 0.0, 0.0)),
        });
        let packet =
            build_image_render_frame_packet(panel.source, self.frame, &views, panel.target)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Editor panel has no render View".to_owned())?;
        let backend = if panel.retry.is_some() {
            panel.presenter.retry()
        } else {
            panel
                .presenter
                .prepare(Arc::clone(&frame), &UiPreparedImages::default())
        }
        .map_err(|error| error.to_string())?;
        let (packets, sidecar) = WgpuUiSubmission::capture(backend);
        let capture = match self.policy.capture(
            packet,
            Arc::new(WgpuStockCapture::new().with_ui(packets, None, None)),
        ) {
            Ok(capture) => capture,
            Err(error) => {
                let message = error.to_string();
                return match panel.settle(sidecar.content_rejected(message.clone())) {
                    Ok(_) => {
                        panel.retry = None;
                        Err(message)
                    }
                    Err(cleanup) => Err(format!("{message}; UI completion: {cleanup}")),
                };
            }
        };
        match self
            .host
            .submit_for_display(&panel.producer, &capture, sidecar)
        {
            Ok(submission) => {
                let display = submission.display().clone();
                panel.current = Some((Arc::clone(&frame), display.clone()));
                panel.pending = Some(submission);
                panel.retry = None;
                Ok((display, frame))
            }
            Err(failure) => {
                let (error, outcomes) = failure.complete();
                match panel.settle(outcomes) {
                    Ok(events) => {
                        panel.retry = events
                            .iter()
                            .any(|event| {
                                matches!(event, UiSurfacePresentationEvent::Deferred { .. })
                            })
                            .then_some(frame);
                        Err(error.to_string())
                    }
                    Err(cleanup) => Err(format!("{error}; UI completion: {cleanup}")),
                }
            }
        }
    }

    pub(super) fn present(
        &mut self,
        window: &Window,
        candidate: &EditorPanelImageFrame<UiFrame>,
    ) -> Result<WgpuWindowFrameOutcome, String> {
        self.check_open()?;
        let packet = window_packet(window);
        let geometry = candidate.geometry();
        if candidate.window_id() != packet.id
            || candidate.display_frame_index() != self.frame
            || geometry.physical_size
                != [
                    packet.resolution.physical_width,
                    packet.resolution.physical_height,
                ]
            || geometry.scale_factor != window.scale_factor()
        {
            return Err(
                "Editor image candidate does not match the current native window frame".to_owned(),
            );
        }
        let displays: Vec<_> = candidate
            .images()
            .iter()
            .map(|image| image.display().clone())
            .collect();
        for image in candidate.images() {
            let panel = self
                .panels
                .get(&image.token().instance())
                .ok_or_else(|| "Editor image candidate names a retired panel".to_owned())?;
            if panel.current.as_ref().is_none_or(|(frame, display)| {
                !Arc::ptr_eq(frame, image.snapshot()) || display != image.display()
            }) {
                return Err(
                    "Editor image candidate does not retain its original UI frame and display"
                        .to_owned(),
                );
            }
        }
        let placements: Vec<_> = candidate
            .images()
            .iter()
            .map(|image| {
                Placement::new(
                    image.mapping().content_rect(),
                    image.clip(),
                    geometry.physical_size,
                    geometry.scale_factor as f32,
                )
            })
            .collect::<Result<_, _>>()?;
        let painter = &self.painter;
        let groups: Vec<_> = displays
            .iter()
            .zip(&placements)
            .map(|(display, placement)| {
                self.host
                    .with_image(display.output(), |device, _, view| {
                        painter.bind(device, view, placement.uv)
                    })
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<_, _>>()?;
        self.host
            .present_window(
                packet,
                self.frame,
                &displays,
                |frame| {
                    if frame.surface_format() != painter.format {
                        return Err(nara_render_wgpu::WgpuRenderHostError::window_paint(
                            "Editor image blitter format differs from the original window surface",
                        ));
                    }
                    for (index, ((display, placement), group)) in
                        displays.iter().zip(&placements).zip(&groups).enumerate()
                    {
                        frame.encode(|_, _, encoder, target| {
                            painter.draw(
                                encoder,
                                target,
                                group,
                                placement,
                                if index == 0 {
                                    wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                                } else {
                                    wgpu::LoadOp::Load
                                },
                            );
                        });
                        frame.record_image_sample(display)?;
                    }
                    if displays.is_empty() {
                        frame.encode(|_, _, encoder, target| {
                            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("nara-drawn-editor-empty-window"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: target,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                ..Default::default()
                            });
                        });
                    }
                    Ok(())
                },
                || window.pre_present_notify(),
            )
            .map_err(|error| error.to_string())
    }

    pub(super) fn complete(
        &mut self,
        outcome: &WgpuWindowFrameOutcome,
    ) -> Result<Vec<(EditorPanelInstanceId, Vec<UiSurfacePresentationEvent>)>, String> {
        let mut completed = Vec::new();
        for (slot, panel) in &mut self.panels {
            let composite = panel.pending.as_ref().and_then(|pending| {
                outcome.composites().iter().copied().find(|composite| {
                    Some(composite.window()) == outcome.window()
                        && composite.window().window_id() == WindowId::PRIMARY
                        && composite.image() == pending.image()
                        && composite.occurrence() == pending.display().occurrence()
                })
            });
            if let Some(composite) = composite {
                let pending = panel
                    .pending
                    .take()
                    .ok_or_else(|| "Editor original UI image submission is absent".to_owned())?;
                let outcomes = match pending.presented(composite) {
                    Ok(outcomes) => outcomes,
                    Err(pending) => {
                        panel.pending = Some(pending);
                        return Err(format!(
                            "Editor panel {slot:?} refused a stale original image composite"
                        ));
                    }
                };
                completed.push((*slot, panel.settle(outcomes)?));
            } else if !panel.returned.is_empty() {
                completed.push((*slot, panel.settle(Vec::new())?));
            }
        }
        Ok(completed)
    }

    pub(super) fn journal(&self, slot: EditorPanelInstanceId) -> Option<&UiFrameJournal> {
        self.panels
            .get(&slot)
            .map(|panel| panel.presenter.journal())
    }

    pub(super) fn pending_frame(&self, slot: EditorPanelInstanceId) -> Option<&Arc<UiFrame>> {
        self.panels
            .get(&slot)
            .filter(|panel| panel.pending.is_some())
            .and_then(|panel| panel.current.as_ref().map(|(frame, _)| frame))
    }

    pub(super) fn has_pending(&self) -> bool {
        self.panels.values().any(|panel| panel.pending.is_some())
    }

    pub(super) fn retain(&mut self, slots: &[EditorPanelInstanceId]) -> Result<(), String> {
        self.check_open()?;
        let missing: Vec<_> = self
            .panels
            .keys()
            .copied()
            .filter(|slot| !slots.contains(slot))
            .collect();
        for slot in missing {
            self.retire_panel(slot)?;
        }
        self.poll_retired()
    }

    /// Polls original physical retirement once without consuming the owner or waiting.
    ///
    /// The caller must retain this Graphics until `completed` is true, including when errors
    /// are returned. Diagnostics contain only newly observed errors and must be retained by the
    /// caller for its final result. Completion remains monotonic and independent of diagnostics.
    pub(super) fn poll_close(&mut self) -> GraphicsCloseReport {
        let mut errors = Vec::new();
        if self.close.completed {
            return GraphicsCloseReport {
                completed: true,
                errors,
            };
        }
        if !self.close.started {
            self.close.started = true;
            self.policy.close();
        }
        #[cfg(test)]
        if std::mem::take(&mut self.close.fail_next_poll) {
            self.close.record(
                "Injected Editor graphics retirement poll failure".to_owned(),
                &mut errors,
            );
            return GraphicsCloseReport {
                completed: false,
                errors,
            };
        }
        #[cfg(test)]
        if self.close.held_polls != 0 {
            self.close.held_polls -= 1;
            return GraphicsCloseReport {
                completed: false,
                errors,
            };
        }
        let slots: Vec<_> = self.panels.keys().copied().collect();
        for slot in slots {
            if let Err(error) = self.retire_panel(slot) {
                self.close.record(
                    format!("Editor panel {slot:?} retirement: {error}"),
                    &mut errors,
                );
            }
        }
        let host_completed = if self.panels.is_empty() {
            match self.host.poll_retirement() {
                Ok(completed) => completed,
                Err(error) => {
                    self.close.record(
                        format!("Original Editor GPU Host retirement: {error}"),
                        &mut errors,
                    );
                    false
                }
            }
        } else {
            false
        };
        let mut index = 0;
        while index < self.retiring.len() {
            let prefix = &self.retiring[index];
            let result = if prefix.callback_abandoned() {
                prefix.recover_abandoned()
            } else {
                prefix.poll()
            };
            let completed = match result {
                Ok(completed) => completed,
                Err(error) => {
                    self.close.record(
                        format!("Original Editor producer retirement: {error}"),
                        &mut errors,
                    );
                    false
                }
            };
            if prefix.callback_abandoned() {
                self.close.record(
                    "Original Editor producer callback was abandoned".to_owned(),
                    &mut errors,
                );
            }
            if prefix.cleanup_fault_count() != 0 {
                self.close.record(
                    format!(
                        "Original Editor producer reported {} cleanup faults",
                        prefix.cleanup_fault_count()
                    ),
                    &mut errors,
                );
            }
            if completed {
                drop(self.retiring.swap_remove(index));
            } else {
                index += 1;
            }
        }
        if self.host.abandoned_submission_count() != 0 {
            self.close.record(
                format!(
                    "Original Editor GPU Host recovered {} abandoned submissions",
                    self.host.abandoned_submission_count()
                ),
                &mut errors,
            );
        }
        if self.host.cleanup_fault_count() != 0 {
            self.close.record(
                format!(
                    "Original Editor GPU Host reported {} cleanup faults",
                    self.host.cleanup_fault_count()
                ),
                &mut errors,
            );
        }
        self.close.completed = host_completed && self.retiring.is_empty();
        GraphicsCloseReport {
            completed: self.close.completed,
            errors,
        }
    }

    #[cfg(test)]
    pub(super) fn hold_close_for_test(&mut self, polls: usize) {
        self.close.held_polls = polls;
    }

    #[cfg(test)]
    pub(super) fn fail_close_poll_for_test(&mut self) {
        self.close.fail_next_poll = true;
    }

    fn check_open(&self) -> Result<(), String> {
        if self.close.started {
            Err("The original Editor graphics Host has closed rendering admission".to_owned())
        } else {
            Ok(())
        }
    }

    fn create_panel(&mut self, frame: &UiFrame, extent: Extent2d) -> Result<PanelSurface, String> {
        let environment = frame.environment();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().map_err(|error| error.to_string())?,
            extent,
        )
        .map_err(|error| error.to_string())?;
        let source = RenderFrameSourceGeneration::try_acquire()
            .ok_or_else(|| "Editor panel render source identity exhausted".to_owned())?;
        let view = RenderViewId::try_acquire()
            .ok_or_else(|| "Editor panel render View identity exhausted".to_owned())?;
        let presenter = UiSurfacePresenter::with_fonts(
            environment.surface_id(),
            environment.slot_generation().get(),
            RenderTarget::Image(target.image()),
            self.fonts.clone(),
            Default::default(),
            Default::default(),
        )
        .map_err(|error| error.to_string())?;
        let producer = self
            .host
            .create_producer()
            .map_err(|error| error.to_string())?;
        let activation = self
            .host
            .reserve_image_target(&producer, source, target)
            .and_then(|reservation| self.host.activate_image_target(&producer, reservation));
        if let Err(error) = activation {
            if let Some(prefix) = self.host.retire_producer(&producer) {
                self.retiring.push(prefix);
            }
            return Err(error.to_string());
        }
        Ok(PanelSurface {
            surface: environment.surface_id(),
            slot_generation: environment.slot_generation().get(),
            producer,
            target,
            source,
            view,
            presenter,
            current: None,
            pending: None,
            retry: None,
            returned: Vec::new(),
        })
    }

    fn retire_panel(&mut self, slot: EditorPanelInstanceId) -> Result<(), String> {
        let Some(panel) = self.panels.get_mut(&slot) else {
            return Ok(());
        };
        let mut errors = Vec::new();
        if let Err(error) = panel.presenter.retire(self.frame.saturating_add(1)) {
            errors.push(error.to_string());
        }
        let outcomes = panel.pending.take().map_or_else(Vec::new, |pending| {
            pending.deferred("Original Editor panel retired before image composite".to_owned())
        });
        if let Err(error) = panel.settle(outcomes) {
            errors.push(error);
        }
        if !panel.presenter.is_quiescent() {
            errors.push(
                "Retired Editor panel still owns original UI completion obligations".to_owned(),
            );
        } else {
            if let Some(prefix) = self.host.retire_producer(&panel.producer) {
                self.retiring.push(prefix);
            }
            self.panels.remove(&slot);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn poll_retired(&mut self) -> Result<(), String> {
        let mut index = 0;
        while index < self.retiring.len() {
            let prefix = &self.retiring[index];
            if !prefix.poll().map_err(|error| error.to_string())? {
                index += 1;
                continue;
            }
            if prefix.callback_abandoned() || prefix.cleanup_fault_count() != 0 {
                return Err(format!(
                    "Original Editor producer retirement: abandoned={}, cleanup_faults={}",
                    prefix.callback_abandoned(),
                    prefix.cleanup_fault_count()
                ));
            }
            drop(self.retiring.swap_remove(index));
        }
        Ok(())
    }
}

impl PanelSurface {
    fn settle(
        &mut self,
        outcomes: Vec<UiCompletedBackendOutcome>,
    ) -> Result<Vec<UiSurfacePresentationEvent>, String> {
        self.returned.extend(outcomes);
        while let Some(outcome) = self.returned.pop() {
            if let Err(outcome) = self.presenter.return_outcome(outcome) {
                self.returned.push(outcome);
                return Err(
                    "Original UI outcome was refused by its Editor surface presenter".to_owned(),
                );
            }
        }
        self.presenter.drain().map_err(|error| error.to_string())
    }
}

fn window_packet(window: &Window) -> RenderWindowPacket {
    let size = window.inner_size();
    RenderWindowPacket {
        id: WindowId::PRIMARY,
        resolution: WindowResolution::new(size.width, size.height)
            .with_scale_factor(window.scale_factor()),
        present_mode: PresentMode::AutoNoVsync,
    }
}

struct Placement {
    viewport: [f32; 4],
    clip: [u32; 4],
    uv: [f32; 4],
}

impl Placement {
    fn new(
        content: DisplayRect,
        clip: DisplayRect,
        size: [u32; 2],
        scale: f32,
    ) -> Result<Self, String> {
        if !content.is_valid() || !clip.is_valid() || !scale.is_finite() || scale <= 0.0 {
            return Err("Editor image candidate has invalid native placement".to_owned());
        }
        let min = content.min.map(|value| value * scale);
        let max = content.max.map(|value| value * scale);
        let bounds = size.map(|value| value as f32);
        let start = [min[0].max(0.0), min[1].max(0.0)];
        let end = [max[0].min(bounds[0]), max[1].min(bounds[1])];
        let clip_min = [
            (clip.min[0] * scale).floor().clamp(0.0, bounds[0]) as u32,
            (clip.min[1] * scale).floor().clamp(0.0, bounds[1]) as u32,
        ];
        let clip_max = [
            (clip.max[0] * scale).ceil().clamp(0.0, bounds[0]) as u32,
            (clip.max[1] * scale).ceil().clamp(0.0, bounds[1]) as u32,
        ];
        if min.into_iter().chain(max).any(|value| !value.is_finite())
            || (0..2).any(|axis| {
                end[axis] <= start[axis]
                    || clip_max[axis] <= clip_min[axis]
                    || end[axis].min(clip_max[axis] as f32)
                        <= start[axis].max(clip_min[axis] as f32)
            })
        {
            return Err("Editor image candidate is fully clipped in the native window".to_owned());
        }
        Ok(Self {
            viewport: [start[0], start[1], end[0] - start[0], end[1] - start[1]],
            clip: [
                clip_min[0],
                clip_min[1],
                clip_max[0] - clip_min[0],
                clip_max[1] - clip_min[1],
            ],
            uv: [
                (start[0] - min[0]) / (max[0] - min[0]),
                (start[1] - min[1]) / (max[1] - min[1]),
                (end[0] - start[0]) / (max[0] - min[0]),
                (end[1] - start[1]) / (max[1] - min[1]),
            ],
        })
    }
}

struct ImageBlitter {
    format: wgpu::TextureFormat,
    device_epoch: u64,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl ImageBlitter {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, device_epoch: u64) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nara-drawn-editor-image-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nara-drawn-editor-image-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nara-drawn-editor-image-shader"),
            source: wgpu::ShaderSource::Wgsl(IMAGE_SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("nara-drawn-editor-image-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("image"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nara-drawn-editor-linear-image-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            format,
            device_epoch,
            layout,
            pipeline,
            sampler,
        }
    }

    fn bind(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        uv: [f32; 4],
    ) -> wgpu::BindGroup {
        let mut bytes = [0_u8; 16];
        for (destination, value) in bytes.chunks_exact_mut(4).zip(uv) {
            destination.copy_from_slice(&value.to_ne_bytes());
        }
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("nara-drawn-editor-image-uv"),
            contents: &bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nara-drawn-editor-exact-image-binding"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        })
    }

    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        group: &wgpu::BindGroup,
        placement: &Placement,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara-drawn-editor-exact-image-blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        let [x, y, width, height] = placement.viewport;
        let [clip_x, clip_y, clip_width, clip_height] = placement.clip;
        pass.set_viewport(x, y, width, height, 0.0, 1.0);
        pass.set_scissor_rect(clip_x, clip_y, clip_width, clip_height);
        pass.draw(0..3, 0..1);
    }
}

const IMAGE_SHADER: &str = r#"
@group(0) @binding(0) var panel_image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
@group(0) @binding(2) var<uniform> mapping: vec4<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position = positions[index];
    var output: VertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = mapping.xy + vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5) * mapping.zw;
    return output;
}

@fragment
fn image(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(panel_image, image_sampler, input.uv);
}
"#;
