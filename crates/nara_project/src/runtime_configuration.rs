use std::time::Duration;

use nara_app::{FixedCatchUpPolicy, FixedTime, RuntimeTimeSettings};
use nara_diagnostic::DiagnosticReport;
use nara_window::{PresentMode, WindowMode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capability::{ProductCapabilitySet, RuntimePreset};
use crate::effective::{
    EffectiveDiagnosticsSettings, EffectiveProjectSettings, EffectiveRuntimeSettings,
    EffectiveTaskSettings, EffectiveUiSettings, EffectiveWindowSettings,
    enforce_runtime_preset_invariants,
};
use crate::profile::ProjectProfileError;
use crate::render::{EffectiveRenderSettings, ProjectRenderManifest, ProjectWorldCanvasManifest};
use crate::sections::{
    ProjectCapabilitiesManifest, ProjectDiagnosticsManifest, ProjectFixedCatchUpPolicy,
    ProjectPresentMode, ProjectTasksManifest, ProjectUiManifest, ProjectUiSafeAreaManifest,
    ProjectUiSurfaceManifest, ProjectWindowManifest, ProjectWindowMode, lower_runtime_settings,
};
use crate::validation::{
    error, validate_fixed_step_limits, validate_nonzero_duration, validate_time_scale,
    with_field_path,
};

/// Exact runtime timing values, independent of the source manifest's floating-point seconds.
///
/// Duration fields serialize as strict `{ secs, nanos }` maps with `nanos < 1_000_000_000`.
/// Zero durations may deserialize but fail admission through [`RuntimeConfiguration::resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeTimeConfiguration {
    pub preset: RuntimePreset,
    pub paused: bool,
    pub time_scale: f32,
    #[serde(with = "duration_serde")]
    pub max_delta: Duration,
    #[serde(with = "duration_serde")]
    pub fixed_timestep: Duration,
    pub max_fixed_steps_per_frame: u32,
    pub max_fixed_debt_steps: u32,
    pub catch_up_policy: ProjectFixedCatchUpPolicy,
}

impl Default for RuntimeTimeConfiguration {
    fn default() -> Self {
        Self::from_effective(
            RuntimePreset::default(),
            &EffectiveRuntimeSettings::new(RuntimeTimeSettings::default(), FixedTime::default()),
        )
    }
}

impl RuntimeTimeConfiguration {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        validate_time_scale(
            diagnostics,
            &format!("{prefix}.time_scale"),
            self.time_scale,
        );
        validate_nonzero_duration(diagnostics, &format!("{prefix}.max_delta"), self.max_delta);
        validate_nonzero_duration(
            diagnostics,
            &format!("{prefix}.fixed_timestep"),
            self.fixed_timestep,
        );
        validate_fixed_step_limits(
            diagnostics,
            prefix,
            Some(self.max_fixed_steps_per_frame),
            Some(self.max_fixed_debt_steps),
        );
    }

    fn lower(self) -> Result<EffectiveRuntimeSettings, ProjectProfileError> {
        lower_runtime_settings(
            self.paused,
            self.time_scale,
            self.max_delta,
            self.fixed_timestep,
            self.max_fixed_steps_per_frame,
            self.max_fixed_debt_steps,
            self.catch_up_policy,
        )
    }

    fn from_effective(preset: RuntimePreset, runtime: &EffectiveRuntimeSettings) -> Self {
        let time = runtime.runtime_time_settings();
        let fixed = runtime.fixed_time();
        Self {
            preset,
            paused: time.paused(),
            time_scale: time.time_scale(),
            max_delta: time.max_delta(),
            fixed_timestep: fixed.timestep(),
            max_fixed_steps_per_frame: fixed.max_steps_per_frame(),
            max_fixed_debt_steps: fixed.max_debt_steps(),
            catch_up_policy: match fixed.catch_up_policy() {
                FixedCatchUpPolicy::DiscardExcess => ProjectFixedCatchUpPolicy::DiscardExcess,
                FixedCatchUpPolicy::PreserveDebt => ProjectFixedCatchUpPolicy::PreserveDebt,
            },
        }
    }
}

