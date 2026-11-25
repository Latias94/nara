use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use nara_app::{
    App, CoreStage, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
    PluginId, StartupStage,
};

use super::{propagate_spatial, register_spatial};

pub const SPATIAL_PLUGIN_ID: PluginId = PluginId::new("nara.scene.spatial");
const SPATIAL_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SPATIAL_PLUGIN_ID, PluginCategory::Core);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, SystemSet)]
pub enum SpatialSet {
    /// Initialize globals after startup scene content has been materialized.
    Startup,
    /// Publish fixed-step inputs after preparation and its deferred commands.
    BeforeFixed,
    /// Publish simulation results in fixed-step finalization.
    AfterFixed,
    PostUpdate,
}

/// Updates ECS transforms and visibility around simulation and before render extraction.
#[derive(Default)]
pub struct SpatialPlugin;

impl Plugin for SpatialPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SPATIAL_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        register_spatial(app.world_mut()?);
        app.add_systems(
            StartupStage::Scene,
            propagate_spatial.in_set(SpatialSet::Startup),
        )?
        .configure_sets(
            CoreStage::FixedUpdate,
            SpatialSet::BeforeFixed
                .after(FixedUpdateSet::Prepare)
                .before(FixedUpdateSet::Simulate),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            (
                propagate_spatial.in_set(SpatialSet::BeforeFixed),
                propagate_spatial
                    .in_set(SpatialSet::AfterFixed)
                    .in_set(FixedUpdateSet::Finalize),
            ),
        )?
        .add_systems(
            CoreStage::PostUpdate,
            propagate_spatial.in_set(SpatialSet::PostUpdate),
        )?;
        Ok(())
    }
}
