use nara_ecs::World;
use nara_tooling::EditorWorkspace;

fn main() {
    let mut world = World::new();
    world.insert_resource(EditorWorkspace::new());
}
