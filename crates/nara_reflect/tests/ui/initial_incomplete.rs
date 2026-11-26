use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.InitialIncomplete",
    version = 1,
    alias = "Initial incomplete",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialIncomplete {
    #[schema(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
