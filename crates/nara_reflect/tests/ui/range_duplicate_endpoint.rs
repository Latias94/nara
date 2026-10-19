use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(id = "test.Range", version = 1, alias = "Range",
    component_capabilities(scene), field_capabilities(scene))]
struct Invalid {
    #[nara(id = "value", alias = "Value", range(min_inclusive = 0, min_exclusive = 1))]
    value: i32,
}
fn main() {}
