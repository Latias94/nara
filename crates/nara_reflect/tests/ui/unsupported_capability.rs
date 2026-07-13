use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.UnsupportedCapability",
    version = 1,
    alias = "Unsupported capability",
    component_capabilities(scene, script),
    field_capabilities(scene)
)]
struct UnsupportedCapability {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
