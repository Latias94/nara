use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.TombstoneReactivation",
    version = 1,
    alias = "Tombstone reactivation",
    component_capabilities(scene),
    field_capabilities(scene),
    tombstone = "value"
)]
struct TombstoneReactivation {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
