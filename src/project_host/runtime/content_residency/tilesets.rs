use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetId, AssetStates, Assets, Handle};
#[cfg(feature = "project-source")]
use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, SourceHash};
use nara_ecs::World;
use nara_render::image::ImageAsset;
#[cfg(feature = "project-source")]
use nara_render::tilemap::TileSetAssets;
use nara_render::tilemap::{
    TileSet, TileSetAssetError, TileSetContentReceipt, TileSetPublicationInput,
};
use nara_scene::advanced::SceneAssetReservations;

use super::{
    HostFault, ProjectSceneResidency, ResidencyInput, binding_changed, failure,
    images::ImagePublications, resources_missing, stock::StockChange,
};

#[derive(Default)]
pub(super) struct TileSetChanges {
    pub(super) inputs: Vec<TileSetPublicationInput>,
    pub(super) retirements: Vec<TileSetContentReceipt>,
    pub(super) uses: Vec<Handle<TileSet>>,
    pub(super) new_handles: BTreeSet<AssetId>,
}

impl TileSetChanges {
    pub(super) fn has_publications(&self) -> bool {
        !self.inputs.is_empty() || !self.retirements.is_empty()
    }
}

pub(super) fn prepare(
    world: &World,
    content: ResidencyInput<'_>,
    change: StockChange<'_>,
    bindings: &mut SceneAssetReservations,
    images: &ImagePublications,
) -> Result<TileSetChanges, HostFault> {
    let owned = change
        .current()
        .into_iter()
        .flat_map(ProjectSceneResidency::owned_tilesets);
    let additive = change.is_additive();
    if content.tileset_count() == 0 && (additive || owned.clone().next().is_none()) {
        return Ok(TileSetChanges::default());
    }
    let values = world.resource::<Assets<TileSet>>();
    let states = world.resource::<AssetStates>();
    let validate_slot = |handle| {
        values
            .prepare_mutation(handle, states)
            .map(|_| ())
            .map_err(|_| tileset_failure(TileSetAssetError::Superseded))
    };
    let old = owned
        .clone()
        .map(|receipt| (receipt.handle().id(), receipt))
        .collect::<BTreeMap<_, _>>();
    let mut changes = TileSetChanges::default();
    let mut next_ids = BTreeSet::new();
    let mut insert = |handle: Handle<TileSet>, mut input: TileSetPublicationInput| {
        if let Some(receipt) = old.get(&handle.id()) {
            input = if additive {
                input.with_shared_receipt((*receipt).clone())
            } else {
                input.with_receipt((*receipt).clone())
            };
        } else if additive {
            changes.new_handles.insert(handle.id());
        }
        if !next_ids.insert(handle.id()) {
            return Err(binding_changed());
        }
        validate_slot(handle)?;
        if additive {
            changes.uses.push(handle);
        }
        changes.inputs.push(input);
        Ok(())
    };
    match content {
        #[cfg(feature = "project-source")]
        ResidencyInput::Source(snapshot) => {
            for content in snapshot.tilesets() {
                let handle = bindings
                    .reserve::<TileSet>(
                        &AssetRef::StableId(content.stable_id()),
                        snapshot.asset_resolution(),
                        &tileset_kind(),
                    )
                    .map_err(|_| binding_changed())?;
                let record = snapshot
                    .asset_resolution()
                    .resolve_ref(&content.source().image)
                    .map_err(|_| resources_missing())?;
                let product = match &content.source().image {
                    AssetRef::Product(product) => *product,
                    AssetRef::Path(_) | AssetRef::StableId(_) => nara_asset::AssetProductRef::new(
                        record.stable_id(),
                        nara_asset::ImportedProductId::PRIMARY,
                    )
                    .map_err(|_| resources_missing())?,
                };
                let (image_handle, image) = images
                    .retained_image_for_product(product, world.resource::<Assets<ImageAsset>>())
                    .ok_or_else(resources_missing)?;
                insert(
                    handle,
                    TileSetPublicationInput::from_source(
                        handle,
                        AssetRecord::new(
                            content.stable_id(),
                            content.path().clone(),
                            tileset_kind(),
                        ),
                        content.source().clone(),
                        SourceHash::from_digest(*content.source_hash().as_bytes()),
                        image_handle,
                        image,
                        snapshot.asset_resolution().scope(),
                        world
                            .resource::<TileSetAssets>()
                            .limits()
                            .source_reference_bytes(),
                    )
                    .map_err(tileset_failure)?,
                )?;
            }
        }
        ResidencyInput::Processed(content) => {
            for acquired in &content.tilesets {
                if !content.base().same_base(acquired.base()) {
                    return Err(binding_changed());
                }
                let descriptor = *acquired.product();
                if descriptor.product() != acquired.reference() {
                    return Err(binding_changed());
                }
                let handle = bindings
                    .reserve_product::<TileSet>(descriptor.product())
                    .map_err(|_| binding_changed())?;
                let (image_handle, image) = images
                    .retained_image_for_product(
                        descriptor.image_product(),
                        world.resource::<Assets<ImageAsset>>(),
                    )
                    .ok_or_else(resources_missing)?;
                insert(
                    handle,
                    TileSetPublicationInput::from_product(handle, descriptor, image_handle, image),
                )?;
            }
        }
    }
    if !additive {
        changes.retirements.extend(
            owned
                .filter(|receipt| !next_ids.contains(&receipt.handle().id()))
                .cloned(),
        );
        for receipt in &changes.retirements {
            validate_slot(receipt.handle())?;
        }
    }
    Ok(changes)
}

#[cfg(feature = "project-source")]
fn tileset_kind() -> AssetSourceKind {
    AssetSourceKind::Other(String::from("tileset"))
}

pub(super) fn tileset_failure(error: TileSetAssetError) -> HostFault {
    let summary = match error {
        TileSetAssetError::InvalidSourceKind => "Prepared Scene TileSet source kind is invalid",
        TileSetAssetError::InvalidSource => "Prepared Scene TileSet source is invalid",
        TileSetAssetError::InvalidDescriptor => "Prepared Scene TileSet descriptor is invalid",
        TileSetAssetError::Capacity => "Prepared Scene exceeds TileSet source capacity",
        TileSetAssetError::RetentionCapacity => {
            "Prepared Scene exceeds retained TileSet Image capacity"
        }
        TileSetAssetError::SourceReferenceCapacity => {
            "Prepared Scene exceeds TileSet source reference capacity"
        }
        TileSetAssetError::DependencyUnavailable => {
            "Prepared Scene TileSet Image dependency is unavailable"
        }
        TileSetAssetError::ImageExtentMismatch => {
            "Prepared Scene TileSet atlas does not match its Image extent"
        }
        TileSetAssetError::AtlasLayoutChangeRequiresRebuild => {
            "Prepared Scene changes a resident TileSet atlas layout; rebuild its consumers first"
        }
        TileSetAssetError::IdentityMismatch => {
            "Prepared Scene TileSet identity conflicts with the runtime"
        }
        TileSetAssetError::Superseded => "Another writer changed the current TileSet publication",
        TileSetAssetError::InvalidPublicationBatch => {
            "Prepared Scene repeats or overlaps TileSet publication identities"
        }
        TileSetAssetError::Asset(_) => "Prepared Scene TileSet asset publication is invalid",
        TileSetAssetError::State(_) => "Prepared Scene TileSet asset state cannot advance",
    };
    failure("project.scene.tileset-preparation-failed", summary)
}
