use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.InitialWithoutOptIn",
    version = 1,
    alias = "Initial without opt-in",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct InitialWithoutOptIn {
    #[nara(id = "value", alias = "Value", initial = 1)]
    value: i64,
}

fn main() {}
