#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::Arc,
    time::Duration,
};

use nara::core::Vec2;
use nara_render::{Extent2d, RenderImage2d, RenderImagePacket};
use nara_render_wgpu::{
    WgpuEmptyHistoryReason, WgpuHistoryLineage, WgpuHistoryReset, WgpuHistoryView,
    WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuProgramTextureOutput, WgpuPublishedFrame,
    WgpuRenderError, WgpuRenderHost, WgpuRenderHostError, WgpuRenderInput, WgpuRenderProducer,
    WgpuRenderProgramError, WgpuStockPolicy,
};

#[path = "support/accumulation.rs"]
mod accumulation;
#[path = "support/history_readback.rs"]
mod history_readback;
#[path = "support/joint_scene.rs"]
mod joint_scene;
#[path = "support/material_readback.rs"]
mod material_readback;
#[path = "support/motion.rs"]
mod motion;
pub mod support;

use accumulation::{
    Effect, HEIGHT, HISTORY_SLOT, HistoryCapture, HistoryColor, HistoryPolicy, Predecessor,
    Topology, VIEW_WIDTH, ViewTopology, WIDTH, history_descriptor,
};
use history_readback::{Reader, Samples, finish, settle};
use joint_scene::{Change, Sample, Scene, TIMESTEP};

const EMPTY: Predecessor = Predecessor::Empty(WgpuEmptyHistoryReason::Uninitialized);
const RESET: Predecessor = Predecessor::Empty(WgpuEmptyHistoryReason::Reset);

#[derive(Clone)]
struct Oracle {
    histories: [Vec<[f64; 4]>; 4],
}

impl Oracle {
    fn new() -> Self {
        Self {
            histories: std::array::from_fn(|_| vec![[0.0; 4]; (VIEW_WIDTH * HEIGHT) as usize]),
        }
    }

    fn advance(&mut self, source: &[[u8; 4]], reset: bool) {
        assert_eq!(source.len(), (WIDTH * HEIGHT) as usize);
        for (index, history) in self.histories.iter_mut().enumerate() {
            let view = index / 2;
            if reset && view == 0 {
                history.fill([0.0; 4]);
            }
            for y in 0..HEIGHT {
                for x in 0..VIEW_WIDTH {
                    let current = source[(y * WIDTH + view as u32 * VIEW_WIDTH + x) as usize];
                    let previous = &mut history[(y * VIEW_WIDTH + x) as usize];
                    for channel in 0..4 {
                        previous[channel] =
                            f64::from(current[channel]) / 1020.0 + previous[channel] * 0.75;
                    }
                }
            }
        }
    }

    fn assert_samples(&self, actual: &Samples) {
        for (slot, (actual, expected)) in actual.histories.iter().zip(&self.histories).enumerate() {
            assert_eq!(actual.len(), expected.len());
            for (pixel, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                for channel in 0..4 {
                    assert!(actual[channel].is_finite());
                    assert!(
                        (f64::from(actual[channel]) - expected[channel]).abs() <= 0.000_003,
                        "slot {slot}, pixel {pixel}, channel {channel}: {} != {}",
                        actual[channel],
                        expected[channel]
                    );
                }
            }
        }
        assert_eq!(actual.image.len(), (WIDTH * HEIGHT) as usize);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let slot = 2 * (x / VIEW_WIDTH) as usize + usize::from(y >= HEIGHT / 2);
                let expected = self.histories[slot][(y * VIEW_WIDTH + x % VIEW_WIDTH) as usize];
                let actual = actual.image[(y * WIDTH + x) as usize];
                for channel in 0..3 {
                    assert!(
                        actual[channel].abs_diff((expected[channel] * 255.0).round() as u8) <= 1,
                        "final ({x}, {y}) channel {channel}: {actual:?} != {expected:?}"
                    );
                }
                assert_eq!(actual[3], 255);
            }
        }
    }
}

struct Record {
    frame: WgpuPublishedFrame,
    oracle: Oracle,
}

