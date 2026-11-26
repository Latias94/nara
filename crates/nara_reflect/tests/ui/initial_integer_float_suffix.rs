#![allow(overflowing_literals)]

use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.InitialIntegerFloatSuffix",
    version = 1,
    alias = "Initial integer float suffix",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialIntegerFloatSuffix {
    #[schema(id = "value", alias = "Value", initial = 1000000000000000000000000000000000000000f32)]
    value: f32,
}

fn main() {
    let _ = InitialIntegerFloatSuffix::type_schema();
}
