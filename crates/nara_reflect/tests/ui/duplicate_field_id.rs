use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.DuplicateFieldId",
    version = 1,
    alias = "Duplicate field ID",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct DuplicateFieldId {
    #[nara(id = "value", alias = "First")]
    first: i64,
    #[nara(id = "value", alias = "Second")]
    second: i64,
}

fn main() {}
