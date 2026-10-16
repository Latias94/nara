#![allow(overflowing_literals)]

use nara_ecs::Component;
use nara_reflect::{PersistentComponent, PersistentComponentProvider};

mod narrow_float_alias {
    #[allow(non_camel_case_types)]
    pub type f64 = std::primitive::f32;
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.InitialAliasOverflow",
    version = 1,
    alias = "Initial alias overflow",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialAliasOverflow {
    #[nara(id = "value", alias = "Value", initial = 1e100)]
    value: narrow_float_alias::f64,
}

fn main() {
    InitialAliasOverflow::persistent_component_schema();
}
