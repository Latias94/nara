use nara_ecs::{Entity, Resource, World};

#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct WorldSnapshot {
    /// Total entities visible to `bevy_ecs::World`, including engine/internal entities.
    pub entity_count: u32,
    pub entities: Vec<Entity>,
}

impl WorldSnapshot {
    #[must_use]
    pub fn capture(world: &mut World) -> Self {
        let mut query = world.query::<Entity>();
        let mut entities = query.iter(world).collect::<Vec<_>>();
        entities.sort_by_key(|entity| entity.index());
        Self {
            entity_count: entities.len() as u32,
            entities,
        }
    }
}
