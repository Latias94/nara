use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.AssetDemandString",
    version = 1,
    alias = "Asset demand string",
    component_capabilities(scene),
    field_capabilities(scene, asset_ref)
)]
struct AssetDemandString {
    #[nara(id = "target", alias = "Target", asset_demand = "deferred")]
    target: AssetRef,
}

fn main() {}
