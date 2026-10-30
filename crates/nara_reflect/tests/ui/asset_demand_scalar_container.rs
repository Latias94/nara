use std::collections::BTreeMap;

use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.AssetDemandScalarContainer",
    version = 1,
    alias = "Asset demand scalar container",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct AssetDemandScalarContainer {
    #[nara(id = "values", alias = "Values", asset_demand = deferred)]
    values: Option<Vec<u32>>,
    #[nara(id = "labels", alias = "Labels", asset_demand = immediate)]
    labels: BTreeMap<String, Option<String>>,
}

fn main() {}
