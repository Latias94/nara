//! Application diagnostics, runtime pressure snapshots, and tracing sinks.

mod plugin;
mod pressure;
mod runtime;
mod sink;

pub use nara_core::diagnostic::{
    PressureMetricId, PressureSourceId, RuntimeDiagnosticRetention, RuntimeDiagnosticsSettings,
};
pub use plugin::{
    DIAGNOSTICS_PLUGIN_DECLARATION, DIAGNOSTICS_PLUGIN_ID, DiagnosticCleanupSet, DiagnosticsPlugin,
    plugin,
};
pub use pressure::{
    PressureDraftError, PressureMeasurement, PressureMetricKind, PressurePublishOutcome,
    PressurePublishRejection, PressureSettingsError, PressureStats, PressureUnit,
    RuntimePressureRetention, RuntimePressureSettings, RuntimePressureSnapshot,
    RuntimePressureSnapshotDraft, RuntimePressureSnapshots, RuntimePressureSnapshotsSnapshot,
};
pub use runtime::{
    DiagnosticDedupePolicy, RuntimeDiagnosticDraft, RuntimeDiagnosticEntry,
    RuntimeDiagnosticFilter, RuntimeDiagnosticFilterError, RuntimeDiagnostics,
    RuntimeDiagnosticsSnapshot, RuntimeDiagnosticsStats, RuntimePublishOutcome,
    RuntimePublishRejection, RuntimeTracingCursor,
};

pub use sink::{emit_diagnostic_to_tracing, emit_report_to_tracing};

#[cfg(test)]
mod tests;
