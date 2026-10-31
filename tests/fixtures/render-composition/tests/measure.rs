//! Private CPU-stage measurement on the existing, numerically qualified native GPU workload.
//!
//! Explicit serial entry (reuse the repository target, set CARGO_BUILD_JOBS=1):
//! cargo +1.95.0 nextest run --locked --release --manifest-path
//! tests/fixtures/render-composition/Cargo.toml --test measure --run-ignored only
//! --test-threads=1 --no-capture
//!
//! The common denominator is one immutable uploaded input, five NEW output textures, two NEW
//! buffers, one NEW encoder, identical initialization/kernels and an accepted-prefix custodian.
//! Pipelines and the ECS schedule topology are prepared once. No reused-output/encoder control
//! is compared with graph allocation. Graph declaration/admission is rebuilt per sample; the
//! static controls have no equivalent per-candidate model admission. Their absence is N/A,
//! not zero-cost equivalent validation. Schedule construction/initialization is separate setup.
//! The fresh World/Schedule path in controls::execute is not called or timed by this entry.
//! This file keeps one World and initialized Schedule for the entire run, changing only captures
//! and the fresh encoder per sample. Its numbers cannot establish inherent Bevy schedule cost.
//! The shared input upload is submitted and prefix-complete before qualification and warmups;
//! every path excludes source allocation/upload from its per-candidate denominator.
//!
//! native::prepare intentionally remains intact. Its allocation, initialization, native checks,
//! encoder work and error-scope inspection cannot be separately observed here. Its total and
//! total minus nested kernel-call time are reported, never relabeled as allocation time. The
//! control allocation/encoding split is diagnostic only. Kernel time excludes texture-view
//! construction and graph lookups, and includes the shared Effects methods' bind-group/pass
//! encoding. Timers themselves are not subtracted. No GPU timestamp or kernel-execution latency
//! is measured. Batch prefix waits, retirement and readback are outside every sample interval.

#![forbid(unsafe_code)]

use std::{
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nara_ecs::{
    Res, ResMut, Resource, World,
    schedule::{IntoScheduleConfigs, Schedule},
};

#[path = "support/controls.rs"]
mod controls;
#[path = "support/effects.rs"]
mod effects;
#[path = "support/gpu.rs"]
mod gpu;
#[path = "support/graph.rs"]
mod graph;
#[path = "support/native.rs"]
mod native;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/workload.rs"]
mod workload;

const WARMUPS: usize = 128;
const SAMPLES: usize = 1000;
const SERIES: usize = 3;
const BATCH: usize = 8;
const TIMEOUT: Duration = Duration::from_secs(10);
const TEXTURE_BYTES: u64 = workload::WIDTH as u64 * workload::HEIGHT as u64 * 16;
const LOCAL_BYTES: u64 = 5 * TEXTURE_BYTES + 2 * effects::COUNTS_BUFFER_SIZE;

// This is the current graph's deterministic order, also supplied explicitly to all controls.
// Matching it removes a different native command order from this measurement's denominator.
const EXECUTION_ORDER: [graph::OperationKey; 7] = [
    workload::LUMINANCE,
    workload::NORMALIZE,
    workload::HISTOGRAM,
    workload::BLUR_HORIZONTAL,
    workload::BLUR_VERTICAL,
    workload::COMPLEMENT,
    workload::HUD,
];

#[derive(Clone, Copy, Debug)]
enum Variant {
    Graph,
    Ordered,
    Slots,
    Ecs,
}

impl Variant {
    const ALL: [Self; 4] = [Self::Graph, Self::Ordered, Self::Slots, Self::Ecs];

    fn name(self) -> &'static str {
        match self {
            Self::Graph => "graph-fresh-model",
            Self::Ordered => "ordered-static-plan",
            Self::Slots => "slot-private-blur",
            Self::Ecs => "ecs-reused-topology",
        }
    }
}

#[derive(Default)]
struct Sample {
    model: Option<Duration>,
    prepare: Duration,
    allocation: Option<Duration>,
    encoding: Option<Duration>,
    kernel: Duration,
    submit: Duration,
    cpu_total: Duration,
}

struct Measurements {
    samples: Vec<Sample>,
    waits: Vec<Duration>,
    retirement: Vec<Duration>,
    readbacks: Vec<Duration>,
    oracle_checks: Vec<Duration>,
}

impl Measurements {
    fn reserved() -> Self {
        Self {
            samples: Vec::with_capacity(SAMPLES),
            waits: Vec::with_capacity(SAMPLES.div_ceil(BATCH)),
            retirement: Vec::with_capacity(SAMPLES.div_ceil(BATCH)),
            readbacks: Vec::with_capacity(1),
            oracle_checks: Vec::with_capacity(1),
        }
    }
}

#[derive(Clone, Copy)]
struct Distribution {
    p50: Duration,
    p95: Duration,
    min: Duration,
    max: Duration,
}

