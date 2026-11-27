#[cfg(feature = "source")]
mod source;

use std::{
    collections::{HashMap, hash_map::Entry},
    error::Error,
};

use game_components::Spin;
use nara::{
    asset::{add_asset, image::ImageAsset},
    core::AssetId,
    prelude::{
        App, Camera2d, Color, DefaultPlugins, Entity, Query, Res, Sprite, Transform2d, Update,
        VirtualTime, World,
    },
    scene::components::SpriteSource,
};

const ENTRY_SCENE: &str = "{entry_scene}";

type GameResult<T> = Result<T, Box<dyn Error>>;

fn main() -> GameResult<()> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new(env!("CARGO_PKG_NAME")))?;
    let world = app.world_mut()?;
    world.spawn((
        Camera2d {
            clear_color: Some(Color::rgb(0.04, 0.06, 0.1)),
            ..Camera2d::default()
        },
        Transform2d::default(),
    ));

    let entry = ENTRY_SCENE.parse()?;
    #[cfg(feature = "source")]
    source::load_scene(world, entry)?;
    #[cfg(not(feature = "source"))]
    load_cooked_scene(world, entry)?;

    app.add_systems(Update, spin)?;
    app.run()?;
    Ok(())
}

#[cfg(not(feature = "source"))]
fn load_cooked_scene(world: &mut World, entry: AssetId) -> GameResult<()> {
    use nara::{
        asset::cooked::{NpakArchive, NpakEntryKind},
        scene::cooked::{CookedRegistry, CookedScene},
    };

    let path = std::env::current_exe()?.with_file_name("game.npak");
    let archive = NpakArchive::open(&path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    let bytes = archive.read_kind(entry, NpakEntryKind::Scene)?;
    let scene = CookedScene::parse(&bytes)?;
    let mut registry = CookedRegistry::new();
    registry.register_builtins()?.register::<Spin>()?;
    let entities = scene.instantiate(world, &registry)?;
    bind_sprites(world, entities.into_values(), |id| {
        let bytes = archive.read_kind(id, NpakEntryKind::Image)?;
        ImageAsset::from_cooked(&bytes)
            .map_err(|error| format!("could not decode image asset {id}: {error}").into())
    })
}

fn bind_sprites(
    world: &mut World,
    entities: impl IntoIterator<Item = Entity>,
    mut load_image: impl FnMut(AssetId) -> GameResult<ImageAsset>,
) -> GameResult<()> {
    let mut images = HashMap::new();
    for entity in entities {
        let Some(source) = world.get::<SpriteSource>(entity).cloned() else {
            continue;
        };
        if let Some(material) = source.material {
            return Err(format!("starter sprite uses unsupported material {material}").into());
        }
        let sprite = match source.image {
            Some(id) => {
                let handle = match images.entry(id) {
                    Entry::Occupied(slot) => *slot.get(),
                    Entry::Vacant(slot) => *slot.insert(add_asset(world, load_image(id)?)?),
                };
                Sprite::from_texture(handle, source.size).with_tint(source.tint())
            }
            None => Sprite::from_color(source.size, source.tint()),
        };
        world
            .entity_mut(entity)
            .insert_if_new(Transform2d::default())
            .insert(sprite.with_layer(source.layer));
    }
    Ok(())
}

fn spin(time: Res<VirtualTime>, mut sprites: Query<(&mut Transform2d, &Spin)>) {
    for (mut transform, spin) in &mut sprites {
        transform.rotation += spin.radians_per_second * time.delta_seconds();
    }
}
