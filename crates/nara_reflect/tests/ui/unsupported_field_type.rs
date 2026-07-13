use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.UnsupportedFieldType",
    version = 1,
    alias = "Unsupported field type",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct UnsupportedFieldType {
    #[nara(id = "values", alias = "Values")]
    values: Vec<i64>,
}

fn main() {}