mod duration_serde {
    use std::{fmt, time::Duration};

    use serde::de::{Error, MapAccess, Visitor, value::MapAccessDeserializer};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct DurationRecord {
        secs: u64,
        nanos: u32,
    }

    pub(super) fn serialize<S: Serializer>(
        value: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        DurationRecord {
            secs: value.as_secs(),
            nanos: value.subsec_nanos(),
        }
        .serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        struct DurationVisitor;

        impl<'de> Visitor<'de> for DurationVisitor {
            type Value = Duration;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a duration map with secs and nanos")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let record = DurationRecord::deserialize(MapAccessDeserializer::new(map))?;
                if record.nanos >= 1_000_000_000 {
                    return Err(A::Error::custom(
                        "duration nanos must be less than 1_000_000_000",
                    ));
                }
                // Bounds on both integers and nanos prevent normalization or seconds overflow.
                Ok(Duration::new(record.secs, record.nanos))
            }
        }

        deserializer.deserialize_map(DurationVisitor)
    }
}

/// Serializable runtime values, independent of project source and profile selection.
///
/// Deserialization produces an untrusted record. Call [`Self::resolve`] to validate and admit
/// these values before runtime composition. This record neither selects executable code nor
/// carries project paths, content, source identity, or an external Play execution contract.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfiguration {
    #[serde(default)]
    pub capabilities: ProjectCapabilitiesManifest,
    #[serde(default)]
    pub runtime: RuntimeTimeConfiguration,
    #[serde(default)]
    pub tasks: ProjectTasksManifest,
    #[serde(default)]
    pub render: ProjectRenderManifest,
    #[serde(default)]
    pub ui: ProjectUiManifest,
    #[serde(default)]
    pub window: ProjectWindowManifest,
    #[serde(default)]
    pub diagnostics: ProjectDiagnosticsManifest,
}

impl RuntimeConfiguration {
    /// Validates every section before lowering and applies the shared runtime preset rules.
    pub fn resolve(&self) -> Result<EffectiveRuntimeConfiguration, RuntimeConfigurationError> {
        let mut diagnostics = DiagnosticReport::default();
        self.capabilities
            .validate_into(&mut diagnostics, "capabilities");
        self.runtime.validate_into(&mut diagnostics, "runtime");
        self.tasks.validate_into(&mut diagnostics, "tasks");
        self.render.validate_into(&mut diagnostics, "render");
        self.ui.validate_into(&mut diagnostics, "ui");
        self.window.validate_into(&mut diagnostics, "window");
        self.diagnostics
            .validate_into(&mut diagnostics, "diagnostics");
        if diagnostics.has_errors() {
            return Err(RuntimeConfigurationError::invalid(diagnostics));
        }

        let mut runtime = self
            .runtime
            .lower()
            .map_err(|source| RuntimeConfigurationError::lowering("runtime", source))?;
        let tasks = self
            .tasks
            .lower()
            .map_err(|source| RuntimeConfigurationError::lowering("tasks", source))?;
        let diagnostics = self
            .diagnostics
            .lower()
            .map_err(|source| RuntimeConfigurationError::lowering("diagnostics", source))?;
        let mut window = self.window.clone().lower();
        enforce_runtime_preset_invariants(self.runtime.preset, &mut runtime, &mut window);

        Ok(EffectiveRuntimeConfiguration {
            runtime_preset: self.runtime.preset,
            requested_capabilities: self.capabilities.lower(),
            runtime,
            tasks,
            render: self.render.into(),
            ui: EffectiveUiSettings::from_manifest(self.ui.clone()),
            window,
            diagnostics,
        })
    }
}

