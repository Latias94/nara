use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.InitialExpression",
    version = 1,
    alias = "Initial expression",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialExpression {
    #[schema(id = "value", alias = "Value", initial = default_value())]
    value: i64,
}

fn default_value() -> i64 { 1 }
fn main() {}
