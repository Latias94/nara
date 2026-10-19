use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(id = "test.Range", version = 1, alias = "Range",
    component_capabilities(scene), field_capabilities(scene))]
struct Invalid {
    #[nara(id = "value", alias = "Value", range(max_inclusive = 5.0_f64))]
    value: f32,
}
fn main() {}