fn distribution(values: impl Iterator<Item = Duration>) -> Option<Distribution> {
    let mut values: Vec<_> = values.collect();
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let percentile = |percent: usize| values[(values.len() * percent).div_ceil(100) - 1];
    Some(Distribution {
        p50: percentile(50),
        p95: percentile(95),
        min: values[0],
        max: values[values.len() - 1],
    })
}

fn micros(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000_000.0
}

fn report_stage(series: usize, variant: Variant, stage: &str, values: Vec<Duration>) {
    if let Some(stats) = distribution(values.iter().copied()) {
        eprintln!(
            "measure series={series} variant={} stage={stage} n={} unit=us p50={:.3} p95={:.3} min={:.3} max={:.3}",
            variant.name(),
            values.len(),
            micros(stats.p50),
            micros(stats.p95),
            micros(stats.min),
            micros(stats.max),
        );
    } else {
        eprintln!(
            "measure series={series} variant={} stage={stage} n=0 value=N/A",
            variant.name(),
        );
    }
}

fn report_series(series: usize, variant: Variant, result: &Measurements) {
    assert_eq!(result.samples.len(), SAMPLES);
    for (stage, values) in [
        (
            "model-declare-and-admit-cpu",
            result.samples.iter().filter_map(|s| s.model).collect(),
        ),
        (
            "native-prepare-inclusive",
            result.samples.iter().map(|s| s.prepare).collect(),
        ),
        (
            "native-prepare-minus-nested-kernel",
            result
                .samples
                .iter()
                .map(|s| s.prepare - s.kernel)
                .collect(),
        ),
        (
            "control-allocation-only-not-graph-comparable",
            result.samples.iter().filter_map(|s| s.allocation).collect(),
        ),
        (
            "control-encoding-inclusive-not-graph-comparable",
            result.samples.iter().filter_map(|s| s.encoding).collect(),
        ),
        (
            "shared-effects-kernel-encoding-sum",
            result.samples.iter().map(|s| s.kernel).collect(),
        ),
        (
            "submit-scopes-and-prefix-custody",
            result.samples.iter().map(|s| s.submit).collect(),
        ),
        (
            "candidate-cpu-through-submit-exclusive-of-drain",
            result.samples.iter().map(|s| s.cpu_total).collect(),
        ),
        ("batch-prefix-wait-wall", result.waits.clone()),
        ("batch-owner-retirement-wall", result.retirement.clone()),
        (
            "final-output-readback-allocation-copy-map-wait-decode-wall",
            result.readbacks.clone(),
        ),
        (
            "final-output-oracle-check-cpu",
            result.oracle_checks.clone(),
        ),
    ] {
        report_stage(series, variant, stage, values);
    }
}

fn report_repeat_variation(results: &[[Measurements; 4]]) {
    for (index, variant) in Variant::ALL.into_iter().enumerate() {
        for (stage, select) in [
            (
                "candidate-cpu-through-submit",
                cpu_total as fn(&Sample) -> Duration,
            ),
            ("native-prepare-inclusive", prepare_time),
            ("shared-effects-kernel-encoding-sum", kernel_time),
            ("submit-scopes-and-prefix-custody", submit_time),
        ] {
            let per_series: Vec<_> = results
                .iter()
                .map(|result| {
                    distribution(result[index].samples.iter().map(select))
                        .expect("each series has measured samples")
                        .p95
                })
                .collect();
            let stats = distribution(per_series.into_iter()).unwrap();
            eprintln!(
                "repeat variant={} stage={stage} series={} p95-min-us={:.3} p95-max-us={:.3} p95-span-us={:.3}",
                variant.name(),
                results.len(),
                micros(stats.min),
                micros(stats.max),
                micros(stats.max - stats.min),
            );
        }
    }
    // This is an attribution trigger, not a timing assertion or a declaration of a winner.
    // Use the median of repeated series' P95 values, never a sum of stage percentiles.
    let p95s = |index: usize| {
        distribution(results.iter().map(|result| {
            distribution(result[index].samples.iter().map(cpu_total))
                .unwrap()
                .p95
        }))
        .unwrap()
    };
    let graph_p95 = p95s(0).p50;
    for (index, variant) in Variant::ALL.into_iter().enumerate().skip(1) {
        let control = p95s(index);
        let variation = control.max - control.min;
        let threshold = (control.p50.as_secs_f64() * 0.1).max(variation.as_secs_f64() * 2.0);
        let delta = graph_p95.as_secs_f64() - control.p50.as_secs_f64();
        eprintln!(
            "attribution control={} denominator=candidate-cpu-through-submit median-series-p95-delta-us={:.3} trigger-us={:.3} pause-and-attribute={} selection=none",
            variant.name(),
            delta * 1_000_000.0,
            threshold * 1_000_000.0,
            delta > threshold,
        );
    }
}

