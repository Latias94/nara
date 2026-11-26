use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.UnsupportedPointerSized",
    version = 1,
    alias = "Unsupported pointer sized",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct UnsupportedPointerSized {
    #[schema(id = "value", alias = "Value")]
    value: usize,
}

fn main() {}
