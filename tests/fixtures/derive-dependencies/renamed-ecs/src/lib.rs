use substrate::{Component, Resource, ScheduleLabel, SystemSet, World};

#[derive(Component)]
#[component(storage = "SparseSet", immutable)]
pub struct RenamedEcsComponent;

#[derive(Debug, Default, Resource)]
pub struct RenamedEcsResource<T: Send + Sync + 'static>(pub T);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct RenamedSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct RenamedSet;

pub fn insert_component(world: &mut World) {
    world.spawn(RenamedEcsComponent);
    world.init_resource::<RenamedEcsResource<u32>>();
}
