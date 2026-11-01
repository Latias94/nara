//! Native publication regressions use ordinary policy graphs and actual Host submissions.

use super::*;
use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDeclaration,
    WgpuFeatureDefinition, WgpuFeatureEncodeContext, WgpuFeaturePrepareContext, WgpuHistoryLineage,
    WgpuHistoryTexture, WgpuHistoryView, WgpuPolicyBindingContext, WgpuPolicyBuildContext,
    WgpuPolicyDefinition, WgpuPolicyFrameFacts, WgpuPolicyProgram, WgpuProgramTextureDescriptor,
    WgpuRenderFeature, WgpuRenderPolicy, WgpuRenderProgramBuilder, WgpuRenderProgramError,
};
use nara_core::Mat3;
use nara_ecs::Entity;
use nara_render::{
    Color, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, RenderViewId, ViewLoadOp,
    ViewMapping2d, ViewportRect, build_image_render_frame_packet,
};
use std::sync::{Mutex, atomic::AtomicUsize};

struct FinalColor;
struct HistoryIdentity;

impl WgpuRenderFeature for HistoryIdentity {
    type Capture = ();
    type State = ();
    type Prepared = ();
    type Report = ();

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        panic!("a history identity must not execute a native Feature")
    }

    fn prepare(
        &self,
        _: &mut (),
        _: &(),
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        unreachable!("the policy only initializes its graph-local final texture")
    }

    fn encode(
        &self,
        _: &(),
        _: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        unreachable!("the policy only initializes its graph-local final texture")
    }
}

#[derive(Default)]
struct CaptureProbe {
    panic_on_binding_drop: AtomicBool,
    binding_drops: AtomicUsize,
    predecessors: Mutex<Vec<Option<u64>>>,
}

struct ClearCapture {
    lineage: WgpuHistoryLineage,
    view: WgpuHistoryView,
    effect: WgpuFeatureDeclaration<HistoryIdentity>,
    extra_usage: wgpu::TextureUsages,
    color: wgpu::Color,
    probe: Arc<CaptureProbe>,
}

struct ClearBindings {
    history: WgpuHistoryTexture<FinalColor>,
    probe: Arc<CaptureProbe>,
}

impl Drop for ClearBindings {
    fn drop(&mut self) {
        self.probe.binding_drops.fetch_add(1, Ordering::SeqCst);
        assert!(
            !self
                .probe
                .panic_on_binding_drop
                .swap(false, Ordering::SeqCst),
            "injected retired policy Bindings destructor panic"
        );
    }
}

struct ClearPolicy;

impl WgpuRenderPolicy for ClearPolicy {
    type Capture = ClearCapture;
    type Bindings = ClearBindings;

    fn history_lineage(&self, capture: &ClearCapture) -> Option<WgpuHistoryLineage> {
        Some(capture.lineage.clone())
    }

    fn bind(
        &self,
        capture: &ClearCapture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<ClearBindings, WgpuRenderProgramError> {
        Ok(ClearBindings {
            history: context.history_texture(
                &capture.view,
                &capture.effect,
                0,
                final_descriptor(capture.extra_usage),
            )?,
            probe: Arc::clone(&capture.probe),
        })
    }

    fn build(
        &self,
        bindings: &ClearBindings,
        capture: &ClearCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        capture.probe.predecessors.lock().unwrap().push(
            context
                .previous_texture(&bindings.history)?
                .map(|previous| previous.frame_index()),
        );
        let mut builder = WgpuRenderProgramBuilder::new(
            context.packet().source_generation(),
            context.packet().frame_index(),
            GraphLimits::default(),
        )?;
        let texture = builder.texture::<FinalColor>(final_descriptor(capture.extra_usage))?;
        builder.initialize(0, &[texture.clear(capture.color, AccessRole::Attachment)])?;
        Ok(WgpuPolicyProgram::new(
            builder.finish(&[texture.output()])?,
            &texture,
            WgpuPolicyFrameFacts::default(),
        )?
        .with_history(vec![bindings.history.bind(&texture)]))
    }
}

fn final_descriptor(extra_usage: wgpu::TextureUsages) -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("publication-regression-final"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | extra_usage,
            view_formats: &[],
        },
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    }
}

