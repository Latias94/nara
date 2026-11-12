use super::engine::{Engine, Preview};
use super::views_panel::{ViewPublication, ViewSnapshot};
use anyhow::{Context as _, Result, ensure};
use nara_render::DisplayMapping2d;
use nara_render_wgpu::{
    TextureAlpha, TextureTransfer, WgpuImageCompositeReceipt, WgpuImageDisplay,
    WgpuRenderHostError, WgpuSubmissionPrefix,
};
use open_gpui::{
    AtlasTextureLease, Bounds, DevicePixels, HostedSurfaceGeometry, HostedSurfaceRenderer,
    ImageTexture, ImageTextureAlpha, ImageTextureSampling, ImageTextureTransfer, PlatformAtlas,
    PlatformFrameSubmission, PlatformPresentationShutdownOutcome, PlatformWindowPresentOutcome,
    PreparedPlatformPresentationShutdown, Scene, WindowPresentationShutdownTicket, point, size,
};
use open_gpui_wgpu::{
    WgpuImportedImageDescriptor, WgpuSceneColorSpace, WgpuSceneRecording, WgpuSceneRenderer,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct Snapshot {
    pub preview: Rc<Preview>,
    pub mapping: DisplayMapping2d,
}

pub(super) struct Publication {
    pub generation: u64,
    pub snapshot: Rc<Snapshot>,
}

struct Candidate {
    scene: Scene,
    lease: AtlasTextureLease,
    geometry: HostedSurfaceGeometry,
    submission: PlatformFrameSubmission,
    snapshot: Option<Rc<Snapshot>>,
    content: Vec<WgpuImageDisplay>,
    views: Vec<Rc<ViewSnapshot>>,
}

struct InFlight {
    prefix: WgpuSubmissionPrefix,
    _candidate: Candidate,
    _recordings: Vec<WgpuSceneRecording>,
}

struct Readback {
    buffer: wgpu::Buffer,
    prefix: WgpuSubmissionPrefix,
    width: u32,
    height: u32,
    stride: u32,
    format: wgpu::TextureFormat,
    scale: f32,
}

pub(super) struct Diagnostic {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub scale: f32,
}

impl Diagnostic {
    pub fn logical_pixel(&self, x: f32, y: f32) -> [u8; 4] {
        self.pixel((x * self.scale) as u32, (y * self.scale) as u32)
    }
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * self.width + x) * 4) as usize;
        self.rgba[offset..offset + 4].try_into().unwrap()
    }
}

struct Submitted {
    submission: PlatformFrameSubmission,
    snapshot: Rc<Snapshot>,
    composite: WgpuImageCompositeReceipt,
}

struct SubmittedView {
    submission: PlatformFrameSubmission,
    snapshot: Rc<ViewSnapshot>,
    composite: WgpuImageCompositeReceipt,
}

#[derive(Default)]
struct Delivery {
    pending: Option<Candidate>,
    in_flight: Vec<InFlight>,
    submitted: VecDeque<Submitted>,
    submitted_views: VecDeque<SubmittedView>,
    quiesced: bool,
    error: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) enum Failure {
    Refuse,
    Panic,
}

pub(super) struct Renderer {
    engine: Rc<RefCell<Engine>>,
    recorder: RefCell<WgpuSceneRenderer>,
    device: wgpu::Device,
    delivery: Rc<RefCell<Delivery>>,
    wake: Rc<dyn Fn()>,
    pub candidate_snapshot: Rc<RefCell<Option<Rc<Snapshot>>>>,
    pub candidate_content: Rc<RefCell<Vec<WgpuImageDisplay>>>,
    pub publications: Rc<RefCell<VecDeque<Publication>>>,
    pub candidate_views: Rc<RefCell<Vec<Rc<ViewSnapshot>>>>,
    pub view_publications: Rc<RefCell<VecDeque<ViewPublication>>>,
    pub presented: RefCell<Option<Rc<Snapshot>>>,
    pub paused: Cell<bool>,
    pub failure: Cell<Option<Failure>>,
    pub submissions: Cell<u64>,
    pub discarded: Cell<u64>,
    pub refusals: Cell<u64>,
    pub diagnostics: Cell<u64>,
    pub diagnostic_next: Cell<bool>,
    #[cfg(feature = "ui-work-observation")]
    pub record_cpu: Cell<Duration>,
    #[cfg(feature = "ui-work-observation")]
    pub record_calls: Cell<u64>,
    readbacks: RefCell<VecDeque<Readback>>,
}

