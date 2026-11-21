use nara_asset::{AssetEvents, AssetServer, AssetStates, Assets, ProjectAssetDatabase};
use nara_ecs::{Mut, Res, ResMut, World, bevy_ecs::system::SystemState};
use nara_image::ImageAsset;
use nara_material::{Material2dAsset, Material2dDomain};
use nara_scene::advanced::SceneAssetReservations;
use nara_tilemap::{
    PreparedTileSetPublications, TileSet, TileSetAssetError, TileSetAssets, TileSetContentReceipt,
    TileSetPublicationInput,
};

use super::{
    HostFault, PreparedCatalog, ProjectSceneResidency, ResidencyInput,
    images::{ImageChange, ImagePublications, OwnedImage},
    materials::{MaterialPublications, MaterialResidency, OwnedMaterial},
    resources_missing,
    tilesets::{TileSetChanges, tileset_failure},
};

#[derive(Default)]
pub(super) struct StockResidency {
    pub(super) images: Vec<OwnedImage>,
    pub(super) tilesets: Vec<TileSetContentReceipt>,
    pub(super) materials: MaterialResidency,
}

impl ProjectSceneResidency {
    pub(super) fn owned_images(&self) -> impl Iterator<Item = &OwnedImage> + Clone {
        self.stock.images.iter().chain(&self.additions.stock.images)
    }

    pub(super) fn image_count(&self) -> usize {
        self.stock.images.len() + self.additions.stock.images.len()
    }

    pub(super) fn owned_materials(&self) -> impl Iterator<Item = &OwnedMaterial> + Clone {
        self.stock
            .materials
            .iter()
            .chain(self.additions.stock.materials.iter())
    }

    pub(super) fn material_count(&self) -> usize {
        self.owned_materials().count()
    }

    pub(super) fn owned_tilesets(&self) -> impl Iterator<Item = &TileSetContentReceipt> + Clone {
        self.stock
            .tilesets
            .iter()
            .chain(&self.additions.stock.tilesets)
    }

    pub(super) fn tileset_count(&self) -> usize {
        self.stock.tilesets.len() + self.additions.stock.tilesets.len()
    }
}

impl ResidencyInput<'_> {
    pub(super) fn material_count(self) -> usize {
        match self {
            Self::Source(snapshot) => snapshot
                .domain_assets()
                .collection::<Material2dDomain>()
                .map_or(0, |collection| collection.iter().len()),
            Self::Processed(content) => content.materials.len(),
        }
    }

    pub(super) fn image_count(self) -> usize {
        match self {
            Self::Source(snapshot) => snapshot.images().len(),
            Self::Processed(content) => content.images.len(),
        }
    }

    pub(super) fn tileset_count(self) -> usize {
        match self {
            Self::Source(snapshot) => snapshot.tilesets().len(),
            Self::Processed(content) => content.tilesets.len(),
        }
    }
}

pub(super) struct PreparedStock {
    pub(super) images: ImagePublications,
    pub(super) materials: MaterialPublications,
    pub(super) tilesets: TileSetChanges,
}

impl PreparedStock {
    pub(super) fn validate_replacement(&self, world: &World) -> Result<(), HostFault> {
        for baseline in &self.tilesets.slots {
            baseline
                .validate(
                    world.resource::<Assets<TileSet>>(),
                    world.resource::<AssetStates>(),
                )
                .map_err(|_| tileset_failure(TileSetAssetError::Superseded))?;
        }
        validate_images(world, &self.images)?;
        self.materials.validate(world)
    }
}

pub(super) fn prepare_images(
    world: &World,
    content: ResidencyInput<'_>,
    change: ImageChange<'_>,
    bindings: &mut SceneAssetReservations,
    catalog: &PreparedCatalog,
) -> Result<ImagePublications, HostFault> {
    let retire_or_replace =
        matches!(change, ImageChange::Replace(Some(current)) if current.image_count() != 0);
    if !retire_or_replace && content.image_count() == 0 {
        return Ok(ImagePublications::empty());
    }
    if !world.contains_resource::<AssetEvents>() {
        return Err(resources_missing());
    }
    ImagePublications::prepare(
        content,
        change,
        bindings,
        world
            .get_resource::<AssetServer>()
            .ok_or_else(resources_missing)?,
        world
            .get_resource::<Assets<ImageAsset>>()
            .ok_or_else(resources_missing)?,
        world
            .get_resource::<AssetStates>()
            .ok_or_else(resources_missing)?,
        catalog.runtime_lease(),
    )
}

pub(super) fn prepare_tilesets<'owner>(
    world: &World,
    owner: &'owner mut TileSetAssets,
    inputs: Vec<TileSetPublicationInput>,
    retirements: &[TileSetContentReceipt],
    server: &AssetServer,
    database: Option<&ProjectAssetDatabase>,
) -> Result<PreparedTileSetPublications<'owner>, HostFault> {
    owner
        .prepare_publications(
            inputs,
            retirements,
            server,
            database,
            world.resource::<Assets<TileSet>>(),
            world.resource::<AssetStates>(),
        )
        .map_err(tileset_failure)
}

