use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    version = 1,
    alias = "Missing ID",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct MissingComponentId {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
