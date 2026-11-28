use nara::{prelude::*, scene::SceneError};

use crate::RunRules;

#[derive(Component, Default, Schema)]
#[schema(id = "night-relay.Pilot", version = 1, default, alias = "Pilot")]
pub(crate) struct Pilot;

#[derive(Component, Schema)]
#[schema(
    id = "night-relay.PowerCell",
    version = 1,
    authoring_initial,
    alias = "Power Cell"
)]
pub(crate) struct PowerCell {
    #[schema(alias = "Order", range = 1..=3, initial = 1)]
    pub order: u32,
}

#[derive(Component, Schema)]
#[schema(
    id = "night-relay.PatrolDrone",
    version = 1,
    default,
    alias = "Patrol Drone"
)]
pub(crate) struct PatrolDrone {
    #[schema(alias = "Order", range = 1..=3)]
    pub order: u32,
}

impl Default for PatrolDrone {
    fn default() -> Self {
        Self { order: 1 }
    }
}

#[derive(Component, Default, Schema)]
#[schema(
    id = "night-relay.ExtractionGate",
    version = 1,
    default,
    alias = "Extraction Gate"
)]
pub(crate) struct ExtractionGate;

pub(crate) fn register_scene_roles(app: &mut App) -> Result<(), SceneError> {
    app.register_scene_component::<RunRules>()?
        .register_scene_component::<Pilot>()?
        .register_scene_component::<PowerCell>()?
        .register_scene_component::<PatrolDrone>()?
        .register_scene_component::<ExtractionGate>()?;
    Ok(())
}

/// Registers the catalog in the exporter, whose minimal plugins do not install audio.
#[cfg(feature = "schema")]
pub fn register_game_schema(app: &mut App) -> Result<(), SceneError> {
    register_scene_roles(app)?;
    app.register_scene_component::<AudioClipSource>()?;
    Ok(())
}
