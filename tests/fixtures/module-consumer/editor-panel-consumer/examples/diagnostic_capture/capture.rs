use std::{
    fmt,
    fs::OpenOptions,
    io,
    mem::size_of,
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard, TryLockError,
        atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    },
    time::{Duration, Instant},
};

use serde::{Serialize, Serializer, ser::SerializeMap};
use tracing::{
    Dispatch, Event, Metadata,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use tracing_subscriber::{Layer, Registry, layer::Context, prelude::*};

const NUMBERS: [&str; 16] = [
    "task_id",
    "document",
    "source",
    "revision",
    "runtime",
    "app_frame",
    "render_frame",
    "window",
    "surface_generation",
    "backend_instance",
    "device_epoch",
    "poll",
    "bytes",
    "queue_wait_ns",
    "current_revision",
    "admission_to_start_ns",
];
const OUTCOMES: [&str; 22] = [
    "pending",
    "admitted",
    "published",
    "failed",
    "cancelled",
    "reconcile",
    "rejected",
    "abandoned",
    "complete",
    "incomplete",
    "busy",
    "prepared",
    "committed",
    "unchanged",
    "stale_revision",
    "stale_source",
    "closing",
    "structure_mismatch",
    "workspace_fenced",
    "domain_rejected",
    "unavailable",
    "success",
];

fn accepted_metadata(metadata: &Metadata<'_>) -> bool {
    let name = metadata.name();
    let accepted = match metadata.target() {
        "nara::capture::host" => matches!(name, "host_workflow" | "native_poll"),
        "nara::editor::work" => matches!(
            name,
            "editor_preview"
                | "editor_preview_compile"
                | "editor_preview_drive"
                | "editor_publish_model"
                | "editor_property_edit_prepare"
                | "editor_edit_prepare"
                | "editor_edit_commit"
                | "editor_preview_runtime"
                | "editor_preview_materialize"
                | "editor_source_import"
                | "editor_source_import_validate"
                | "editor_source_import_revalidate"
                | "editor_source_import_publish"
        ),
        "nara::scene::work" => matches!(
            name,
            "scene_flat_admission"
                | "scene_authoring_finish"
                | "scene_retirement_hierarchy"
                | "scene_prepare_unbound"
                | "scene_predecessor"
                | "scene_identity_adopt"
                | "scene_identity_prepare"
                | "scene_identity_bind"
                | "scene_decode"
                | "scene_native_preview"
                | "scene_bind_revalidate"
                | "scene_bind"
                | "scene_publish"
                | "scene_final_validate"
                | "scene_hierarchy_publish"
                | "scene_epoch_publish"
                | "scene_expand"
                | "scene_structure"
                | "scene_compile_output"
        ),
        _ => false,
    };
    #[cfg(test)]
    let accepted = accepted
        || (matches!(
            metadata.target(),
            "nara::editor::work" | "nara::scene::work"
        ) && matches!(
            name,
            "disabled"
                | "root"
                | "overflow"
                | "nested"
                | "worker"
                | "published"
                | "parent"
                | "child"
                | "reused"
                | "privacy"
                | "privacy_event"
                | "unfinished"
                | "capture_tick"
                | "crossing_a"
                | "crossing_b"
        ));
    accepted
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub events: usize,
    pub active_spans: usize,
    pub retained_bytes: usize,
}

#[derive(Clone, Copy)]
pub struct GpuIdentity {
    pub runtime: u64,
    pub app_frame: u64,
    pub render_frame: u64,
    pub window: u64,
    pub surface_generation: u64,
    pub backend_instance: u64,
    pub device_epoch: u64,
}

/// Storage counts cover only the two Vec payloads, excluding Registry, Arc, allocator metadata and RSS.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct CaptureReport {
    pub events: usize,
    pub active_spans: usize,
    pub reserved_bytes: usize,
    pub high_water_bytes: usize,
    pub dropped_events: usize,
    pub dropped_spans: usize,
    pub rejected_fields: usize,
    pub faults: usize,
    pub incomplete: usize,
}

#[derive(Clone)]
pub struct Capture {
    shared: Option<Arc<Shared>>,
    dispatch: Dispatch,
}

