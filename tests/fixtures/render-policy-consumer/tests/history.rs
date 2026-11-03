#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

// Deliberately independent of support/mod.rs: the ordinary renderer is migrated separately.
#[path = "support/accumulation.rs"]
mod accumulation;
#[path = "support/history_readback.rs"]
mod history_readback;
#[path = "support/motion.rs"]
mod motion;

use std::sync::Arc;

use nara::core::{Color, Mat3};
use nara_ecs::Entity;
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
    RenderImagePacket, RenderTarget, RenderViewId, ViewLoadOp, ViewMapping2d, ViewportRect,
    build_image_render_frame_packet,
};
use nara_render_wgpu::{
    AccessRole, WgpuDeviceRequirements, WgpuEmptyHistoryReason, WgpuHistoryLineage,
    WgpuHistoryReset, WgpuHistoryTexture, WgpuHistoryView, WgpuPolicyBindingContext,
    WgpuPolicyBuildContext, WgpuPolicyDefinition, WgpuPolicyFrameFacts, WgpuPolicyInstallation,
    WgpuPolicyProgram, WgpuProgramTextureDescriptor, WgpuProgramTextureOutput, WgpuPublishedFrame,
    WgpuRenderError, WgpuRenderHost, WgpuRenderHostError, WgpuRenderInput, WgpuRenderPolicy,
    WgpuRenderProducer, WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

use accumulation::{
    Effect, FinalColor, HEIGHT, HISTORY_SLOT, HistoryColor, HistoryPolicy, Predecessor, Topology,
    VIEW_WIDTH, WIDTH, history_descriptor, limits,
};
use history_readback::{Reader, Samples, finish, require_native_selection, settle};

const UNINITIALIZED: Predecessor = Predecessor::Empty(WgpuEmptyHistoryReason::Uninitialized);
const RESET: Predecessor = Predecessor::Empty(WgpuEmptyHistoryReason::Reset);
const RECREATED: Predecessor = Predecessor::Empty(WgpuEmptyHistoryReason::DeviceRecreated);
const RESOURCE_LIMIT: u64 = 8 * 1024 * 1024;

struct Stream {
    producer: WgpuRenderProducer,
    source: RenderFrameSourceGeneration,
    target: RenderImagePacket,
    lineage: WgpuHistoryLineage,
    topology: Topology,
    installation: WgpuPolicyInstallation<HistoryPolicy>,
}

impl Stream {
    fn new(host: &mut WgpuRenderHost, definition: &WgpuPolicyDefinition<HistoryPolicy>) -> Self {
        assert_eq!(
            definition.device_requirements(),
            &WgpuDeviceRequirements::default(),
            "unfilterable RGBA32Float history must fit the closed default device request"
        );
        let producer = host.create_producer().unwrap();
        host.prepare_policy(&producer, definition).unwrap();
        let source = RenderFrameSourceGeneration::try_acquire().unwrap();
        Self {
            producer,
            source,
            target: RenderImagePacket::new(
                RenderImage2d::allocate_handle().unwrap(),
                Extent2d::new(WIDTH, HEIGHT).unwrap(),
            )
            .unwrap(),
            lineage: WgpuHistoryLineage::new(source).unwrap(),
            topology: Topology::new(),
            installation: definition.install().unwrap(),
        }
    }

    fn admit(&self, host: &mut WgpuRenderHost) {
        admit(host, &self.producer, self.source, self.target);
    }

    fn input(
        &self,
        frame: u64,
        phase: u32,
        reverse: bool,
        predecessors: [Predecessor; 4],
    ) -> WgpuRenderInput {
        let capture = self
            .topology
            .capture(&self.lineage, phase, reverse, predecessors);
        self.installation
            .capture(capture.packet(frame, self.target), capture)
            .unwrap()
    }

    fn submit(&self, host: &mut WgpuRenderHost, input: &WgpuRenderInput) -> WgpuPublishedFrame {
        let candidate = host.prepare(&self.producer, input).unwrap();
        let published = host
            .submit_candidate(&self.producer, candidate)
            .expect("one Host transaction must publish all four histories and the final image");
        assert_receipt(&published, input);
        published
    }

    fn reset(&self, host: &mut WgpuRenderHost, slots: &[(usize, usize)]) {
        let requests: Vec<_> = slots
            .iter()
            .map(|&(view, effect)| {
                WgpuHistoryReset::texture::<HistoryColor, _>(
                    &self.topology.views[view].history_view,
                    &self.topology.views[view].effects[effect].declaration,
                    HISTORY_SLOT,
                    history_descriptor(),
                )
            })
            .collect();
        host.reset_history(&self.producer, &self.lineage, &requests)
            .unwrap();
    }

    fn record(&self, published: WgpuPublishedFrame, oracle: &Oracle) -> Record {
        Record {
            published,
            source: self.source,
            topology: self.topology.clone(),
            oracle: oracle.clone(),
        }
    }

    fn retire(&self, host: &mut WgpuRenderHost) {
        if let Some(prefix) = host.retire_producer(&self.producer) {
            settle(&prefix);
        }
    }
}

struct Record {
    published: WgpuPublishedFrame,
    source: RenderFrameSourceGeneration,
    topology: Topology,
    oracle: Oracle,
}

/// CPU-only double-precision state. It never reads a GPU predecessor or reuses shader output.
#[derive(Clone)]
struct Oracle {
    histories: [Vec<[f64; 4]>; 4],
}

impl Oracle {
    fn zero() -> Self {
        Self {
            histories: std::array::from_fn(|_| vec![[0.0; 4]; (VIEW_WIDTH * HEIGHT) as usize]),
        }
    }

    fn reset(&mut self, view: usize, effect: usize) {
        self.histories[2 * view + effect].fill([0.0; 4]);
    }

    fn advance(&mut self, topology: &Topology, phase: u32) {
        for view in &topology.views {
            for effect in &view.effects {
                let history = &mut self.histories[2 * view.index + effect.ordinal];
                for y in 0..HEIGHT {
                    for x in 0..VIEW_WIDTH {
                        let red = (x + phase + 3 * effect.tag) % 8;
                        let green = (y + 2 * phase + effect.tag) % 8;
                        let blue = (x + y + 3 * phase + 2 * effect.tag) % 8;
                        let current =
                            [red as f64 / 8.0, green as f64 / 8.0, blue as f64 / 8.0, 1.0];
                        let previous = &mut history[(y * VIEW_WIDTH + x) as usize];
                        for (stored, sample) in previous.iter_mut().zip(current) {
                            *stored = sample / 4.0 + 3.0 * *stored / 4.0;
                        }
                    }
                }
            }
        }
    }

    fn assert_samples(&self, samples: &Samples) {
        for (slot, (actual, expected)) in samples.histories.iter().zip(&self.histories).enumerate()
        {
            assert_eq!(actual.len(), (VIEW_WIDTH * HEIGHT) as usize);
            for (pixel, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                for channel in 0..4 {
                    assert!(actual[channel].is_finite());
                    assert!(
                        (f64::from(actual[channel]) - expected[channel]).abs() <= 0.000_003,
                        "history {slot}, pixel ({}, {}), channel {channel}: actual {}, expected {}",
                        pixel % VIEW_WIDTH as usize,
                        pixel / VIEW_WIDTH as usize,
                        actual[channel],
                        expected[channel]
                    );
                }
            }
        }
        assert_eq!(samples.image.len(), (WIDTH * HEIGHT) as usize);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let view = (x / VIEW_WIDTH) as usize;
                let effect = usize::from(y >= HEIGHT / 2);
                let expected =
                    self.histories[2 * view + effect][(y * VIEW_WIDTH + x % VIEW_WIDTH) as usize];
                let actual = samples.image[(y * WIDTH + x) as usize];
                for channel in 0..3 {
                    let expected = (expected[channel] * 255.0).round() as u8;
                    assert!(
                        actual[channel].abs_diff(expected) <= 1,
                        "final image ({x}, {y}), channel {channel}: actual {}, expected {expected}",
                        actual[channel]
                    );
                }
                assert_eq!(actual[3], 255, "the composed image is opaque");
            }
        }
    }
}

