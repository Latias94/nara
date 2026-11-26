#![allow(overflowing_literals)]

use nara_ecs::Component;
use nara_reflect::Schema;

mod narrow_float_alias {
    #[allow(non_camel_case_types)]
    pub type f64 = std::primitive::f32;
}

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.InitialAliasOverflow",
    version = 1,
    alias = "Initial alias overflow",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialAliasOverflow {
    #[schema(id = "value", alias = "Value", initial = 1e100)]
    value: narrow_float_alias::f64,
}

fn main() {
    let _ = InitialAliasOverflow::type_schema();
}