impl Capture {
    pub fn new(limits: Limits) -> Self {
        let mut state = State {
            limits,
            active: Vec::new(),
            events: Vec::new(),
            next_id: 1,
        };
        let allocated = limits.events <= 16_384
            && limits.active_spans <= 64
            && state.active.try_reserve_exact(limits.active_spans).is_ok()
            && state.events.try_reserve_exact(limits.events).is_ok();
        let bytes = state
            .active
            .capacity()
            .checked_mul(size_of::<SpanState>())
            .and_then(|active| {
                state
                    .events
                    .capacity()
                    .checked_mul(size_of::<TraceEvent>())?
                    .checked_add(active)
            });
        let failed = !allocated || bytes.is_none_or(|bytes| bytes > limits.retained_bytes);
        if failed {
            state.active = Vec::new();
            state.events = Vec::new();
        }
        let shared = Arc::new(Shared {
            epoch: Instant::now(),
            reserved_bytes: if failed { 0 } else { bytes.unwrap_or(0) },
            state: Mutex::new(state),
            stopped: AtomicBool::new(failed),
            events: AtomicUsize::new(0),
            active_spans: AtomicUsize::new(0),
            high_water_bytes: AtomicUsize::new(0),
            dropped_events: AtomicUsize::new(0),
            dropped_spans: AtomicUsize::new(0),
            rejected_fields: AtomicUsize::new(0),
            faults: AtomicUsize::new(usize::from(failed)),
        });
        let dispatch = Dispatch::new(Registry::default().with(CaptureLayer(Arc::clone(&shared))));
        Self {
            shared: Some(shared),
            dispatch,
        }
    }

    pub fn disabled() -> Self {
        Self {
            shared: None,
            dispatch: Dispatch::new(tracing::subscriber::NoSubscriber::default()),
        }
    }

    pub fn dispatch(&self) -> Dispatch {
        self.dispatch.clone()
    }

    pub fn fail(&self) {
        if let Some(shared) = &self.shared {
            shared.fail();
        }
    }

    pub fn report(&self) -> CaptureReport {
        let Some(shared) = &self.shared else {
            return CaptureReport::default();
        };
        let mut report = CaptureReport {
            events: shared.events.load(Relaxed),
            active_spans: shared.active_spans.load(Relaxed),
            reserved_bytes: shared.reserved_bytes,
            high_water_bytes: shared.high_water_bytes.load(Relaxed),
            dropped_events: shared.dropped_events.load(Relaxed),
            dropped_spans: shared.dropped_spans.load(Relaxed),
            rejected_fields: shared.rejected_fields.load(Relaxed),
            faults: shared.faults.load(Relaxed),
            incomplete: 0,
        };
        report.incomplete = [
            report.active_spans,
            report.dropped_events,
            report.dropped_spans,
            report.rejected_fields,
            report.faults,
        ]
        .into_iter()
        .fold(0usize, usize::saturating_add);
        report
    }

    pub fn gpu_complete(&self, app_started: Instant, duration: Duration, identity: GpuIdentity) {
        self.frame_interval("app_start_to_gpu_complete", app_started, duration, identity);
    }

    pub fn presented(&self, app_started: Instant, duration: Duration, identity: GpuIdentity) {
        self.frame_interval("app_start_to_presented", app_started, duration, identity);
    }

    fn frame_interval(
        &self,
        name: &'static str,
        app_started: Instant,
        duration: Duration,
        identity: GpuIdentity,
    ) {
        let Some(shared) = &self.shared else { return };
        let interval = shared
            .timestamp(app_started)
            .zip(u64::try_from(duration.as_nanos()).ok())
            .filter(|(start, duration_ns)| start.checked_add(*duration_ns).is_some())
            .filter(|_| app_started.checked_add(duration).is_some());
        let Some((start, duration)) = interval else {
            increment(&shared.dropped_events);
            return;
        };
        let Some(mut state) = shared.lock() else {
            increment(&shared.dropped_events);
            return;
        };
        let mut fields = Fields::default();
        for (name, value) in [
            ("runtime", identity.runtime),
            ("app_frame", identity.app_frame),
            ("render_frame", identity.render_frame),
            ("window", identity.window),
            ("surface_generation", identity.surface_generation),
            ("backend_instance", identity.backend_instance),
            ("device_epoch", identity.device_epoch),
        ] {
            fields.number(name, value);
        }
        if let Some(mut event) = state.event(name, "nara::capture::host", "X", start, None, fields)
        {
            event.dur = Some(duration as f64 / 1_000.0);
            shared.push(&mut state, event);
        } else {
            shared.fail();
            increment(&shared.dropped_events);
        }
    }