fn image_target() -> RenderImagePacket {
    RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(2, 2).unwrap(),
    )
    .unwrap()
}

struct Fixture {
    installation: crate::WgpuPolicyInstallation<ClearPolicy>,
    lineage: WgpuHistoryLineage,
    view: WgpuHistoryView,
    effect: WgpuFeatureDeclaration<HistoryIdentity>,
    target: RenderImagePacket,
    extra_usage: wgpu::TextureUsages,
}

impl Fixture {
    fn new(extra_usage: wgpu::TextureUsages) -> Self {
        Self {
            installation: WgpuPolicyDefinition::new(ClearPolicy).install().unwrap(),
            lineage: WgpuHistoryLineage::new(RenderFrameSourceGeneration::try_acquire().unwrap())
                .unwrap(),
            view: WgpuHistoryView::new(RenderViewId::try_acquire().unwrap()),
            effect: WgpuFeatureDefinition::new(HistoryIdentity).declare(),
            target: image_target(),
            extra_usage,
        }
    }

    fn input(&self, frame: u64, color: wgpu::Color) -> (crate::WgpuRenderInput, Arc<CaptureProbe>) {
        let viewport = ViewportRect::new(0, 0, 2, 2).unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: self.view.id(),
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(self.target.image()),
            viewport,
            mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        });
        let packet = build_image_render_frame_packet(
            self.lineage.source_generation(),
            frame,
            &views,
            self.target,
        )
        .unwrap()
        .unwrap();
        let probe = Arc::new(CaptureProbe::default());
        let capture = Arc::new(ClearCapture {
            lineage: self.lineage.clone(),
            view: self.view.clone(),
            effect: self.effect.clone(),
            extra_usage: self.extra_usage,
            color,
            probe: Arc::clone(&probe),
        });
        (self.installation.capture(packet, capture).unwrap(), probe)
    }

    fn activate(&self, host: &mut WgpuRenderHost, producer: &WgpuRenderProducer) {
        let reservation = host
            .reserve_image_target(producer, self.lineage.source_generation(), self.target)
            .unwrap();
        host.activate_image_target(producer, reservation).unwrap();
    }
}

struct LastCapture {
    panic_on_drop: Arc<AtomicBool>,
    drops: Arc<AtomicUsize>,
}

impl Drop for LastCapture {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        assert!(
            !self.panic_on_drop.swap(false, Ordering::SeqCst),
            "injected last policy Capture destructor panic"
        );
    }
}

struct CaptureDropPolicy;

impl WgpuRenderPolicy for CaptureDropPolicy {
    type Capture = LastCapture;
    type Bindings = ();

