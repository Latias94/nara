use std::{
    collections::{BTreeMap, btree_map::Entry},
    error::Error,
    path::Path,
};

use nara::{
    prelude::*,
    render::material::SpriteMaterialAsset,
    scene::{
        components::SpriteSource,
        cooked::{CookedRegistry, CookedScene},
    },
};
use nara_asset::cooked::{NpakArchive, NpakEntryKind};

use crate::{ENTRY_SCENE_ID, ExtractionGate, GamePlugin, PatrolDrone, Pilot, PowerCell, RunRules};

use super::world::SceneVisuals;

pub(super) fn install(app: &mut App, path: &Path) -> Result<(), Box<dyn Error>> {
    let archive = NpakArchive::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    let bytes = archive.read_kind(ENTRY_SCENE_ID.parse()?, NpakEntryKind::Scene)?;
    let scene = CookedScene::parse(&bytes)?;
    for &id in scene.asset_dependencies() {
        if archive.entry(id).is_none() {
            return Err(format!("Night Relay package is missing required asset {id}").into());
        }
    }

    let mut registry = CookedRegistry::new();
    registry
        .register_builtins()?
        .register::<RunRules>()?
        .register::<Pilot>()?
        .register::<PowerCell>()?
        .register::<PatrolDrone>()?
        .register::<ExtractionGate>()?;

    let world = app.world_mut()?;
    let entities = scene
        .instantiate(world, &registry)?
        .into_values()
        .collect::<Vec<_>>();
    let mut images = BTreeMap::new();
    let mut materials = BTreeMap::new();
    for source in entities
        .iter()
        .filter_map(|&entity| world.get::<SpriteSource>(entity))
    {
        if let Some(id) = source.image
            && let Entry::Vacant(entry) = images.entry(id)
        {
            let image = archive.read_kind(id, NpakEntryKind::Image)?;
            entry.insert(ImageAsset::from_cooked(&image)?);
        }
        if let Some(id) = source.material
            && let Entry::Vacant(entry) = materials.entry(id)
        {
            let bytes = archive.read_kind(id, NpakEntryKind::Material)?;
            let material = SpriteMaterialAsset::from_cooked(&bytes)?;
            let shader = archive.read_kind(material.shader(), NpakEntryKind::Shader)?;
            entry.insert(material.bind(std::str::from_utf8(&shader)?)?);
        }
    }
    SceneVisuals::install(world, images, materials)?;
    app.add_plugins(GamePlugin::from_cooked_entities(entities))?;
    Ok(())
}