pub(super) fn probe_tilesets(
    world: &mut World,
    changes: &TileSetChanges,
    server: &AssetServer,
    catalog: &PreparedCatalog,
) -> Result<(), HostFault> {
    if !changes.has_publications() {
        return Ok(());
    }
    let inputs = changes.share_inputs()?;
    world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
        prepare_tilesets(
            world,
            &mut owner,
            inputs,
            &changes.retirements,
            server,
            catalog
                .asset_resolution()
                .map(|resolver| resolver.database()),
        )
        .map(|_| ())
    })
}

pub(super) fn validate_resources(
    world: &World,
    content: ResidencyInput<'_>,
    current: Option<&ProjectSceneResidency>,
) -> Result<(), HostFault> {
    let uses_tilesets =
        content.tileset_count() != 0 || current.is_some_and(|current| current.tileset_count() != 0);
    let uses_images =
        content.image_count() != 0 || current.is_some_and(|current| current.image_count() != 0);
    let uses_materials = content.material_count() != 0
        || current.is_some_and(|current| current.material_count() != 0);
    if uses_tilesets {
        validate_tileset_resources(world)?;
    } else if uses_images {
        validate_image_resources(world)?;
    }
    if uses_materials
        && (!world.contains_resource::<AssetServer>()
            || !world.contains_resource::<Assets<Material2dAsset>>()
            || !world.contains_resource::<AssetStates>()
            || !world.contains_resource::<AssetEvents>())
    {
        return Err(resources_missing());
    }
    Ok(())
}

pub(super) fn validate_tileset_resources(world: &World) -> Result<(), HostFault> {
    validate_image_resources(world)?;
    if !world.contains_resource::<TileSetAssets>() || !world.contains_resource::<Assets<TileSet>>()
    {
        return Err(resources_missing());
    }
    Ok(())
}

fn validate_image_resources(world: &World) -> Result<(), HostFault> {
    if !world.contains_resource::<AssetServer>()
        || !world.contains_resource::<Assets<ImageAsset>>()
        || !world.contains_resource::<AssetStates>()
        || !world.contains_resource::<AssetEvents>()
    {
        return Err(resources_missing());
    }
    Ok(())
}

pub(super) fn validate_images(world: &World, images: &ImagePublications) -> Result<(), HostFault> {
    if images.is_empty() {
        return Ok(());
    }
    validate_image_resources(world)?;
    images.validate(
        world.resource::<AssetServer>(),
        world.resource::<Assets<ImageAsset>>(),
        world.resource::<AssetStates>(),
    )
}

pub(super) fn commit_publications(
    world: &mut World,
    images: ImagePublications,
    tilesets: Option<PreparedTileSetPublications<'_>>,
    materials: MaterialPublications,
) -> StockResidency {
    let images = commit_images(world, images);
    let tilesets = tilesets.map_or_else(Vec::new, |prepared| {
        let mut access = SystemState::<(
            Res<AssetServer>,
            Res<Assets<ImageAsset>>,
            ResMut<Assets<TileSet>>,
            ResMut<AssetStates>,
            ResMut<AssetEvents>,
        )>::new(world);
        let (server, images, mut values, mut states, mut events) = access
            .get_mut(world)
            .expect("TileSet residency resources passed preflight");
        prepared.commit(&server, &images, &mut values, &mut states, &mut events)
    });
    let materials = commit_materials(world, materials);
    StockResidency {
        images,
        tilesets,
        materials,
    }
}

pub(super) fn commit_materials(
    world: &mut World,
    materials: MaterialPublications,
) -> MaterialResidency {
    if materials.is_empty() {
        MaterialResidency::default()
    } else {
        let mut access = SystemState::<(
            Res<AssetServer>,
            ResMut<Assets<Material2dAsset>>,
            ResMut<AssetStates>,
            ResMut<AssetEvents>,
        )>::new(world);
        let (server, mut values, mut states, mut events) = access
            .get_mut(world)
            .expect("Material residency resources passed preflight");
        materials.commit(&server, &mut values, &mut states, &mut events)
    }
}

fn commit_images(world: &mut World, images: ImagePublications) -> Vec<OwnedImage> {
    if images.is_empty() {
        return Vec::new();
    }
    let mut access = SystemState::<(
        Res<AssetServer>,
        ResMut<Assets<ImageAsset>>,
        ResMut<AssetStates>,
        ResMut<AssetEvents>,
    )>::new(world);
    let (server, mut values, mut states, mut events) = access
        .get_mut(world)
        .expect("Scene residency resources passed preflight");
    images.commit(&server, &mut values, &mut states, &mut events)
}
