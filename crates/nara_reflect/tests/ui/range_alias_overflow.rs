#![allow(overflowing_literals)]

use nara_ecs::Component;
use nara_reflect::PersistentComponent;

mod narrow_float_alias {
    #[allow(non_camel_case_types)]
    pub type f64 = std::primitive::f32;
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.RangeAliasOverflow",
    version = 1,
    alias = "Range alias overflow",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct RangeAliasOverflow {
    #[nara(id = "value", alias = "Value", range(max_inclusive = 1e100))]
    value: narrow_float_alias::f64,
}

fn main() {}
