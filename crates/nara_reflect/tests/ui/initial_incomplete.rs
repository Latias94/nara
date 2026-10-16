use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.InitialIncomplete",
    version = 1,
    alias = "Initial incomplete",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialIncomplete {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
