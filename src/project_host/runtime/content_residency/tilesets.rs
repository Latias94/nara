use nara_asset::{AssetId, Handle};

use super::*;

#[derive(Clone, Copy)]
pub(super) enum TileSetChange<'a> {
    Replace(Option<&'a ProjectSceneResidency>),
    Add(&'a ProjectSceneResidency),
}

impl<'a> TileSetChange<'a> {
    fn current(self) -> Option<&'a ProjectSceneResidency> {
        match self {
            Self::Replace(current) => current,
            Self::Add(current) => Some(current),
        }
    }
}

#[derive(Default)]
pub(super) struct TileSetChanges {
    pub(super) inputs: Vec<TileSetPublicationInput>,
    pub(super) retirements: Vec<TileSetContentReceipt>,
    pub(super) slots: Vec<AssetMutationBaseline<TileSet>>,
    pub(super) uses: Vec<Handle<TileSet>>,
    pub(super) new_handles: BTreeSet<AssetId>,
}

pub(super) fn prepare(
    world: &World,
    content: ResidencyInput<'_>,
    change: TileSetChange<'_>,
    bindings: &mut SceneAssetReservations,
    images: &ImagePublications,
) -> Result<TileSetChanges, HostFault> {
    let owned = change
        .current()
        .into_iter()
        .flat_map(ProjectSceneResidency::owned_tilesets);
    let additive = matches!(change, TileSetChange::Add(_));
    if content.tileset_count() == 0 && (additive || owned.clone().next().is_none()) {
        return Ok(TileSetChanges::default());
    }
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
        changes.slots.push(
            world
                .resource::<Assets<TileSet>>()
                .prepare_mutation(handle, world.resource::<AssetStates>())
                .map_err(|_| tileset_failure(TileSetAssetError::Superseded))?,
        );
        if additive {
            changes.uses.push(handle);
        }
        changes.inputs.push(input);
        Ok(())
    };
    match content {
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
            changes.slots.push(
                world
                    .resource::<Assets<TileSet>>()
                    .prepare_mutation(receipt.handle(), world.resource::<AssetStates>())
                    .map_err(|_| tileset_failure(TileSetAssetError::Superseded))?,
            );
        }
    }
    Ok(changes)
}
