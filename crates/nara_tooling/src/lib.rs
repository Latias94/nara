//! Tooling-facing runtime inspection seam.

use nara_app::{App, CoreStage, Plugin};
use nara_ecs::{Entity, Resource, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct WorldSnapshot {
    pub entity_count: u32,
}

impl WorldSnapshot {
    #[must_use]
    pub fn capture(world: &mut World) -> Self {
        let mut query = world.query::<Entity>();
        Self {
            entity_count: query.iter(world).count() as u32,
        }
    }
}

#[derive(Debug, Default)]
pub struct ToolingPlugin;

impl Plugin for ToolingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(CoreStage::Last, || {});
    }
}
