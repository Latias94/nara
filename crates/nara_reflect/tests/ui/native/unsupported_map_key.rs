use std::collections::BTreeMap;

use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.UnsupportedMapKey",
    version = 1,
    alias = "Unsupported map key",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct UnsupportedMapKey {
    #[schema(id = "values", alias = "Values")]
    values: BTreeMap<u32, i64>,
}

fn main() {}