/// Admitted runtime values with no project source authority.
///
/// Construction always passes through [`RuntimeConfiguration::resolve`]. Getters expose shared
/// references so callers cannot invalidate admission by changing the effective section fields.
///
/// ```compile_fail
/// fn requires_deserialize<T: serde::de::DeserializeOwned>() {}
/// requires_deserialize::<nara_project::EffectiveRuntimeConfiguration>();
/// ```
///
/// ```compile_fail
/// let mut effective = nara_project::RuntimeConfiguration::default().resolve().unwrap();
/// effective.window().width = 0;
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveRuntimeConfiguration {
    runtime_preset: RuntimePreset,
    requested_capabilities: ProductCapabilitySet,
    runtime: EffectiveRuntimeSettings,
    tasks: EffectiveTaskSettings,
    render: EffectiveRenderSettings,
    ui: EffectiveUiSettings,
    window: EffectiveWindowSettings,
    diagnostics: EffectiveDiagnosticsSettings,
}

impl EffectiveRuntimeConfiguration {
    #[must_use]
    pub const fn runtime_preset(&self) -> RuntimePreset {
        self.runtime_preset
    }

    #[must_use]
    pub const fn requested_capabilities(&self) -> ProductCapabilitySet {
        self.requested_capabilities
    }

    #[must_use]
    pub const fn runtime(&self) -> &EffectiveRuntimeSettings {
        &self.runtime
    }

    #[must_use]
    pub const fn tasks(&self) -> &EffectiveTaskSettings {
        &self.tasks
    }

    #[must_use]
    pub const fn render(&self) -> &EffectiveRenderSettings {
        &self.render
    }

    #[must_use]
    pub const fn ui(&self) -> &EffectiveUiSettings {
        &self.ui
    }

