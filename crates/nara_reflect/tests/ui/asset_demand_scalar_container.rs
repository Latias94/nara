use std::collections::BTreeMap;

use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.AssetDemandScalarContainer",
    version = 1,
    alias = "Asset demand scalar container",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct AssetDemandScalarContainer {
    #[schema(id = "values", alias = "Values", asset_demand = deferred)]
    values: Option<Vec<u32>>,
    #[schema(id = "labels", alias = "Labels", asset_demand = immediate)]
    labels: BTreeMap<String, Option<String>>,
}

fn main() {}
