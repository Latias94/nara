use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.UnsupportedPointerSized",
    version = 1,
    alias = "Unsupported pointer sized",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct UnsupportedPointerSized {
    #[nara(id = "value", alias = "Value")]
    value: usize,
}

fn main() {}
