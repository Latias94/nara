//! Opt-in observations of the existing source, retained Views and original renderer.

use super::{Viewer, renderer::Renderer};
use anyhow::{Context as _, Result, ensure};
use open_gpui::{FrameEvent, FrameTimingCollector, profiler};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{Subscriber, span::Id};
use tracing_subscriber::{
    Layer,
    layer::{Context as TraceContext, SubscriberExt},
    registry::LookupSpan,
};

#[cfg(feature = "ui-work-allocations")]
use {
    criterion::measurement::Measurement as _,
    open_gpui_bench_metrics::{AllocationCounter, AllocationMetric, MetricScope, allocation_stats},
};

const WARMUPS: usize = 8;
const SAMPLES: usize = 64;
const CASES: [&str; 5] = [
    "equal_observation",
    "static_panels_live_preview",
    "local_property_edit",
    "list_height_change",
    "property_edit_broad_refresh",
];
const PHASES: [&str; 5] = ["mutation", "query_snapshot", "publish", "preview", "settle"];
const SOURCE_PHASES: [&str; 4] = [
    "source_selection",
    "source_prepare",
    "source_commit",
    "source_drive",
];
const DRIVE_PHASES: &[&str] = &[
    "editor_preview",
    "editor_preview_compile",
    "editor_preview_drive",
    "editor_preview_runtime",
    "editor_preview_materialize",
    "editor_publish_model",
    "scene_expand",
    "scene_structure",
    "scene_compile_output",
    "scene_authoring_finish",
    "scene_native_preview",
    "scene_flat_admission",
    "scene_prepare_unbound",
    "scene_bind",
    "scene_bind_revalidate",
    "scene_publish",
    "scene_final_validate",
];

#[derive(Clone, Copy, Default)]
struct SpanObservation {
    cpu: Duration,
    calls: u64,
}

