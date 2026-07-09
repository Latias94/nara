//! Structured diagnostics for runtime, tooling, and asset pipelines.

use std::{
    collections::VecDeque,
    fmt::{self, Display, Formatter},
};

use nara_app::{App, Plugin, PluginError};
use nara_ecs::Resource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticCode(String);

impl DiagnosticCode {
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub context: DiagnosticContext,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticContext {
    pub operation_index: Option<usize>,
    pub entity_id: Option<String>,
    pub component_id: Option<String>,
    pub field_path: Option<String>,
    pub asset_ref: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: DiagnosticCode::new(code),
            severity,
            message: message.into(),
            context: DiagnosticContext::default(),
        }
    }

    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, DiagnosticSeverity::Error, message)
    }

    #[must_use]
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, DiagnosticSeverity::Warning, message)
    }

    #[must_use]
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, DiagnosticSeverity::Info, message)
    }

    #[must_use]
    pub fn with_operation_index(mut self, operation_index: usize) -> Self {
        self.context.operation_index = Some(operation_index);
        self
    }

    #[must_use]
    pub fn with_entity_id(mut self, entity_id: impl Into<String>) -> Self {
        self.context.entity_id = Some(entity_id.into());
        self
    }

    #[must_use]
    pub fn with_component_id(mut self, component_id: impl Into<String>) -> Self {
        self.context.component_id = Some(component_id.into());
        self
    }

    #[must_use]
    pub fn with_field_path(mut self, field_path: impl Into<String>) -> Self {
        self.context.field_path = Some(field_path.into());
        self
    }

    #[must_use]
    pub fn with_asset_ref(mut self, asset_ref: impl Into<String>) -> Self {
        self.context.asset_ref = Some(asset_ref.into());
        self
    }

    pub fn emit_to_tracing(&self) {
        match self.severity {
            DiagnosticSeverity::Error => {
                tracing::error!(code = self.code.as_str(), "{}", self.message)
            }
            DiagnosticSeverity::Warning => {
                tracing::warn!(code = self.code.as_str(), "{}", self.message);
            }
            DiagnosticSeverity::Info => {
                tracing::info!(code = self.code.as_str(), "{}", self.message)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RuntimeDiagnosticDomain(String);

impl RuntimeDiagnosticDomain {
    #[must_use]
    pub fn new(domain: impl Into<String>) -> Self {
        Self(domain.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RuntimeDiagnosticDomain {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&str> for RuntimeDiagnosticDomain {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for RuntimeDiagnosticDomain {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RuntimeDiagnosticContext {
    pub frame: Option<u64>,
    pub stage: Option<String>,
    pub task_id: Option<String>,
    pub window_id: Option<String>,
    pub backend: Option<String>,
    pub asset_ref: Option<String>,
    pub source: Option<String>,
    pub component_id: Option<String>,
    pub entity_id: Option<String>,
    pub field_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RuntimeDiagnosticEntry {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub domain: RuntimeDiagnosticDomain,
    pub message: String,
    pub context: RuntimeDiagnosticContext,
    pub dedupe_key: Option<String>,
    pub first_frame: Option<u64>,
    pub last_frame: Option<u64>,
    pub repeat_count: u64,
}

impl RuntimeDiagnosticEntry {
    #[must_use]
    pub fn new(
        domain: impl Into<RuntimeDiagnosticDomain>,
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: DiagnosticCode::new(code),
            severity,
            domain: domain.into(),
            message: message.into(),
            context: RuntimeDiagnosticContext::default(),
            dedupe_key: None,
            first_frame: None,
            last_frame: None,
            repeat_count: 1,
        }
    }

    #[must_use]
    pub fn error(
        domain: impl Into<RuntimeDiagnosticDomain>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(domain, code, DiagnosticSeverity::Error, message)
    }

    #[must_use]
    pub fn warning(
        domain: impl Into<RuntimeDiagnosticDomain>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(domain, code, DiagnosticSeverity::Warning, message)
    }

    #[must_use]
    pub fn info(
        domain: impl Into<RuntimeDiagnosticDomain>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(domain, code, DiagnosticSeverity::Info, message)
    }

    #[must_use]
    pub fn with_dedupe_key(mut self, dedupe_key: impl Into<String>) -> Self {
        self.dedupe_key = Some(dedupe_key.into());
        self
    }

    #[must_use]
    pub fn with_frame(mut self, frame: u64) -> Self {
        self.context.frame = Some(frame);
        self.first_frame = Some(frame);
        self.last_frame = Some(frame);
        self
    }

    #[must_use]
    pub fn with_stage(mut self, stage: impl Into<String>) -> Self {
        self.context.stage = Some(stage.into());
        self
    }

    #[must_use]
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.context.task_id = Some(task_id.into());
        self
    }

    #[must_use]
    pub fn with_window_id(mut self, window_id: impl Into<String>) -> Self {
        self.context.window_id = Some(window_id.into());
        self
    }

    #[must_use]
    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.context.backend = Some(backend.into());
        self
    }

    #[must_use]
    pub fn with_asset_ref(mut self, asset_ref: impl Into<String>) -> Self {
        self.context.asset_ref = Some(asset_ref.into());
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.context.source = Some(source.into());
        self
    }

    #[must_use]
    pub fn with_component_id(mut self, component_id: impl Into<String>) -> Self {
        self.context.component_id = Some(component_id.into());
        self
    }

    #[must_use]
    pub fn with_entity_id(mut self, entity_id: impl Into<String>) -> Self {
        self.context.entity_id = Some(entity_id.into());
        self
    }

    #[must_use]
    pub fn with_field_path(mut self, field_path: impl Into<String>) -> Self {
        self.context.field_path = Some(field_path.into());
        self
    }

    fn absorb_repeated(&mut self, latest: Self) {
        self.repeat_count += latest.repeat_count.max(1);
        self.last_frame = latest
            .last_frame
            .or(latest.context.frame)
            .or(self.last_frame);
        self.context = latest.context;
    }

    pub fn emit_to_tracing(&self) {
        match self.severity {
            DiagnosticSeverity::Error => tracing::error!(
                domain = self.domain.as_str(),
                code = self.code.as_str(),
                repeat_count = self.repeat_count,
                "{}",
                self.message
            ),
            DiagnosticSeverity::Warning => tracing::warn!(
                domain = self.domain.as_str(),
                code = self.code.as_str(),
                repeat_count = self.repeat_count,
                "{}",
                self.message
            ),
            DiagnosticSeverity::Info => tracing::info!(
                domain = self.domain.as_str(),
                code = self.code.as_str(),
                repeat_count = self.repeat_count,
                "{}",
                self.message
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RuntimeDiagnosticsSettings {
    pub capacity: usize,
}

impl Default for RuntimeDiagnosticsSettings {
    fn default() -> Self {
        Self { capacity: 256 }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuntimeDiagnosticFilter {
    severity: Option<DiagnosticSeverity>,
    domain: Option<RuntimeDiagnosticDomain>,
    code: Option<DiagnosticCode>,
}

impl RuntimeDiagnosticFilter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn severity(mut self, severity: DiagnosticSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    #[must_use]
    pub fn domain(mut self, domain: impl Into<RuntimeDiagnosticDomain>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    #[must_use]
    pub fn code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(DiagnosticCode::new(code));
        self
    }

    fn matches(&self, entry: &RuntimeDiagnosticEntry) -> bool {
        self.severity
            .is_none_or(|severity| entry.severity == severity)
            && self
                .domain
                .as_ref()
                .is_none_or(|domain| entry.domain == *domain)
            && self.code.as_ref().is_none_or(|code| entry.code == *code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RuntimeDiagnostics {
    settings: RuntimeDiagnosticsSettings,
    entries: VecDeque<RuntimeDiagnosticEntry>,
    dropped_entries: u64,
}

impl Default for RuntimeDiagnostics {
    fn default() -> Self {
        Self::new(RuntimeDiagnosticsSettings::default())
    }
}

impl RuntimeDiagnostics {
    #[must_use]
    pub fn new(settings: RuntimeDiagnosticsSettings) -> Self {
        Self {
            settings,
            entries: VecDeque::with_capacity(settings.capacity),
            dropped_entries: 0,
        }
    }

    #[must_use]
    pub const fn settings(&self) -> RuntimeDiagnosticsSettings {
        self.settings
    }

    #[must_use]
    pub const fn dropped_entries(&self) -> u64 {
        self.dropped_entries
    }

    pub fn push(&mut self, entry: RuntimeDiagnosticEntry) {
        if let Some(dedupe_key) = entry.dedupe_key.as_deref()
            && let Some(existing) = self
                .entries
                .iter_mut()
                .find(|existing| existing.dedupe_key.as_deref() == Some(dedupe_key))
        {
            existing.absorb_repeated(entry);
            return;
        }

        if self.settings.capacity == 0 {
            self.dropped_entries += 1;
            return;
        }

        while self.entries.len() >= self.settings.capacity {
            self.entries.pop_front();
            self.dropped_entries += 1;
        }

        self.entries.push_back(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RuntimeDiagnosticEntry> {
        self.entries.iter()
    }

    pub fn iter_filtered(
        &self,
        filter: RuntimeDiagnosticFilter,
    ) -> impl Iterator<Item = &RuntimeDiagnosticEntry> {
        self.entries
            .iter()
            .filter(move |entry| filter.matches(entry))
    }

    pub fn emit_to_tracing(&self) {
        for entry in &self.entries {
            entry.emit_to_tracing();
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticsPlugin {
    settings: RuntimeDiagnosticsSettings,
}

impl Default for DiagnosticsPlugin {
    fn default() -> Self {
        Self {
            settings: RuntimeDiagnosticsSettings::default(),
        }
    }
}

impl DiagnosticsPlugin {
    #[must_use]
    pub const fn new(settings: RuntimeDiagnosticsSettings) -> Self {
        Self { settings }
    }
}

impl Plugin for DiagnosticsPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.diagnostic"),
            nara_app::PluginCategory::Core,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if !app.world().contains_resource::<RuntimeDiagnostics>() {
            app.insert_resource(RuntimeDiagnostics::new(self.settings));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticReport {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReport {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn extend(&mut self, report: Self) {
        self.diagnostics.extend(report.diagnostics);
    }

    pub fn emit_to_tracing(&self) {
        for diagnostic in &self.diagnostics {
            diagnostic.emit_to_tracing();
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }
}

pub mod prelude {
    pub use crate::{
        Diagnostic, DiagnosticCode, DiagnosticContext, DiagnosticReport, DiagnosticSeverity,
        DiagnosticsPlugin, RuntimeDiagnosticContext, RuntimeDiagnosticDomain,
        RuntimeDiagnosticEntry, RuntimeDiagnosticFilter, RuntimeDiagnostics,
        RuntimeDiagnosticsSettings,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_tracks_error_severity() {
        let mut report = DiagnosticReport::default();
        report.push(Diagnostic::warning(
            "asset.missing-meta",
            "missing metadata",
        ));
        assert!(!report.has_errors());

        report.push(Diagnostic::error("scene.invalid", "invalid scene"));

        assert!(report.has_errors());
        assert_eq!(report.diagnostics().len(), 2);
    }

    #[test]
    fn report_collects_diagnostics_without_implicit_logging_bridge() {
        let mut report = DiagnosticReport::default();
        let mut other = DiagnosticReport::default();

        report.push(Diagnostic::info("scene.ok", "scene is valid"));
        other.push(Diagnostic::warning("asset.pending", "asset is pending"));
        report.extend(other);

        assert_eq!(
            report
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            vec!["scene.ok", "asset.pending"]
        );
        report.emit_to_tracing();
    }

    #[test]
    fn diagnostic_context_identifies_scene_problem_location() {
        let diagnostic = Diagnostic::error("scene.invalid-field", "invalid field")
            .with_operation_index(3)
            .with_entity_id("player")
            .with_component_id("nara.transform.Transform2d")
            .with_field_path("translation.x")
            .with_asset_ref("textures/player.png");

        assert_eq!(diagnostic.context.operation_index, Some(3));
        assert_eq!(diagnostic.context.entity_id.as_deref(), Some("player"));
        assert_eq!(
            diagnostic.context.component_id.as_deref(),
            Some("nara.transform.Transform2d")
        );
        assert_eq!(
            diagnostic.context.field_path.as_deref(),
            Some("translation.x")
        );
        assert_eq!(
            diagnostic.context.asset_ref.as_deref(),
            Some("textures/player.png")
        );
    }

    #[test]
    fn runtime_diagnostics_drop_oldest_entries_at_capacity() {
        let mut diagnostics = RuntimeDiagnostics::new(RuntimeDiagnosticsSettings { capacity: 2 });

        diagnostics.push(RuntimeDiagnosticEntry::info("asset", "asset.a", "first"));
        diagnostics.push(RuntimeDiagnosticEntry::info("asset", "asset.b", "second"));
        diagnostics.push(RuntimeDiagnosticEntry::info("asset", "asset.c", "third"));

        assert_eq!(diagnostics.dropped_entries(), 1);
        assert_eq!(
            diagnostics
                .iter()
                .map(|entry| entry.code.as_str())
                .collect::<Vec<_>>(),
            vec!["asset.b", "asset.c"]
        );
    }

    #[test]
    fn runtime_diagnostics_dedupe_repeated_entries() {
        let mut diagnostics = RuntimeDiagnostics::new(RuntimeDiagnosticsSettings { capacity: 8 });

        diagnostics.push(
            RuntimeDiagnosticEntry::warning("watch", "watch.error", "notify failed")
                .with_frame(3)
                .with_dedupe_key("watch:error"),
        );
        diagnostics.push(
            RuntimeDiagnosticEntry::warning("watch", "watch.error", "notify failed")
                .with_frame(5)
                .with_dedupe_key("watch:error"),
        );

        let entries = diagnostics.iter().collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].repeat_count, 2);
        assert_eq!(entries[0].first_frame, Some(3));
        assert_eq!(entries[0].last_frame, Some(5));
    }

    #[test]
    fn runtime_diagnostics_filter_by_severity_domain_and_code() {
        let mut diagnostics = RuntimeDiagnostics::default();
        diagnostics.push(RuntimeDiagnosticEntry::error(
            "render",
            "render.device",
            "lost",
        ));
        diagnostics.push(RuntimeDiagnosticEntry::warning(
            "asset",
            "asset.reload",
            "failed",
        ));
        diagnostics.push(RuntimeDiagnosticEntry::warning(
            "asset",
            "asset.watch",
            "missed",
        ));

        let filtered = diagnostics
            .iter_filtered(
                RuntimeDiagnosticFilter::new()
                    .severity(DiagnosticSeverity::Warning)
                    .domain("asset")
                    .code("asset.reload"),
            )
            .map(|entry| entry.message.as_str())
            .collect::<Vec<_>>();

        assert_eq!(filtered, vec!["failed"]);
        diagnostics.emit_to_tracing();
    }

    #[test]
    fn diagnostics_plugin_installs_runtime_bus() {
        let mut app = App::new();

        app.add_plugin(DiagnosticsPlugin::new(RuntimeDiagnosticsSettings {
            capacity: 4,
        }))
        .unwrap();

        assert_eq!(
            app.world()
                .resource::<RuntimeDiagnostics>()
                .settings()
                .capacity,
            4
        );
    }
}