    fn bind(
        &self,
        _: &LastCapture,
        _: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn build(
        &self,
        _: &(),
        _: &LastCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut builder = WgpuRenderProgramBuilder::new(
            context.packet().source_generation(),
            context.packet().frame_index(),
            GraphLimits::default(),
        )?;
        let texture =
            builder.texture::<FinalColor>(final_descriptor(wgpu::TextureUsages::empty()))?;
        builder.initialize(
            0,
            &[texture.clear(wgpu::Color::WHITE, AccessRole::Attachment)],
        )?;
        WgpuPolicyProgram::new(
            builder.finish(&[texture.output()])?,
            &texture,
            WgpuPolicyFrameFacts::default(),
        )
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; unavailable hardware fails the prerequisite"]
fn native_published_output_survives_old_binding_drop_panic_without_reusing_its_initial_storage() {
    let mut fixture = Fixture::new(wgpu::TextureUsages::empty());
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    host.begin_frame(1).unwrap();
    fixture.activate(&mut host, &producer);

    let (input_a, probe_a) = fixture.input(1, wgpu::Color::RED);
    let candidate_a = host.prepare(&producer, &input_a).unwrap();
    let published_a = host.submit_candidate(&producer, candidate_a).unwrap();
    assert!(!published_a.outputs().cleanup_panicked());
    assert_eq!(*probe_a.predecessors.lock().unwrap(), [None]);
    drop(input_a);
    drain_prefix(published_a.outputs().submitted_prefix());
    drop(published_a);
    // The moved candidate and its completed native custody no longer retain A's Bindings.
    // Only the latest published policy snapshot must keep that original owner alive.
    assert_eq!(probe_a.binding_drops.load(Ordering::SeqCst), 0);

    host.retire_image_target(&producer);
    fixture.target = image_target();
    fixture.activate(&mut host, &producer);
    let initial_texture = host.views[&producer.id()]
        .target
        .as_ref()
        .unwrap()
        .initial_storage
        .as_ref()
        .unwrap()
        .0
        .clone();
    let (input_b, probe_b) = fixture.input(2, wgpu::Color::GREEN);
    let candidate_b = host.prepare(&producer, &input_b).unwrap();
    assert_eq!(*probe_b.predecessors.lock().unwrap(), [Some(1)]);
    probe_a.panic_on_binding_drop.store(true, Ordering::SeqCst);
    let result = host.submit_candidate(&producer, candidate_b);
    // Disarm before asserting so a failed regression cannot panic again during Host teardown.
    probe_a.panic_on_binding_drop.store(false, Ordering::SeqCst);
    let published_b = result.expect("post-publication cleanup must preserve the successful output");

    assert_eq!(probe_a.binding_drops.load(Ordering::SeqCst), 1);
    assert!(published_b.outputs().cleanup_panicked());
    assert_eq!(host.cleanup_fault_count(), 1);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(2)
    );
    assert!(
        host.views[&producer.id()]
            .target
            .as_ref()
            .unwrap()
            .initial_storage
            .is_none()
    );
    assert_eq!(published_b.image().0.texture(), &initial_texture);
    let final_b = published_b.final_texture::<FinalColor>().unwrap();
    let history_b = published_b
        .history_texture::<FinalColor, _>(fixture.view.id(), &fixture.effect, 0)
        .unwrap();
    assert!(Arc::ptr_eq(&final_b.storage, &history_b.storage));
    assert_eq!(history_b.frame_index(), 2);
    assert_eq!(read_pixel(&mut host, published_b.image()), [0, 255, 0, 255]);
    assert!(matches!(
        host.prepare(&producer, &input_b).unwrap_err(),
        WgpuRenderHostError::Backend(WgpuRenderError::FrameAlreadyConsumed { frame_index: 2 })
    ));
    let image_b = published_b.into_image();
    let image_texture_b = image_b.texture::<FinalColor>().unwrap();
    assert!(Arc::ptr_eq(&final_b.storage, &image_texture_b.storage));
    assert!(matches!(
        image_b.texture::<crate::WgpuWorldColor>(),
        Err(WgpuRenderProgramError::ResourceSemantic)
    ));

    let (input_c, probe_c) = fixture.input(3, wgpu::Color::BLUE);
    let candidate_c = host.prepare(&producer, &input_c).unwrap();
    assert_eq!(*probe_c.predecessors.lock().unwrap(), [Some(2)]);
    let published_c = host.submit_candidate(&producer, candidate_c).unwrap();
    assert!(!published_c.outputs().cleanup_panicked());
    assert_eq!(host.cleanup_fault_count(), 1);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(3)
    );
    assert_ne!(image_b.0.texture(), published_c.image().0.texture());
    assert_eq!(read_pixel(&mut host, published_c.image()), [0, 0, 255, 255]);
    assert_eq!(read_pixel(&mut host, &image_b), [0, 255, 0, 255]);
    assert_eq!(image_texture_b.frame_index(), 2);
    assert_eq!(history_b.frame_index(), 2);
    assert_eq!(probe_a.binding_drops.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; unavailable hardware fails the prerequisite"]
fn native_final_storage_usage_replaces_incompatible_initial_storage_on_the_first_submission() {
    let fixture = Fixture::new(wgpu::TextureUsages::STORAGE_BINDING);
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    host.begin_frame(1).unwrap();
    fixture.activate(&mut host, &producer);
    let (_, _, reservation) = host.views[&producer.id()]
        .target
        .as_ref()
        .unwrap()
        .initial_storage
        .as_ref()
        .unwrap();
    let initial_reservation = Arc::downgrade(reservation);
    assert!(
        !host.views[&producer.id()]
            .target
            .as_ref()
            .unwrap()
            .initial_storage
            .as_ref()
            .unwrap()
            .0
            .usage()
            .contains(wgpu::TextureUsages::STORAGE_BINDING)
    );

    let (first_input, first_probe) = fixture.input(1, wgpu::Color::RED);
    let first_candidate = host.prepare(&producer, &first_input).unwrap();
    let first = host
        .submit_candidate(&producer, first_candidate)
        .expect("a legal storage-capable final descriptor must succeed on the fresh target");
    assert!(!first.outputs().cleanup_panicked());
    assert_eq!(*first_probe.predecessors.lock().unwrap(), [None]);
    assert!(
        host.views[&producer.id()]
            .target
            .as_ref()
            .unwrap()
            .initial_storage
            .is_none()
    );
    assert!(
        initial_reservation.upgrade().is_none(),
        "incompatible target storage must be released"
    );
    let required_usage = final_descriptor(wgpu::TextureUsages::STORAGE_BINDING)
        .native
        .usage;
    assert!(first.image().0.texture().usage().contains(required_usage));
    assert_eq!(read_pixel(&mut host, first.image()), [255, 0, 0, 255]);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );

    let (second_input, second_probe) = fixture.input(2, wgpu::Color::BLUE);
    let second_candidate = host.prepare(&producer, &second_input).unwrap();
    assert_eq!(*second_probe.predecessors.lock().unwrap(), [Some(1)]);
    let second = host.submit_candidate(&producer, second_candidate).unwrap();
    assert!(!second.outputs().cleanup_panicked());
    assert!(second.image().0.texture().usage().contains(required_usage));
    assert_ne!(first.image().0.texture(), second.image().0.texture());
    assert_eq!(read_pixel(&mut host, second.image()), [0, 0, 255, 255]);
    assert_eq!(read_pixel(&mut host, first.image()), [255, 0, 0, 255]);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(2)
    );
    assert_eq!(host.cleanup_fault_count(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; unavailable hardware fails the prerequisite"]
fn native_published_output_survives_last_capture_drop_when_the_consumed_candidate_is_released() {
    let source = RenderFrameSourceGeneration::try_acquire().unwrap();
    let target = image_target();
    let viewport = ViewportRect::new(0, 0, 2, 2).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Image(target.image()),
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    let packet = || {
        build_image_render_frame_packet(source, 1, &views, target)
            .unwrap()
            .unwrap()
    };
    let installation = WgpuPolicyDefinition::new(CaptureDropPolicy)
        .install()
        .unwrap();
    let panic_on_drop = Arc::new(AtomicBool::new(false));
    let drops = Arc::new(AtomicUsize::new(0));
    let capture = Arc::new(LastCapture {
        panic_on_drop: Arc::clone(&panic_on_drop),
        drops: Arc::clone(&drops),
    });
    let weak_capture = Arc::downgrade(&capture);
    let input = installation.capture(packet(), capture).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    host.begin_frame(1).unwrap();
    let reservation = host
        .reserve_image_target(&producer, source, target)
        .unwrap();
    host.activate_image_target(&producer, reservation).unwrap();
    let candidate = host.prepare(&producer, &input).unwrap();
    drop(input);
    assert_eq!(weak_capture.strong_count(), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(host.cleanup_fault_count(), 0);

    // Drain the actual native prefix before release; timing must not decide whether the
    // candidate or a later GPU callback releases the final Capture reference. Bindings is ().
    crate::candidate::COMPLETE_BEFORE_RELEASE.with(|complete| complete.set(true));
    panic_on_drop.store(true, Ordering::SeqCst);
    let submitted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.submit_candidate(&producer, candidate)
    }));
    crate::candidate::COMPLETE_BEFORE_RELEASE.with(|complete| complete.set(false));
    panic_on_drop.store(false, Ordering::SeqCst);
    let published = submitted
        .expect("the public Host submission must isolate the consumed Capture destructor panic")
        .expect("consumed candidate cleanup must preserve the successful publication");

    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(weak_capture.upgrade().is_none());
    assert!(published.outputs().submitted_prefix().is_complete());
    assert!(published.outputs().cleanup_panicked());
    assert_eq!(host.cleanup_fault_count(), 1);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    assert!(
        host.views[&producer.id()]
            .target
            .as_ref()
            .unwrap()
            .initial_storage
            .is_none()
    );
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
    assert_eq!(
        read_pixel(&mut host, published.image()),
        [255, 255, 255, 255]
    );

    let replay = installation
        .capture(
            packet(),
            Arc::new(LastCapture {
                panic_on_drop: Arc::new(AtomicBool::new(false)),
                drops: Arc::new(AtomicUsize::new(0)),
            }),
        )
        .unwrap();
    assert!(matches!(
        host.prepare(&producer, &replay).unwrap_err(),
        WgpuRenderHostError::Backend(WgpuRenderError::FrameAlreadyConsumed { frame_index: 1 })
    ));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(host.cleanup_fault_count(), 1);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; unavailable hardware fails the prerequisite"]
fn native_submission_cleanup_observation_survives_host_token_retirement() {
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let namespace = host.prepare_namespace(None).unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let reservation = crate::submission::WgpuSubmissionReservation::new(
        &namespace.device,
        &namespace.queue,
        LastCapture {
            panic_on_drop: Arc::new(AtomicBool::new(true)),
            drops: Arc::clone(&drops),
        },
    );
    let prefix = reservation.submitted(namespace.queue.submit([]));
    host.retiring_prefixes.push(prefix.clone());
    // This tests the observer handoff, not output publication. The wrapper retains the same
    // actual submission token used by complete groups; no physical completion is fabricated.
    let outputs = crate::WgpuProgramOutputs {
        outputs: Vec::new(),
        reports: Vec::new(),
        prefix,
        cleanup_panicked: false,
    };
    drain_prefix(outputs.submitted_prefix());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(outputs.cleanup_panicked());
    assert!(!outputs.submitted_prefix().callback_abandoned());
    assert_eq!(
        host.cleanup_fault_count(),
        1,
        "visible before Host maintenance"
    );
    assert_eq!(host.retiring_prefixes.len(), 1);

    host.maintain_producers().unwrap();
    assert!(host.retiring_prefixes.is_empty());
    assert_eq!(
        host.cleanup_fault_count(),
        1,
        "transfer before removing the token"
    );
    assert_eq!(host.abandoned_submission_count(), 0);
    assert!(
        outputs.cleanup_panicked(),
        "original output observation survives retirement"
    );
    assert_eq!(outputs.submitted_prefix().cleanup_fault_count(), 1);
    host.maintain_producers().unwrap();
    assert_eq!(
        host.cleanup_fault_count(),
        1,
        "maintenance does not count a prefix twice"
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

fn drain_prefix(prefix: &WgpuSubmissionPrefix) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !prefix.poll().unwrap() {
        assert!(
            std::time::Instant::now() < deadline,
            "native publication prefix did not settle"
        );
        std::thread::yield_now();
    }
}

fn read_pixel(host: &mut WgpuRenderHost, output: &WgpuImageOutput) -> [u8; 4] {
    let ((device, readback), prefix) = host
        .submit_image_use(std::slice::from_ref(output), |device, _, encoder, views| {
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("publication-regression-readback"),
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                views[0].texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            Ok((device.clone(), readback))
        })
        .unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    drain_prefix(&prefix);
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let pixel = {
        let bytes = readback.get_mapped_range(..).unwrap();
        bytes[..4].try_into().unwrap()
    };
    readback.unmap();
    pixel
}
