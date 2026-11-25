//! Admission and export of the v0 cooked runtime configuration.

use std::time::Duration;

use nara_app::{FixedCatchUpPolicy, FixedTime, RuntimeTimeSettings};
use nara_asset::{AssetProductRef, ImportedProductId};
use nara_core::diagnostic::{DiagnosticReport, ReportDiagnostic};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod capability;
mod defaults;
mod render;
mod sections;
mod settings;
pub mod validation;

pub use capability::{
    ProductCapability, ProductCapabilitySet, RuntimeCapabilityAdmission, RuntimeCapabilityError,
    RuntimeCapabilityErrorKind, RuntimePreset, admit_runtime_capabilities,
};
pub use render::{
    EffectiveRenderSettings, EffectiveWorldCanvas, ProjectRenderManifest,
    ProjectWorldCanvasManifest,
};
pub use sections::{
    ProjectCapabilitiesManifest, ProjectDiagnosticsManifest, ProjectFixedCatchUpPolicy,
    ProjectPresentMode, ProjectTaskPoolManifest, ProjectTaskShutdownManifest, ProjectTasksManifest,
    ProjectUiManifest, ProjectUiSafeAreaManifest, ProjectUiSurfaceManifest, ProjectWindowManifest,
    ProjectWindowMode,
};
pub use settings::{
    EffectiveDiagnosticsSettings, EffectivePrimaryUiSurface, EffectiveRuntimeSettings,
    EffectiveTaskSettings, EffectiveUiSafeArea, EffectiveUiSettings, EffectiveWindowSettings,
    RuntimeSettingsError, enforce_runtime_preset_invariants,
};
use validation::{
    error, validate_fixed_step_limits, validate_nonzero_duration, validate_time_scale,
    with_field_path,
};

/// Maximum fixed schedules a v0 configuration may request in one app frame.
pub const MAX_PROJECT_FIXED_STEPS_PER_FRAME: u32 = 256;
/// Maximum retained fixed debt, equal to 64 frames at the step ceiling.
pub const MAX_PROJECT_FIXED_DEBT_STEPS: u32 = MAX_PROJECT_FIXED_STEPS_PER_FRAME * 64;
/// Maximum logical width or height for the primary UI surface.
pub const MAX_PROJECT_UI_LOGICAL_EXTENT: u32 = 65_536;

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

impl TryFrom<RuntimeTimeConfiguration> for EffectiveRuntimeSettings {
    type Error = RuntimeSettingsError;

    fn try_from(time: RuntimeTimeConfiguration) -> Result<Self, Self::Error> {
        let runtime_time = RuntimeTimeSettings::new(time.paused, time.time_scale, time.max_delta)?;
        let fixed_time = FixedTime::new(time.fixed_timestep)?
            .with_max_steps_per_frame(time.max_fixed_steps_per_frame)?
            .with_max_debt_steps(time.max_fixed_debt_steps)?
            .with_catch_up_policy(time.catch_up_policy.into());
        Ok(Self::new(runtime_time, fixed_time))
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

/// Processed startup Input selection, independent of source action-map paths.
///
/// Admission through [`RuntimeConfiguration::resolve`] requires a complete primary Product.
/// The catalogue and compiled recipe separately prove membership, the typed codec and Input owner.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeInputConfiguration {
    pub action_map: Option<AssetProductRef>,
}

impl RuntimeInputConfiguration {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if self
            .action_map
            .is_some_and(|reference| reference.product() != ImportedProductId::PRIMARY)
        {
            diagnostics.push(with_field_path(
                error(
                    "runtime.configuration.input-primary-required",
                    "Runtime input action-map selection requires a primary product",
                ),
                &format!("{prefix}.action_map"),
            ));
        }
    }
}

