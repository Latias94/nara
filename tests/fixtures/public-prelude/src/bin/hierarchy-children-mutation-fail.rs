use nara::{ecs::Entity, hierarchy::Children};

fn mutate(children: &mut Children, child: Entity) {
    children.0.push(child);
}

fn main() {
    let _ = mutate;
}