struct Harness {
    scene: Scene,
    host: WgpuRenderHost,
    producers: Vec<WgpuRenderProducer>,
    reader: Option<Reader>,
    installation: WgpuPolicyInstallation<HistoryPolicy>,
    topology: Option<Topology>,
    lineage: Option<WgpuHistoryLineage>,
    target: RenderImagePacket,
    late_target: RenderImagePacket,
    records: Vec<Record>,
    retained_stock: Vec<(nara_render_wgpu::WgpuImageOutput, Vec<[u8; 4]>)>,
    oracle: Oracle,
    host_frame: u64,
    last: Option<u64>,
    pipeline_count: Option<isize>,
    fallback: bool,
}

fn target() -> RenderImagePacket {
    RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(WIDTH, HEIGHT).unwrap(),
    )
    .unwrap()
}

impl Harness {
    fn new(fallback: bool) -> Self {
        Self {
            scene: Scene::new(),
            host: WgpuRenderHost::offscreen().with_resource_byte_limit(16 * 1024 * 1024),
            producers: Vec::new(),
            reader: None,
            installation: WgpuPolicyDefinition::new(HistoryPolicy::new())
                .install()
                .unwrap(),
            topology: None,
            lineage: None,
            target: target(),
            late_target: target(),
            records: Vec::new(),
            retained_stock: Vec::new(),
            oracle: Oracle::new(),
            host_frame: 0,
            last: None,
            pipeline_count: None,
            fallback,
        }
    }

    fn begin(&mut self) {
        self.host_frame += 1;
        self.host.begin_frame(self.host_frame).unwrap();
        let adapter = self.host.runtime_facts().adapter.unwrap();
        assert_eq!(adapter.backend(), std::env::var("WGPU_BACKEND").unwrap());
        if !self.fallback {
            assert_ne!(adapter.device_type(), "cpu");
        }
        if self.host_frame == 1 {
            eprintln!(
                "joint-history: adapter={adapter:?}, forced_fallback={}",
                self.fallback
            );
        }
    }

    fn initialize(&mut self, sample: &Sample) {
        let packet = sample.input.packet();
        assert_eq!(packet.views().len(), 2);
        self.topology = Some(Topology {
            views: std::array::from_fn(|index| {
                let id = packet.views()[index].id;
                ViewTopology {
                    index,
                    id,
                    history_view: WgpuHistoryView::new(id),
                    effects: std::array::from_fn(|effect| Effect::new(effect, 0)),
                }
            }),
        });
        self.lineage = Some(WgpuHistoryLineage::new(packet.source_generation()).unwrap());
        for _ in 0..2 {
            self.producers.push(self.host.create_producer().unwrap());
        }
        self.host
            .prepare_policy(&self.producers[0], self.installation.definition())
            .unwrap();
        self.begin();
        for (producer, target) in self.producers.iter().zip([self.target, self.late_target]) {
            let reservation = self
                .host
                .reserve_image_target(producer, packet.source_generation(), target)
                .unwrap();
            self.host
                .activate_image_target(producer, reservation)
                .unwrap();
        }
        self.reader = Some(Reader::new(&mut self.host));
    }

    fn capture(
        &self,
        sample: &Sample,
        reset: bool,
    ) -> (RenderImagePacket, Arc<HistoryCapture>, WgpuRenderInput) {
        let packet = sample.input.packet();
        let time = packet.captured_time().unwrap();
        assert_eq!(time.spatial_stamp(), sample.motion.stamp);
        assert_eq!(
            time.spatial_interpolated(),
            sample.motion.spatial_interpolated
        );
        assert_eq!(time.settings().paused(), sample.motion.paused);
        let previous = self.last.map_or(EMPTY, Predecessor::Frame);
        let predecessors = if reset {
            [RESET, RESET, previous, previous]
        } else {
            [previous; 4]
        };
        let capture = self.topology.as_ref().unwrap().capture_stock(
            self.lineage.as_ref().unwrap(),
            Arc::new(sample.input.captured::<WgpuStockPolicy>().unwrap().clone()),
            time,
            sample.motion.phase(),
            false,
            predecessors,
        );
        let packet = packet.with_image_target(self.target).unwrap();
        assert_eq!(packet.captured_time(), Some(time));
        let input = self
            .installation
            .capture(packet, Arc::clone(&capture))
            .unwrap();
        (self.target, capture, input)
    }

