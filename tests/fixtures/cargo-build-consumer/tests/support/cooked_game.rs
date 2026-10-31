//! One ordinary game library compiled by both the actual cooker and the source-free Player.

#![forbid(unsafe_code)]

use engine::{
    ProductRecipe, SchemaContribution,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId},
    ecs::Component,
    image::ImageRuntimeCodec,
    input::{ActionMapDomain, ActionMapRuntimeCodec},
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    runtime_content::RuntimeCodecDefinitionId,
    scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord},
    tilemap::TileSetRuntimeCodec,
};
use nara_asset_domain_consumer::TextBundleDomain;

pub mod probe;

pub const IMAGE: u8 = 20;
pub const TILESET: u8 = 21;
pub const BUNDLE: u8 = 22;
pub const INPUT: u8 = 23;

pub fn product(index: u8) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("9dc10000-0000-4000-8000-{index:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

pub fn caption() -> AssetProductRef {
    AssetProductRef::new(
        product(BUNDLE).source(),
        ImportedProductId::parse_subproduct("9dc10000-0000-4000-8000-000000000100").unwrap(),
    )
    .unwrap()
}

pub fn marker(scene: u8) -> SceneEntityId {
    SceneEntityId::new(format!("level-{scene}")).unwrap()
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.cooked-player.Links",
    version = 1,
    alias = "Delivery Links",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit, asset_ref)
)]
pub struct Links {
    #[nara(id = "image", alias = "Shared Image")]
    pub image: AssetRef,
    #[nara(id = "tileset", alias = "Shared TileSet")]
    pub tileset: AssetRef,
    #[nara(id = "captions", alias = "Compound Products")]
    pub captions: Vec<AssetRef>,
    #[nara(id = "destination", alias = "Deferred Destination", asset_demand = deferred)]
    pub destination: Option<AssetRef>,
}

engine::persistent_components! { mod components = [Links]; }

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.cooked-player.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.cooked-player"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.cooked-player.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.cooked-player"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[
            engine::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
            engine::input::INPUT_PLUGIN_ID,
            engine::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
        ])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        probe::install(app)
    }
}

pub fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<GamePlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
        .add_runtime_asset_codec::<ActionMapDomain, ActionMapRuntimeCodec>(
            RuntimeCodecDefinitionId::new("consumer.cooked-player.input", 1),
        )
        .unwrap()
        .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "consumer.cooked-player.image",
            1,
        ))
        .unwrap()
        .add_runtime_tileset_codec::<TileSetRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "consumer.cooked-player.tileset",
            1,
        ))
        .unwrap()
        .add_runtime_asset_codec::<TextBundleDomain, TextBundleDomain>(
            RuntimeCodecDefinitionId::new("consumer.cooked-player.caption", 1),
        )
        .unwrap()
}

pub fn document(scene: u8) -> SceneDocument {
    let links = Links {
        image: AssetRef::Product(product(IMAGE)),
        tileset: AssetRef::Product(product(TILESET)),
        captions: vec![
            AssetRef::Product(product(BUNDLE)),
            AssetRef::Product(caption()),
        ],
        destination: (scene != 3).then(|| AssetRef::Product(product(3))),
    };
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(marker(scene)).with_component(
        ComponentTypeId::new("consumer.cooked-player.Links"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            links.__encode_persistent_component().unwrap(),
        ),
    )])
}

pub fn cooked_pixels() -> Vec<u8> {
    // FitNearest samples the centers of each two-pixel source interval: x=1,3,5,7.
    [1u8, 3, 5, 7]
        .into_iter()
        .flat_map(|x| [x, x * 17, 255 - x, 255])
        .collect()
}