struct DriveLayer(Arc<Mutex<[SpanObservation; DRIVE_PHASES.len()]>>);
struct SpanStart(Instant);

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for DriveLayer {
    fn on_enter(&self, id: &Id, cx: TraceContext<'_, S>) {
        let span = cx.span(id).expect("an entered observation span exists");
        if matches!(
            span.metadata().target(),
            "nara::editor::work" | "nara::scene::work"
        ) && DRIVE_PHASES.contains(&span.name())
        {
            span.extensions_mut().insert(SpanStart(Instant::now()));
        }
    }

    fn on_exit(&self, id: &Id, cx: TraceContext<'_, S>) {
        let span = cx.span(id).expect("an exiting observation span exists");
        if let Some(start) = span.extensions_mut().remove::<SpanStart>() {
            let duration = start.0.elapsed();
            let index = DRIVE_PHASES
                .iter()
                .position(|name| *name == span.name())
                .unwrap();
            let mut work = self.0.lock().unwrap();
            work[index].cpu += duration;
            work[index].calls += 1;
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Observation {
    cpu: Duration,
    #[cfg(feature = "ui-work-allocations")]
    allocations: u64,
    #[cfg(feature = "ui-work-allocations")]
    allocated_bytes: u64,
}

pub(super) fn observe<T>(operation: impl FnOnce() -> T) -> Result<(T, Observation)> {
    #[cfg(feature = "ui-work-allocations")]
    let (calls, bytes, call_start, byte_start) = {
        let calls =
            AllocationCounter::new(AllocationMetric::Allocations, MetricScope::CallingThread)?;
        let bytes =
            AllocationCounter::new(AllocationMetric::AllocatedBytes, MetricScope::CallingThread)?;
        let call_start = calls.start();
        let byte_start = bytes.start();
        (calls, bytes, call_start, byte_start)
    };
    let start = Instant::now();
    let value = operation();
    let observation = Observation {
        cpu: start.elapsed(),
        #[cfg(feature = "ui-work-allocations")]
        allocations: calls.end(call_start) as u64,
        #[cfg(feature = "ui-work-allocations")]
        allocated_bytes: bytes.end(byte_start) as u64,
    };
    Ok((value, observation))
}

#[derive(Default)]
struct Sample {
    phases: [Observation; 5],
    source_phases: [Observation; 4],
    drive_phases: [SpanObservation; DRIVE_PHASES.len()],
    total: Duration,
    draw: [Duration; 3],
    draws: u64,
    record: Duration,
    counters: [u64; 9],
    retained_frames: usize,
}

fn counters(viewer: &Viewer) -> [u64; 9] {
    let work = &viewer.panel_work;
    let engine = viewer.engine.as_ref().unwrap().borrow();
    let renderer = viewer.renderer.as_ref().unwrap();
    [
        engine.panel_query_pages.get(),
        engine.panel_query_records.get(),
        work.projected_notifications.get(),
        work.hierarchy_renders.get(),
        work.inspector_renders[0].get(),
        work.inspector_renders[1].get(),
        work.hierarchy_row_callbacks.get(),
        renderer.record_calls.get(),
        renderer.submissions.get(),
    ]
}

fn settle(viewer: &Viewer) -> Result<()> {
    let application = viewer.application.as_ref().unwrap();
    let surface = viewer.surface.as_ref().unwrap();
    let renderer = viewer.renderer.as_ref().unwrap();
    for _ in 0..8 {
        application.update(|_| {});
        renderer.promote()?;
        renderer.submit_pending()?;
        application.update(|_| {});
        renderer.promote()?;
        if renderer.pending() {
            continue;
        }
        if !surface.needs_frame() {
            return Ok(());
        }
        surface.drive_frame()?;
    }
    anyhow::bail!("the bounded observation did not settle its original hosted demand")
}

pub(super) struct UiWork {
    case: usize,
    iteration: usize,
    samples: Vec<Sample>,
    collector: FrameTimingCollector,
    initialized: bool,
    trace_was_changed: bool,
    drive_work: Arc<Mutex<[SpanObservation; DRIVE_PHASES.len()]>>,
    _subscriber: tracing::subscriber::DefaultGuard,
}

impl UiWork {
    pub fn new() -> Self {
        let trace_was_changed = profiler::set_trace_enabled(true);
        let drive_work = Arc::new(Mutex::new([SpanObservation::default(); DRIVE_PHASES.len()]));
        let subscriber =
            tracing_subscriber::registry().with(DriveLayer(drive_work.clone()).with_filter(
                tracing_subscriber::filter::filter_fn(|metadata| {
                    matches!(
                        metadata.target(),
                        "nara::editor::work" | "nara::scene::work"
                    )
                }),
            ));
        let subscriber = tracing::subscriber::set_default(subscriber);
        println!(
            "NARA_GPUI_WORK_CONFIG warmups={WARMUPS} samples={SAMPLES} source_rows=10001 debug_assertions={} allocation_observer={} font=Segoe_UI geometry=1280x640 scale=host_logical no_readback=true timings=instrumented_cpu_not_gpu",
            cfg!(debug_assertions),
            cfg!(feature = "ui-work-allocations")
        );
        Self {
            case: 0,
            iteration: 0,
            samples: Vec::with_capacity(SAMPLES),
            collector: FrameTimingCollector::new(),
            initialized: false,
            trace_was_changed,
            drive_work,
            _subscriber: subscriber,
        }
    }

    pub fn advance(&mut self, viewer: &Viewer) -> Result<bool> {
        if self.case == CASES.len() {
            return Ok(true);
        }
        if !self.initialized {
            settle(viewer)?;
            let snapshot = viewer.engine.as_ref().unwrap().borrow().panels_snapshot()?;
            let sprite = snapshot
                .sprite
                .context("the measurement requires its original Sprite")?;
            viewer
                .engine
                .as_ref()
                .unwrap()
                .borrow_mut()
                .select_entity(&snapshot.basis, sprite)?;
            viewer.publish_panels()?;
            viewer.request_frame(false)?;
            settle(viewer)?;
            let geometry = viewer.surface.as_ref().unwrap().geometry();
            println!(
                "NARA_GPUI_WORK_ROOT logical_size={:?} physical_size={:?} scale={} retained_model_rows=10001 retained_list_metadata=O_logical_rows",
                geometry.size(),
                geometry.physical_size(),
                geometry.scale_factor()
            );
            self.initialized = true;
        }
        settle(viewer)?;
        self.collector.collect_unseen();
        let before = counters(viewer);
        let renderer = viewer.renderer.as_ref().unwrap();
        let record_before = renderer.record_cpu.get();
        *self.drive_work.lock().unwrap() = [SpanObservation::default(); DRIVE_PHASES.len()];
        let start = Instant::now();
        let mut sample = Sample::default();
        let (mutation, observation) = observe(|| -> Result<()> {
            if matches!(self.case, 2 | 4) {
                let (basis, sprite) = viewer.application.as_ref().unwrap().update(|cx| {
                    let snapshot = &viewer.model.as_ref().unwrap().read(cx).snapshot;
                    (snapshot.basis.clone(), snapshot.sprite.clone().unwrap())
                });
                viewer.engine.as_ref().unwrap().borrow_mut().set_sprite_x(
                    &basis,
                    &sprite,
                    if self.iteration.is_multiple_of(2) {
                        16.
                    } else {
                        24.
                    },
                )?;
            }
            Ok(())
        })?;
        mutation?;
        sample.phases[0] = observation;
        sample.drive_phases = *self.drive_work.lock().unwrap();
        if matches!(self.case, 2 | 4) {
            for (name, phase) in DRIVE_PHASES.iter().zip(&sample.drive_phases) {
                ensure!(
                    phase.calls == 1,
                    "the original source drive reached {name} {} times instead of once",
                    phase.calls
                );
            }
            sample.source_phases = std::array::from_fn(|index| {
                viewer.engine.as_ref().unwrap().borrow().source_steps[index].take()
            });
        }
        let (snapshot, observation) =
            observe(|| viewer.engine.as_ref().unwrap().borrow().panels_snapshot())?;
        let snapshot = snapshot?;
        ensure!(
            snapshot.entities.len() == 10_001 && snapshot.selected == snapshot.sprite,
            "the observation lost its original selected source"
        );
        if matches!(self.case, 2 | 4) {
            ensure!(
                snapshot.x
                    == Some(if self.iteration.is_multiple_of(2) {
                        16.
                    } else {
                        24.
                    }),
                "the original property edit did not reach its source projection"
            );
        }
        sample.phases[1] = observation;
        let (_, observation) = observe(|| {
            viewer.application.as_ref().unwrap().update(|cx| {
                viewer
                    .model
                    .as_ref()
                    .unwrap()
                    .update(cx, |model, cx| model.publish(snapshot, cx));
                if self.case == 3 {
                    viewer
                        .root
                        .unwrap()
                        .update(cx, |root, _, cx| {
                            root.hierarchy
                                .update(cx, |hierarchy, cx| hierarchy.toggle_details(cx));
                        })
                        .unwrap();
                }
            });
        })?;
        sample.phases[2] = observation;
        let (preview, observation) = observe(|| -> Result<()> {
            if self.case != 0 {
                viewer.refresh_preview()?;
            }
            if self.case == 4 {
                viewer.request_frame(true)?;
            }
            Ok(())
        })?;
        preview?;
        sample.phases[3] = observation;
        let (settled, observation) = observe(|| settle(viewer))?;
        settled?;
        sample.phases[4] = observation;
        sample.total = start.elapsed();
        sample.record = renderer.record_cpu.get() - record_before;
        sample.retained_frames = renderer.retained_frames();
        let after = counters(viewer);
        sample.counters = std::array::from_fn(|index| after[index] - before[index]);
        for event in self.collector.collect_unseen() {
            if let FrameEvent::Draw(draw) = event {
                ensure!(
                    draw.window_id == viewer.root.unwrap().window_id(),
                    "an unrelated Window entered the observation"
                );
                sample.draws += 1;
                sample.draw[0] += draw.phases.request_layout;
                sample.draw[1] += draw.phases.prepaint;
                sample.draw[2] += draw.phases.paint;
            }
        }
        if self.case == 0 {
            ensure!(
                sample.draws == 0 && sample.counters[7..] == [0, 0],
                "an equal observation rebuilt or submitted UI"
            );
        } else {
            ensure!(
                sample.draws == 1 && sample.counters[7..] == [1, 1],
                "a changed observation did not produce one original draw/record/submission"
            );
        }
        if self.case < 3 {
            ensure!(
                sample.counters[3] == 0 && sample.counters[6] == 0,
                "an unchanged Hierarchy rebuilt rows"
            );
        }
        if matches!(self.case, 0 | 1 | 3) {
            ensure!(
                sample.counters[4..6] == [0, 0],
                "an unchanged Inspector rebuilt"
            );
        } else {
            ensure!(
                sample.counters[4] > 0 && sample.counters[5] > 0,
                "a property change did not rebuild both Inspector projections"
            );
        }
        if matches!(self.case, 3 | 4) {
            ensure!(
                sample.counters[3] > 0 && sample.counters[6] > 0,
                "the list change or broad refresh did not rebuild visible rows"
            );
        }
        if self.iteration >= WARMUPS {
            self.samples.push(sample);
        }
        self.iteration += 1;
        if self.iteration == WARMUPS + SAMPLES {
            self.report(viewer);
            self.samples.clear();
            self.case += 1;
            self.iteration = 0;
        }
        Ok(self.case == CASES.len())
    }

    fn report(&self, viewer: &Viewer) {
        let name = CASES[self.case];
        report_cpu(
            name,
            "owner_sample",
            self.samples.iter().map(|sample| sample.total),
        );
        for (index, phase) in PHASES.iter().enumerate() {
            report_cpu(
                name,
                phase,
                self.samples.iter().map(|sample| sample.phases[index].cpu),
            );
            #[cfg(feature = "ui-work-allocations")]
            println!(
                "NARA_GPUI_WORK_ALLOC case={name} phase={phase} scope=calling_thread totals_calls={} totals_requested_bytes={}",
                self.samples
                    .iter()
                    .map(|sample| sample.phases[index].allocations)
                    .sum::<u64>(),
                self.samples
                    .iter()
                    .map(|sample| sample.phases[index].allocated_bytes)
                    .sum::<u64>(),
            );
        }
        if matches!(self.case, 2 | 4) {
            for (index, phase) in SOURCE_PHASES.iter().enumerate() {
                report_cpu(
                    name,
                    phase,
                    self.samples
                        .iter()
                        .map(|sample| sample.source_phases[index].cpu),
                );
                #[cfg(feature = "ui-work-allocations")]
                println!(
                    "NARA_GPUI_WORK_ALLOC case={name} phase={phase} scope=calling_thread totals_calls={} totals_requested_bytes={}",
                    self.samples
                        .iter()
                        .map(|sample| sample.source_phases[index].allocations)
                        .sum::<u64>(),
                    self.samples
                        .iter()
                        .map(|sample| sample.source_phases[index].allocated_bytes)
                        .sum::<u64>(),
                );
            }
            for (index, phase) in DRIVE_PHASES.iter().enumerate() {
                report_cpu(
                    name,
                    phase,
                    self.samples
                        .iter()
                        .map(|sample| sample.drive_phases[index].cpu),
                );
                println!(
                    "NARA_GPUI_WORK_SPANS case={name} phase={phase} calls={} scope=current_thread_nested_in_mutation",
                    self.samples
                        .iter()
                        .map(|sample| sample.drive_phases[index].calls)
                        .sum::<u64>()
                );
            }
        }
        for (index, phase) in ["gpui_request_layout", "gpui_prepaint", "gpui_paint"]
            .iter()
            .enumerate()
        {
            if self.case != 0 {
                report_cpu(
                    name,
                    phase,
                    self.samples.iter().map(|sample| sample.draw[index]),
                );
            }
        }
        if self.case != 0 {
            report_cpu(
                name,
                "recorder_cpu_nested_in_settle",
                self.samples.iter().map(|sample| sample.record),
            );
        }
        let totals: [u64; 9] = std::array::from_fn(|index| {
            self.samples
                .iter()
                .map(|sample| sample.counters[index])
                .sum()
        });
        println!(
            "NARA_GPUI_WORK_COUNTS case={name} samples={} order=query_pages,query_records,projection_notifications,hierarchy_renders,inspector0_renders,inspector1_renders,row_callbacks,records,submissions totals={totals:?} accepted_draws={} max_sample_end_native_frames={} current_retained_native_frames={} diagnostics={}",
            self.samples.len(),
            self.samples.iter().map(|sample| sample.draws).sum::<u64>(),
            self.samples
                .iter()
                .map(|sample| sample.retained_frames)
                .max()
                .unwrap(),
            viewer.renderer.as_ref().unwrap().retained_frames(),
            viewer.renderer.as_ref().unwrap().diagnostics.get(),
        );
        #[cfg(feature = "ui-work-allocations")]
        println!(
            "NARA_GPUI_WORK_HEAP case={name} scope=process_requested_not_rss live_bytes={} includes_trace_and_samples=true",
            allocation_stats().unwrap().live_bytes()
        );
    }

    pub fn report_retired(&self, renderer: &Renderer) -> Result<()> {
        ensure!(
            self.case == CASES.len(),
            "the observation did not finish its declared cases"
        );
        ensure!(
            renderer.retained_frames() == 0 && renderer.quiesced(),
            "retired UI still retains native candidate frames"
        );
        ensure!(
            renderer.diagnostics.get() == 0,
            "measurement included diagnostic readback"
        );
        println!(
            "NARA_GPUI_WORK_OK cases={} retained_native_frames=0 diagnostics=0 original_host=true",
            CASES.len()
        );
        Ok(())
    }
}

impl Drop for UiWork {
    fn drop(&mut self) {
        if self.trace_was_changed {
            profiler::set_trace_enabled(false);
        }
    }
}

fn report_cpu(case: &str, phase: &str, values: impl Iterator<Item = Duration>) {
    let mut values: Vec<_> = values.map(|duration| duration.as_nanos()).collect();
    values.sort_unstable();
    let quantile =
        |percent: usize| values[(values.len() * percent).div_ceil(100).saturating_sub(1)];
    println!(
        "NARA_GPUI_WORK_CPU case={case} phase={phase} samples={} p50_ns={} p95_ns={} max_ns={}",
        values.len(),
        quantile(50),
        quantile(95),
        values.last().unwrap(),
    );
}
