//! Authored content for Meadow's optional side room.

use nara::{
    asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId},
    prelude::{Component, PersistentComponent},
};

pub const SCENE_PATH: &str = "side-room.scene.json";
pub const IMAGE_PATH: &str = "textures/side-room.png";
pub const RELIC: &str = "side-room-relic";

#[must_use]
pub fn scene_product() -> AssetProductRef {
    product("4e544862-d52e-4f92-9a4a-c96aeaf6cb7e")
}

#[must_use]
pub fn image_product() -> AssetProductRef {
    product("7adc6278-30bd-4660-92ed-6012fbbe178b")
}

fn product(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).expect("valid side room Product identity"),
        ImportedProductId::PRIMARY,
    )
    .expect("non-nil side room Product identity")
}

#[derive(Debug, Clone, Component, PersistentComponent)]
#[nara(
    id = "two-level-game.SideRoomPortal",
    version = 1,
    alias = "Side room portal",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct SideRoomPortal {
    #[nara(
        id = "room",
        alias = "Side room Scene",
        asset_demand = deferred,
        capabilities(scene, inspect, edit, asset_ref)
    )]
    pub room: AssetRef,
}
