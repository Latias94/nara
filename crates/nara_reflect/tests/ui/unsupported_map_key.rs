use std::collections::BTreeMap;

use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.UnsupportedMapKey",
    version = 1,
    alias = "Unsupported map key",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct UnsupportedMapKey {
    #[nara(id = "values", alias = "Values")]
    values: BTreeMap<u32, i64>,
}

fn main() {}