fn cpu_total(sample: &Sample) -> Duration {
    sample.cpu_total
}
fn prepare_time(sample: &Sample) -> Duration {
    sample.prepare
}
fn kernel_time(sample: &Sample) -> Duration {
    sample.kernel
}
fn submit_time(sample: &Sample) -> Duration {
    sample.submit
}

struct ReadyControl {
    output: controls::Outputs,
    retained: controls::Outputs,
    commands: wgpu::CommandBuffer,
}

struct SubmittedControl {
    output: controls::Outputs,
    submission: wgpu::SubmissionIndex,
    complete: Arc<AtomicBool>,
}

enum Pending {
    Graph(native::SubmittedGraph, workload::Versions),
    Control(SubmittedControl),
}

impl Pending {
    fn submission_index(&self) -> &wgpu::SubmissionIndex {
        match self {
            Self::Graph(submitted, _) => submitted.submission_index(),
            Self::Control(submitted) => &submitted.submission,
        }
    }

    fn is_complete(&self) -> bool {
        match self {
            Self::Graph(submitted, _) => submitted.is_complete(),
            Self::Control(submitted) => submitted.complete.load(Ordering::Acquire),
        }
    }

    fn textures(&self) -> [&wgpu::Texture; 6] {
        match self {
            Self::Graph(submitted, v) => [
                &v.original,
                &v.horizontal,
                &v.blurred,
                &v.complement,
                &v.composite,
                &v.histogram,
            ]
            .map(|version| submitted.resources().texture(version).unwrap()),
            Self::Control(submitted) => {
                let o = &submitted.output;
                [
                    &o.original,
                    &o.horizontal,
                    &o.blurred,
                    &o.complement,
                    &o.composite,
                    &o.histogram,
                ]
            }
        }
    }

    fn buffers(&self) -> [&wgpu::Buffer; 2] {
        match self {
            Self::Graph(submitted, v) => [&v.raw, &v.normalized]
                .map(|version| submitted.resources().buffer(version).unwrap()),
            Self::Control(submitted) => [&submitted.output.raw, &submitted.output.normalized],
        }
    }
}

// One narrow helper makes the timed Effects calls identical across all four choreographies.
// Resource lookup and view creation occur before the timer, inside the surrounding prepare.
enum Kernel<'a> {
    Image(graph::OperationKey, &'a wgpu::Texture, &'a wgpu::Texture),
    Luminance(&'a wgpu::Texture, &'a wgpu::Buffer),
    Normalize(&'a wgpu::Buffer),
    Histogram(&'a wgpu::Buffer, &'a wgpu::Texture),
}

impl Kernel<'_> {
    fn encode(
        self,
        device: &wgpu::Device,
        effects: &effects::Effects,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Duration {
        let view =
            |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
        match self {
            Self::Image(key, source, destination) => {
                let source = view(source);
                let destination = view(destination);
                let start = Instant::now();
                match key {
                    workload::BLUR_HORIZONTAL => {
                        effects.encode_blur_horizontal(device, encoder, &source, &destination)
                    }
                    workload::BLUR_VERTICAL => {
                        effects.encode_blur_vertical(device, encoder, &source, &destination)
                    }
                    workload::COMPLEMENT => {
                        effects.encode_complement(device, encoder, &source, &destination)
                    }
                    workload::HUD => effects.encode_hud(device, encoder, &source, &destination),
                    _ => panic!("unknown image operation in private measurement"),
                }
                start.elapsed()
            }
            Self::Luminance(source, counts) => {
                let source = view(source);
                let start = Instant::now();
                effects.encode_luminance(device, encoder, &source, counts);
                start.elapsed()
            }
            Self::Normalize(counts) => {
                let start = Instant::now();
                effects.encode_normalize(device, encoder, counts);
                start.elapsed()
            }
            Self::Histogram(counts, destination) => {
                let destination = view(destination);
                let start = Instant::now();
                effects.encode_histogram(encoder, counts, &destination);
                start.elapsed()
            }
        }
    }
}

fn graph_kernel<'a>(
    key: graph::OperationKey,
    r: &'a native::NativeResources,
    v: &workload::Versions,
) -> Result<Kernel<'a>, native::NativeError> {
    Ok(match key {
        workload::BLUR_HORIZONTAL => {
            Kernel::Image(key, r.texture(&v.original)?, r.texture(&v.horizontal)?)
        }
        workload::BLUR_VERTICAL => {
            Kernel::Image(key, r.texture(&v.horizontal)?, r.texture(&v.blurred)?)
        }
        workload::COMPLEMENT => {
            Kernel::Image(key, r.texture(&v.blurred)?, r.texture(&v.complement)?)
        }
        workload::HUD => Kernel::Image(key, r.texture(&v.complement)?, r.texture(&v.composite)?),
        workload::LUMINANCE => Kernel::Luminance(r.texture(&v.original)?, r.buffer(&v.raw)?),
        workload::NORMALIZE => Kernel::Normalize(r.buffer(&v.normalized)?),
        workload::HISTOGRAM => {
            Kernel::Histogram(r.buffer(&v.normalized)?, r.texture(&v.histogram)?)
        }
        _ => {
            return Err(native::NativeError::Callback(
                "unknown measurement operation",
            ));
        }
    })
}

