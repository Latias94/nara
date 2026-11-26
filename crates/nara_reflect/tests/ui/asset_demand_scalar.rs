use nara_ecs::Component;
use nara_reflect::Schema;

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.AssetDemandScalar",
    version = 1,
    alias = "Asset demand scalar",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct AssetDemandScalar {
    #[schema(id = "value", alias = "Value", asset_demand = deferred)]
    value: i64,
}

fn main() {}