    /// Write after product cleanup; timestamps use microseconds as required by Trace Event JSON.
    pub fn write_new(&self, path: &Path) -> io::Result<()> {
        let write = |events: &[TraceEvent]| {
            #[derive(Serialize)]
            struct Document<'a> {
                #[serde(rename = "traceEvents")]
                events: &'a [TraceEvent],
                #[serde(rename = "displayTimeUnit")]
                unit: &'static str,
                nara_capture: CaptureReport,
                reserved_bytes_scope: &'static str,
                gpu_interval_scope: &'static str,
                track_scope: &'static str,
            }
            let file = OpenOptions::new().write(true).create_new(true).open(path)?;
            serde_json::to_writer(file, &Document {
                events, unit: "ms", nara_capture: self.report(),
                reserved_bytes_scope: "Vec payload capacities only; excludes Registry, Arc, allocator metadata and RSS",
                gpu_interval_scope: "App start to GPU completion, including CPU submission and queue time; not pure GPU execution",
                track_scope: "one retained operation per observation track; tid is a capture-local track identity, not an OS thread; parent_id remains an explicit correlation",
            }).map_err(io::Error::other)
        };
        let result = match &self.shared {
            None => write(&[]),
            Some(shared) => match shared.state.try_lock() {
                Ok(state) => write(&state.events),
                Err(TryLockError::Poisoned(_)) => {
                    shared.fail();
                    Err(io::Error::other("capture state poisoned"))
                }
                Err(TryLockError::WouldBlock) => Err(io::Error::other("capture still active")),
            },
        };
        if result.is_err() {
            self.fail();
        }
        result
    }
}

struct Shared {
    epoch: Instant,
    reserved_bytes: usize,
    state: Mutex<State>,
    stopped: AtomicBool,
    events: AtomicUsize,
    active_spans: AtomicUsize,
    high_water_bytes: AtomicUsize,
    dropped_events: AtomicUsize,
    dropped_spans: AtomicUsize,
    rejected_fields: AtomicUsize,
    faults: AtomicUsize,
}

impl Shared {
    fn fail(&self) {
        if !self.stopped.swap(true, Relaxed) {
            increment(&self.faults);
        }
    }

    fn lock(&self) -> Option<MutexGuard<'_, State>> {
        if self.stopped.load(Relaxed) {
            return None;
        }
        match self.state.lock() {
            Ok(state) => Some(state),
            Err(_) => {
                self.fail();
                None
            }
        }
    }

    fn timestamp(&self, instant: Instant) -> Option<u64> {
        u64::try_from(instant.checked_duration_since(self.epoch)?.as_nanos()).ok()
    }

    fn observe(&self, state: &State) {
        self.events.store(state.events.len(), Relaxed);
        self.active_spans.store(state.active.len(), Relaxed);
        self.high_water_bytes.fetch_max(
            state.events.len() * size_of::<TraceEvent>()
                + state.active.len() * size_of::<SpanState>(),
            Relaxed,
        );
    }

    fn push(&self, state: &mut State, event: TraceEvent) {
        if state.events.len() < state.limits.events {
            state.events.push(event);
        } else {
            increment(&self.dropped_events);
        }
        self.observe(state);
    }
}

fn increment(counter: &AtomicUsize) {
    let _ = counter.fetch_update(Relaxed, Relaxed, |value| Some(value.saturating_add(1)));
}

struct State {
    limits: Limits,
    active: Vec<SpanState>,
    events: Vec<TraceEvent>,
    next_id: u64,
}

impl State {
    fn event(
        &mut self,
        name: &'static str,
        cat: &'static str,
        ph: &'static str,
        time: u64,
        parent: Option<Id>,
        mut args: Fields,
    ) -> Option<TraceEvent> {
        args.capture_id = Some(self.next_id);
        self.next_id = self.next_id.checked_add(1)?;
        args.parent_id = parent
            .and_then(|id| self.active.iter().find(|span| span.registry_id == id))
            .and_then(|span| span.event.args.capture_id);
        Some(TraceEvent {
            name,
            cat,
            ph,
            ts: time as f64 / 1_000.0,
            dur: None,
            pid: std::process::id(),
            tid: args.capture_id?,
            s: (ph == "i").then_some("t"),
            args,
        })
    }
}

struct SpanState {
    registry_id: Id,
    started: u64,
    event: TraceEvent,
}

#[derive(Serialize)]
struct TraceEvent {
    name: &'static str,
    cat: &'static str,
    ph: &'static str,
    ts: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    dur: Option<f64>,
    pid: u32,
    tid: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    s: Option<&'static str>,
    args: Fields,
}

#[derive(Default)]
struct Fields {
    numbers: [Option<u64>; NUMBERS.len()],
    outcome: Option<&'static str>,
    capture_id: Option<u64>,
    parent_id: Option<u64>,
}

impl Fields {
    fn number(&mut self, name: &str, value: u64) -> bool {
        if let Some(index) = NUMBERS.iter().position(|allowed| *allowed == name) {
            self.numbers[index] = Some(value);
            true
        } else {
            false
        }
    }

    fn merge(&mut self, update: Self) {
        for (stored, update) in self.numbers.iter_mut().zip(update.numbers) {
            if update.is_some() {
                *stored = update;
            }
        }
        if update.outcome.is_some() {
            self.outcome = update.outcome;
        }
    }
}

