use nara_ecs::Component;
use nara_reflect::PersistentComponent;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.AssetDemandScalar",
    version = 1,
    alias = "Asset demand scalar",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct AssetDemandScalar {
    #[nara(id = "value", alias = "Value", asset_demand = deferred)]
    value: i64,
}

fn main() {}
