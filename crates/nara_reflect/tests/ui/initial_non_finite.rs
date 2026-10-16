use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.InitialNonFinite",
    version = 1,
    alias = "Initial non-finite",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialNonFinite {
    #[nara(id = "value", alias = "Value", initial = 1e100)]
    value: f32,
}

fn main() {}
