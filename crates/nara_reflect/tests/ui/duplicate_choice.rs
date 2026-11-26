use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(id = "game.DuplicateChoice", version = 1)]
struct DuplicateChoice {
    #[schema(choices = ["walk", "walk"])]
    mode: String,
}

fn main() {}
