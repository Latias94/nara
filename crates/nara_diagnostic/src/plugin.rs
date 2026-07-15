use nara_app::{App, CoreStage, Plugin, PluginError, RealTime};
use nara_ecs::{
    schedule::{IntoScheduleConfigs, SystemSet},
    system::{Res, ResMut},
};

use crate::{
    RuntimeDiagnostics, RuntimeDiagnosticsSettings, RuntimePressureSettings,
    RuntimePressureSnapshots,
};

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