fn begin(host: &mut WgpuRenderHost, frame: u64) {
    host.begin_frame(frame)
        .expect("the selected native adapter is a required prerequisite, never a passing skip");
    let adapter = host
        .runtime_facts()
        .adapter
        .expect("actual native adapter facts must exist");
    assert_ne!(
        adapter.device_type(),
        "cpu",
        "native hardware qualification excludes a CPU adapter"
    );
    assert_eq!(host.frame_transaction_stats().surface_acquire_attempts(), 0);
    assert_eq!(host.frame_transaction_stats().presents(), 0);
}

fn admit(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    source: RenderFrameSourceGeneration,
    target: RenderImagePacket,
) {
    let reservation = host.reserve_image_target(producer, source, target).unwrap();
    host.activate_image_target(producer, reservation).unwrap();
}

fn assert_receipt(published: &WgpuPublishedFrame, input: &WgpuRenderInput) {
    assert_eq!(
        published.image().receipt().source_generation(),
        input.packet().source_generation()
    );
    assert_eq!(
        published.image().receipt().render_transaction_frame_index(),
        input.packet().frame_index()
    );
    assert_eq!(published.image().extent(), input.packet().target().extent());
}

fn assert_record(host: &mut WgpuRenderHost, reader: &mut Reader, record: &Record) {
    let receipt = record.published.image().receipt();
    assert_eq!(receipt.source_generation(), record.source);
    let histories: [WgpuProgramTextureOutput<HistoryColor>; 4] = std::array::from_fn(|index| {
        let view = &record.topology.views[index / 2];
        let effect = &view.effects[index % 2];
        let output = record
            .published
            .history_texture::<HistoryColor, _>(view.id, &effect.declaration, HISTORY_SLOT)
            .expect("retrieve this publication's exact original View/effect/slot output");
        assert_eq!(output.descriptor(), history_descriptor());
        assert_eq!(output.source_generation(), record.source);
        assert_eq!(
            output.frame_index(),
            receipt.render_transaction_frame_index()
        );
        assert_eq!(output.producer_id(), receipt.producer_id());
        output
    });
    let samples = reader.read(host, &histories, record.published.image());
    record.oracle.assert_samples(&samples);
}

