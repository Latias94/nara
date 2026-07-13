use substrate::{Component, World};

#[derive(Component)]
#[component(storage = "SparseSet", immutable)]
pub struct RenamedEcsComponent;

pub fn insert_component(world: &mut World) {
    world.spawn(RenamedEcsComponent);
}
