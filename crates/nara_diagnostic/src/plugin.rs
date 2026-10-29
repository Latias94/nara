use nara_app::{App, CoreStage, Plugin, PluginError, RealTime};
use nara_ecs::{
    schedule::{IntoScheduleConfigs, SystemSet},
    system::{Res, ResMut},
};

use crate::{
    RuntimeDiagnostics, RuntimeDiagnosticsSettings, RuntimePressureRetention,
    RuntimePressureSettings, RuntimePressureSnapshots,
};

#[cfg(test)]
mod configuration_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum DiagnosticCleanupSet {
    Retention,
}

#[derive(Debug, Default, Clone)]
pub struct DiagnosticsPlugin {
    runtime_settings: RuntimeDiagnosticsSettings,
    pressure_settings: RuntimePressureSettings,
}

pub const DIAGNOSTICS_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.diagnostic");
const DIAGNOSTICS_PLUGIN_DEFINITION_ID: nara_app::PluginDefinitionId =
    nara_app::PluginDefinitionId::new("nara.diagnostic.configured", 1);
pub const DIAGNOSTICS_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(DIAGNOSTICS_PLUGIN_ID, nara_app::PluginCategory::Core);

impl DiagnosticsPlugin {
    #[must_use]
    pub const fn new(
        runtime_settings: RuntimeDiagnosticsSettings,
        pressure_settings: RuntimePressureSettings,
    ) -> Self {
        Self {
            runtime_settings,
            pressure_settings,
        }
    }

    #[must_use]
    pub const fn runtime_settings(&self) -> RuntimeDiagnosticsSettings {
        self.runtime_settings
    }

    #[must_use]
    pub const fn pressure_settings(&self) -> RuntimePressureSettings {
        self.pressure_settings
    }
}

/// Creates a repeatable diagnostics plugin definition with canonical runtime and pressure settings.
#[must_use]
pub fn plugin(
    runtime_settings: RuntimeDiagnosticsSettings,
    pressure_settings: RuntimePressureSettings,
) -> nara_app::PluginDefinition {
    let canonical_configuration =
        diagnostics_plugin_configuration(runtime_settings, pressure_settings);
    nara_app::PluginDefinition::infallible::<DiagnosticsPlugin, _>(
        DIAGNOSTICS_PLUGIN_DEFINITION_ID,
        canonical_configuration,
        move || DiagnosticsPlugin::new(runtime_settings, pressure_settings),
    )
}

fn diagnostics_plugin_configuration(
    runtime_settings: RuntimeDiagnosticsSettings,
    pressure_settings: RuntimePressureSettings,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(112);
    bytes.extend_from_slice(b"nara.diagnostic.plugin-config.v1\0");
    // V1 encodes diagnostic budgets, pressure budgets, then their tagged retention windows.
    for limit in [
        runtime_settings.entry_limit().get(),
        runtime_settings.byte_limit().get(),
        runtime_settings.field_limit().get(),
        runtime_settings.summary_byte_limit().get(),
        runtime_settings.field_text_byte_limit().get(),
        pressure_settings.source_limit().get(),
        pressure_settings.measurements_per_source().get(),
    ] {
        let limit =
            u64::try_from(limit).expect("validated diagnostic and pressure limits fit in u64");
        bytes.extend_from_slice(&limit.to_le_bytes());
    }
    for frames in [
        runtime_settings.retention().frames(),
        match pressure_settings.retention() {
            RuntimePressureRetention::Manual => None,
            RuntimePressureRetention::FrameWindow(frames) => Some(frames),
        },
    ] {
        match frames {
            None => bytes.push(0),
            Some(frames) => {
                bytes.push(1);
                bytes.extend_from_slice(&frames.get().to_le_bytes());
            }
        }
    }
    bytes
}

impl Plugin for DiagnosticsPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &DIAGNOSTICS_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if !app.world().contains_resource::<RuntimeDiagnostics>() {
            app.insert_resource(RuntimeDiagnostics::new(self.runtime_settings))?;
        }
        if !app.world().contains_resource::<RuntimePressureSnapshots>() {
            app.insert_resource(RuntimePressureSnapshots::new(self.pressure_settings))?;
        }
        app.add_systems(
            CoreStage::First,
            maintain_runtime_observations.in_set(DiagnosticCleanupSet::Retention),
        )?;
        Ok(())
    }
}

fn maintain_runtime_observations(
    real_time: Res<RealTime>,
    mut diagnostics: ResMut<RuntimeDiagnostics>,
    mut pressure: ResMut<RuntimePressureSnapshots>,
) {
    diagnostics.maintain(real_time.frame);
    pressure.maintain(real_time.frame);
}
