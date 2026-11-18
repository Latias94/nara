//! Authored marker content shared by level declarations and gameplay validation.

use nara::{
    asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId},
    identity::EntityReference,
    prelude::{Component, PersistentComponent},
};

mod definition;

pub(crate) use definition::MarkerDefinition;

/// Scene alias relative to the project's effective Scene directory.
pub const SCENE_PATH: &str = "marker.scene.json";
/// Prefab alias relative to the project's effective Prefab directory.
pub const PREFAB_PATH: &str = "marker.prefab.json";

/// The exact primary Scene Product shared by both level cameras.
#[must_use]
pub fn marker_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("36654481-1e16-5b3f-844e-3f18380591f7")
            .expect("fixed valid marker Scene source identity"),
        ImportedProductId::PRIMARY,
    )
    .expect("fixed non-nil primary marker Scene Product")
}

/// Immediate authored marker dependency declared by the level camera.
#[derive(Debug, Clone, Component, PersistentComponent)]
#[nara(
    id = "two-level-game.MarkerPalette",
    version = 1,
    alias = "Marker palette",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct MarkerPalette {
    #[nara(
        id = "template",
        alias = "Template Scene",
        asset_demand = immediate,
        capabilities(scene, inspect, edit, asset_ref)
    )]
    pub template: AssetRef,
}

/// An internal reference resolved independently for each authored marker occurrence.
#[derive(Debug, Clone, PartialEq, Component, PersistentComponent)]
#[nara(
    id = "two-level-game.MarkerLink",
    version = 1,
    alias = "Marker link",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct MarkerLink {
    #[nara(
        id = "target",
        alias = "Target",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    pub target: EntityReference,
}
