//! Real native windows and an independently scheduled image consumer on one public render Host.
//!
//! The 30/60/144 rates below are deterministic logical schedules, not measured frame rates.
//! This consumer proves exact images, native receipts and retirement, not a complete custom UI.

#![forbid(unsafe_code)]

use std::{
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use nara_render::{
    Extent2d, RenderFrameSourceGeneration, RenderImage2d, RenderImagePacket, RenderViewId,
    RenderWindowPacket,
};
use nara_render_wgpu::{
    WgpuImageCompositeReceipt, WgpuImageDisplay, WgpuImageOutput, WgpuPolicyDefinition,
    WgpuPolicyInstallation, WgpuPublishedFrame, WgpuRenderError, WgpuRenderHost,
    WgpuRenderHostError, WgpuRenderInput, WgpuRenderPolicy, WgpuRenderProducer,
    WgpuSubmissionPrefix, WgpuWindowPresentedReceipt,
};
use nara_window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId as NativeWindowId},
};

#[path = "../tests/support/window_gpu.rs"]
mod window_gpu;

use window_gpu::{
    BLUE, ClearPolicy, CompositeCapture, CompositePolicy, GREEN, HEIGHT, ImagePainter, RED, Result,
    StreamColor, WIDTH, YELLOW,
};

const TICKS: u64 = 720;
const A_RATE: u64 = 30;
const B_RATE: u64 = 60;
const IMAGE_RATE: u64 = 144;
const A_PAUSE_TICK: u64 = 24;
const MIRROR_TICK: u64 = 60;
const PAINT_REFUSAL_TICK: u64 = 72;
const PAINT_PANIC_TICK: u64 = 192;
const PRE_PRESENT_PANIC_TICK: u64 = 216;
const MINIMIZE_TICK: u64 = 96;
const RESTORE_TICK: u64 = 168;
const A_RETIRE_TICK: u64 = 240;
const RESIZE_TICK: u64 = 360;
const REFUSAL: &str = "the consumer deliberately refused one acquired A paint";
const SCENARIO_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaintFailure {
    None,
    Refuse,
    PaintPanic,
    PrePresentPanic,
}

struct Stream<P: WgpuRenderPolicy> {
    owner: WgpuRenderProducer,
    installation: WgpuPolicyInstallation<P>,
    source: RenderFrameSourceGeneration,
    view: RenderViewId,
    target: RenderImagePacket,
    frame: u64,
    last_input: Option<WgpuRenderInput>,
}

impl<P: WgpuRenderPolicy> Stream<P> {
    fn new(host: &mut WgpuRenderHost, definition: &WgpuPolicyDefinition<P>) -> Result<Self> {
        let owner = host.create_producer()?;
        let source = RenderFrameSourceGeneration::try_acquire()
            .ok_or_else(|| io::Error::other("independent render-source identities exhausted"))?;
        assert!(source.runtime_generation().is_none());
        let view = RenderViewId::try_acquire()
            .ok_or_else(|| io::Error::other("render View identities exhausted"))?;
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle()?,
            Extent2d::new(WIDTH, HEIGHT).unwrap(),
        )?;
        host.prepare_policy(&owner, definition)?;
        let reservation = host.reserve_image_target(&owner, source, target)?;
        host.activate_image_target(&owner, reservation)?;
        Ok(Self {
            owner,
            installation: definition.install()?,
            source,
            view,
            target,
            frame: 0,
            last_input: None,
        })
    }

    fn publish(
        &mut self,
        host: &mut WgpuRenderHost,
        capture: P::Capture,
    ) -> Result<WgpuPublishedFrame> {
        let frame = self.frame + 1;
        let input = self.installation.capture(
            window_gpu::packet(self.source, self.view, frame, self.target)?,
            Arc::new(capture),
        )?;
        let candidate = host.prepare(&self.owner, &input)?;
        let published = host
            .submit_candidate(&self.owner, candidate)
            .map_err(|failure| failure.error)?;
        let receipt = published.image().receipt();
        assert_eq!(receipt.producer_id(), self.owner.id());
        assert_eq!(receipt.source_generation(), self.source);
        assert_eq!(receipt.render_transaction_frame_index(), frame);
        assert_eq!(receipt.backend_instance_id(), host.backend_instance_id());
        assert_eq!(receipt.device_epoch(), host.device_epoch());
        self.frame = frame;
        self.last_input = Some(input);
        Ok(published)
    }
}

