use nara::{
    app::{
        App, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    },
    asset::{AssetServer, AssetStates, AssetVersion, Assets, Handle},
    ecs::{Resource, World, schedule::IntoScheduleConfigs},
    image::{ImageAsset, image_resource_key},
    material::{Material2dAsset, Material2dBinding},
    render::RenderFrame,
    render_wgpu::{
        WGPU_STOCK_POLICY_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID, WgpuPolicyCaptureSet,
        WgpuRenderInput, WgpuStockPolicy, take_captured_policy_batch,
    },
    sprite_render::__private::SpriteBatches,
    window::{
        Window, WindowResolution,
        backend::{BackendWindowHandles, WindowPresentationRequirement},
    },
};
use two_level_game::{GameSnapshot, region_material, side_room};

use crate::{gpu::EXTENT, support::Game};

#[derive(Default, Resource)]
struct Latest(Option<WgpuRenderInput>);

#[derive(Default)]
pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test.region-material-pixels"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[WGPU_STOCK_POLICY_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.spawn(Window::new(
            "Region Material CPU packet capture",
            WindowResolution::new(EXTENT.width, EXTENT.height),
        ));
        app.init_resource::<Latest>()?;
        app.add_systems(
            CoreStage::Render,
            retain.in_set(WgpuPolicyCaptureSet::Publish),
        )?;
        Ok(())
    }
}

fn retain(world: &mut World) {
    let Some(batch) = take_captured_policy_batch(world) else {
        return;
    };
    world.resource_mut::<Latest>().0 = batch
        .into_render_input()
        .expect("the original ordinary stock Seal");
}

pub struct Capture {
    pub input: WgpuRenderInput,
    pub publication: Option<(Handle<Material2dAsset>, AssetVersion)>,
    pub binding: Option<Material2dBinding>,
    pub image: Option<(Handle<ImageAsset>, AssetVersion)>,
}

pub fn sample(game: &mut Game, expected_sprites: usize) -> Capture {
    game.maintenance();
    game.session
        .observe(|world| {
            assert!(!world.contains_resource::<BackendWindowHandles>());
            assert!(!world.contains_resource::<WindowPresentationRequirement>());
            assert_eq!(
                world.resource::<GameSnapshot>().tick,
                world.resource::<FixedTime>().tick()
            );
            let input = world
                .resource::<Latest>()
                .0
                .clone()
                .expect("the original game packet");
            assert_eq!(
                input.packet().frame_index(),
                world.resource::<RenderFrame>().index
            );
            assert_eq!(
                input
                    .packet()
                    .source_generation()
                    .runtime_generation()
                    .map(|generation| generation.get()),
                game.session.runtime_generation(),
            );
            let stock = input
                .captured::<WgpuStockPolicy>()
                .expect("the original stock policy");
            let batches = stock.sprite_batches().expect("the original Sprite batches");
            assert_eq!(batches, world.resource::<SpriteBatches>());
            let server = world.resource::<AssetServer>();
            let states = world.resource::<AssetStates>();
            let materials = world.resource::<Assets<Material2dAsset>>();
            let images = world.resource::<Assets<ImageAsset>>();
            let resident = server
                .get_product_handle::<Material2dAsset>(region_material::material_product())
                .and_then(|handle| materials.get(handle).map(|asset| (handle, asset)));
            let publication = resident.map(|(handle, asset)| {
                assert_eq!(asset.product(), region_material::material_product());
                assert_eq!(
                    asset.generation().declaration(),
                    &region_material::declaration()
                );
                assert_eq!(
                    states.published_slot_revision(handle.id()),
                    Some(&materials.slot_revision(handle)),
                );
                let version = states.state(handle.id()).unwrap().usable_version().unwrap();
                assert!(version.raw() > 0);
                (handle, version)
            });
            let image_handle = server.get_product_handle::<ImageAsset>(side_room::image_product());
            let image = image_handle.and_then(|handle| {
                images.get(handle).map(|asset| {
                    assert_eq!(asset.product(), Some(side_room::image_product()));
                    (
                        handle,
                        states.state(handle.id()).unwrap().usable_version().unwrap(),
                    )
                })
            });
            let key = image_handle.map(image_resource_key);
            let mut binding = None;
            let mut sprites = 0;
            for batch in batches.as_slice() {
                let Some(base) = batch
                    .image
                    .as_ref()
                    .filter(|base| Some(base.identity().key()) == key)
                else {
                    continue;
                };
                let (handle, asset) =
                    resident.expect("a genuine resident Material for every relic draw");
                assert_eq!(
                    base.resource().pixels(),
                    region_material::SOURCE_PIXEL.repeat(4)
                );
                assert_eq!(
                    Some(base.identity().asset_version()),
                    image.map(|(_, version)| version)
                );
                let extension = batch
                    .material_extension
                    .as_ref()
                    .expect("the original relic Material binding");
                assert_eq!(extension.binding.generation(), asset.generation());
                assert_eq!(
                    extension.binding.parameters(),
                    &region_material::parameters()
                );
                assert_eq!(Some(handle), publication.map(|(handle, _)| handle));
                assert_eq!(extension.textures.len(), 1);
                assert_eq!(extension.textures[0].0, 1);
                let palette = server
                    .get_product_handle::<ImageAsset>(region_material::palette_product())
                    .unwrap();
                assert_eq!(
                    extension.binding.textures()[region_material::PALETTE_ID],
                    palette
                );
                assert_eq!(
                    extension.textures[0].1.identity().key(),
                    image_resource_key(palette)
                );
                assert_eq!(
                    &extension.textures[0].1.resource().pixels()[..4],
                    &region_material::PALETTE_PIXEL
                );
                if let Some(previous) = &binding {
                    assert_eq!(previous, &extension.binding);
                } else {
                    binding = Some(extension.binding.clone());
                }
                sprites += batch
                    .instances
                    .iter()
                    .filter(|sprite| sprite.color.a > 0.0)
                    .count();
            }
            assert_eq!(sprites, expected_sprites);
            if expected_sprites == 0 {
                assert!(publication.is_none() && image.is_none());
                assert!(batches.as_slice().iter().all(|batch| {
                    batch.material_extension.as_ref().is_none_or(|extension| {
                        extension.binding.generation().declaration().id()
                            != region_material::MATERIAL_ID
                    })
                }));
            }
            Capture {
                input,
                publication,
                binding,
                image,
            }
        })
        .unwrap()
}

pub fn assert_released(game: &Game, material: Handle<Material2dAsset>, image: Handle<ImageAsset>) {
    game.session
        .observe(|world| {
            assert!(
                world
                    .resource::<Assets<Material2dAsset>>()
                    .get(material)
                    .is_none()
            );
            assert!(world.resource::<Assets<ImageAsset>>().get(image).is_none());
            for id in [material.id(), image.id()] {
                assert!(
                    world
                        .resource::<AssetStates>()
                        .state(id)
                        .unwrap()
                        .usable_version()
                        .is_none()
                );
            }
        })
        .unwrap();
}
