use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    crate = "nara_reflect",
    id = "nara.test.CrateOverride",
    version = 1,
    alias = "Crate override",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct CrateOverride {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