    fn publish(&mut self, sample: &Sample, reset: bool) -> usize {
        self.begin();
        let (_, _, input) = self.capture(sample, reset);
        let candidate = self.host.prepare(&self.producers[0], &input).unwrap();
        let published = self
            .host
            .submit_candidate(&self.producers[0], candidate)
            .unwrap();
        let frame = input.packet().frame_index();
        assert_eq!(
            published.image().receipt().render_transaction_frame_index(),
            frame
        );
        assert_eq!(
            published.image().receipt().source_generation(),
            input.packet().source_generation()
        );
        self.oracle.advance(&source_pixels(sample), reset);
        self.last = Some(frame);
        let index = self.records.len();
        self.records.push(Record {
            frame: published,
            oracle: self.oracle.clone(),
        });
        self.assert_record(index);
        index
    }

    fn assert_record(&mut self, index: usize) {
        let record = &self.records[index];
        let topology = self.topology.as_ref().unwrap();
        let histories: [WgpuProgramTextureOutput<HistoryColor>; 4] = std::array::from_fn(|index| {
            let view = &topology.views[index / 2];
            record
                .frame
                .history_texture::<HistoryColor, _>(
                    view.id,
                    &view.effects[index % 2].declaration,
                    HISTORY_SLOT,
                )
                .unwrap()
        });
        for history in &histories {
            assert_eq!(
                history.frame_index(),
                record
                    .frame
                    .image()
                    .receipt()
                    .render_transaction_frame_index()
            );
            assert_eq!(history.producer_id(), self.producers[0].id());
        }
        let actual =
            self.reader
                .as_mut()
                .unwrap()
                .read(&mut self.host, &histories, record.frame.image());
        record.oracle.assert_samples(&actual);
        assert_eq!(
            self.host
                .frame_transaction_stats()
                .surface_acquire_attempts(),
            0
        );
        assert_eq!(self.host.frame_transaction_stats().presents(), 0);
    }

    fn pipelines(&mut self) -> isize {
        let record = self.records.last().unwrap();
        let pipelines = self
            .host
            .with_image(record.frame.image(), |device, _, _| {
                device.get_internal_counters().hal.render_pipelines.read()
            })
            .unwrap();
        assert!(pipelines > 0);
        pipelines
    }

    fn cut(&mut self) {
        let lineage = self.lineage.as_mut().unwrap();
        lineage.discontinue().unwrap();
        let view = &self.topology.as_ref().unwrap().views[0];
        let resets = view
            .effects
            .iter()
            .map(|effect| {
                WgpuHistoryReset::texture::<HistoryColor, _>(
                    &view.history_view,
                    &effect.declaration,
                    HISTORY_SLOT,
                    history_descriptor(),
                )
            })
            .collect::<Vec<_>>();
        self.host
            .reset_history(&self.producers[0], lineage, &resets)
            .unwrap();
    }

    fn late_stock(&mut self, sample: &Sample) {
        self.begin();
        let input = sample.input.with_image_target(self.late_target).unwrap();
        assert_eq!(
            input.packet().captured_time(),
            sample.input.packet().captured_time()
        );
        let candidate = self.host.prepare(&self.producers[1], &input).unwrap();
        let published = self
            .host
            .submit_candidate(&self.producers[1], candidate)
            .unwrap();
        let pixels = material_readback::read(&mut self.host, published.image());
        assert_eq!(pixels, source_pixels(sample));
        self.retained_stock.push((published.into_image(), pixels));
        let reversed = sample.input.with_image_target(self.target).unwrap();
        assert_consumed(
            self.host
                .prepare(&self.producers[0], &reversed)
                .unwrap_err(),
        );
    }

