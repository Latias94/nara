use serde_json::{Value, json};

const PHASES: &[&str] = &[
    "scene_decode",
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
    "scene_predecessor",
    "scene_retirement_hierarchy",
    "scene_identity_adopt",
    "scene_identity_prepare",
    "scene_identity_bind",
    "scene_hierarchy_publish",
    "scene_epoch_publish",
];

#[cfg(feature = "scene-work-observation")]
mod observed {
    use super::{PHASES, Value, json};
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    use tracing::{Subscriber, span::Id};
    use tracing_subscriber::{
        Layer,
        layer::{Context, SubscriberExt},
        registry::LookupSpan,
    };

    #[derive(Clone, Copy, Default)]
    struct SpanObservation {
        wall: Duration,
        calls: u64,
    }

    struct SpanStart(Instant);

    struct SceneWorkLayer(Arc<Mutex<Vec<SpanObservation>>>);

    impl<S> Layer<S> for SceneWorkLayer
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        fn on_enter(&self, id: &Id, context: Context<'_, S>) {
            let Some(span) = context.span(id) else {
                return;
            };
            if span.metadata().target() == "nara::scene::work" && PHASES.contains(&span.name()) {
                span.extensions_mut().insert(SpanStart(Instant::now()));
            }
        }

        fn on_exit(&self, id: &Id, context: Context<'_, S>) {
            let Some(span) = context.span(id) else {
                return;
            };
            let Some(start) = span.extensions_mut().remove::<SpanStart>() else {
                return;
            };
            let Some(index) = PHASES.iter().position(|name| *name == span.name()) else {
                return;
            };
            let mut observations = self.0.lock().unwrap_or_else(|error| error.into_inner());
            observations[index].wall += start.0.elapsed();
            observations[index].calls += 1;
        }
    }

    pub struct RunObservation {
        observations: Arc<Mutex<Vec<SpanObservation>>>,
        _subscriber: tracing::subscriber::DefaultGuard,
    }

    impl RunObservation {
        pub fn start() -> Self {
            let observations = Arc::new(Mutex::new(vec![SpanObservation::default(); PHASES.len()]));
            let subscriber =
                tracing_subscriber::registry().with(SceneWorkLayer(Arc::clone(&observations)));
            let guard = tracing::subscriber::set_default(subscriber);
            Self {
                observations,
                _subscriber: guard,
            }
        }

        pub fn snapshot(&self) -> Value {
            let observations = self
                .observations
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let mut phases = serde_json::Map::new();
            let mut total_wall_ns = 0_u128;
            let mut total_calls = 0_u64;
            for (name, observation) in PHASES.iter().zip(observations.iter()) {
                let wall_ns = observation.wall.as_nanos();
                total_wall_ns += wall_ns;
                total_calls += observation.calls;
                phases.insert(
                    (*name).to_owned(),
                    json!({
                        "calls": observation.calls,
                        "wall_ns": wall_ns,
                    }),
                );
            }
            json!({
                "enabled": true,
                "target": "nara::scene::work",
                "phases": phases,
                "total_calls": total_calls,
                "total_wall_ns": total_wall_ns,
                "scope": "instrumented tracing span wall time across the complete original process; spans can run on worker or owner threads and nested phase totals are not additive CPU time",
            })
        }
    }
}

#[cfg(feature = "scene-work-observation")]
pub(super) use observed::RunObservation;

#[cfg(not(feature = "scene-work-observation"))]
pub(super) struct RunObservation;

#[cfg(not(feature = "scene-work-observation"))]
impl RunObservation {
    pub(super) fn start() -> Self {
        Self
    }

    pub(super) fn snapshot(&self) -> Value {
        json!({
            "enabled": false,
            "target": "nara::scene::work",
            "scope": "compile with the scene-work-observation feature to collect existing Scene owner spans",
        })
    }
}