fn assert_consumed(error: WgpuRenderHostError) {
    assert!(
        matches!(
            error,
            WgpuRenderHostError::Backend(WgpuRenderError::FrameAlreadyConsumed { .. })
                | WgpuRenderHostError::Program(WgpuRenderProgramError::HistoryAlreadyConsumed)
        ),
        "an already published capture stays consumed after reset: {error}"
    );
}

struct MotionPolicy(HistoryPolicy);

struct MotionCapture {
    history: Arc<accumulation::HistoryCapture>,
    sample: motion::MotionFrame,
}

impl WgpuRenderPolicy for MotionPolicy {
    type Capture = MotionCapture;
    type Bindings = accumulation::HistoryBindings;

    fn history_lineage(&self, capture: &Self::Capture) -> Option<WgpuHistoryLineage> {
        Some(capture.history.lineage.clone())
    }

    fn bind(
        &self,
        capture: &Self::Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        self.0.bind(&capture.history, context)
    }

    fn validate_capture(
        &self,
        packet: &nara_render::RenderFramePacket,
        capture: &Self::Capture,
    ) -> Result<(), WgpuRenderProgramError> {
        assert_eq!(capture.history.phase, capture.sample.phase());
        assert_eq!(
            packet.source_generation().runtime_generation(),
            capture.sample.stamp.runtime_generation
        );
        assert_eq!(
            capture.sample.stamp.phase,
            nara::transform::SpatialSnapshotPhase::Presented
        );
        self.0.validate_capture(packet, &capture.history)
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        self.0.build(bindings, &capture.history, context)
    }
}

fn motion_input(
    installation: &WgpuPolicyInstallation<MotionPolicy>,
    lineage: &WgpuHistoryLineage,
    topology: &Topology,
    target: RenderImagePacket,
    frame: u64,
    sample: motion::MotionFrame,
    predecessors: [Predecessor; 4],
) -> WgpuRenderInput {
    let history = topology.capture(lineage, sample.phase(), false, predecessors);
    let packet = history.packet(frame, target);
    installation
        .capture(packet, Arc::new(MotionCapture { history, sample }))
        .unwrap()
}

