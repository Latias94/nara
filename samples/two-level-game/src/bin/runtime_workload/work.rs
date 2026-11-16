//! Explicit binary-local collection of existing Scene spans across original worker threads.

use serde_json::{Value, json};

#[cfg(feature = "scene-work-observation")]
mod observed {
    use super::{Value, json};
    use std::{
        sync::{Arc, Mutex},
        thread::{self, ThreadId},
        time::{Duration, Instant},
    };
    use tracing::{
        Subscriber,
        span::{Attributes, Id},
    };
    use tracing_subscriber::{
        Layer,
        layer::{Context, SubscriberExt},
        registry::LookupSpan,
    };

    const PHASES: [&str; 25] = [
        "project_content_read",
        "scene_source_decode",
        "prefab_source_decode",
        "scene_decode",
        "scene_expand",
        "scene_structure",
        "scene_program_composition",
        "scene_component_materialization",
        "scene_reference_closure",
        "scene_compile_output",
        "scene_authoring_finish",
        "scene_native_preview",
        "scene_flat_admission",
        "scene_prepare_unbound",
        "scene_bind",
        "scene_bind_revalidate",
        "scene_publish",
        "scene_final_validate",
        "scene_predecessor",
        "scene_retirement_hierarchy",
        "scene_identity_adopt",
        "scene_identity_prepare",
        "scene_identity_bind",
        "scene_hierarchy_publish",
        "scene_epoch_publish",
    ];

    #[derive(Clone, Copy, Default)]
    struct Phase {
        wall: Duration,
        owner_calls: u64,
        worker_calls: u64,
    }

    struct Run {
        owner_thread: ThreadId,
        phases: [Phase; PHASES.len()],
        active_entries: usize,
        live_spans: usize,
        invalid: bool,
    }

    type ActiveRun = Arc<Mutex<Option<Arc<Mutex<Run>>>>>;

    struct SceneLayer(ActiveRun);

    struct SpanTiming {
        run: Arc<Mutex<Run>>,
        phase: usize,
        start: Option<Instant>,
        owner: bool,
    }

    impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for SceneLayer {
        fn enabled(&self, metadata: &tracing::Metadata<'_>, _: Context<'_, S>) -> bool {
            metadata.target() == "nara::scene::work" && PHASES.contains(&metadata.name())
        }

        fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
            if attributes.metadata().target() != "nara::scene::work" {
                return;
            }
            let Some(phase) = PHASES
                .iter()
                .position(|name| *name == attributes.metadata().name())
            else {
                return;
            };
            let active = self
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone();
            if let (Some(run), Some(span)) = (active, context.span(id)) {
                run.lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .live_spans += 1;
                span.extensions_mut().insert(SpanTiming {
                    run,
                    phase,
                    start: None,
                    owner: false,
                });
            }
        }

        fn on_enter(&self, id: &Id, context: Context<'_, S>) {
            let Some(span) = context.span(id) else { return };
            let mut extensions = span.extensions_mut();
            let Some(timing) = extensions.get_mut::<SpanTiming>() else {
                return;
            };
            let mut run = timing.run.lock().unwrap_or_else(|error| error.into_inner());
            if timing.start.is_some() {
                run.invalid = true;
                return;
            }
            timing.owner = thread::current().id() == run.owner_thread;
            timing.start = Some(Instant::now());
            run.active_entries += 1;
        }

        fn on_exit(&self, id: &Id, context: Context<'_, S>) {
            let ended = Instant::now();
            let Some(span) = context.span(id) else { return };
            let mut extensions = span.extensions_mut();
            let Some(timing) = extensions.get_mut::<SpanTiming>() else {
                return;
            };
            let mut run = timing.run.lock().unwrap_or_else(|error| error.into_inner());
            let Some(start) = timing.start.take() else {
                run.invalid = true;
                return;
            };
            run.active_entries -= 1;
            let phase = &mut run.phases[timing.phase];
            if let Some(wall) = phase.wall.checked_add(ended.duration_since(start)) {
                phase.wall = wall;
                if timing.owner {
                    phase.owner_calls += 1;
                } else {
                    phase.worker_calls += 1;
                }
            } else {
                run.invalid = true;
            }
        }

