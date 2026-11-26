use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.AssetDemandDuplicate",
    version = 1,
    alias = "Asset demand duplicate",
    component_capabilities(scene),
    field_capabilities(scene, asset_ref)
)]
struct AssetDemandDuplicate {
    #[schema(id = "target", alias = "Target", asset_demand = immediate)]
    #[schema(asset_demand = deferred)]
    target: AssetRef,
}

fn main() {}