impl Serialize for Fields {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for (name, value) in NUMBERS.iter().zip(self.numbers.iter()) {
            if let Some(value) = value {
                map.serialize_entry(name, value)?;
            }
        }
        if let Some(outcome) = self.outcome {
            map.serialize_entry("outcome", outcome)?;
        }
        if let Some(id) = self.capture_id {
            map.serialize_entry("capture_id", &id)?;
        }
        if let Some(id) = self.parent_id {
            map.serialize_entry("parent_id", &id)?;
        }
        map.end()
    }
}

struct Visitor<'a>(&'a mut Fields, &'a AtomicUsize);

impl Visit for Visitor<'_> {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if !self.0.number(field.name(), value) {
            increment(self.1);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if let Ok(value) = u64::try_from(value) {
            self.record_u64(field, value);
        } else {
            increment(self.1);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "outcome" {
            if let Some(value) = OUTCOMES.iter().copied().find(|allowed| *allowed == value) {
                self.0.outcome = Some(value);
                return;
            }
        }
        increment(self.1);
    }

    fn record_debug(&mut self, _: &Field, _: &dyn fmt::Debug) {
        increment(self.1);
    }
}

struct CaptureLayer(Arc<Shared>);

impl Layer<Registry> for CaptureLayer {
    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, Registry>) -> bool {
        accepted_metadata(metadata)
    }

    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, Registry>) {
        if self.0.stopped.load(Relaxed) {
            increment(&self.0.dropped_spans);
            return;
        }
        let time = self.0.timestamp(Instant::now());
        let mut fields = Fields::default();
        attrs.record(&mut Visitor(&mut fields, &self.0.rejected_fields));
        let parent = parent(attrs.parent(), attrs.is_contextual(), &ctx);
        let Some(mut state) = self.0.lock() else {
            increment(&self.0.dropped_spans);
            return;
        };
        if state.active.len() >= state.limits.active_spans {
            increment(&self.0.dropped_spans);
            return;
        }
        let event = time.and_then(|time| {
            state.event(
                attrs.metadata().name(),
                attrs.metadata().target(),
                "X",
                time,
                parent,
                fields,
            )
        });
        if let Some(event) = event {
            state.active.push(SpanState {
                registry_id: id.clone(),
                started: time.unwrap_or(0),
                event,
            });
            self.0.observe(&state);
        } else {
            self.0.fail();
            increment(&self.0.dropped_spans);
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, _: Context<'_, Registry>) {
        if self.0.stopped.load(Relaxed) {
            increment(&self.0.dropped_events);
            return;
        }
        let mut fields = Fields::default();
        values.record(&mut Visitor(&mut fields, &self.0.rejected_fields));
        let Some(mut state) = self.0.lock() else {
            increment(&self.0.dropped_events);
            return;
        };
        if let Some(span) = state.active.iter_mut().find(|span| &span.registry_id == id) {
            span.event.args.merge(fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, Registry>) {
        if self.0.stopped.load(Relaxed) {
            increment(&self.0.dropped_events);
            return;
        }
        let time = self.0.timestamp(Instant::now());
        let mut fields = Fields::default();
        event.record(&mut Visitor(&mut fields, &self.0.rejected_fields));
        let parent = parent(event.parent(), event.is_contextual(), &ctx);
        let Some(mut state) = self.0.lock() else {
            increment(&self.0.dropped_events);
            return;
        };
        if let Some(event) = time.and_then(|time| {
            state.event(
                event.metadata().name(),
                event.metadata().target(),
                "i",
                time,
                parent,
                fields,
            )
        }) {
            self.0.push(&mut state, event);
        } else {
            self.0.fail();
            increment(&self.0.dropped_events);
        }
    }

    fn on_close(&self, id: Id, _: Context<'_, Registry>) {
        let time = self.0.timestamp(Instant::now());
        let Some(mut state) = self.0.lock() else {
            self.0.fail();
            increment(&self.0.dropped_spans);
            return;
        };
        if let Some(index) = state.active.iter().position(|span| span.registry_id == id) {
            let mut span = state.active.swap_remove(index);
            if let Some(duration) = time.and_then(|time| time.checked_sub(span.started)) {
                span.event.dur = Some(duration as f64 / 1_000.0);
                self.0.push(&mut state, span.event);
            } else {
                self.0.fail();
                increment(&self.0.dropped_events);
                self.0.observe(&state);
            }
        }
    }
}

fn parent(explicit: Option<&Id>, contextual: bool, ctx: &Context<'_, Registry>) -> Option<Id> {
    explicit.cloned().or_else(|| {
        if contextual {
            ctx.current_span().id().cloned()
        } else {
            None
        }
    })
}