#[test]
#[ignore = "requires selected native hardware/backend; adapter acquisition failure fails, never skips"]
fn motion_capture_discontinuity_fences_late_preparation_and_preserves_other_view_and_submission() {
    use nara::core::Vec2;
    use nara_app::FixedTime;
    use std::time::Duration;

    require_native_selection();
    let mut motion = motion::MotionRuntime::new();
    let zero = motion.drive(Duration::ZERO);
    assert_eq!(zero.stamp.tick, 0);
    let multiple = motion.drive(FixedTime::DEFAULT_TIMESTEP * 2 + FixedTime::DEFAULT_TIMESTEP / 2);
    assert_eq!(multiple.stamp.tick, 2);
    assert!((multiple.interpolation_alpha - 0.5).abs() < 1e-5);
    assert!((multiple.kinematic.x - 0.375).abs() < 1e-5);
    let no_tick = motion.drive(Duration::ZERO);
    assert_eq!(no_tick.stamp.tick, multiple.stamp.tick);
    assert_eq!(no_tick.virtual_elapsed, multiple.virtual_elapsed);
    assert_eq!(no_tick.kinematic, multiple.kinematic);

    let source = RenderFrameSourceGeneration::from(multiple.stamp.runtime_generation.unwrap());
    let mut lineage = WgpuHistoryLineage::new(source).unwrap();
    let topology = Topology::new();
    let definition = WgpuPolicyDefinition::new(MotionPolicy(HistoryPolicy::new()));
    let installation = definition.install().unwrap();
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT);
    let producer = host.create_producer().unwrap();
    host.prepare_policy(&producer, &definition).unwrap();
    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(WIDTH, HEIGHT).unwrap(),
    )
    .unwrap();
    let other_definition = WgpuPolicyDefinition::new(HistoryPolicy::new());
    let other = Stream::new(&mut host, &other_definition);
    let mut reader = Reader::new(&mut host);
    begin(&mut host, 1);
    admit(&mut host, &producer, source, target);
    other.admit(&mut host);
    let first_input = motion_input(
        &installation,
        &lineage,
        &topology,
        target,
        1,
        multiple,
        [UNINITIALIZED; 4],
    );
    let first = host.prepare(&producer, &first_input).unwrap();
    let first = host.submit_candidate(&producer, first).unwrap();
    let original_prefix = first.outputs().submitted_prefix().clone();
    let mut oracle = Oracle::zero();
    oracle.advance(&topology, multiple.phase());
    let first = Record {
        published: first,
        source,
        topology: topology.clone(),
        oracle: oracle.clone(),
    };
    let other_input = other.input(1, 3, false, [UNINITIALIZED; 4]);
    let mut other_oracle = Oracle::zero();
    other_oracle.advance(&other.topology, 3);
    let other_first = other.record(other.submit(&mut host, &other_input), &other_oracle);

    let unprepared = motion_input(
        &installation,
        &lineage,
        &topology,
        target,
        2,
        no_tick,
        [Predecessor::Frame(1); 4],
    );
    let prepared_input = motion_input(
        &installation,
        &lineage,
        &topology,
        target,
        3,
        no_tick,
        [Predecessor::Frame(1); 4],
    );
    let prepared = host.prepare(&producer, &prepared_input).unwrap();
    let raw_late = Arc::new(MotionCapture {
        history: topology.capture(&lineage, no_tick.phase(), false, [Predecessor::Frame(1); 4]),
        sample: no_tick,
    });
    let raw_packet = raw_late.history.packet(3, target);
    motion.pause();
    let paused = motion.drive(Duration::from_secs(1));
    assert!(paused.paused);
    assert_eq!(paused.virtual_elapsed, multiple.virtual_elapsed);
    assert!(paused.real_elapsed > no_tick.real_elapsed);
    assert!((paused.kinematic.x - 0.5).abs() < 1e-5);

    lineage.discontinue().unwrap();
    let resets = topology.views[0]
        .effects
        .iter()
        .map(|effect| {
            WgpuHistoryReset::texture::<HistoryColor, _>(
                &topology.views[0].history_view,
                &effect.declaration,
                HISTORY_SLOT,
                history_descriptor(),
            )
        })
        .collect::<Vec<_>>();
    host.reset_history(&producer, &lineage, &resets).unwrap();
    assert!(matches!(
        host.prepare(&producer, &unprepared).unwrap_err(),
        WgpuRenderHostError::Program(WgpuRenderProgramError::StaleHistory)
    ));
    assert!(matches!(
        installation.capture(raw_packet, raw_late),
        Err(WgpuRenderProgramError::StaleHistory)
    ));
    let refused = host.submit_candidate(&producer, prepared).unwrap_err();
    assert!(matches!(
        refused.error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::StaleHistory)
    ));
    assert!(!refused.is_retryable());
    assert!(std::ptr::eq(
        refused
            .candidate
            .input()
            .captured::<MotionPolicy>()
            .unwrap(),
        prepared_input.captured::<MotionPolicy>().unwrap()
    ));
    drop(refused);
    assert_eq!(
        first_input.captured::<MotionPolicy>().unwrap().sample,
        multiple
    );
    settle(&original_prefix);
    assert_record(&mut host, &mut reader, &first);

    motion.correct_and_reparent(Vec2::new(8.0, 2.0));
    let (exact_stamp, exact_poses) = motion.exact_step();
    assert_eq!(exact_stamp.tick, 3);
    assert_eq!(
        exact_stamp.phase,
        nara::transform::SpatialSnapshotPhase::Fixed
    );
    assert!((exact_poses[0].x - 8.25).abs() < 1e-5);
    let corrected = motion.drive(Duration::ZERO);
    assert_eq!(corrected.stamp.tick, 3);
    assert_eq!(corrected.stamp.frame, exact_stamp.frame + 1);
    assert!(corrected.paused);
    assert!((corrected.kinematic.x - 8.25).abs() < 1e-5);
    assert_eq!(corrected.fixed_positions, exact_poses);
    assert_eq!(corrected.kinematic, exact_poses[0]);
    assert_eq!(corrected.dynamic, exact_poses[1]);
    begin(&mut host, 2);
    let post_cut = motion_input(
        &installation,
        &lineage,
        &topology,
        target,
        4,
        corrected,
        [RESET, RESET, Predecessor::Frame(1), Predecessor::Frame(1)],
    );
    let candidate = host.prepare(&producer, &post_cut).unwrap();
    let published = host.submit_candidate(&producer, candidate).unwrap();
    oracle.reset(0, 0);
    oracle.reset(0, 1);
    oracle.advance(&topology, corrected.phase());
    let post_cut = Record {
        published,
        source,
        topology: topology.clone(),
        oracle: oracle.clone(),
    };
    let other_input = other.input(2, 6, false, [Predecessor::Frame(1); 4]);
    other_oracle.advance(&other.topology, 6);
    let other_second = other.record(other.submit(&mut host, &other_input), &other_oracle);

    let same_tick = motion.drive(Duration::ZERO);
    assert_eq!(same_tick.stamp.tick, corrected.stamp.tick);
    assert_eq!(same_tick.virtual_elapsed, corrected.virtual_elapsed);
    assert_eq!(same_tick.phase(), corrected.phase());
    let next_input = motion_input(
        &installation,
        &lineage,
        &topology,
        target,
        5,
        same_tick,
        [Predecessor::Frame(4); 4],
    );
    let next = host.prepare(&producer, &next_input).unwrap();
    let next = host.submit_candidate(&producer, next).unwrap();
    oracle.advance(&topology, same_tick.phase());
    let next = Record {
        published: next,
        source,
        topology,
        oracle,
    };
    for record in [&first, &post_cut, &next, &other_first, &other_second] {
        assert_record(&mut host, &mut reader, record);
    }
    assert_consumed(host.prepare(&producer, &next_input).unwrap_err());
    settle(&original_prefix);
    settle(&host.retire_producer(&producer).unwrap());
    other.retire(&mut host);
    reader.retire(&mut host);
    drop((
        first,
        post_cut,
        next,
        other_first,
        other_second,
        original_prefix,
    ));
    finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
    motion.finish();
}