fn control_step(
    device: &wgpu::Device,
    effects: &effects::Effects,
    output: &controls::Outputs,
    encoder: &mut wgpu::CommandEncoder,
    key: graph::OperationKey,
) -> Duration {
    let kernel = match key {
        workload::BLUR_HORIZONTAL => {
            controls::clear(encoder, &output.horizontal);
            Kernel::Image(key, &output.original, &output.horizontal)
        }
        workload::BLUR_VERTICAL => {
            controls::clear(encoder, &output.blurred);
            Kernel::Image(key, &output.horizontal, &output.blurred)
        }
        workload::COMPLEMENT => {
            controls::clear(encoder, &output.complement);
            Kernel::Image(key, &output.blurred, &output.complement)
        }
        workload::HUD => {
            controls::clear(encoder, &output.composite);
            Kernel::Image(key, &output.complement, &output.composite)
        }
        workload::LUMINANCE => {
            encoder.clear_buffer(&output.raw, 0, None);
            Kernel::Luminance(&output.original, &output.raw)
        }
        workload::NORMALIZE => {
            encoder.copy_buffer_to_buffer(
                &output.raw,
                0,
                &output.normalized,
                0,
                effects::COUNTS_BUFFER_SIZE,
            );
            Kernel::Normalize(&output.normalized)
        }
        workload::HISTOGRAM => {
            controls::clear(encoder, &output.histogram);
            Kernel::Histogram(&output.normalized, &output.histogram)
        }
        _ => panic!("unknown static control operation"),
    };
    kernel.encode(device, effects, encoder)
}

struct BlurSlot<'a> {
    output: &'a controls::Outputs,
}

impl BlurSlot<'_> {
    fn encode(
        &self,
        device: &wgpu::Device,
        effects: &effects::Effects,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Duration {
        control_step(
            device,
            effects,
            self.output,
            encoder,
            workload::BLUR_HORIZONTAL,
        ) + control_step(
            device,
            effects,
            self.output,
            encoder,
            workload::BLUR_VERTICAL,
        )
    }
}

#[derive(Resource)]
struct Capture {
    device: wgpu::Device,
    effects: effects::Effects,
    output: Option<controls::Outputs>,
}

#[derive(Resource, Default)]
struct Encoding(Option<wgpu::CommandEncoder>);

#[derive(Resource, Default)]
struct KernelTiming {
    elapsed: Duration,
    calls: usize,
}

struct ScheduleControl {
    world: World,
    schedule: Schedule,
}

impl ScheduleControl {
    fn new(device: &wgpu::Device, effects: &effects::Effects) -> Self {
        let mut world = World::new();
        world.insert_resource(Capture {
            device: device.clone(),
            effects: effects.clone(),
            output: None,
        });
        world.insert_resource(Encoding::default());
        world.insert_resource(KernelTiming::default());
        let mut schedule = Schedule::default();
        schedule.set_executor(nara_ecs::bevy_ecs::schedule::SingleThreadedExecutor::new());
        schedule.add_systems(
            (
                luminance, normalize, histogram, horizontal, vertical, complement, hud,
            )
                .chain(),
        );
        // Compile topology once without encoding or submitting an unqualified warmup.
        schedule
            .initialize(&mut world)
            .expect("measurement schedule admission");
        Self { world, schedule }
    }

    fn encode(
        &mut self,
        output: &controls::Outputs,
        encoder: wgpu::CommandEncoder,
    ) -> (wgpu::CommandBuffer, Duration) {
        self.world.resource_mut::<Capture>().output = Some(output.clone());
        self.world.resource_mut::<Encoding>().0 = Some(encoder);
        *self.world.resource_mut::<KernelTiming>() = KernelTiming::default();
        self.schedule.run(&mut self.world);
        let commands = self
            .world
            .resource_mut::<Encoding>()
            .0
            .take()
            .unwrap()
            .finish();
        // Do not leave a hidden last-output owner in the reused World between batches.
        self.world.resource_mut::<Capture>().output.take();
        let timing = self.world.resource::<KernelTiming>();
        assert_eq!(timing.calls, EXECUTION_ORDER.len());
        (commands, timing.elapsed)
    }
}

fn schedule_step(
    key: graph::OperationKey,
    capture: Res<Capture>,
    mut encoding: ResMut<Encoding>,
    mut timing: ResMut<KernelTiming>,
) {
    timing.elapsed += control_step(
        &capture.device,
        &capture.effects,
        capture.output.as_ref().unwrap(),
        encoding.0.as_mut().unwrap(),
        key,
    );
    timing.calls += 1;
}

