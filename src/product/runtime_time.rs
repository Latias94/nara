//! Initial Runtime clocks installed before ordinary product plugin construction.

use nara_app::{
    App, FixedCatchUpPolicy, FixedTime, Plugin, PluginCategory, PluginDeclaration,
    PluginDefinition, PluginDefinitionId, PluginError, PluginId, RuntimeTimeSettings,
};
use nara_project::EffectiveRuntimeSettings;

pub(super) const PLUGIN_ID: PluginId = PluginId::new("nara.product.runtime-time");

struct RuntimeTimeConfigurationPlugin {
    runtime: RuntimeTimeSettings,
    fixed: FixedTime,
}

impl Plugin for RuntimeTimeConfigurationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PLUGIN_ID, PluginCategory::Core);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.runtime)?
            .insert_resource(self.fixed)?;
        Ok(())
    }
}

pub(super) fn definition(settings: &EffectiveRuntimeSettings) -> PluginDefinition {
    let runtime = settings.runtime_time_settings();
    let fixed = settings.fixed_time();
    let mut canonical = Vec::from(b"nara.product.runtime-time.v1\0".as_slice());
    canonical.push(u8::from(runtime.paused()));
    canonical.extend_from_slice(&runtime.time_scale().to_bits().to_le_bytes());
    for duration in [runtime.max_delta(), fixed.timestep()] {
        canonical.extend_from_slice(&duration.as_secs().to_le_bytes());
        canonical.extend_from_slice(&duration.subsec_nanos().to_le_bytes());
    }
    canonical.extend_from_slice(&fixed.max_steps_per_frame().to_le_bytes());
    canonical.extend_from_slice(&fixed.max_debt_steps().to_le_bytes());
    canonical.push(match fixed.catch_up_policy() {
        FixedCatchUpPolicy::DiscardExcess => 0,
        FixedCatchUpPolicy::PreserveDebt => 1,
    });
    PluginDefinition::infallible::<RuntimeTimeConfigurationPlugin, _>(
        PluginDefinitionId::new("nara.product.runtime-time", 1),
        canonical,
        move || RuntimeTimeConfigurationPlugin { runtime, fixed },
    )
}
