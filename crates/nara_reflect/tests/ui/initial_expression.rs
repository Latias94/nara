use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.InitialExpression",
    version = 1,
    alias = "Initial expression",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialExpression {
    #[nara(id = "value", alias = "Value", initial = default_value())]
    value: i64,
}

fn default_value() -> i64 { 1 }
fn main() {}
