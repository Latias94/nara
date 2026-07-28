use nara_app::RealTime;
use nara_asset::{AssetSourceChanges, AssetSourceRoot};
use nara_diagnostic::{
    DiagnosticCode, DiagnosticDomain, DiagnosticProducer, DiagnosticSeverity, PressureMeasurement,
    PressureMetricId, PressureSourceId, PressureUnit, RuntimeDiagnosticDraft, RuntimeDiagnostics,
    RuntimePressureSnapshotDraft, RuntimePressureSnapshots, SafeSummary,
};
use nara_ecs::{Res, ResMut, Resource};

use crate::{
    AssetWatchTranslator,
    backend::AssetWatcher,
    queue::{
        AssetWatchEventQueue, AssetWatchFailureKind, AssetWatchQueueObservation,
        AssetWatchQueueObserver, AssetWatchQueueStats, usize_to_u64,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AssetWatchRuntimeState {
    #[default]
    Running,
    RescanRequired,
}

#[derive(Debug, Default, Resource)]
pub struct AssetWatchRuntimeStatus {
    state: AssetWatchRuntimeState,
}

impl AssetWatchRuntimeStatus {
    #[must_use]
    pub const fn state(&self) -> AssetWatchRuntimeState {
        self.state
    }

    #[must_use]
    pub const fn requires_rescan(&self) -> bool {
        matches!(self.state, AssetWatchRuntimeState::RescanRequired)
    }

    fn require_rescan(&mut self) {
        self.state = AssetWatchRuntimeState::RescanRequired;
    }
}

pub(crate) fn drain_asset_watch_events(
    mut queue: Option<ResMut<AssetWatchEventQueue>>,
    root: Res<AssetSourceRoot>,
    mut changes: ResMut<AssetSourceChanges>,
    time: Option<Res<RealTime>>,
    mut diagnostics: ResMut<RuntimeDiagnostics>,
    mut pressure: ResMut<RuntimePressureSnapshots>,
    mut observer: ResMut<AssetWatchQueueObserver>,
    mut status: ResMut<AssetWatchRuntimeStatus>,
    mut watcher: Option<ResMut<AssetWatcher>>,
) {
    if let Some(queue) = queue.as_mut() {
        let drained = queue.drain();
        if !drained.rescan_required() {
            let captured_events = drained.captured_events();
            let translator = AssetWatchTranslator;
            let mut translated = Vec::new();
            let mut failed = false;
            for event in drained.into_events() {
                match translator.translate_event(&root, &event) {
                    Ok(event_changes) => translated.extend(event_changes),
                    Err(_) => {
                        observer.record_translation_failure(captured_events);
                        failed = true;
                        break;
                    }
                }
            }
            if !failed {
                if changes.try_extend(translated).is_err() {
                    observer.record_source_admission_failure(captured_events);
                }
            }
        }
    }

    let frame = time.as_ref().map_or(0, |time| time.frame);
    let observation = observer.observe();
    if observation.stats.rescan_required() {
        status.require_rescan();
        if let Some(watcher) = watcher.as_mut() {
            watcher.stop_for_rescan();
        }
    }
    publish_watch_diagnostics(&mut diagnostics, observation, frame);
    publish_watch_pressure(&mut pressure, observation.stats, frame);
}

fn publish_watch_diagnostics(
    diagnostics: &mut RuntimeDiagnostics,
    observation: AssetWatchQueueObservation,
    frame: u64,
) {
    for kind in AssetWatchFailureKind::ALL {
        if observation.new_failures.get(kind) > 0 {
            let description = failure_description(kind);
            diagnostics.publish(
                watch_warning(description.diagnostic_code, description.summary),
                frame,
            );
        }
    }
    if observation.rescan_started {
        diagnostics.publish(
            watch_warning(
                "asset-watch.rescan-required",
                "Incremental asset watching stopped until the host completes a full rescan and reconstructs the runtime",
            ),
            frame,
        );
    }
}

#[derive(Clone, Copy)]
struct AssetWatchFailureDescription {
    diagnostic_code: &'static str,
    summary: &'static str,
    pressure_metric: &'static str,
}

const fn failure_description(kind: AssetWatchFailureKind) -> AssetWatchFailureDescription {
    match kind {
        AssetWatchFailureKind::Overflow => AssetWatchFailureDescription {
            diagnostic_code: "asset-watch.queue-overflow",
            summary: "Asset watch queue capacity was exceeded",
            pressure_metric: "overflow-rejections",
        },
        AssetWatchFailureKind::Busy => AssetWatchFailureDescription {
            diagnostic_code: "asset-watch.queue-contention",
            summary: "Concurrent asset watch producers could not admit a batch without blocking",
            pressure_metric: "busy-rejections",
        },
        AssetWatchFailureKind::Disconnected => AssetWatchFailureDescription {
            diagnostic_code: "asset-watch.queue-disconnected",
            summary: "Asset watch queue receiver was disconnected",
            pressure_metric: "disconnect-rejections",
        },
        AssetWatchFailureKind::Translation => AssetWatchFailureDescription {
            diagnostic_code: "asset-watch.translation-failed",
            summary: "Asset watch event translation failed",
            pressure_metric: "translation-failures",
        },
        AssetWatchFailureKind::Backend => AssetWatchFailureDescription {
            diagnostic_code: "asset-watch.backend-failed",
            summary: "Asset watch backend reported an event failure",
            pressure_metric: "backend-failures",
        },
        AssetWatchFailureKind::Unavailable => AssetWatchFailureDescription {
            diagnostic_code: "asset-watch.queue-unavailable",
            summary: "Asset watch queue became unavailable",
            pressure_metric: "unavailable-failures",
        },
        AssetWatchFailureKind::SourceAdmission => AssetWatchFailureDescription {
            diagnostic_code: "asset-watch.source-change-admission-failed",
            summary: "Translated asset changes exceeded the downstream source-change budget",
            pressure_metric: "source-admission-failures",
        },
    }
}

fn watch_warning(code: &'static str, summary: &'static str) -> RuntimeDiagnosticDraft {
    RuntimeDiagnosticDraft::new(
        DiagnosticProducer::new("nara.asset-watch")
            .expect("asset watch diagnostic producer is source-authored"),
        DiagnosticDomain::new("asset").expect("asset diagnostic domain is source-authored"),
        DiagnosticCode::new(code).expect("asset watch diagnostic code is source-authored"),
        DiagnosticSeverity::Warning,
        SafeSummary::new(summary).expect("asset watch diagnostic summary is source-authored"),
    )
    .dedupe_by_code()
}

fn publish_watch_pressure(
    pressure: &mut RuntimePressureSnapshots,
    stats: AssetWatchQueueStats,
    frame: u64,
) {
    let mut draft = RuntimePressureSnapshotDraft::new(
        PressureSourceId::new("nara.asset-watch.queue")
            .expect("asset watch pressure source is source-authored"),
    );
    for measurement in [
        gauge(
            "retained-events",
            PressureUnit::Items,
            usize_to_u64(stats.retained_events()),
        ),
        gauge(
            "retained-bytes",
            PressureUnit::Bytes,
            usize_to_u64(stats.retained_bytes()),
        ),
        gauge(
            "high-water-events",
            PressureUnit::Items,
            usize_to_u64(stats.high_water_events()),
        ),
        gauge(
            "high-water-bytes",
            PressureUnit::Bytes,
            usize_to_u64(stats.high_water_bytes()),
        ),
        gauge(
            "rescan-required",
            PressureUnit::Count,
            u64::from(stats.rescan_required()),
        ),
        counter(
            "accepted-batches",
            PressureUnit::Count,
            stats.accepted_batches(),
        ),
        counter(
            "accepted-events",
            PressureUnit::Items,
            stats.accepted_events(),
        ),
        counter(
            "suppressed-batches",
            PressureUnit::Count,
            stats.suppressed_batches(),
        ),
        counter(
            "discarded-events",
            PressureUnit::Items,
            stats.discarded_events(),
        ),
    ] {
        draft = draft
            .try_with_measurement(measurement)
            .expect("asset watch pressure metrics are unique and bounded");
    }
    for kind in AssetWatchFailureKind::ALL {
        draft = draft
            .try_with_measurement(counter(
                failure_description(kind).pressure_metric,
                PressureUnit::Count,
                stats.failure(kind),
            ))
            .expect("asset watch failure pressure metrics are unique and bounded");
    }
    pressure.publish(draft, frame);
}

fn gauge(metric: &'static str, unit: PressureUnit, value: u64) -> PressureMeasurement {
    PressureMeasurement::gauge(watch_pressure_metric(metric), unit, value)
}

fn counter(metric: &'static str, unit: PressureUnit, value: u64) -> PressureMeasurement {
    PressureMeasurement::counter(watch_pressure_metric(metric), unit, value)
}

fn watch_pressure_metric(value: &'static str) -> PressureMetricId {
    PressureMetricId::new(value).expect("asset watch pressure metric is source-authored")
}
