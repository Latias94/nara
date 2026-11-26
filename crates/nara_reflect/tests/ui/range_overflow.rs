use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(id = "test.Range", version = 1, alias = "Range",
    component_capabilities(scene), field_capabilities(scene))]
struct Invalid {
    #[schema(id = "value", alias = "Value", range(max_inclusive = 256_u8))]
    value: u8,
}
fn main() {}
