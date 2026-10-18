use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_identity::EntityReference;
use nara_reflect::{ComponentRegistry, PersistentComponent};

type EntityTargets = Vec<Option<EntityReference>>;
type AssetGroups = BTreeMap<String, Vec<Option<AssetRef>>>;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.NestedAliases",
    version = 1,
    alias = "Nested aliases",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct NestedAliases {
    #[nara(id = "targets", alias = "Targets", capabilities(scene, entity_ref))]
    targets: EntityTargets,
    #[nara(id = "assets", alias = "Assets", capabilities(scene, asset_ref))]
    assets: AssetGroups,
    #[nara(id = "counts", alias = "Counts")]
    counts: std::vec::Vec<std::option::Option<u16>>,
}

fn main() {
    let mut registry = ComponentRegistry::new();
    registry.register_persistent_component::<NestedAliases>().unwrap();
    registry.freeze().unwrap();
}