impl Renderer {
    pub fn new(engine: Rc<RefCell<Engine>>, wake: Rc<dyn Fn()>) -> Result<Rc<Self>> {
        let (recorder, device) = {
            let mut engine = engine.borrow_mut();
            let (packet, _) = engine.begin_window()?;
            engine.host.with_device(packet, |scope| {
                let info = scope.adapter().get_info();
                println!("NARA_GPUI_GPU backend={:?} name={} driver={} driver_info={} target={:?} device_epoch={}",
                    info.backend, info.name, info.driver, info.driver_info, scope.surface_format(), scope.device_epoch());
                WgpuSceneRenderer::new_with_color_space(
                    scope.adapter(),
                    Arc::new(scope.device().clone()),
                    Arc::new(scope.queue().clone()),
                    scope.surface_format(),
                    WgpuSceneColorSpace::LinearSrgb,
                )
                .map(|renderer| (renderer, scope.device().clone()))
            })??
        };
        Ok(Rc::new(Self {
            engine,
            recorder: RefCell::new(recorder),
            device,
            delivery: Rc::new(RefCell::new(Delivery::default())),
            wake,
            candidate_snapshot: Rc::default(),
            candidate_content: Rc::default(),
            publications: Rc::default(),
            candidate_views: Rc::default(),
            view_publications: Rc::default(),
            presented: RefCell::new(None),
            paused: Cell::new(false),
            failure: Cell::new(None),
            submissions: Cell::new(0),
            discarded: Cell::new(0),
            refusals: Cell::new(0),
            diagnostics: Cell::new(0),
            diagnostic_next: Cell::new(false),
            #[cfg(feature = "ui-work-observation")]
            record_cpu: Cell::new(Duration::ZERO),
            #[cfg(feature = "ui-work-observation")]
            record_calls: Cell::new(0),
            readbacks: RefCell::default(),
        }))
    }

    pub fn import(&self, preview: &Preview) -> Result<ImageTexture> {
        self.import_display(&preview.display)
    }

    pub fn import_display(&self, display: &WgpuImageDisplay) -> Result<ImageTexture> {
        let descriptor = display.output().descriptor();
        let texture = self
            .engine
            .borrow_mut()
            .host
            .with_image(display.output(), |_, _, view| view.texture().clone())?;
        ensure!(
            texture.format() == descriptor.native.format
                && texture.size() == descriptor.native.size,
            "the imported image differs from its original output descriptor"
        );
        let extent = display.output().extent();
        Ok(self
            .recorder
            .borrow()
            .sprite_atlas()
            .import_image(WgpuImportedImageDescriptor {
                transfer: match descriptor.transfer {
                    TextureTransfer::Srgb => ImageTextureTransfer::Srgb,
                    TextureTransfer::Linear => ImageTextureTransfer::Linear,
                },
                texture,
                mip_level: 0,
                array_layer: 0,
                region: Bounds::new(
                    point(DevicePixels(0), DevicePixels(0)),
                    size(
                        DevicePixels(extent.width as i32),
                        DevicePixels(extent.height as i32),
                    ),
                ),
                alpha: match descriptor.alpha {
                    TextureAlpha::Opaque => ImageTextureAlpha::Opaque,
                    TextureAlpha::Straight => ImageTextureAlpha::Straight,
                    TextureAlpha::Premultiplied => ImageTextureAlpha::Premultiplied,
                },
                sampling: ImageTextureSampling::Linear,
                owner: Some(Arc::new(display.clone())),
            })?)
    }

    pub fn pending(&self) -> bool {
        self.delivery.borrow().pending.is_some()
    }

    pub fn quiesced(&self) -> bool {
        self.delivery.borrow().quiesced
    }

    #[cfg(feature = "ui-work-observation")]
    pub fn retained_frames(&self) -> usize {
        let delivery = self.delivery.borrow();
        delivery.in_flight.len() + usize::from(delivery.pending.is_some())
    }

    pub fn poll(&self) -> Result<()> {
        let mut delivery = self.delivery.borrow_mut();
        let mut index = 0;
        while index < delivery.in_flight.len() {
            if delivery.in_flight[index].prefix.poll()? {
                delivery.in_flight.remove(index);
            } else {
                index += 1;
            }
        }
        if let Some(error) = delivery.error.take() {
            anyhow::bail!(error);
        }
        Ok(())
    }