#[test]
#[ignore = "requires selected native hardware/backend; adapter acquisition failure fails, never skips"]
fn four_numeric_chains_survive_view_effect_output_reorder_and_one_effect_replacement() {
    require_native_selection();
    let definition = WgpuPolicyDefinition::new(HistoryPolicy::new());
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT);
    let mut stream = Stream::new(&mut host, &definition);
    let mut reader = Reader::new(&mut host);
    let mut oracle = Oracle::zero();
    let mut records = Vec::new();
    begin(&mut host, 1);
    stream.admit(&mut host);
    let first = stream.input(1, 1, false, [UNINITIALIZED; 4]);
    let published = stream.submit(&mut host, &first);
    oracle.advance(&stream.topology, 1);
    records.push(stream.record(published, &oracle));
    assert_record(&mut host, &mut reader, &records[0]);

    // Accept a reordered capture before replacing the live author's one effect incarnation.
    let second = stream.input(2, 3, true, [Predecessor::Frame(1); 4]);
    let original_topology = stream.topology.clone();
    stream.topology.replace(0, 1, 7);
    begin(&mut host, 2);
    let published = stream.submit(&mut host, &second);
    oracle.advance(&original_topology, 3);
    records.push(Record {
        published,
        source: stream.source,
        topology: original_topology,
        oracle: oracle.clone(),
    });
    assert_record(&mut host, &mut reader, &records[1]);

    begin(&mut host, 3);
    let third = stream.input(
        3,
        4,
        false,
        [
            Predecessor::Frame(2),
            UNINITIALIZED,
            Predecessor::Frame(2),
            Predecessor::Frame(2),
        ],
    );
    let published = stream.submit(&mut host, &third);
    oracle.reset(0, 1);
    oracle.advance(&stream.topology, 4);
    records.push(stream.record(published, &oracle));
    assert!(
        matches!(
            records[2].published.history_texture::<HistoryColor, _>(
                records[1].topology.views[0].id,
                &records[1].topology.views[0].effects[1].declaration,
                HISTORY_SLOT,
            ),
            Err(WgpuRenderProgramError::UnpublishedResource)
        ),
        "the new publication cannot substitute its replacement for the original effect"
    );

    begin(&mut host, 4);
    let fourth = stream.input(4, 6, true, [Predecessor::Frame(3); 4]);
    let published = stream.submit(&mut host, &fourth);
    oracle.advance(&stream.topology, 6);
    records.push(stream.record(published, &oracle));
    for record in &records {
        assert_record(&mut host, &mut reader, record);
    }
    stream.retire(&mut host);
    reader.retire(&mut host);
    drop(records);
    finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
}

#[test]
#[ignore = "requires selected native hardware/backend; adapter acquisition failure fails, never skips"]
fn sibling_and_reset_stale_candidates_reject_the_whole_group_and_reset_preserves_deduplication() {
    require_native_selection();
    let definition = WgpuPolicyDefinition::new(HistoryPolicy::new());
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT);
    let stream = Stream::new(&mut host, &definition);
    let mut reader = Reader::new(&mut host);
    let mut oracle = Oracle::zero();
    let mut records = Vec::new();
    begin(&mut host, 1);
    stream.admit(&mut host);
    let first = stream.input(1, 0, false, [UNINITIALIZED; 4]);
    let published = stream.submit(&mut host, &first);
    oracle.advance(&stream.topology, 0);
    records.push(stream.record(published, &oracle));

    begin(&mut host, 2);
    let winner_input = stream.input(2, 2, true, [Predecessor::Frame(1); 4]);
    let sibling_input = stream.input(3, 5, false, [Predecessor::Frame(1); 4]);
    let winner = host.prepare(&stream.producer, &winner_input).unwrap();
    let sibling = host.prepare(&stream.producer, &sibling_input).unwrap();
    let published = host.submit_candidate(&stream.producer, winner).unwrap();
    oracle.advance(&stream.topology, 2);
    records.push(stream.record(published, &oracle));
    let stale = host
        .submit_candidate(&stream.producer, sibling)
        .unwrap_err();
    assert!(matches!(
        stale.error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::StaleHistory)
    ));
    assert!(!stale.is_retryable());
    drop(stale);

    let reset_input = stream.input(4, 7, false, [Predecessor::Frame(2); 4]);
    let reset_candidate = host.prepare(&stream.producer, &reset_input).unwrap();
    stream.reset(&mut host, &[(1, 0)]);
    let stale = host
        .submit_candidate(&stream.producer, reset_candidate)
        .unwrap_err();
    assert!(matches!(
        stale.error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::StaleHistory)
    ));
    drop(stale);
    assert_consumed(host.prepare(&stream.producer, &winner_input).unwrap_err());

    // Neither the losing sibling nor the candidate invalidated by reset advanced any chain.
    begin(&mut host, 3);
    let third = stream.input(
        3,
        5,
        true,
        [
            Predecessor::Frame(2),
            Predecessor::Frame(2),
            RESET,
            Predecessor::Frame(2),
        ],
    );
    let published = stream.submit(&mut host, &third);
    oracle.reset(1, 0);
    oracle.advance(&stream.topology, 5);
    records.push(stream.record(published, &oracle));
    for record in &records {
        assert_record(&mut host, &mut reader, record);
    }

    stream.reset(&mut host, &[(0, 0), (0, 1), (1, 0), (1, 1)]);
    assert_consumed(host.prepare(&stream.producer, &third).unwrap_err());
    begin(&mut host, 4);
    let fourth = stream.input(4, 7, false, [RESET; 4]);
    let published = stream.submit(&mut host, &fourth);
    oracle = Oracle::zero();
    oracle.advance(&stream.topology, 7);
    records.push(stream.record(published, &oracle));
    assert_record(&mut host, &mut reader, &records[3]);
    assert_record(&mut host, &mut reader, &records[1]);
    stream.retire(&mut host);
    reader.retire(&mut host);
    drop(records);
    finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
}

