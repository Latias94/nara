use nara::{ecs::World, hierarchy::prepare_retirement_detach};

fn main() {
    let world = World::new();
    let _ = prepare_retirement_detach(&world, &[]);
}