fn luminance(c: Res<Capture>, e: ResMut<Encoding>, t: ResMut<KernelTiming>) {
    schedule_step(workload::LUMINANCE, c, e, t);
}
fn normalize(c: Res<Capture>, e: ResMut<Encoding>, t: ResMut<KernelTiming>) {
    schedule_step(workload::NORMALIZE, c, e, t);
}
fn histogram(c: Res<Capture>, e: ResMut<Encoding>, t: ResMut<KernelTiming>) {
    schedule_step(workload::HISTOGRAM, c, e, t);
}
fn horizontal(c: Res<Capture>, e: ResMut<Encoding>, t: ResMut<KernelTiming>) {
    schedule_step(workload::BLUR_HORIZONTAL, c, e, t);
}
fn vertical(c: Res<Capture>, e: ResMut<Encoding>, t: ResMut<KernelTiming>) {
    schedule_step(workload::BLUR_VERTICAL, c, e, t);
}
fn complement(c: Res<Capture>, e: ResMut<Encoding>, t: ResMut<KernelTiming>) {
    schedule_step(workload::COMPLEMENT, c, e, t);
}
fn hud(c: Res<Capture>, e: ResMut<Encoding>, t: ResMut<KernelTiming>) {
    schedule_step(workload::HUD, c, e, t);
}

// Match the executor's three checked scope categories at allocation, encoding and submission.
// This is benchmark-local instrumentation, not a replacement native executor or fault proof.
fn checked_native<T>(device: &wgpu::Device, work: impl FnOnce() -> T) -> T {
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = catch_unwind(AssertUnwindSafe(work));
    let validation = pollster::block_on(validation.pop());
    let oom = pollster::block_on(oom.pop());
    let internal = pollster::block_on(internal.pop());
    assert!(
        validation.is_none() && oom.is_none() && internal.is_none(),
        "control native error: validation={validation:?} oom={oom:?} internal={internal:?}",
    );
    result.expect("private measurement native call panicked")
}

fn allocate_control(device: &wgpu::Device, input: &wgpu::Texture) -> controls::Outputs {
    let graph::ResourceDesc::Texture {
        native: texture, ..
    } = workload::texture_desc(2)
    else {
        unreachable!()
    };
    let graph::ResourceDesc::Buffer { native: raw, .. } = workload::buffer_desc(false) else {
        unreachable!()
    };
    let graph::ResourceDesc::Buffer {
        native: normalized, ..
    } = workload::buffer_desc(true)
    else {
        unreachable!()
    };
    controls::Outputs {
        original: input.clone(),
        horizontal: device.create_texture(&texture),
        blurred: device.create_texture(&texture),
        complement: device.create_texture(&texture),
        composite: device.create_texture(&texture),
        histogram: device.create_texture(&texture),
        raw: device.create_buffer(&raw),
        normalized: device.create_buffer(&normalized),
    }
}

fn prepare_control(
    gpu: &gpu::Gpu,
    effects: &effects::Effects,
    schedule: &mut ScheduleControl,
    variant: Variant,
    input: &wgpu::Texture,
    sample: &mut Sample,
) -> ReadyControl {
    let prepare_start = Instant::now();
    let start = Instant::now();
    let output = checked_native(&gpu.device, || allocate_control(&gpu.device, input));
    sample.allocation = Some(start.elapsed());
    let retained = output.clone();
    let start = Instant::now();
    let (commands, kernel) = checked_native(&gpu.device, || {
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("measure-fresh-control-encoder"),
            });
        if matches!(variant, Variant::Ecs) {
            return schedule.encode(&output, encoder);
        }
        let mut kernel = Duration::ZERO;
        for key in EXECUTION_ORDER {
            if matches!(variant, Variant::Slots) {
                if key == workload::BLUR_HORIZONTAL {
                    kernel +=
                        BlurSlot { output: &output }.encode(&gpu.device, effects, &mut encoder);
                    continue;
                }
                if key == workload::BLUR_VERTICAL {
                    continue;
                }
            }
            kernel += control_step(&gpu.device, effects, &output, &mut encoder, key);
        }
        (encoder.finish(), kernel)
    });
    sample.encoding = Some(start.elapsed());
    sample.kernel = kernel;
    sample.prepare = prepare_start.elapsed();
    ReadyControl {
        output,
        retained,
        commands,
    }
}

fn submit_control(gpu: &gpu::Gpu, ready: ReadyControl) -> SubmittedControl {
    let ReadyControl {
        output,
        retained,
        commands,
    } = ready;
    let complete = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&complete);
    let submission = checked_native(&gpu.device, || {
        let submission = gpu.queue.submit([commands]);
        gpu.queue.on_submitted_work_done(move || {
            drop(retained);
            observed.store(true, Ordering::Release);
        });
        submission
    });
    SubmittedControl {
        output,
        submission,
        complete,
    }
}

