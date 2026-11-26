use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.TombstoneReactivation",
    version = 1,
    alias = "Tombstone reactivation",
    component_capabilities(scene),
    field_capabilities(scene),
    tombstone = "value"
)]
struct TombstoneReactivation {
    #[schema(id = "value", alias = "Value")]
    value: i64,
}

fn main() {}