#[test]
#[ignore = "requires selected native hardware/backend; adapter acquisition failure fails, never skips"]
fn resource_refusal_returns_the_pinned_candidate_while_b_can_publish_under_the_same_limit() {
    require_native_selection();
    let definition = WgpuPolicyDefinition::new(HistoryPolicy::new());
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT);
    let mut a = Stream::new(&mut host, &definition);
    let b = Stream::new(&mut host, &definition);
    let mut reader = Reader::new(&mut host);
    let mut a_oracle = Oracle::zero();
    let mut b_oracle = Oracle::zero();
    begin(&mut host, 1);
    a.admit(&mut host);
    b.admit(&mut host);
    let a_first_input = a.input(1, 1, false, [UNINITIALIZED; 4]);
    let a_first = a.submit(&mut host, &a_first_input);
    a_oracle.advance(&a.topology, 1);
    let a_first = a.record(a_first, &a_oracle);
    let b_first_input = b.input(1, 2, true, [UNINITIALIZED; 4]);
    let b_first = b.submit(&mut host, &b_first_input);
    b_oracle.advance(&b.topology, 2);
    let b_first = b.record(b_first, &b_oracle);
    settle(a_first.published.outputs().submitted_prefix());
    settle(b_first.published.outputs().submitted_prefix());

    begin(&mut host, 2);
    a.topology.replace(0, 0, 7);
    let a_input = a.input(
        2,
        4,
        true,
        [
            UNINITIALIZED,
            Predecessor::Frame(1),
            Predecessor::Frame(1),
            Predecessor::Frame(1),
        ],
    );
    let candidate_topology = a.topology.clone();
    let candidate = host.prepare(&a.producer, &a_input).unwrap();
    let retained = host.retained_resource_bytes();
    let history_bytes = u64::from(VIEW_WIDTH * HEIGHT) * 16;
    let final_bytes = u64::from(WIDTH * HEIGHT) * 4;
    // B has four history destinations and two composition targets. A additionally needs a
    // real zero seed for its replacement. Half a seed leaves ample room for B's four uniforms.
    host = host.with_resource_byte_limit(
        retained + 4 * history_bytes + 2 * final_bytes + history_bytes / 2,
    );
    let refused = host.submit_candidate(&a.producer, candidate).unwrap_err();
    assert!(matches!(
        refused.error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::ResourceCapacity)
    ));
    assert!(refused.is_retryable());
    assert_eq!(
        host.retained_resource_bytes(),
        retained,
        "known whole-group resource refusal releases every provisional native guard"
    );
    assert!(
        std::ptr::eq(
            refused
                .candidate
                .input()
                .captured::<HistoryPolicy>()
                .unwrap(),
            a_input.captured::<HistoryPolicy>().unwrap(),
        ),
        "retry retains the exact original immutable Capture Arc"
    );

    let b_second_input = b.input(9, 5, false, [Predecessor::Frame(1); 4]);
    let b_second = b.submit(&mut host, &b_second_input);
    b_oracle.advance(&b.topology, 5);
    let b_second = b.record(b_second, &b_oracle);
    assert_eq!(
        b_second
            .published
            .image()
            .receipt()
            .render_transaction_frame_index(),
        9
    );

    // A live author change cannot reinterpret the candidate returned by the failed attempt.
    a.topology.replace(0, 0, 6);
    host = host.with_resource_byte_limit(RESOURCE_LIMIT);
    begin(&mut host, 3);
    let a_second = host
        .submit_candidate(&a.producer, refused.candidate)
        .expect("retry the returned candidate without prepare or rebinding");
    assert_receipt(&a_second, &a_input);
    a_oracle.reset(0, 0);
    a_oracle.advance(&candidate_topology, 4);
    let a_second = Record {
        published: a_second,
        source: a.source,
        topology: candidate_topology,
        oracle: a_oracle.clone(),
    };
    for record in [&a_first, &a_second, &b_first, &b_second] {
        assert_record(&mut host, &mut reader, record);
    }
    assert_consumed(host.prepare(&a.producer, &a_input).unwrap_err());
    a.retire(&mut host);
    b.retire(&mut host);
    reader.retire(&mut host);
    drop((a_first, a_second, b_first, b_second));
    finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
}