    pub fn submit_pending(&self) -> Result<bool> {
        self.poll()?;
        let Some(candidate) = self.delivery.borrow_mut().pending.take() else {
            return Ok(false);
        };
        if !candidate.submission.is_pending() || candidate.lease.validate().is_err() {
            candidate
                .submission
                .complete(PlatformWindowPresentOutcome::RepaintRequired);
            self.discarded.set(self.discarded.get() + 1);
            return Ok(false);
        }
        if self.paused.get() {
            self.delivery.borrow_mut().pending = Some(candidate);
            return Ok(false);
        }
        let mut engine = self.engine.borrow_mut();
        let window = engine.window.clone();
        let (packet, frame_index) = engine.begin_window()?;
        let extent = candidate.geometry.physical_size();
        ensure!(
            packet.resolution.physical_width == extent.width.0 as u32
                && packet.resolution.physical_height == extent.height.0 as u32,
            "the original UI candidate differs from its current Nara window target"
        );
        let displays = candidate
            .snapshot
            .iter()
            .map(|snapshot| snapshot.preview.display.clone())
            .chain(candidate.content.iter().cloned())
            .chain(
                candidate
                    .views
                    .iter()
                    .map(|view| view.image.display.clone()),
            )
            .collect::<Vec<_>>();
        let failure = self.failure.take();
        let mut prefix = None;
        let mut recordings = Vec::new();
        let mut readback = None;
        let diagnostic = self.diagnostic_next.replace(false);
        let result = engine.host.present_window(
            packet,
            frame_index,
            &displays,
            |frame| {
                prefix = Some(frame.submission_prefix());
                #[cfg(feature = "ui-work-observation")]
                let record_start = Instant::now();
                let recording = frame
                    .encode(|_, _, encoder, view| {
                        self.recorder.borrow_mut().record(
                            &candidate.scene,
                            encoder,
                            view.texture(),
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        )
                    })
                    .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?;
                #[cfg(feature = "ui-work-observation")]
                {
                    self.record_cpu
                        .set(self.record_cpu.get() + record_start.elapsed());
                    self.record_calls.set(self.record_calls.get() + 1);
                }
                recordings.push(recording);
                if diagnostic {
                    frame
                        .encode(|device, _, encoder, target| -> Result<()> {
                            let target = target.texture();
                            let width = target.width();
                            let height = target.height();
                            let stride = (width * 4).div_ceil(256) * 256;
                            let texture = device.create_texture(&wgpu::TextureDescriptor {
                                label: Some("GPUI qualification-only same-Scene diagnostic"),
                                size: target.size(),
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: target.format(),
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                    | wgpu::TextureUsages::COPY_SRC,
                                view_formats: &[],
                            });
                            recordings.push(self.recorder.borrow_mut().record(
                                &candidate.scene,
                                encoder,
                                &texture,
                                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            )?);
                            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                                label: Some("GPUI qualification-only diagnostic readback"),
                                size: u64::from(stride) * u64::from(height),
                                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                                mapped_at_creation: false,
                            });
                            encoder.copy_texture_to_buffer(
                                texture.as_image_copy(),
                                wgpu::TexelCopyBufferInfo {
                                    buffer: &buffer,
                                    layout: wgpu::TexelCopyBufferLayout {
                                        offset: 0,
                                        bytes_per_row: Some(stride),
                                        rows_per_image: Some(height),
                                    },
                                },
                                target.size(),
                            );
                            readback = Some((buffer, width, height, stride, target.format()));
                            Ok(())
                        })
                        .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?;
                }
                if let Some(failure) = failure {
                    match failure {
                        Failure::Refuse => {
                            return Err(WgpuRenderHostError::window_paint(
                                "injected GPUI paint refusal after recording",
                            ));
                        }
                        Failure::Panic => panic!("injected GPUI paint panic after recording"),
                    }
                }
                for display in &displays {
                    frame.record_image_sample(display)?;
                }
                Ok(())
            },
            || window.pre_present_notify(),
        );
        let submission = candidate.submission.clone();
        let snapshot = candidate.snapshot.clone();
        let view_snapshots = candidate.views.clone();
        if let Some(prefix) = prefix {
            if let Some((buffer, width, height, stride, format)) = readback {
                self.readbacks.borrow_mut().push_back(Readback {
                    buffer,
                    prefix: prefix.clone(),
                    width,
                    height,
                    stride,
                    format,
                    scale: candidate.geometry.scale_factor(),
                });
                self.diagnostics.set(self.diagnostics.get() + 1);
            }
            self.delivery.borrow_mut().in_flight.push(InFlight {
                prefix,
                _candidate: candidate,
                _recordings: recordings,
            });
        }
        match result {
            Ok(outcome) if outcome.window().is_some() => {
                let work = engine.host.frame_transaction_stats();
                ensure!(
                    work.queue_submissions() == 1 && work.presents() == 1,
                    "normal UI composition did not use exactly one Host window submission/presentation"
                );
                if let Some(snapshot) = snapshot {
                    let composite = outcome
                        .composites()
                        .iter()
                        .copied()
                        .find(|receipt| {
                            receipt.image() == snapshot.preview.display.receipt()
                                && receipt.occurrence() == snapshot.preview.display.occurrence()
                                && Some(receipt.window()) == outcome.window()
                        })
                        .context(
                            "actual GPUI image sampling did not return its exact Nara composite",
                        )?;
                    self.delivery.borrow_mut().submitted.push_back(Submitted {
                        submission: submission.clone(),
                        snapshot,
                        composite,
                    });
                }
                for snapshot in view_snapshots {
                    let display = &snapshot.image.display;
                    let composite = outcome
                        .composites()
                        .iter()
                        .copied()
                        .find(|receipt| {
                            receipt.image() == display.receipt()
                                && receipt.occurrence() == display.occurrence()
                                && Some(receipt.window()) == outcome.window()
                        })
                        .context("the original Host did not sample the exact graph View")?;
                    self.delivery
                        .borrow_mut()
                        .submitted_views
                        .push_back(SubmittedView {
                            submission: submission.clone(),
                            snapshot,
                            composite,
                        });
                }
                ensure!(
                    submission.complete(PlatformWindowPresentOutcome::Submitted),
                    "original GPUI attempt refused actual Nara submission"
                );
                self.submissions.set(self.submissions.get() + 1);
            }
            Ok(_) => {
                submission.complete(PlatformWindowPresentOutcome::RepaintRequired);
            }
            Err(error) if failure.is_some() => {
                self.refusals.set(self.refusals.get() + 1);
                submission.complete(PlatformWindowPresentOutcome::RepaintRequired);
                println!("GPUI_EXPECTED_PAINT_FAILURE {error}");
            }
            Err(error) => {
                submission.complete(PlatformWindowPresentOutcome::Rejected);
                return Err(error.into());
            }
        }
        Ok(true)
    }

    pub fn take_diagnostic(&self) -> Result<Option<Diagnostic>> {
        let mut readbacks = self.readbacks.borrow_mut();
        let Some(readback) = readbacks.front() else {
            return Ok(None);
        };
        if !readback.prefix.poll()? {
            return Ok(None);
        }
        let readback = readbacks.pop_front().unwrap();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        readback
            .buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = send.send(result);
            });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.device.poll(wgpu::PollType::Poll)?;
            match receive.try_recv() {
                Ok(result) => {
                    result?;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) if Instant::now() < deadline => {
                    std::thread::yield_now()
                }
                result => anyhow::bail!("diagnostic mapping did not complete: {result:?}"),
            }
        }
        let mapped = readback.buffer.get_mapped_range(..)?;
        let mut rgba = Vec::with_capacity((readback.width * readback.height * 4) as usize);
        for row in mapped
            .chunks_exact(readback.stride as usize)
            .take(readback.height as usize)
        {
            for pixel in row[..(readback.width * 4) as usize].chunks_exact(4) {
                if matches!(
                    readback.format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                ) {
                    rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                } else {
                    rgba.extend_from_slice(pixel);
                }
            }
        }
        drop(mapped);
        readback.buffer.unmap();
        Ok(Some(Diagnostic {
            width: readback.width,
            height: readback.height,
            rgba,
            scale: readback.scale,
        }))
    }

    pub fn promote(&self) -> Result<()> {
        let mut accepted_view_generation = None;
        while let Some(publication) = self.view_publications.borrow_mut().pop_front() {
            accepted_view_generation = Some(publication.generation);
            let submitted = {
                let mut delivery = self.delivery.borrow_mut();
                let Some(index) = delivery.submitted_views.iter().position(|submitted| {
                    submitted.submission.frame_generation() == publication.generation
                        && Rc::ptr_eq(&submitted.snapshot, &publication.snapshot)
                }) else {
                    continue;
                };
                delivery.submitted_views.remove(index).unwrap()
            };
            if self.engine.borrow().accepts_view(&submitted.snapshot.image) {
                let presented = self.engine.borrow_mut().complete_view(
                    submitted.snapshot.image.clone(),
                    submitted.composite,
                    submitted.snapshot.mapping,
                )?;
                *submitted.snapshot.presented.borrow_mut() = Some(presented);
            }
        }
        self.delivery
            .borrow_mut()
            .submitted_views
            .retain(|submitted| {
                submitted.submission.status() == open_gpui::PlatformFrameSubmissionStatus::Submitted
                    && accepted_view_generation.is_none_or(|generation| {
                        submitted.submission.frame_generation() >= generation
                    })
            });
        while let Some(publication) = self.publications.borrow_mut().pop_front() {
            let submitted = {
                let mut delivery = self.delivery.borrow_mut();
                let Some(index) = delivery.submitted.iter().position(|submitted| {
                    submitted.submission.frame_generation() == publication.generation
                        && Rc::ptr_eq(&submitted.snapshot, &publication.snapshot)
                }) else {
                    continue;
                };
                delivery.submitted.remove(index).unwrap()
            };
            self.delivery.borrow_mut().submitted.retain(|candidate| {
                candidate.submission.frame_generation() > publication.generation
            });
            if !self
                .engine
                .borrow()
                .accepts_preview(&submitted.snapshot.preview)
            {
                continue;
            }
            self.engine.borrow_mut().complete(
                &submitted.snapshot.preview,
                submitted.composite,
                submitted.snapshot.mapping,
            )?;
            *self.presented.borrow_mut() = Some(submitted.snapshot);
        }
        self.delivery.borrow_mut().submitted.retain(|submitted| {
            submitted.submission.status() == open_gpui::PlatformFrameSubmissionStatus::Submitted
        });
        Ok(())
    }
}

