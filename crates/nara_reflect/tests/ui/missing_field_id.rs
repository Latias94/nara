use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.MissingFieldId",
    version = 1,
    alias = "Missing field ID",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct MissingFieldId {
    #[nara(alias = "Value")]
    value: i64,
}

fn main() {}