/// Serializable runtime values, independent of project source and profile selection.
///
/// Deserialization produces an untrusted record. Call [`Self::resolve`] to validate and admit
/// these values before runtime composition. This record neither selects executable code nor
/// carries project paths, authoring state, source authority, Scene selection, or an external Play
/// execution contract.
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
    pub input: RuntimeInputConfiguration,
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
        self.input.validate_into(&mut diagnostics, "input");
        self.diagnostics
            .validate_into(&mut diagnostics, "diagnostics");
        if diagnostics.has_errors() {
            return Err(RuntimeConfigurationError::invalid(diagnostics));
        }

        let mut runtime = EffectiveRuntimeSettings::try_from(self.runtime)
            .map_err(|source| RuntimeConfigurationError::lowering("runtime", source))?;
        let tasks = EffectiveTaskSettings::try_from(self.tasks)
            .map_err(|source| RuntimeConfigurationError::lowering("tasks", source))?;
        let diagnostics = EffectiveDiagnosticsSettings::try_from(self.diagnostics)
            .map_err(|source| RuntimeConfigurationError::lowering("diagnostics", source))?;
        let mut window = self.window.clone().into();
        enforce_runtime_preset_invariants(self.runtime.preset, &mut runtime, &mut window);

        Ok(EffectiveRuntimeConfiguration {
            settings: RuntimeConfigurationSettings {
                runtime_preset: self.runtime.preset,
                requested_capabilities: (&self.capabilities).into(),
                runtime,
                tasks,
                render: self.render.into(),
                ui: self.ui.clone().into(),
                window,
                diagnostics,
            },
            input: self.input.clone(),
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
/// requires_deserialize::<nara_scene::runtime_configuration::EffectiveRuntimeConfiguration>();
/// ```
///
/// ```compile_fail
/// use nara_scene::runtime_configuration::RuntimeConfiguration;
/// let mut effective = RuntimeConfiguration::default().resolve().unwrap();
/// effective.window().width = 0;
/// ```
///
/// ```compile_fail
/// use nara_scene::runtime_configuration::RuntimeConfiguration;
/// let effective = RuntimeConfiguration::default().resolve().unwrap();
/// effective.input().action_map = None;
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveRuntimeConfiguration {
    settings: RuntimeConfigurationSettings,
    input: RuntimeInputConfiguration,
}

impl EffectiveRuntimeConfiguration {
    #[must_use]
    pub const fn runtime_preset(&self) -> RuntimePreset {
        self.settings.runtime_preset
    }

    #[must_use]
    pub const fn requested_capabilities(&self) -> ProductCapabilitySet {
        self.settings.requested_capabilities
    }

    #[must_use]
    pub const fn runtime(&self) -> &EffectiveRuntimeSettings {
        &self.settings.runtime
    }

    #[must_use]
    pub const fn tasks(&self) -> &EffectiveTaskSettings {
        &self.settings.tasks
    }

    #[must_use]
    pub const fn render(&self) -> &EffectiveRenderSettings {
        &self.settings.render
    }

    #[must_use]
    pub const fn ui(&self) -> &EffectiveUiSettings {
        &self.settings.ui
    }

    #[must_use]
    pub const fn window(&self) -> &EffectiveWindowSettings {
        &self.settings.window
    }

    /// Returns the admitted processed startup Input selection.
    #[must_use]
    pub const fn input(&self) -> &RuntimeInputConfiguration {
        &self.input
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &EffectiveDiagnosticsSettings {
        &self.settings.diagnostics
    }

    /// Exports the complete admitted configuration, preserving the effective runtime values.
    ///
    /// Duration fields must resolve back to exactly the same nanoseconds. The exported record
    /// includes preset normalization and needs no source manifest or profile to resolve again.
    #[must_use]
    pub fn to_configuration(&self) -> RuntimeConfiguration {
        self.settings.clone().into_configuration(self.input.clone())
    }
}

/// Runtime settings supplied by trusted tooling for fresh admission as a v0 record.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeConfigurationSettings {
    pub runtime_preset: RuntimePreset,
    pub requested_capabilities: ProductCapabilitySet,
    pub runtime: EffectiveRuntimeSettings,
    pub tasks: EffectiveTaskSettings,
    pub render: EffectiveRenderSettings,
    pub ui: EffectiveUiSettings,
    pub window: EffectiveWindowSettings,
    pub diagnostics: EffectiveDiagnosticsSettings,
}

impl RuntimeConfigurationSettings {
    /// Revalidates the settings and checks that the v0 record preserves their task and
    /// diagnostic policies exactly. The resulting configuration has no input selection.
    pub fn resolve(self) -> Result<EffectiveRuntimeConfiguration, RuntimeConfigurationError> {
        let tasks = self.tasks;
        let diagnostic_settings = self.diagnostics;
        let effective = self
            .into_configuration(RuntimeInputConfiguration::default())
            .resolve()?;
        let mut diagnostics = DiagnosticReport::default();
        if *effective.tasks() != tasks {
            diagnostics.push(with_field_path(
                error(
                    "runtime.configuration.unrepresentable-tasks",
                    "Task settings cannot be represented exactly by the runtime record",
                ),
                "tasks",
            ));
        }
        if *effective.diagnostics() != diagnostic_settings {
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

    fn into_configuration(self, input: RuntimeInputConfiguration) -> RuntimeConfiguration {
        RuntimeConfiguration {
            capabilities: ProjectCapabilitiesManifest {
                requested: self.requested_capabilities.iter().collect(),
            },
            runtime: RuntimeTimeConfiguration::from_effective(self.runtime_preset, &self.runtime),
            tasks: ProjectTasksManifest::from_config(self.tasks.pool_config),
            render: ProjectRenderManifest {
                world_canvas: self
                    .render
                    .world_canvas
                    .map(|canvas| ProjectWorldCanvasManifest {
                        width: canvas.width,
                        height: canvas.height,
                    }),
            },
            ui: ProjectUiManifest {
                primary_surface: self
                    .ui
                    .primary_surface
                    .map(|surface| ProjectUiSurfaceManifest {
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
                enabled: self.window.enabled,
                title: self.window.title,
                width: self.window.width,
                height: self.window.height,
                scale_factor: self.window.scale_factor,
                mode: self.window.mode,
                present_mode: self.window.present_mode,
                resizable: self.window.resizable,
            },
            input,
            diagnostics: ProjectDiagnosticsManifest {
                runtime_capacity: self.diagnostics.runtime.entry_limit().get(),
            },
        }
    }
}

/// Structured admission or projection failure without source data in its public message.
#[derive(Debug, Error)]
#[error("runtime configuration is invalid")]
pub struct RuntimeConfigurationError {
    diagnostics: Box<DiagnosticReport>,
    #[source]
    source: Option<RuntimeSettingsError>,
}

impl RuntimeConfigurationError {
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Wraps a diagnostic emitted by a source projection before runtime admission.
    #[must_use]
    pub fn from_diagnostic(diagnostic: ReportDiagnostic) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic);
        Self::invalid(diagnostics)
    }

    fn invalid(diagnostics: DiagnosticReport) -> Self {
        Self {
            diagnostics: Box::new(diagnostics),
            source: None,
        }
    }

    fn field(code: &'static str, summary: &'static str, field: &'static str) -> Self {
        Self::from_diagnostic(with_field_path(error(code, summary), field))
    }

    fn lowering(field: &'static str, source: RuntimeSettingsError) -> Self {
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
