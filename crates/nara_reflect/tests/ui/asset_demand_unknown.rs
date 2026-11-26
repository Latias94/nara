use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.AssetDemandUnknown",
    version = 1,
    alias = "Asset demand unknown",
    component_capabilities(scene),
    field_capabilities(scene, asset_ref)
)]
struct AssetDemandUnknown {
    #[schema(id = "target", alias = "Target", asset_demand = eager)]
    target: AssetRef,
}

fn main() {}