struct Harness {
    gpu: gpu::Gpu,
    effects: effects::Effects,
    schedule: ScheduleControl,
    input: Arc<wgpu::Texture>,
}

impl Harness {
    fn sample(&mut self, variant: Variant) -> (Sample, Pending) {
        let mut sample = Sample::default();
        let total_start = Instant::now();
        let input = &self.input;
        let pending = match variant {
            Variant::Graph => {
                let start = Instant::now();
                let workload = workload::build(&workload::OPERATION_KEYS).unwrap();
                sample.model = Some(start.elapsed());
                let versions = workload.versions;
                let imports = [(
                    versions.original.clone(),
                    native::NativeResource::Texture(Arc::clone(input)),
                )];
                let mut kernel_calls = 0;
                let start = Instant::now();
                let ready =
                    native::prepare(
                        &self.gpu,
                        workload.graph,
                        &imports,
                        |operation, resources, encoder| {
                            sample.kernel += graph_kernel(operation.key, resources, &versions)?
                                .encode(&self.gpu.device, &self.effects, encoder);
                            kernel_calls += 1;
                            Ok(())
                        },
                    )
                    .expect("native graph preparation before measurement submission");
                sample.prepare = start.elapsed();
                assert_eq!(kernel_calls, EXECUTION_ORDER.len());
                let start = Instant::now();
                let submitted = native::submit(&self.gpu, ready).expect("measured graph submit");
                sample.submit = start.elapsed();
                Pending::Graph(submitted, versions)
            }
            Variant::Ordered | Variant::Slots | Variant::Ecs => {
                let ready = prepare_control(
                    &self.gpu,
                    &self.effects,
                    &mut self.schedule,
                    variant,
                    input,
                    &mut sample,
                );
                let start = Instant::now();
                let submitted = submit_control(&self.gpu, ready);
                sample.submit = start.elapsed();
                Pending::Control(submitted)
            }
        };
        sample.cpu_total = total_start.elapsed();
        (sample, pending)
    }

    fn phase(
        &mut self,
        series: usize,
        samples: usize,
        measured: bool,
        expected: &Expected,
    ) -> [Measurements; 4] {
        assert_eq!(
            samples % BATCH,
            0,
            "use full fixed-size batches for equal wait denominators"
        );
        let mut results = std::array::from_fn(|_| Measurements::reserved());
        let mut pending = Vec::with_capacity(BATCH);
        for batch in 0..samples / BATCH {
            // Interleave complete bounded batches, rotating the first variant across both batch
            // and series. Every variant enters with a completed prior prefix and no old outputs.
            for offset in 0..Variant::ALL.len() {
                let index = (series + batch + offset) % Variant::ALL.len();
                let variant = Variant::ALL[index];
                for _ in 0..BATCH {
                    let (sample, output) = self.sample(variant);
                    if measured {
                        results[index].samples.push(sample);
                    }
                    pending.push(output);
                }
                let last = pending.last().unwrap();
                let start = Instant::now();
                wait_for(&self.gpu, last.submission_index().clone());
                let wait = start.elapsed();
                assert!(pending.iter().all(Pending::is_complete));
                if measured {
                    results[index].waits.push(wait);
                    if batch + 1 == samples / BATCH {
                        let (readback, check) = qualify(&self.gpu, last, expected);
                        results[index].readbacks.push(readback);
                        results[index].oracle_checks.push(check);
                    }
                }
                let start = Instant::now();
                pending.clear();
                let retirement = start.elapsed();
                if measured {
                    results[index].retirement.push(retirement);
                }
            }
        }
        results
    }
}

fn wait_for(gpu: &gpu::Gpu, submission: wgpu::SubmissionIndex) {
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(TIMEOUT),
        })
        .expect("measurement exact-prefix completion timeout");
}

struct Expected {
    textures: [Vec<[f32; 4]>; 6],
    counts: [u32; 4],
    normalized: [f32; 4],
}

impl Expected {
    fn new() -> Self {
        let original = oracle::scene(workload::WIDTH, workload::HEIGHT, 0);
        let horizontal = oracle::blur(&original, workload::WIDTH, workload::HEIGHT, true);
        let blurred = oracle::blur(&horizontal, workload::WIDTH, workload::HEIGHT, false);
        let complement = oracle::complement(&blurred);
        let composite = oracle::hud(&complement, workload::WIDTH);
        let counts = oracle::luminance_counts(&original);
        let normalized = oracle::normalized(counts);
        let mut histogram = Vec::with_capacity(original.len());
        for y in 0..workload::HEIGHT {
            for x in 0..workload::WIDTH {
                let bin = (x * 4 / workload::WIDTH) as usize;
                let covered = y as f32 + 0.5 >= workload::HEIGHT as f32 * (1.0 - normalized[bin]);
                histogram.push(if covered { [1.0; 4] } else { [0.0; 4] });
            }
        }
        Self {
            textures: [
                original, horizontal, blurred, complement, composite, histogram,
            ],
            counts,
            normalized,
        }
    }
}

