use std::collections::HashMap;

use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.UnsupportedFieldType",
    version = 1,
    alias = "Unsupported field type",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct UnsupportedFieldType {
    #[schema(id = "values", alias = "Values")]
    values: HashMap<String, i64>,
}

fn main() {}