struct NativeTarget {
    id: WindowId,
    window: Arc<Window>,
    resolution: WindowResolution,
    painter: ImagePainter,
    initial_size: PhysicalSize<u32>,
}

impl NativeTarget {
    fn packet(&self) -> RenderWindowPacket {
        RenderWindowPacket {
            id: self.id,
            resolution: self.resolution,
            present_mode: PresentMode::AutoNoVsync,
        }
    }

    fn placement(&self, role: DisplayRole) -> Placement {
        let destination = PixelRect {
            x: 0,
            y: 0,
            width: self.resolution.physical_width,
            height: self.resolution.physical_height,
        };
        Placement {
            window: self.id,
            destination,
            clip: destination,
            dpi: self.resolution.scale_factor,
            role,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    fn native(self) -> [u32; 4] {
        [self.x, self.y, self.width, self.height]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayRole {
    RetainedImage,
    CurrentImage,
    PassiveMirror,
}

/// Immutable frontend semantics for this exact display admission. The Host does not prove them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    window: WindowId,
    destination: PixelRect,
    clip: PixelRect,
    dpi: f64,
    role: DisplayRole,
}

impl Placement {
    fn visible_clip(self) -> Option<PixelRect> {
        let x = self.destination.x.max(self.clip.x);
        let y = self.destination.y.max(self.clip.y);
        let right =
            (self.destination.x + self.destination.width).min(self.clip.x + self.clip.width);
        let bottom =
            (self.destination.y + self.destination.height).min(self.clip.y + self.clip.height);
        (right > x && bottom > y).then_some(PixelRect {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        })
    }

    fn same_mapping(self, other: Self) -> bool {
        self.window == other.window
            && self.destination == other.destination
            && self.clip == other.clip
            && self.dpi == other.dpi
    }
}

#[derive(Clone)]
struct DisplayAdmission {
    display: WgpuImageDisplay,
    placement: Placement,
}

struct PlacedReceipt {
    composite: WgpuImageCompositeReceipt,
    placement: Placement,
}

#[derive(Debug, PartialEq, Eq)]
enum SemanticJoinError {
    StalePlacement,
    OtherDisplay,
}

impl PlacedReceipt {
    /// Consumer-local semantics only: no input routing or shader inverse is inferred here.
    fn join(&self, admission: &DisplayAdmission) -> std::result::Result<(), SemanticJoinError> {
        if self.placement != admission.placement {
            return Err(SemanticJoinError::StalePlacement);
        }
        if self.composite.image() != admission.display.receipt()
            || self.composite.occurrence() != admission.display.occurrence()
            || self.composite.window().window_id() != admission.placement.window
        {
            return Err(SemanticJoinError::OtherDisplay);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Proof {
    attempts: [u64; 2],
    windows: [Vec<WgpuWindowPresentedReceipt>; 2],
    composites: [Vec<PlacedReceipt>; 2],
    unavailable: [u64; 2],
    paint_refused: bool,
    paint_panicked: bool,
    pre_present_panicked: bool,
    a_recovered: bool,
    b_presents_after_refusal: u64,
    old_a_presents_after_close: u64,
    passive_mirror_receipts: u64,
    stale_placement_rejections: u64,
    stale_mapping_rejections: u64,
    b_captures_after_close: u64,
    image_captures_after_close: u64,
    minimize_observed: bool,
    zero_size_observed: bool,
    b_resize_observed: bool,
    b_presents_after_resize: u64,
    pixels_checked: usize,
}

struct Scenario {
    // Declaration order also keeps the Host ahead of all native Window owners on error unwind.
    host: WgpuRenderHost,
    a: Stream<ClearPolicy>,
    b: Stream<ClearPolicy>,
    image: Stream<CompositePolicy>,
    old_a: Option<WgpuPublishedFrame>,
    a2: Option<WgpuPublishedFrame>,
    current_b: Option<WgpuPublishedFrame>,
    current_image: Option<WgpuImageOutput>,
    displays: [Option<DisplayAdmission>; 2],
    a_prefix: Option<WgpuSubmissionPrefix>,
    b_color: [u8; 4],
    image_b_frame: u64,
    tick: u64,
    started: Instant,
    proof: Proof,
    // These are deliberately last; final queue retirement and Host Drop precede Window Drop.
    targets: [NativeTarget; 2],
}

impl Scenario {
    fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        let make_window = |name, x| -> Result<Arc<Window>> {
            Ok(Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title(name)
                        .with_inner_size(PhysicalSize::new(160, 96))
                        .with_position(PhysicalPosition::new(x, 80))
                        .with_active(false),
                )?,
            ))
        };
        let windows = [
            make_window("Nara streams A: exact retained red", 80)?,
            make_window("Nara streams B: independent current image", 300)?,
        ];
        let ids = [WindowId::PRIMARY, WindowId::new(2)];
        let mut host = WgpuRenderHost::offscreen();
        for (id, window) in ids.into_iter().zip(&windows) {
            host.register_window(id, WindowHandleProvider::new(Arc::clone(window)))?;
        }
        let mut make_target = |index: usize| -> Result<NativeTarget> {
            let window = Arc::clone(&windows[index]);
            let size = window.inner_size();
            let resolution = WindowResolution::new(size.width, size.height)
                .with_scale_factor(window.scale_factor());
            let packet = RenderWindowPacket {
                id: ids[index],
                resolution,
                present_mode: PresentMode::AutoNoVsync,
            };
            let painter = host.with_device(packet, |scope| {
                if index == 0 {
                    eprintln!("streams-window adapter: {:?}", scope.adapter().get_info());
                }
                ImagePainter::new(scope.device(), scope.surface_format(), false)
            })?;
            Ok(NativeTarget {
                id: ids[index],
                window,
                resolution,
                painter,
                initial_size: size,
            })
        };
        let targets = [make_target(0)?, make_target(1)?];
        let clear = WgpuPolicyDefinition::new(ClearPolicy);
        let composite = WgpuPolicyDefinition::new(CompositePolicy::new());
        let a = Stream::new(&mut host, &clear)?;
        let b = Stream::new(&mut host, &clear)?;
        let image = Stream::new(&mut host, &composite)?;
        assert_ne!(a.source, b.source);
        assert_ne!(a.source, image.source);
        assert_ne!(b.source, image.source);
        assert_ne!(a.owner.id(), b.owner.id());
        assert_ne!(a.owner.id(), image.owner.id());
        assert_ne!(b.owner.id(), image.owner.id());
        eprintln!(
            "streams-window: {TICKS} logical ticks; schedules A={A_RATE}, B={B_RATE}, image={IMAGE_RATE}; these are not measured rates",
        );
        Ok(Self {
            host,
            a,
            b,
            image,
            old_a: None,
            a2: None,
            current_b: None,
            current_image: None,
            displays: [None, None],
            a_prefix: None,
            b_color: GREEN,
            image_b_frame: 0,
            tick: 0,
            started: Instant::now(),
            proof: Proof::default(),
            targets,
        })
    }

    fn step(&mut self) -> Result<Option<String>> {
        if self.started.elapsed() >= SCENARIO_TIMEOUT {
            return Err(
                io::Error::other("the logical native scenario exceeded sixty seconds").into(),
            );
        }
        let tick = self.tick;
        self.host
            .begin_window_frame(self.targets[1].packet(), tick + 1)?;

        if tick == MINIMIZE_TICK {
            self.targets[0].window.set_minimized(true);
        }
        if tick == RESTORE_TICK {
            self.targets[0].window.set_minimized(false);
        }
        if tick == RESIZE_TICK {
            // Only the actual Resized event updates the render packet and the observed proof.
            let _ = self.targets[1]
                .window
                .request_inner_size(PhysicalSize::new(192, 112));
        }
        self.proof.minimize_observed |= self.targets[0].window.is_minimized() == Some(true);

        if tick <= A_PAUSE_TICK && due(tick, A_RATE) {
            let color = if self.a.frame == 0 { RED } else { YELLOW };
            let published = self.a.publish(&mut self.host, color)?;
            if self.a.frame == 1 {
                self.old_a = Some(published);
            } else {
                assert_ne!(
                    published.image().receipt().output_version(),
                    self.old_a
                        .as_ref()
                        .unwrap()
                        .image()
                        .receipt()
                        .output_version(),
                );
                self.a2 = Some(published);
            }
        }

        if tick == A_RETIRE_TICK {
            // No idle wait or readback of A2 precedes this frozen producer-prefix observation.
            self.a_prefix = Some(self.host.retire_producer(&self.a.owner).ok_or_else(|| {
                io::Error::other("submitted A did not return its original frozen prefix")
            })?);
            assert!(matches!(
                self.host
                    .submit(&self.a.owner, self.a.last_input.as_ref().unwrap()),
                Err(WgpuRenderHostError::ProducerClosed),
            ));
            assert!(matches!(
                self.host
                    .reserve_image_target(&self.a.owner, self.a.source, self.a.target),
                Err(WgpuRenderHostError::ProducerClosed),
            ));
            assert!(self.host.retire_producer(&self.a.owner).is_none());
            let before_close = self.displays[0].take().unwrap();
            let after_close = self
                .host
                .display_image(self.old_a.as_ref().unwrap().image().clone())?;
            assert_eq!(before_close.display.receipt(), after_close.receipt());
            assert_ne!(before_close.display.occurrence(), after_close.occurrence());
            self.displays[0] = Some(DisplayAdmission {
                display: after_close,
                placement: self.targets[0].placement(DisplayRole::RetainedImage),
            });
            eprintln!(
                "streams-window: A closed; B and the image producer continue on their own clocks"
            );
        }

        if due(tick, B_RATE) {
            self.b_color = if (self.b.frame + 1) % 2 == 1 {
                GREEN
            } else {
                BLUE
            };
            let published = self.b.publish(&mut self.host, self.b_color)?;
            self.current_b = Some(published);
            if self.a_prefix.is_some() {
                self.proof.b_captures_after_close += 1;
            }
        }

        if due(tick, IMAGE_RATE) {
            let before = self.host.frame_transaction_stats();
            let published = self.image.publish(
                &mut self.host,
                CompositeCapture {
                    old_a: self
                        .old_a
                        .as_ref()
                        .unwrap()
                        .final_texture::<StreamColor>()?,
                    current_b: self
                        .current_b
                        .as_ref()
                        .unwrap()
                        .final_texture::<StreamColor>()?,
                },
            )?;
            let after = self.host.frame_transaction_stats();
            assert_eq!(
                before.surface_acquire_attempts(),
                after.surface_acquire_attempts()
            );
            assert_eq!(before.presents(), after.presents());
            self.current_image = Some(published.into_image());
            self.image_b_frame = self.b.frame;
            if self.a_prefix.is_some() {
                self.proof.image_captures_after_close += 1;
            }
        }

        if due(tick, A_RATE) {
            let output = self.old_a.as_ref().unwrap().image().clone();
            let admission = self.admit(0, output, DisplayRole::RetainedImage)?;
            let failure = if tick >= PAINT_REFUSAL_TICK && !self.proof.paint_refused {
                PaintFailure::Refuse
            } else if tick >= PAINT_PANIC_TICK && !self.proof.paint_panicked {
                PaintFailure::PaintPanic
            } else if tick >= PRE_PRESENT_PANIC_TICK && !self.proof.pre_present_panicked {
                PaintFailure::PrePresentPanic
            } else {
                PaintFailure::None
            };
            self.present(0, &admission, failure)?;
        }
        if due(tick, B_RATE) {
            let mirror = tick >= MIRROR_TICK && self.proof.passive_mirror_receipts == 0;
            let (output, role) = if mirror {
                (
                    self.old_a.as_ref().unwrap().image().clone(),
                    DisplayRole::PassiveMirror,
                )
            } else {
                (
                    self.current_b.as_ref().unwrap().image().clone(),
                    DisplayRole::CurrentImage,
                )
            };
            let admission = self.admit(1, output, role)?;
            if mirror {
                let original = self.displays[0].as_ref().unwrap();
                assert_eq!(original.display.receipt(), admission.display.receipt());
                assert_ne!(
                    original.display.occurrence(),
                    admission.display.occurrence()
                );
                assert_ne!(original.placement.window, admission.placement.window);
            }
            self.present(1, &admission, PaintFailure::None)?;
        }

        // Observe A's original completion while later B/image submissions remain authorized.
        if let Some(prefix) = &self.a_prefix {
            let _ = prefix.poll()?;
        }
        if matches!(tick, 0 | 245 | 715) {
            assert!(due(tick, IMAGE_RATE));
            assert_eq!(self.image_b_frame, self.b.frame);
            self.proof.pixels_checked += window_gpu::assert_pixels(
                &mut self.host,
                self.current_image.as_ref().unwrap(),
                RED,
                self.b_color,
            )?;
        }
        self.tick += 1;
        if self.tick == TICKS {
            return Ok(Some(self.verify()?));
        }
        Ok(None)
    }

    fn admit(
        &mut self,
        index: usize,
        output: WgpuImageOutput,
        role: DisplayRole,
    ) -> Result<DisplayAdmission> {
        let placement = self.targets[index].placement(role);
        if let Some(previous) = &self.displays[index] {
            if previous.placement == placement && previous.display.receipt() == output.receipt() {
                return Ok(previous.clone());
            }
        }
        let admission = DisplayAdmission {
            display: self.host.display_image(output)?,
            placement,
        };
        if let Some(previous) = &self.displays[index] {
            assert_ne!(
                previous.display.occurrence(),
                admission.display.occurrence()
            );
            if previous.placement != admission.placement {
                if let Some(receipt) = self.proof.composites[index]
                    .iter()
                    .rev()
                    .find(|receipt| receipt.composite.occurrence() == previous.display.occurrence())
                {
                    assert_eq!(receipt.join(previous), Ok(()));
                    assert_eq!(
                        receipt.join(&admission),
                        Err(SemanticJoinError::StalePlacement)
                    );
                    self.proof.stale_placement_rejections += 1;
                    if !previous.placement.same_mapping(admission.placement) {
                        self.proof.stale_mapping_rejections += 1;
                    }
                }
            }
        }
        self.displays[index] = Some(admission.clone());
        Ok(admission)
    }

    fn present(
        &mut self,
        index: usize,
        admission: &DisplayAdmission,
        failure: PaintFailure,
    ) -> Result<()> {
        let target = &self.targets[index];
        let display = &admission.display;
        let placement = admission.placement;
        assert_eq!(placement, target.placement(placement.role));
        let group = self
            .host
            .with_image(display.output(), |device, _queue, view| {
                target.painter.bind(device, view, view)
            })?;
        self.proof.attempts[index] += 1;
        let display_frame = self.proof.attempts[index];
        let before = self.host.frame_transaction_stats();
        let original_namespace = (self.host.backend_instance_id(), self.host.device_epoch());
        let mut painted = false;
        let mut sampled = false;
        let mut notified = false;
        let outcome = self.host.present_window(
            target.packet(),
            display_frame,
            std::slice::from_ref(display),
            |frame| {
                painted = true;
                assert_eq!(frame.surface_format(), target.painter.format);
                assert_eq!(frame.surface_extent().width, placement.destination.width);
                assert_eq!(frame.surface_extent().height, placement.destination.height);
                if let Some(clip) = placement.visible_clip() {
                    frame.encode(|_device, _queue, encoder, target_view| {
                        target.painter.draw_placed(
                            encoder,
                            target_view,
                            &group,
                            placement.destination.native(),
                            clip.native(),
                        );
                    });
                    // Trusted declaration follows the actual non-fully-clipped shader sample.
                    frame.record_image_sample(display)?;
                    sampled = true;
                }
                if failure == PaintFailure::Refuse {
                    return Err(WgpuRenderError::ContentRejected {
                        message: REFUSAL.into(),
                    }
                    .into());
                }
                assert!(
                    failure != PaintFailure::PaintPanic,
                    "injected acquired window paint panic"
                );
                Ok(())
            },
            || {
                assert!(
                    failure != PaintFailure::PrePresentPanic,
                    "injected native pre-present panic"
                );
                notified = true;
                target.window.pre_present_notify();
            },
        );
        let outcome = match outcome {
            Err(WgpuRenderHostError::Backend(WgpuRenderError::ContentRejected { message }))
                if failure == PaintFailure::Refuse && painted && message == REFUSAL =>
            {
                assert!(
                    !notified,
                    "a refused acquired paint cannot request presentation"
                );
                assert_eq!(
                    self.host.frame_transaction_stats().presents(),
                    before.presents()
                );
                self.proof.paint_refused = true;
                return Ok(());
            }
            Err(WgpuRenderHostError::NativeCallbackPanicked { stage })
                if matches!(
                    failure,
                    PaintFailure::PaintPanic | PaintFailure::PrePresentPanic
                ) =>
            {
                assert!(painted && !notified);
                assert_eq!(
                    self.host.frame_transaction_stats().presents(),
                    before.presents()
                );
                assert_eq!(
                    (self.host.backend_instance_id(), self.host.device_epoch()),
                    original_namespace
                );
                match failure {
                    PaintFailure::PaintPanic => {
                        assert_eq!(stage, "window paint");
                        self.proof.paint_panicked = true;
                    }
                    PaintFailure::PrePresentPanic => {
                        assert_eq!(stage, "pre-present");
                        self.proof.pre_present_panicked = true;
                    }
                    _ => unreachable!(),
                }
                return Ok(());
            }
            result => result?,
        };
        let Some(window) = outcome.window() else {
            assert!(outcome.composites().is_empty());
            assert!(!painted && !notified);
            self.proof.unavailable[index] += 1;
            return Ok(());
        };
        assert!(painted && notified && sampled);
        assert!(
            failure == PaintFailure::None,
            "a failed native callback must not publish a window receipt"
        );
        assert_eq!(window.window_id(), target.id);
        assert_eq!(window.display_frame_index(), display_frame);
        assert_eq!(
            window.backend_instance_id(),
            self.host.backend_instance_id()
        );
        assert_eq!(window.device_epoch(), self.host.device_epoch());
        assert!(window.surface_generation().is_some());
        assert_eq!(outcome.composites().len(), 1);
        let composite = outcome.composites()[0];
        assert_eq!(composite.window(), window);
        assert_eq!(composite.image(), display.receipt());
        assert_eq!(composite.occurrence(), display.occurrence());
        if let Some(previous) = self.proof.composites[index].last() {
            assert!(
                composite.presentation_generation() > previous.composite.presentation_generation()
            );
        }
        let placed = PlacedReceipt {
            composite,
            placement,
        };
        assert_eq!(placed.join(admission), Ok(()));
        self.proof.windows[index].push(window);
        self.proof.composites[index].push(placed);
        if placement.role == DisplayRole::PassiveMirror {
            self.proof.passive_mirror_receipts += 1;
        }
        if index == 0 && self.proof.paint_refused {
            self.proof.a_recovered = true;
        }
        if index == 0 && self.a_prefix.is_some() {
            assert_eq!(
                composite.image(),
                self.old_a.as_ref().unwrap().image().receipt()
            );
            self.proof.old_a_presents_after_close += 1;
        }
        if index == 1 && self.proof.paint_refused {
            self.proof.b_presents_after_refusal += 1;
        }
        if index == 1 && self.proof.b_resize_observed {
            self.proof.b_presents_after_resize += 1;
        }
        Ok(())
    }

    fn window_event(&mut self, id: NativeWindowId, event: &WindowEvent) -> Result<()> {
        let Some(index) = self
            .targets
            .iter()
            .position(|target| target.window.id() == id)
        else {
            return Ok(());
        };
        let target = &mut self.targets[index];
        match event {
            WindowEvent::Resized(size) => {
                target.resolution.physical_width = size.width;
                target.resolution.physical_height = size.height;
                if index == 0 && (size.width == 0 || size.height == 0) {
                    self.proof.zero_size_observed = true;
                }
                if index == 1
                    && self.tick > RESIZE_TICK
                    && *size != target.initial_size
                    && size.width != 0
                    && size.height != 0
                {
                    self.proof.b_resize_observed = true;
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                target.resolution.scale_factor = *scale_factor;
            }
            _ => {}
        }
        if let Some(previous) = self.displays[index].clone() {
            if previous.placement != self.targets[index].placement(previous.placement.role) {
                // Re-admit the same immutable pixels immediately on an actual placement change.
                let replacement = self.admit(
                    index,
                    previous.display.output().clone(),
                    previous.placement.role,
                )?;
                assert_eq!(replacement.display.receipt(), previous.display.receipt());
                assert_ne!(
                    replacement.display.occurrence(),
                    previous.display.occurrence()
                );
            }
        }
        Ok(())
    }

    fn verify(&mut self) -> Result<String> {
        assert_eq!(self.a.frame, 2, "A pauses after its second capture");
        assert_eq!(self.b.frame, B_RATE);
        assert_eq!(self.image.frame, IMAGE_RATE);
        assert_eq!(self.proof.attempts, [A_RATE, B_RATE]);
        assert!(self.proof.windows.iter().all(|windows| !windows.is_empty()));
        assert!(
            self.proof.paint_refused,
            "the controlled paint refusal must acquire an actual A surface"
        );
        assert!(self.proof.paint_panicked && self.proof.pre_present_panicked);
        assert!(self.proof.a_recovered);
        assert!(self.proof.b_presents_after_refusal > 0);
        assert!(self.proof.old_a_presents_after_close > 0);
        assert!(self.proof.passive_mirror_receipts > 0);
        assert!(self.proof.stale_placement_rejections > 0);
        assert!(self.proof.b_captures_after_close > 0);
        assert!(self.proof.image_captures_after_close > 0);
        let prefix = self.a_prefix.as_ref().unwrap();
        assert!(
            prefix.poll()?,
            "A must physically settle while the other producers remain open"
        );
        assert!(prefix.is_complete());
        assert!(
            prefix.poll()?,
            "original-prefix completion must remain monotonic"
        );
        let old_a = self.old_a.as_ref().unwrap().image();
        let a2 = self.a2.as_ref().unwrap().image();
        assert_eq!(
            old_a.receipt().target_generation(),
            a2.receipt().target_generation()
        );
        assert_eq!(
            self.displays[0].as_ref().unwrap().display.receipt(),
            old_a.receipt()
        );
        assert_eq!(old_a.receipt().render_transaction_frame_index(), 1);
        assert_eq!(a2.receipt().render_transaction_frame_index(), 2);
        self.proof.pixels_checked += window_gpu::assert_pixels(&mut self.host, old_a, RED, RED)?;
        self.proof.pixels_checked += window_gpu::assert_pixels(&mut self.host, a2, YELLOW, YELLOW)?;
        self.proof.pixels_checked += window_gpu::assert_pixels(
            &mut self.host,
            self.current_b.as_ref().unwrap().image(),
            BLUE,
            BLUE,
        )?;
        if self.proof.b_resize_observed {
            assert!(self.proof.b_presents_after_resize > 0);
        }
        Ok(format!(
            "STREAMS_WINDOW_OK ticks={TICKS} schedules=30/60/144 a_captures={} b_captures={} image_captures={} window_a_receipts={} window_b_receipts={} old_a_after_close={} b_after_close={} image_after_close={} passive_mirror_receipts={} stale_placement_rejections={} stale_mapping_rejections={} frozen_prefix_complete=true paint_refused=true paint_panicked=true pre_present_panicked=true a_recovered=true minimize_observed={} zero_size_observed={} window_a_unavailable={} resize_observed={} b_after_resize={} pixels_checked={}",
            self.a.frame,
            self.b.frame,
            self.image.frame,
            self.proof.windows[0].len(),
            self.proof.windows[1].len(),
            self.proof.old_a_presents_after_close,
            self.proof.b_captures_after_close,
            self.proof.image_captures_after_close,
            self.proof.passive_mirror_receipts,
            self.proof.stale_placement_rejections,
            self.proof.stale_mapping_rejections,
            self.proof.minimize_observed,
            self.proof.zero_size_observed,
            self.proof.unavailable[0],
            self.proof.b_resize_observed,
            self.proof.b_presents_after_resize,
            self.proof.pixels_checked,
        ))
    }

    fn shutdown(&mut self) -> Result<()> {
        // Retained images and native bindings stay alive while the Host drains its original work.
        let _ = self.host.retire_producer(&self.a.owner);
        let _ = self.host.retire_producer(&self.b.owner);
        let _ = self.host.retire_producer(&self.image.owner);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.host.poll_retirement()? {
            if Instant::now() >= deadline {
                return Err(io::Error::other("final Host retirement exceeded ten seconds").into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(self.host.poll_retirement()?);
        Ok(())
    }
}

fn due(tick: u64, rate: u64) -> bool {
    tick % (TICKS / rate) == 0
}

#[derive(Default)]
struct NativeApp {
    scenario: Option<Scenario>,
    result: Option<std::result::Result<String, String>>,
}

impl NativeApp {
    fn finish(&mut self, event_loop: &ActiveEventLoop, result: Result<String>) {
        let mut result = result.map_err(|error| error.to_string());
        if let Some(mut scenario) = self.scenario.take() {
            if let Err(error) = scenario.shutdown() {
                result = Err(match result {
                    Ok(_) => format!("Host retirement failed: {error}"),
                    Err(original) => format!("{original}; Host retirement also failed: {error}"),
                });
            }
            drop(scenario);
        }
        self.result =
            Some(result.map(|summary| format!("{summary} host_retired=true windows_dropped=true")));
        event_loop.exit();
    }
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.scenario.is_some() || self.result.is_some() {
            return;
        }
        match Scenario::new(event_loop) {
            Ok(scenario) => self.scenario = Some(scenario),
            Err(error) => self.finish(event_loop, Err(error)),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: NativeWindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            if self.scenario.is_some() {
                self.finish(
                    event_loop,
                    Err(
                        io::Error::other("a native window closed before qualification finished")
                            .into(),
                    ),
                );
            }
            return;
        }
        if let Some(scenario) = &mut self.scenario {
            if let Err(error) = scenario.window_event(id, &event) {
                self.finish(event_loop, Err(error));
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(scenario) = &mut self.scenario else {
            return;
        };
        let step = catch_unwind(AssertUnwindSafe(|| scenario.step())).unwrap_or_else(|panic| {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    panic
                        .downcast_ref::<&str>()
                        .map(|message| (*message).to_owned())
                })
                .unwrap_or_else(|| "native consumer assertion panicked".to_owned());
            Err(io::Error::other(message).into())
        });
        match step {
            Ok(Some(summary)) => self.finish(event_loop, Ok(summary)),
            Err(error) => self.finish(event_loop, Err(error)),
            Ok(None) => {
                // Let real resize/minimize events arrive without deriving any clock from time.
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(1),
                ));
            }
        }
    }
}

fn main() -> Result<()> {
    // This executable owns the real main thread. The subprocess test never creates an EventLoop.
    let event_loop = EventLoop::new()?;
    let mut app = NativeApp::default();
    event_loop.run_app(&mut app)?;
    let summary = app
        .result
        .ok_or_else(|| io::Error::other("the native event loop exited without a terminal result"))?
        .map_err(io::Error::other)?;
    println!("{summary}");
    Ok(())
}