    #[must_use]
    pub const fn window(&self) -> &EffectiveWindowSettings {
        &self.window
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &EffectiveDiagnosticsSettings {
        &self.diagnostics
    }

    /// Exports the complete admitted configuration, preserving the effective runtime values.
    ///
    /// Duration fields must resolve back to exactly the same nanoseconds. The exported record
    /// includes preset normalization and needs no source manifest or profile to resolve again.
    #[must_use]
    pub fn to_configuration(&self) -> RuntimeConfiguration {
        configuration_record(
            self.requested_capabilities,
            RuntimeTimeConfiguration::from_effective(self.runtime_preset, &self.runtime),
            &self.tasks,
            &self.render,
            &self.ui,
            &self.window,
            &self.diagnostics,
        )
    }
}

impl EffectiveProjectSettings {
    /// Projects the resolved profile into independently admitted runtime values.
    ///
    /// A remaining input action-map path requires source admission and is rejected. A profile
    /// can explicitly clear that path before projection. Public effective fields are validated
    /// again, and values the runtime record cannot represent exactly are rejected.
    pub fn runtime_configuration(
        &self,
    ) -> Result<EffectiveRuntimeConfiguration, RuntimeConfigurationError> {
        if self.input.action_map.is_some() {
            return Err(RuntimeConfigurationError::field(
                "runtime.configuration.source-input-required",
                "Runtime configuration cannot carry a source input action-map path",
                "input.action_map",
            ));
        }

        let configuration = configuration_record(
            self.requested_capabilities,
            RuntimeTimeConfiguration::from_effective(self.runtime_preset, &self.runtime),
            &self.tasks,
            &self.render,
            &self.ui,
            &self.window,
            &self.diagnostics,
        );
        let effective = configuration.resolve()?;
        let mut diagnostics = DiagnosticReport::default();
        // Public source settings can contain low-level policy values beyond the source codec.
        // Never truncate task timeouts or drop non-default diagnostic policy during projection.
        if effective.tasks != self.tasks {
            diagnostics.push(with_field_path(
                error(
                    "runtime.configuration.unrepresentable-tasks",
                    "Task settings cannot be represented exactly by the runtime record",
                ),
                "tasks",
            ));
        }
        if effective.diagnostics != self.diagnostics {
            diagnostics.push(with_field_path(
                error(
                    "runtime.configuration.unrepresentable-diagnostics",
                    "Diagnostic settings cannot be represented exactly by the runtime record",
                ),
                "diagnostics",
            ));
        }
        if diagnostics.has_errors() {
            return Err(RuntimeConfigurationError::invalid(diagnostics));
        }
        Ok(effective)
    }
}

fn configuration_record(
    requested_capabilities: ProductCapabilitySet,
    runtime: RuntimeTimeConfiguration,
    tasks: &EffectiveTaskSettings,
    render: &EffectiveRenderSettings,
    ui: &EffectiveUiSettings,
    window: &EffectiveWindowSettings,
    diagnostics: &EffectiveDiagnosticsSettings,
) -> RuntimeConfiguration {
    RuntimeConfiguration {
        capabilities: ProjectCapabilitiesManifest {
            requested: requested_capabilities.iter().collect(),
        },
        runtime,
        tasks: ProjectTasksManifest::from_config(tasks.pool_config),
        render: ProjectRenderManifest {
            world_canvas: render
                .world_canvas
                .map(|canvas| ProjectWorldCanvasManifest {
                    width: canvas.width,
                    height: canvas.height,
                }),
        },
        ui: ProjectUiManifest {
            primary_surface: ui.primary_surface.map(|surface| ProjectUiSurfaceManifest {
                logical_width: surface.logical_width,
                logical_height: surface.logical_height,
                safe_area: ProjectUiSafeAreaManifest {
                    left: surface.safe_area.left,
                    top: surface.safe_area.top,
                    right: surface.safe_area.right,
                    bottom: surface.safe_area.bottom,
                },
            }),
        },
        window: ProjectWindowManifest {
            enabled: window.enabled,
            title: window.title.clone(),
            width: window.width,
            height: window.height,
            scale_factor: window.scale_factor,
            mode: match window.mode {
                WindowMode::Windowed => ProjectWindowMode::Windowed,
                WindowMode::BorderlessFullscreen => ProjectWindowMode::BorderlessFullscreen,
                WindowMode::Fullscreen => ProjectWindowMode::Fullscreen,
            },
            present_mode: match window.present_mode {
                PresentMode::AutoVsync => ProjectPresentMode::AutoVsync,
                PresentMode::AutoNoVsync => ProjectPresentMode::AutoNoVsync,
                PresentMode::Fifo => ProjectPresentMode::Fifo,
                PresentMode::Immediate => ProjectPresentMode::Immediate,
                PresentMode::Mailbox => ProjectPresentMode::Mailbox,
            },
            resizable: window.resizable,
        },
        diagnostics: ProjectDiagnosticsManifest {
            runtime_capacity: diagnostics.runtime.entry_limit().get(),
        },
    }
}

/// Structured admission or projection failure without source data in its public message.
#[derive(Debug, Error)]
#[error("runtime configuration is invalid")]
pub struct RuntimeConfigurationError {
    diagnostics: Box<DiagnosticReport>,
    #[source]
    source: Option<ProjectProfileError>,
}

impl RuntimeConfigurationError {
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    fn invalid(diagnostics: DiagnosticReport) -> Self {
        Self {
            diagnostics: Box::new(diagnostics),
            source: None,
        }
    }

    fn field(code: &'static str, summary: &'static str, field: &'static str) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(with_field_path(error(code, summary), field));
        Self::invalid(diagnostics)
    }

    fn lowering(field: &'static str, source: ProjectProfileError) -> Self {
        let mut failure = Self::field(
            "runtime.configuration.lowering-failed",
            "Validated runtime settings could not be lowered",
            field,
        );
        failure.source = Some(source);
        failure
    }
}

#[cfg(test)]
mod tests;
