use nara_ecs::Component;
use nara_reflect::Schema;

mod numeric_alias {
    pub type AssetRef = u32;
}

#[derive(Component, Schema)]
#[schema(
    id = "nara.test.AssetDemandScalarAlias",
    version = 1,
    alias = "Asset demand scalar alias",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct AssetDemandScalarAlias {
    #[schema(id = "value", alias = "Value", asset_demand = immediate)]
    value: numeric_alias::AssetRef,
}

fn main() {}