    fn cut_case(&mut self, change: Option<Change>) {
        let before = self.scene.drive(Duration::ZERO);
        let (_, raw, unprepared) = self.capture(&before, false);
        let original_packet = unprepared.packet().clone();
        let second = self.scene.drive(Duration::ZERO);
        assert_eq!(second.captures, before.captures + 1);
        let (_, _, prepared_input) = self.capture(&second, false);
        let prepared = self
            .host
            .prepare(&self.producers[0], &prepared_input)
            .unwrap();
        let prior = self.records.len() - 1;
        let prior_prefix = self.records[prior]
            .frame
            .outputs()
            .submitted_prefix()
            .clone();
        let pipelines = self.pipelines();
        self.cut();
        let corrected = match change {
            Some(change) => {
                self.scene.request(change);
                None
            }
            None => {
                self.scene.correct_and_reparent(Vec2::new(1.0, 2.0));
                let count = self.scene.capture_count();
                let (stamp, poses) = self.scene.exact_step();
                assert_eq!(self.scene.capture_count(), count);
                assert_eq!(stamp.tick, 4);
                assert_eq!(stamp.phase, nara::transform::SpatialSnapshotPhase::Fixed);
                assert!(poses[0].abs_diff_eq(Vec2::new(1.25, 2.0), 1.0e-5));
                Some((stamp, poses))
            }
        };
        let after = self.scene.drive(Duration::ZERO);
        if let Some((stamp, poses)) = corrected {
            assert_eq!(after.motion.stamp.tick, stamp.tick);
            assert_eq!(after.motion.stamp.frame, stamp.frame + 1);
            assert_eq!(after.motion.kinematic, poses[0]);
            assert_eq!(after.motion.dynamic, poses[1]);
            assert_eq!(after.motion.fixed_positions, poses);
            assert!(!after.motion.spatial_interpolated);
        }
        assert_eq!(
            before.input.packet().source_generation(),
            after.input.packet().source_generation()
        );
        assert_eq!(
            before
                .input
                .packet()
                .views()
                .iter()
                .map(|v| v.id)
                .collect::<Vec<_>>(),
            after
                .input
                .packet()
                .views()
                .iter()
                .map(|v| v.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            before.material.generation().declaration().binding_digest(),
            after.material.generation().declaration().binding_digest()
        );
        assert_eq!(
            before.material.generation().shader().interface_digest(),
            after.material.generation().shader().interface_digest()
        );
        if matches!(change, Some(Change::Shader)) {
            assert_ne!(
                before.material.generation().shader().digest(),
                after.material.generation().shader().digest()
            );
        } else {
            assert_eq!(
                before.material.generation().shader().digest(),
                after.material.generation().shader().digest()
            );
        }
        if matches!(change, Some(Change::Parameters)) {
            assert_ne!(
                before.material.uniform_bytes(),
                after.material.uniform_bytes()
            );
        }
        let before_pixels = source_pixels(&before);
        let after_pixels = source_pixels(&after);
        let mut changed = false;
        for (before, after) in before_pixels
            .chunks_exact(WIDTH as usize)
            .zip(after_pixels.chunks_exact(WIDTH as usize))
        {
            assert_eq!(
                &before[VIEW_WIDTH as usize..],
                &after[VIEW_WIDTH as usize..]
            );
            changed |= before[..VIEW_WIDTH as usize] != after[..VIEW_WIDTH as usize];
        }
        assert!(
            changed,
            "each selected cut must change the affected View's actual pixels"
        );
        assert!(matches!(
            self.installation.capture(original_packet, raw),
            Err(WgpuRenderProgramError::StaleHistory)
        ));
        assert_stale(
            self.host
                .prepare(&self.producers[0], &unprepared)
                .unwrap_err(),
        );
        let refused = self
            .host
            .submit_candidate(&self.producers[0], prepared)
            .unwrap_err();
        assert_stale_ref(&refused.error);
        assert!(!refused.is_retryable());
        assert!(std::ptr::eq(
            refused
                .candidate
                .input()
                .captured::<HistoryPolicy>()
                .unwrap(),
            prepared_input.captured::<HistoryPolicy>().unwrap()
        ));
        drop(refused);
        self.publish(&after, true);
        let after_pipelines = self.pipelines();
        if matches!(change, Some(Change::Shader)) {
            assert_eq!(after_pipelines, pipelines + 1);
        } else {
            assert_eq!(after_pipelines, pipelines);
        }
        self.late_stock(&before);
        self.assert_record(prior);
        self.assert_record(self.records.len() - 1);
        settle(&prior_prefix);
        let next = self.scene.drive(Duration::ZERO);
        self.publish(&next, false);
        assert_eq!(self.pipelines(), after_pipelines);
    }

    fn run(&mut self) {
        let zero = self.scene.drive(Duration::ZERO);
        assert_eq!(zero.captures, 1);
        self.initialize(&zero);
        assert_eq!(zero.motion.stamp.tick, 0);
        self.publish(&zero, false);
        self.pipeline_count = Some(self.pipelines());
        let multiple = self.scene.drive(TIMESTEP * 2 + TIMESTEP / 2);
        assert_eq!(multiple.motion.stamp.tick, 2);
        assert!(multiple.motion.spatial_interpolated);
        self.publish(&multiple, false);
        assert_eq!(self.pipelines(), self.pipeline_count.unwrap());
        let no_tick = self.scene.drive(TIMESTEP / 4);
        assert_eq!(no_tick.motion.stamp.tick, 2);
        assert!(no_tick.motion.virtual_elapsed > multiple.motion.virtual_elapsed);
        self.publish(&no_tick, false);
        assert_eq!(self.pipelines(), self.pipeline_count.unwrap());
        self.scene.set_time_scale(0.5);
        let scaled = self.scene.drive(TIMESTEP);
        let time = scaled.input.packet().captured_time().unwrap();
        assert_eq!(time.real().delta, TIMESTEP);
        assert_eq!(time.virtual_time().delta, TIMESTEP / 2);
        assert_eq!(time.settings().time_scale(), 0.5);
        self.publish(&scaled, false);
        assert_eq!(self.pipelines(), self.pipeline_count.unwrap());
        self.scene.pause();
        let paused = self.scene.drive(Duration::from_secs(1));
        assert!(paused.motion.paused);
        assert_eq!(paused.motion.virtual_elapsed, scaled.motion.virtual_elapsed);
        assert!(paused.motion.real_elapsed > scaled.motion.real_elapsed);
        assert!(!paused.motion.spatial_interpolated);
        self.publish(&paused, false);
        assert_eq!(self.pipelines(), self.pipeline_count.unwrap());
        self.cut_case(Some(Change::CameraCut));
        self.cut_case(None);
        self.cut_case(Some(Change::Parameters));
        let valid = self.scene.drive(Duration::ZERO);
        let valid_pipelines = self.pipelines();
        self.scene.request(Change::InvalidShader);
        let rejected = self.scene.drive(Duration::ZERO);
        assert_eq!(valid.material.generation(), rejected.material.generation());
        assert_eq!(
            valid.material.uniform_bytes(),
            rejected.material.uniform_bytes()
        );
        assert_eq!(rejected.state.rejections, valid.state.rejections + 1);
        self.publish(&rejected, false);
        assert_eq!(self.pipelines(), valid_pipelines);
        self.cut_case(Some(Change::Shader));
        for index in 0..self.records.len() {
            self.assert_record(index);
        }
        for (output, expected) in &self.retained_stock {
            assert_eq!(material_readback::read(&mut self.host, output), *expected);
        }
        let live_pipelines = self.pipelines();
        assert_eq!(live_pipelines, self.pipeline_count.unwrap() + 1);
        self.scene.close().unwrap();
        assert!(self.scene.world_dropped());
        for producer in &self.producers {
            if let Some(prefix) = self.host.retire_producer(producer) {
                settle(&prefix);
            }
        }
        for index in 0..self.records.len() {
            self.assert_record(index);
            settle(self.records[index].frame.outputs().submitted_prefix());
        }
        for (output, expected) in &self.retained_stock {
            assert_eq!(material_readback::read(&mut self.host, output), *expected);
        }
        eprintln!(
            "joint-history: cuts=4, temporal_publications={}, late_stock_outputs={}, live_pipelines={:?}->{}, original World dropped={}, retired producer outputs remain exact",
            self.records.len(),
            self.retained_stock.len(),
            self.pipeline_count,
            live_pipelines,
            self.scene.world_dropped()
        );
    }

    fn cleanup(&mut self) -> Vec<String> {
        let mut failures = Vec::new();
        if let Err(error) = self.scene.close() {
            failures.push(error);
        }
        if let Some(reader) = self.reader.take() {
            if catch_unwind(AssertUnwindSafe(|| reader.retire(&mut self.host))).is_err() {
                failures.push("original history Reader retirement failed".to_owned());
            }
        }
        for producer in &self.producers {
            if catch_unwind(AssertUnwindSafe(|| {
                if let Some(prefix) = self.host.retire_producer(producer) {
                    settle(&prefix);
                }
            }))
            .is_err()
            {
                failures.push("original producer retirement failed".to_owned());
            }
        }
        self.records.clear();
        self.retained_stock.clear();
        self.producers.clear();
        if catch_unwind(AssertUnwindSafe(|| finish(&mut self.host))).is_err() {
            failures.push("original Host retirement failed".to_owned());
        }
        for (label, count) in [
            ("native bytes", self.host.retained_resource_bytes()),
            (
                "candidate metadata",
                self.host.retained_candidate_metadata_bytes(),
            ),
            ("cleanup faults", self.host.cleanup_fault_count()),
        ] {
            if count != 0 {
                failures.push(format!("remaining {label}: {count}"));
            }
        }
        eprintln!(
            "joint-history cleanup: original_world_dropped={}, failures={failures:?}",
            self.scene.world_dropped()
        );
        failures
    }
}

fn assert_stale(error: WgpuRenderHostError) {
    assert_stale_ref(&error);
}

fn assert_stale_ref(error: &WgpuRenderHostError) {
    assert!(
        matches!(
            error,
            WgpuRenderHostError::Program(WgpuRenderProgramError::StaleHistory)
        ),
        "{error}"
    );
}

fn assert_consumed(error: WgpuRenderHostError) {
    assert!(
        matches!(
            error,
            WgpuRenderHostError::Backend(WgpuRenderError::FrameAlreadyConsumed { .. })
                | WgpuRenderHostError::Program(WgpuRenderProgramError::HistoryAlreadyConsumed)
        ),
        "{error}"
    );
}

fn source_pixels(sample: &Sample) -> Vec<[u8; 4]> {
    let time = sample.input.packet().captured_time().unwrap();
    let phase = |elapsed: Duration, modulus: u128| {
        (((elapsed.as_nanos() + 2_000_000) / 4_000_000) % modulus) as f64
    };
    let mut color = [
        phase(time.virtual_time().elapsed, 8) * 24.0 + f64::from(sample.state.level) * 256.0,
        phase(time.real().elapsed, 4) * 32.0
            + f64::from(u8::from(time.settings().paused())) * 16.0
            + f64::from(u8::from(time.spatial_interpolated())) * 16.0,
        64.0,
        255.0,
    ];
    if sample.state.shader_swapped {
        color.swap(0, 2);
    }
    let color = color.map(|value| {
        assert!((0.0..=255.0).contains(&value));
        value as u8
    });
    let mut pixels = vec![[0; 4]; (WIDTH * HEIGHT) as usize];
    for y in 0..HEIGHT {
        let world_y = 2.0 + 1.0 - (f64::from(y) + 0.5) * 2.0 / f64::from(HEIGHT);
        for x in 0..WIDTH {
            let view = x / VIEW_WIDTH;
            let center_x = if view == 0 {
                f64::from(sample.state.camera_x)
            } else {
                100.0
            };
            let world_x =
                center_x - 1.0 + (f64::from(x % VIEW_WIDTH) + 0.5) * 2.0 / f64::from(VIEW_WIDTH);
            let sprite = if view == 0 {
                sample.motion.kinematic
            } else {
                Vec2::new(100.0, 2.0)
            };
            if world_x >= f64::from(sprite.x) - 0.25
                && world_x < f64::from(sprite.x) + 0.25
                && world_y >= f64::from(sprite.y) - 0.25
                && world_y < f64::from(sprite.y) + 0.25
            {
                pixels[(y * WIDTH + x) as usize] = if view == 0 { color } else { [0, 0, 255, 255] };
            }
        }
    }
    pixels
}

#[test]
#[ignore = "requires an explicitly selected native hardware or software adapter; missing adapter fails"]
fn real_material_motion_and_camera_cuts_preserve_history_ownership_and_last_good() {
    let backend = std::env::var("WGPU_BACKEND").expect("explicitly select WGPU_BACKEND");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    let fallback = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() {
        Ok("0") => false,
        Ok("1") => true,
        _ => panic!("explicitly select NARA_WGPU_FORCE_FALLBACK=0 or 1"),
    };
    let mut harness = Harness::new(fallback);
    let result = catch_unwind(AssertUnwindSafe(|| harness.run()));
    let failures = harness.cleanup();
    if let Err(panic) = result {
        if !failures.is_empty() {
            eprintln!("joint-history cleanup after failure: {failures:?}");
        }
        resume_unwind(panic);
    }
    assert!(
        failures.is_empty(),
        "original physical cleanup: {failures:?}"
    );
}