#[test]
#[ignore = "requires selected native hardware/backend; adapter acquisition failure fails, never skips"]
fn a_closes_before_readback_while_b_advances_and_retained_a_publications_keep_exact_pixels() {
    require_native_selection();
    let definition = WgpuPolicyDefinition::new(HistoryPolicy::new());
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT);
    let a = Stream::new(&mut host, &definition);
    let b = Stream::new(&mut host, &definition);
    let mut reader = Reader::new(&mut host);
    let mut a_oracle = Oracle::zero();
    let mut b_oracle = Oracle::zero();
    begin(&mut host, 1);
    a.admit(&mut host);
    b.admit(&mut host);
    let a_input = a.input(1, 0, false, [UNINITIALIZED; 4]);
    let a_first = a.submit(&mut host, &a_input);
    a_oracle.advance(&a.topology, 0);
    let a_first = a.record(a_first, &a_oracle);
    let b_input = b.input(1, 2, true, [UNINITIALIZED; 4]);
    let b_first = b.submit(&mut host, &b_input);
    b_oracle.advance(&b.topology, 2);
    let b_first = b.record(b_first, &b_oracle);
    begin(&mut host, 2);
    let a_second_input = a.input(2, 3, true, [Predecessor::Frame(1); 4]);
    let a_second = a.submit(&mut host, &a_second_input);
    a_oracle.advance(&a.topology, 3);
    let a_second = a.record(a_second, &a_oracle);
    let a_pending_input = a.input(3, 7, false, [Predecessor::Frame(2); 4]);
    let a_pending = host.prepare(&a.producer, &a_pending_input).unwrap();

    // Freeze A's original prefix before any readback or blocking completion drive.
    let a_prefix = host
        .retire_producer(&a.producer)
        .expect("A has submitted native work");
    assert!(matches!(
        host.prepare(&a.producer, &a_pending_input),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    let closed = host.submit_candidate(&a.producer, a_pending).unwrap_err();
    assert!(matches!(closed.error, WgpuRenderHostError::ProducerClosed));
    drop(closed);
    assert!(
        host.reserve_image_target(&a.producer, a.source, a.target)
            .is_err()
    );

    let b_second_input = b.input(13, 4, false, [Predecessor::Frame(1); 4]);
    let b_second = b.submit(&mut host, &b_second_input);
    b_oracle.advance(&b.topology, 4);
    let b_second = b.record(b_second, &b_oracle);
    assert_record(&mut host, &mut reader, &a_first);
    begin(&mut host, 3);
    let b_third_input = b.input(29, 6, true, [Predecessor::Frame(13); 4]);
    let b_third = b.submit(&mut host, &b_third_input);
    b_oracle.advance(&b.topology, 6);
    let b_third = b.record(b_third, &b_oracle);
    assert_record(&mut host, &mut reader, &a_second);
    settle(&a_prefix);
    for record in [&b_first, &b_second, &b_third, &a_first, &a_second] {
        assert_record(&mut host, &mut reader, record);
    }
    assert_ne!(
        a_second.published.image().receipt().producer_id(),
        b_third.published.image().receipt().producer_id()
    );
    assert_eq!(host.frame_transaction_stats().surface_acquire_attempts(), 0);
    assert_eq!(host.frame_transaction_stats().presents(), 0);
    b.retire(&mut host);
    reader.retire(&mut host);
    drop((a_prefix, a_first, a_second, b_first, b_second, b_third));
    finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
}

#[test]
#[ignore = "requires selected native hardware/backend; adapter acquisition failure fails, never skips"]
fn replacement_host_reports_device_recreated_and_explicitly_seeds_all_four_histories() {
    require_native_selection();
    let definition = WgpuPolicyDefinition::new(HistoryPolicy::new());
    let mut original = WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT);
    let mut stream = Stream::new(&mut original, &definition);
    let mut reader = Reader::new(&mut original);
    begin(&mut original, 1);
    stream.admit(&mut original);
    let first_input = stream.input(1, 1, false, [UNINITIALIZED; 4]);
    let first = stream.submit(&mut original, &first_input);
    let original_backend = first.image().receipt().backend_instance_id();
    let mut oracle = Oracle::zero();
    oracle.advance(&stream.topology, 1);
    let first = stream.record(first, &oracle);
    assert_record(&mut original, &mut reader, &first);

    // These CPU captures retain their source-bound lineage and original declarations. A fresh
    // Host receives current samples and an explicit reset, not an invented replay of old pixels.
    let recreated_input = stream.input(2, 5, true, [RECREATED; 4]);
    let continuing_input = stream.input(3, 6, false, [Predecessor::Frame(2); 4]);
    stream.installation.close();
    stream.retire(&mut original);
    reader.retire(&mut original);
    drop(first);
    finish(&mut original);
    assert_eq!(original.retained_resource_bytes(), 0);
    drop(original);

    let mut fresh = WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT);
    let producer = fresh.create_producer().unwrap();
    fresh.prepare_policy(&producer, &definition).unwrap();
    let mut reader = Reader::new(&mut fresh);
    begin(&mut fresh, 1);
    admit(&mut fresh, &producer, stream.source, stream.target);
    let candidate = fresh.prepare(&producer, &recreated_input).unwrap();
    let recreated = fresh.submit_candidate(&producer, candidate).unwrap();
    assert_receipt(&recreated, &recreated_input);
    assert_ne!(
        recreated.image().receipt().backend_instance_id(),
        original_backend
    );
    oracle = Oracle::zero();
    oracle.advance(&stream.topology, 5);
    let recreated = stream.record(recreated, &oracle);
    assert_record(&mut fresh, &mut reader, &recreated);

    begin(&mut fresh, 2);
    let candidate = fresh.prepare(&producer, &continuing_input).unwrap();
    let continuing = fresh.submit_candidate(&producer, candidate).unwrap();
    assert_receipt(&continuing, &continuing_input);
    oracle.advance(&stream.topology, 6);
    let continuing = stream.record(continuing, &oracle);
    assert_record(&mut fresh, &mut reader, &continuing);
    assert_record(&mut fresh, &mut reader, &recreated);
    settle(&fresh.retire_producer(&producer).unwrap());
    reader.retire(&mut fresh);
    drop((recreated, continuing, producer));
    finish(&mut fresh);
    assert_eq!(fresh.retained_resource_bytes(), 0);
    assert_eq!(fresh.retained_candidate_metadata_bytes(), 0);
}

