use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.DuplicateFieldId",
    version = 1,
    alias = "Duplicate field ID",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct DuplicateFieldId {
    #[schema(id = "value", alias = "First")]
    first: i64,
    #[schema(id = "value", alias = "Second")]
    second: i64,
}

fn main() {}
