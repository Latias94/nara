use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.InitialWithoutOptIn",
    version = 1,
    alias = "Initial without opt-in",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct InitialWithoutOptIn {
    #[schema(id = "value", alias = "Value", initial = 1)]
    value: i64,
}

fn main() {}
