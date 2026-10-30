use nara_ecs::Component;
use nara_reflect::PersistentComponent;

mod numeric_alias {
    pub type AssetRef = u32;
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.AssetDemandScalarAlias",
    version = 1,
    alias = "Asset demand scalar alias",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct AssetDemandScalarAlias {
    #[nara(id = "value", alias = "Value", asset_demand = immediate)]
    value: numeric_alias::AssetRef,
}

fn main() {}