fn qualify(gpu: &gpu::Gpu, pending: &Pending, expected: &Expected) -> (Duration, Duration) {
    let textures = pending.textures();
    let buffers = pending.buffers();
    let start = Instant::now();
    let textures =
        textures.map(|texture| gpu.read_rgba32(texture, workload::WIDTH, workload::HEIGHT));
    let [raw, normalized] = buffers.map(|buffer| gpu.read_buffer(buffer));
    let readback = start.elapsed();
    let start = Instant::now();
    for (index, (actual, expected)) in textures.iter().zip(&expected.textures).enumerate() {
        // Original and histogram are exact; computed image results permit float rounding.
        oracle::assert_pixels(
            actual,
            expected,
            if index == 0 || index == 5 {
                0.0
            } else {
                1.0e-6
            },
        );
    }
    let actual: [u32; 4] = std::array::from_fn(|index| {
        u32::from_le_bytes(raw[index * 4..index * 4 + 4].try_into().unwrap())
    });
    assert_eq!(actual, expected.counts);
    assert_eq!(normalized.len(), effects::COUNTS_BUFFER_SIZE as usize);
    for (actual, expected) in normalized.chunks_exact(4).zip(expected.normalized) {
        let actual = f32::from_le_bytes(actual.try_into().unwrap());
        assert!(actual.is_finite() && (actual - expected).abs() <= 1.0e-6);
    }
    (readback, start.elapsed())
}

fn print_model_and_scope() {
    let model = workload::build(&workload::OPERATION_KEYS).unwrap();
    assert_eq!(
        model
            .graph
            .operations()
            .map(|op| op.key)
            .collect::<Vec<_>>(),
        EXECUTION_ORDER
    );
    assert_eq!(model.graph.resources().len(), 8);
    assert_eq!(model.graph.outputs().len(), 8);
    assert_eq!(
        model
            .graph
            .resources()
            .iter()
            .filter(|r| r.is_imported())
            .count(),
        1
    );
    assert_eq!(model.graph.copy_dependencies().count(), 1);
    assert_eq!(
        model.graph.modeled_resource_bytes(),
        TEXTURE_BYTES + LOCAL_BYTES
    );
    let declared_accesses: usize = model
        .graph
        .operations()
        .map(|op| op.reads.len() + op.writes.len())
        .sum();
    assert_eq!(declared_accesses, 13);
    eprintln!(
        "model dimensions={}x{} format=Rgba32Float transfer=linear alpha=premultiplied kernels=7 compute-passes=6 render-kernels=1 clear-passes=5 buffer-clears=1 preserved-copy-bytes=16 texture-views-per-candidate=15 effect-bind-groups-per-candidate=6",
        workload::WIDTH,
        workload::HEIGHT,
    );
    eprintln!(
        "model common-local-textures=5 common-local-buffers=2 common-encoders=1 common-submissions=1 imported-textures=1 logical-local-bytes={LOCAL_BYTES} logical-import-bytes={TEXTURE_BYTES} batch={BATCH} peak-batch-local-plus-input-bytes={} resident-uploaded-inputs=1 readback-bytes-per-qualification={} native-allocation-counts-and-bytes=not-measured",
        LOCAL_BYTES * BATCH as u64 + TEXTURE_BYTES,
        6 * TEXTURE_BYTES + 2 * effects::COUNTS_BUFFER_SIZE,
    );
    eprintln!(
        "model graph-default-capacity-metadata-envelope-bytes={} graph-native-two-table-payload-bytes={} control-output-handle-struct-bytes={} allocator-rounding-arc-driver-ecs-heaps=excluded rss-and-gpu-heap-claim=none",
        model.graph.modeled_metadata_bytes(),
        2 * model.graph.resources().len() * size_of::<native::NativeResource>(),
        size_of::<controls::Outputs>(),
    );
    eprintln!(
        "choreography graph=8-resource-declarations,7-operation-declarations,13-explicit-accesses,1-implicit-copy-read,5-local-dependency-edges,8-output-pins ordered=7-static-kernel-calls,6-sequence-adjacencies slots=6-static-invocations-including-2-private-blur-passes ecs=7-systems,6-chain-edges all=5-explicit-clears,1-zero,1-preserve-copy,8-retained-handles core-production-changes=0",
    );
    eprintln!(
        "scope fresh-resources-and-encoder=true reused=pipelines,read-only-input,ecs-topology graph-model-rebuilt=true control-model-admission=N/A graph-allocation-vs-encoding-split=unobservable control-split=diagnostic-only scopes=validation,oom,internal-at-each-of-3-native-stages fault-protocols=not-equivalent GPU-timestamps=none GPU-kernel-time=not-measured timer-overhead=not-corrected readback-submissions-per-qualification=8",
    );
    eprintln!(
        "scope waits=one-exact-prefix-per-full-batch-outside-samples queue-submit-may-still-incur-driver-backpressure=true readback=outside-samples retirement=outside-samples output-custody=all-versions-through-prefix batch-wait-not-per-kernel-latency=true p95=nearest-rank no-p95-addition=true series=within-one-process-not-independent-machines graph-cost-trigger=attribution-only",
    );
    eprintln!(
        "fairness ecs-world-and-schedule-lifetime=whole-run ecs-initialize=once controls-execute-fresh-schedule-path=not-called setup-in-architecture-speed-selection=false input-upload=once-before-qualification-and-warmups per-candidate-upload=none initialization-order=identical retain=all-8-versions-until-exact-prefix-completion Bevy-schedule-generalization=none",
    );
}

