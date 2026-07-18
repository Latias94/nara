use nara::{
    prelude::{Component, World},
    reflect::PreparedComponent,
};

#[derive(Component)]
struct Forged;

fn main() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    PreparedComponent::insert(Forged)
        .apply(&mut world, entity)
        .unwrap();
}
