use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    version = 1,
    alias = "Missing ID",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct MissingComponentId {
    #[schema(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