impl HostedSurfaceRenderer for Renderer {
    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.recorder.borrow().sprite_atlas().clone()
    }

    fn draw(
        &self,
        scene: &Scene,
        geometry: HostedSurfaceGeometry,
        submission: &PlatformFrameSubmission,
    ) -> PlatformWindowPresentOutcome {
        let result = (|| -> Result<()> {
            let mut delivery = self.delivery.borrow_mut();
            if delivery
                .pending
                .as_ref()
                .is_some_and(|candidate| !candidate.submission.is_pending())
            {
                delivery.pending.take();
                self.discarded.set(self.discarded.get() + 1);
            }
            ensure!(
                !delivery.quiesced && delivery.pending.is_none() && submission.is_pending(),
                "GPUI renderer delivery is unavailable"
            );
            let lease = scene.retain_atlas_textures(self.sprite_atlas())?;
            delivery.pending = Some(Candidate {
                scene: scene.clone(),
                lease,
                geometry,
                submission: submission.clone(),
                snapshot: self.candidate_snapshot.borrow().clone(),
                content: self.candidate_content.borrow().clone(),
                views: self.candidate_views.borrow().clone(),
            });
            Ok(())
        })();
        match result {
            Ok(()) => {
                (self.wake)();
                PlatformWindowPresentOutcome::Deferred
            }
            Err(error) => {
                self.delivery.borrow_mut().error = Some(error.to_string());
                PlatformWindowPresentOutcome::Rejected
            }
        }
    }

    fn prepare_shutdown(
        &self,
        ticket: WindowPresentationShutdownTicket,
    ) -> PreparedPlatformPresentationShutdown {
        let delivery = self.delivery.clone();
        PreparedPlatformPresentationShutdown::new(ticket, move |ticket| {
            let result = (|| -> Result<()> {
                let mut delivery = delivery.borrow_mut();
                if let Some(candidate) = delivery.pending.take() {
                    candidate
                        .submission
                        .complete(PlatformWindowPresentOutcome::Rejected);
                }
                let deadline = Instant::now() + Duration::from_secs(10);
                for frame in &delivery.in_flight {
                    while !frame.prefix.poll()? {
                        ensure!(
                            Instant::now() < deadline,
                            "original UI window prefix did not settle"
                        );
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                delivery.in_flight.clear();
                delivery.submitted.clear();
                delivery.submitted_views.clear();
                ensure!(
                    ticket.acknowledge_quiesced(),
                    "logical root rejected original quiescence"
                );
                delivery.quiesced = true;
                Ok(())
            })();
            match result {
                Ok(()) => PlatformPresentationShutdownOutcome::Quiesced,
                Err(error) => {
                    delivery.borrow_mut().error = Some(error.to_string());
                    PlatformPresentationShutdownOutcome::Rejected
                }
            }
        })
    }
}
