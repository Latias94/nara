#![allow(overflowing_literals)]

use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.RangeIntegerFloatSuffix",
    version = 1,
    alias = "Range integer float suffix",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct RangeIntegerFloatSuffix {
    #[nara(id = "value", alias = "Value", range(max_inclusive = 1000000000000000000000000000000000000000f32))]
    value: f32,
}

fn main() {}
