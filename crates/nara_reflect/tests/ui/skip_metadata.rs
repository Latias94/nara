use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(id = "game.SkipMetadata", version = 1)]
struct SkipMetadata {
    #[schema(skip, read_only)]
    cached_ticks: u32,
}

fn main() {}
