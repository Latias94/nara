#![allow(overflowing_literals)]

use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.InitialNonFiniteWithoutSchema",
    version = 1,
    alias = "Initial non-finite without schema",
    component_capabilities(scene),
    field_capabilities(scene),
    authoring_initial
)]
struct InitialNonFiniteWithoutSchema {
    #[nara(id = "value", alias = "Value", initial = 1e100)]
    value: f32,
}

fn main() {}