#[test]
#[ignore = "slow real-GPU measurement: 3 series, each 128 warmups + 1000 samples per variant; run serially with output"]
fn real_gpu_graph_and_viable_controls_measure_cpu_stages() {
    eprintln!(
        "measurement-config os={} arch={} debug-assertions={} compile-rustup-toolchain={} wgpu=30.0.1 warmups-per-variant-per-series={WARMUPS} samples-per-variant-per-series={SAMPLES} series={SERIES} batch={BATCH}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        cfg!(debug_assertions),
        option_env!("RUSTUP_TOOLCHAIN").unwrap_or("not-captured"),
    );
    if cfg!(debug_assertions) {
        eprintln!(
            "measurement-caveat debug profile: exploratory instrumentation, not optimized interface-freeze cost evidence; use --release"
        );
    }
    print_model_and_scope();
    let expected = Expected::new();
    let start = Instant::now();
    let gpu = gpu::Gpu::new();
    let device_setup = start.elapsed();
    let start = Instant::now();
    let effects = effects::Effects::new(&gpu.device);
    let pipeline_setup = start.elapsed();
    let start = Instant::now();
    let graph::ResourceDesc::Texture { native, .. } = workload::texture_desc(1) else {
        unreachable!()
    };
    let input = Arc::new(gpu.device.create_texture(&native));
    gpu.upload_rgba32(
        &input,
        workload::WIDTH,
        workload::HEIGHT,
        &expected.textures[0],
    );
    let upload_setup = start.elapsed();
    let start = Instant::now();
    let upload_submission = gpu.queue.submit([]);
    let upload_submit = start.elapsed();
    let start = Instant::now();
    wait_for(&gpu, upload_submission);
    let upload_wait = start.elapsed();
    let start = Instant::now();
    let schedule = ScheduleControl::new(&gpu.device, &effects);
    let schedule_setup = start.elapsed();
    eprintln!(
        "setup unit=us device={:.3} pipelines={:.3} input-alloc-encode-upload={:.3} upload-submit={:.3} upload-prefix-wait={:.3} ecs-world-topology-initialize={:.3} exclusion=all-sample-intervals",
        micros(device_setup),
        micros(pipeline_setup),
        micros(upload_setup),
        micros(upload_submit),
        micros(upload_wait),
        micros(schedule_setup),
    );
    let mut harness = Harness {
        gpu,
        effects,
        schedule,
        input,
    };

    // Qualify the actual instrumented encoders, not just existing uninstrumented controls.
    // Existing native failure/history tests are not repeated by this measurement entry.
    // A broken comparison fails before ANY measured/warmup sample.
    for variant in Variant::ALL {
        let (_, output) = harness.sample(variant);
        let (readback, check) = qualify(&harness.gpu, &output, &expected);
        wait_for(&harness.gpu, output.submission_index().clone());
        assert!(output.is_complete());
        eprintln!(
            "qualification variant={} readback-total-us={:.3} oracle-cpu-total-us={:.3} status=passed timing-samples=0",
            variant.name(),
            micros(readback),
            micros(check),
        );
    }

    let mut results = Vec::with_capacity(SERIES);
    for series in 0..SERIES {
        eprintln!("measurement-progress series={} phase=warmup", series + 1);
        harness.phase(series, WARMUPS, false, &expected);
        eprintln!("measurement-progress series={} phase=sampling", series + 1);
        let result = harness.phase(series, SAMPLES, true, &expected);
        for (index, variant) in Variant::ALL.into_iter().enumerate() {
            report_series(series + 1, variant, &result[index]);
        }
        results.push(result);
    }
    report_repeat_variation(&results);
    eprintln!(
        "measurement-complete measured-candidates={} warmup-candidates={} qualification=before-samples-and-each-series-final-output adapter-scope=printed-by-Gpu::new production-renderer-qualification=none cost-winner=not-automatically-selected",
        SERIES * SAMPLES * Variant::ALL.len(),
        SERIES * WARMUPS * Variant::ALL.len(),
    );
}