        fn on_close(&self, id: Id, context: Context<'_, S>) {
            let Some(span) = context.span(&id) else {
                return;
            };
            let extensions = span.extensions();
            let Some(timing) = extensions.get::<SpanTiming>() else {
                return;
            };
            let mut run = timing.run.lock().unwrap_or_else(|error| error.into_inner());
            run.live_spans -= 1;
            if timing.start.is_some() {
                run.invalid = true;
            }
        }
    }

    pub struct Collector {
        active: ActiveRun,
    }

    impl Collector {
        pub fn install() -> Result<Self, String> {
            let collector = Self {
                active: Arc::new(Mutex::new(None)),
            };
            let subscriber =
                tracing_subscriber::registry().with(SceneLayer(Arc::clone(&collector.active)));
            tracing::subscriber::set_global_default(subscriber)
                .map_err(|error| error.to_string())?;
            Ok(collector)
        }

        pub fn begin(&self) -> Result<RunObservation, String> {
            let mut active = self
                .active
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if active.is_some() {
                return Err("the previous Scene observation is still active".into());
            }
            let run = Arc::new(Mutex::new(Run {
                owner_thread: thread::current().id(),
                phases: [Phase::default(); PHASES.len()],
                active_entries: 0,
                live_spans: 0,
                invalid: false,
            }));
            *active = Some(Arc::clone(&run));
            Ok(RunObservation {
                active: Arc::clone(&self.active),
                run,
            })
        }
    }

    pub struct RunObservation {
        active: ActiveRun,
        run: Arc<Mutex<Run>>,
    }

    impl RunObservation {
        pub fn finish(self) -> Result<Value, String> {
            self.detach();
            let run = self.run.lock().unwrap_or_else(|error| error.into_inner());
            if run.invalid || run.active_entries != 0 || run.live_spans != 0 {
                return Err(
                    "Scene span observation has unfinished or unsupported reentrant entries".into(),
                );
            }
            let mut phases = serde_json::Map::new();
            let mut entered_wall_ns = 0_u64;
            let mut total_calls = 0_u64;
            for (name, phase) in PHASES.iter().zip(run.phases.iter()) {
                let wall =
                    u64::try_from(phase.wall.as_nanos()).map_err(|_| "Scene wall span overflow")?;
                let calls = phase.owner_calls + phase.worker_calls;
                entered_wall_ns = entered_wall_ns
                    .checked_add(wall)
                    .ok_or("Scene wall sum overflow")?;
                total_calls += calls;
                phases.insert(
                    (*name).to_owned(),
                    json!({
                        "calls": calls, "owner_thread_calls": phase.owner_calls,
                        "worker_thread_calls": phase.worker_calls, "wall_ns": wall,
                    }),
                );
            }
            Ok(json!({
                "enabled": true, "status": "complete", "target": "nara::scene::work", "phases": phases,
                "total_calls": total_calls, "nested_span_wall_sum_ns": entered_wall_ns,
                "unfinished_entries": 0,
                "retained_spans": 0,
                "scope": "one original Runtime from construction through physical retirement, across owner and worker threads; excludes source-copy generation and independent GPU replay; nested wall spans overlap and are not additive CPU time",
            }))
        }

        fn detach(&self) {
            let mut active = self
                .active
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if active
                .as_ref()
                .is_some_and(|run| Arc::ptr_eq(run, &self.run))
            {
                *active = None;
            }
        }
    }

    impl Drop for RunObservation {
        fn drop(&mut self) {
            self.detach();
        }
    }
}

#[cfg(feature = "scene-work-observation")]
pub(super) use observed::Collector;

#[cfg(not(feature = "scene-work-observation"))]
pub(super) struct Collector;

#[cfg(not(feature = "scene-work-observation"))]
impl Collector {
    pub(super) fn install() -> Result<Self, String> {
        Ok(Self)
    }
    pub(super) fn begin(&self) -> Result<RunObservation, String> {
        Ok(RunObservation)
    }
}

#[cfg(not(feature = "scene-work-observation"))]
pub(super) struct RunObservation;

#[cfg(not(feature = "scene-work-observation"))]
impl RunObservation {
    pub(super) fn finish(self) -> Result<Value, String> {
        Ok(json!({ "enabled": false, "status": "disabled", "target": "nara::scene::work" }))
    }
}

pub(super) fn finish_record(result: Result<Value, String>) -> Value {
    match result {
        Ok(report) => report,
        Err(reason) => json!({
            "enabled": cfg!(feature = "scene-work-observation"),
            "status": "rejected", "reason": reason,
        }),
    }
}

pub(super) fn aborted_record() -> Value {
    json!({
        "enabled": cfg!(feature = "scene-work-observation"),
        "status": "aborted",
        "scope": "original Runtime retirement was not confirmed or the observation ended before a completed Runtime lifetime; partial spans do not qualify complete costs",
    })
}