struct ViewLifetimePolicy;

struct ViewLifetimeCapture {
    lineage: WgpuHistoryLineage,
    view: WgpuHistoryView,
    effect: Effect,
}

fn view_lifetime_descriptor() -> WgpuProgramTextureDescriptor {
    let mut descriptor = history_descriptor();
    descriptor.native.label = Some("independent-view-lifetime-history");
    descriptor.native.size.width = 1;
    descriptor.native.size.height = 1;
    descriptor.native.format = wgpu::TextureFormat::Rgba8Unorm;
    descriptor
}

impl WgpuRenderPolicy for ViewLifetimePolicy {
    type Capture = ViewLifetimeCapture;
    type Bindings = WgpuHistoryTexture<FinalColor>;

    fn history_lineage(&self, capture: &Self::Capture) -> Option<WgpuHistoryLineage> {
        Some(capture.lineage.clone())
    }

    fn bind(
        &self,
        capture: &Self::Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        context.history_texture(
            &capture.view,
            &capture.effect.declaration,
            HISTORY_SLOT,
            view_lifetime_descriptor(),
        )
    }

    fn build(
        &self,
        history: &Self::Bindings,
        _capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        assert!(context.previous_texture(history)?.is_none());
        assert_eq!(
            context.empty_texture_reason(history)?,
            Some(WgpuEmptyHistoryReason::Uninitialized)
        );
        assert_eq!(context.format(), wgpu::TextureFormat::Rgba8Unorm);
        let mut builder = WgpuRenderProgramBuilder::new(
            context.packet().source_generation(),
            context.packet().frame_index(),
            limits(),
        )?;
        let output = builder.texture::<FinalColor>(view_lifetime_descriptor())?;
        // One clear serves as both history and final output; no Feature operation or shader runs.
        builder.initialize(
            0,
            &[output.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )?;
        Ok(WgpuPolicyProgram::new(
            builder.finish(&[output.output()])?,
            &output,
            WgpuPolicyFrameFacts::default(),
        )?
        .with_history(vec![history.bind(&output)]))
    }
}

#[test]
#[ignore = "requires selected native hardware/backend; adapter acquisition failure fails, never skips"]
fn shared_feature_reclaims_more_than_512_disposed_view_incarnations_in_one_lineage() {
    require_native_selection();
    let definition = WgpuPolicyDefinition::new(ViewLifetimePolicy);
    let installation = definition.install().unwrap();
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT);
    let producer = host.create_producer().unwrap();
    host.prepare_policy(&producer, &definition).unwrap();
    let source = RenderFrameSourceGeneration::try_acquire().unwrap();
    let lineage = WgpuHistoryLineage::new(source).unwrap();
    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(1, 1).unwrap(),
    )
    .unwrap();
    // This original declaration stays alive for every incarnation. Only the View owner expires.
    let effect = Effect::new(0, 0);
    for frame in 1..=513 {
        begin(&mut host, frame);
        if frame == 1 {
            admit(&mut host, &producer, source, target);
        }
        let id = RenderViewId::try_acquire().unwrap();
        let view = WgpuHistoryView::new(id);
        let mut views = ExtractedViews::default();
        let viewport = ViewportRect::new(0, 0, 1, 1).unwrap();
        views.push(ExtractedView {
            id,
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(target.image()),
            viewport,
            mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 1.0, viewport).unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        });
        let packet = build_image_render_frame_packet(source, frame, &views, target)
            .unwrap()
            .unwrap();
        let input = installation
            .capture(
                packet,
                Arc::new(ViewLifetimeCapture {
                    lineage: lineage.clone(),
                    view: view.clone(),
                    effect: effect.clone(),
                }),
            )
            .unwrap();
        drop(view);
        let candidate = host.prepare(&producer, &input).expect(
            "disposed View rows must retire before the 512-slot history capacity is reached",
        );
        drop(input);
        let published = host
            .submit_candidate(&producer, candidate)
            .expect("the original input and candidate retain this View until publication");
        let history = published
            .history_texture::<FinalColor, _>(id, &effect.declaration, HISTORY_SLOT)
            .unwrap();
        let final_texture = published.final_texture::<FinalColor>().unwrap();
        for output in [&history, &final_texture] {
            assert_eq!(output.frame_index(), frame);
            assert_eq!(output.source_generation(), source);
            assert_eq!(output.descriptor(), view_lifetime_descriptor());
        }
        settle(published.outputs().submitted_prefix());
    }
    settle(&host.retire_producer(&producer).unwrap());
    finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
}
