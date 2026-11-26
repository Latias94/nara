use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.UnsupportedCapability",
    version = 1,
    alias = "Unsupported capability",
    component_capabilities(scene, script),
    field_capabilities(scene)
)]
struct UnsupportedCapability {
    #[schema(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
