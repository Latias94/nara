//! Closed content authority and admission for residency-owned metadata.

use nara_asset::{AssetId, AssetProductRef};
use nara_tilemap::TileSetDescriptor;
use std::sync::Arc;

use super::*;
use crate::{
    runtime_content::{RuntimeContentAsset, RuntimeContentScene},
    scene_retention::{ARC_CONTROL_BLOCK_BYTES, BTREE_ENTRY_OVERHEAD},
};

#[cfg(test)]
mod tests;

pub(super) enum ResidencyAuthority {
    Source {
        lineage: ProjectSettingsLineage,
        catalog: Vec<AssetRecord>,
        scope: nara_asset::AssetPathScope,
        _lease: ProjectContentLease,
    },
    Processed {
        _tilesets: Vec<RuntimeContentAsset<TileSetDescriptor>>,
        scenes: Vec<RuntimeContentScene>,
        deferred_scene_products: Vec<AssetProductRef>,
        _lease: Arc<RuntimeContentLease>,
        base: RuntimeContentBase,
    },
}

impl ResidencyAuthority {
    pub(super) fn validate(&self, content: ResidencyInput<'_>) -> Result<(), HostFault> {
        match (self, content) {
            (Self::Source { lineage, scope, .. }, ResidencyInput::Source(snapshot))
                if *lineage == snapshot.lineage()
                    && scope == snapshot.asset_resolution().scope() =>
            {
                Ok(())
            }
            (Self::Processed { base, .. }, ResidencyInput::Processed(content))
                if base.same_base(content.base()) =>
            {
                Ok(())
            }
            (Self::Source { .. }, ResidencyInput::Source(_)) => Err(failure(
                "project.scene.lineage-mismatch",
                "Prepared Scene belongs to another project",
            )),
            _ => Err(failure(
                "project.scene.content-authority-mismatch",
                "Prepared Scene does not retain the current content authority",
            )),
        }
    }
}

pub(super) enum PreparedCatalog {
    Source {
        original: Option<ProjectAssetDatabase>,
        change: CatalogChange,
        lineage: ProjectSettingsLineage,
        scope: nara_asset::AssetPathScope,
        lease: ProjectContentLease,
    },
    Processed {
        tilesets: Vec<RuntimeContentAsset<TileSetDescriptor>>,
        scenes: Vec<RuntimeContentScene>,
        deferred_scene_products: Vec<AssetProductRef>,
        lease: Arc<RuntimeContentLease>,
        base: RuntimeContentBase,
    },
}

impl PreparedCatalog {
    pub(super) fn prepare(
        world: &World,
        content: ResidencyInput<'_>,
        current: Option<&ProjectSceneResidency>,
        addition_to: Option<&ProjectSceneResidency>,
    ) -> Result<Self, HostFault> {
        let bytes = metadata_bytes(world, content, current, addition_to)?;
        match content {
            ResidencyInput::Source(snapshot) => {
                let lease = snapshot
                    .reserve_consumer_retained(bytes)
                    .map_err(|error| HostFault::new(error.diagnostics().clone()))?;
                let original = world.get_resource::<ProjectAssetDatabase>().cloned();
                let empty = ProjectAssetDatabase::default();
                let change = CatalogChange::prepare(
                    original.as_ref().unwrap_or(&empty),
                    current
                        .into_iter()
                        .flat_map(ProjectSceneResidency::owned_catalog_records),
                    snapshot.asset_database().records().cloned(),
                )?;
                Ok(Self::Source {
                    original,
                    change,
                    lineage: snapshot.lineage(),
                    scope: snapshot.asset_resolution().scope().clone(),
                    lease,
                })
            }
            ResidencyInput::Processed(content) => {
                let lease = content.base().reserve_retained(bytes).map_err(|_| {
                    failure(
                        "project.scene.residency-budget-exceeded",
                        "Prepared Scene exceeds its original Runtime content metadata budget",
                    )
                })?;
                Ok(Self::Processed {
                    tilesets: content.tilesets.clone(),
                    scenes: content.dependencies.clone(),
                    deferred_scene_products: content.deferred_scene_products.clone(),
                    lease: Arc::new(lease),
                    base: content.base().clone(),
                })
            }
        }
    }

    pub(super) fn asset_resolution(&self) -> Option<nara_asset::AssetReferenceResolver<'_>> {
        match self {
            Self::Source { change, scope, .. } => Some(change.database.resolver(scope)),
            Self::Processed { .. } => None,
        }
    }

    pub(super) fn runtime_lease(&self) -> Option<&Arc<RuntimeContentLease>> {
        match self {
            Self::Source { .. } => None,
            Self::Processed { lease, .. } => Some(lease),
        }
    }

    pub(super) fn validate(&self, world: &World) -> Result<(), HostFault> {
        if let Self::Source { original, .. } = self {
            if world.get_resource::<ProjectAssetDatabase>() != original.as_ref() {
                return Err(failure(
                    "project.scene.asset-catalog-changed",
                    "Scene asset catalogue changed after preparation",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn commit(self, world: &mut World) -> ResidencyAuthority {
        match self {
            Self::Source {
                original: _,
                change,
                lineage,
                scope,
                lease,
            } => {
                world.insert_resource(change.database);
                ResidencyAuthority::Source {
                    lineage,
                    catalog: change.owned,
                    scope,
                    _lease: lease,
                }
            }
            Self::Processed {
                tilesets,
                scenes,
                deferred_scene_products,
                lease,
                base,
            } => ResidencyAuthority::Processed {
                _tilesets: tilesets,
                scenes,
                deferred_scene_products,
                _lease: lease,
                base,
            },
        }
    }
}

fn metadata_bytes(
    world: &World,
    content: ResidencyInput<'_>,
    current: Option<&ProjectSceneResidency>,
    addition_to: Option<&ProjectSceneResidency>,
) -> Result<usize, HostFault> {
    // One conservative logical reservation covers both preparation and its committed receipts.
    // Pixel payloads retain their original acquisition/import charge; native TileSet owners keep
    // their own capacity model. No catalog, path or source hash is manufactured for Runtime data.
    let mut bytes = size_of::<ProjectSceneResidency>()
        + size_of::<PreparedResidency>()
        + size_of::<StagedProjectSceneReplacement>();
    // SceneAssetReservations owns an original baseline and a staged catalogue. Residency also
    // retains a staged clone across native waits. Include every existing binding, including
    // unrelated Products, path aliases and anonymous claims, before any of those clones.
    let catalogue_bytes = match world.get_resource::<AssetServer>() {
        Some(server) => server
            .checked_retained_bytes()
            .ok_or_else(metadata_overflow)?,
        None => size_of::<AssetServer>(),
    };
    let staged_copies = 2;
    add_entries(&mut bytes, 1 + staged_copies, catalogue_bytes)?;
    // Only the staged copies grow. A complete new binding is a conservative upper bound
    // even when the target reuses an existing identity; current-only bindings are already above.
    match content {
        ResidencyInput::Source(snapshot) => {
            for record in snapshot.asset_database().records() {
                let growth =
                    AssetServer::checked_product_binding_retained_bytes(Some(record.path()))
                        .ok_or_else(metadata_overflow)?;
                add_entries(&mut bytes, staged_copies, growth)?;
            }
        }
        ResidencyInput::Processed(content) => {
            let growth = AssetServer::checked_product_binding_retained_bytes(None)
                .ok_or_else(metadata_overflow)?;
            let products = content
                .images
                .len()
                .checked_add(content.tilesets.len())
                .ok_or_else(metadata_overflow)?;
            add_entries(
                &mut bytes,
                products,
                growth
                    .checked_mul(staged_copies)
                    .ok_or_else(metadata_overflow)?,
            )?;
        }
    }
    if addition_to.is_some() {
        add_entries(&mut bytes, 1, size_of::<additive::ActiveAddition>())?;
        add_entries(&mut bytes, 1, size_of::<additive::PreparedAddition>())?;
        add_entries(
            &mut bytes,
            content
                .image_count()
                .checked_add(content.material_count())
                .ok_or_else(metadata_overflow)?,
            size_of::<AssetProductRef>(),
        )?;
        add_entries(
            &mut bytes,
            content.tileset_count(),
            size_of::<nara_asset::Handle<TileSet>>(),
        )?;
    }
    let image_metadata = size_of::<images::ImagePublication>()
        + size_of::<OwnedImage>()
        + size_of::<AssetProductRef>()
        + 3 * BTREE_ENTRY_OVERHEAD
        + 5 * ARC_CONTROL_BLOCK_BYTES
        + size_of::<RuntimeContentAsset<ImageAsset>>();
    add_entries(&mut bytes, content.image_count(), image_metadata)?;
    let tileset_metadata = 3 * size_of::<TileSetPublicationInput>()
        + 3 * size_of::<TileSetContentReceipt>()
        + size_of::<RuntimeContentAsset<TileSetDescriptor>>()
        + size_of::<AssetMutationBaseline<TileSet>>()
        + size_of::<AssetId>()
        + 3 * BTREE_ENTRY_OVERHEAD;
    add_entries(&mut bytes, content.tileset_count(), tileset_metadata)?;
    if let Some(current) = current.or(addition_to) {
        add_entries(
            &mut bytes,
            current.image_count(),
            2 * size_of::<OwnedImage>()
                + size_of::<AssetId>()
                + size_of::<&OwnedImage>()
                + BTREE_ENTRY_OVERHEAD,
        )?;
        add_entries(
            &mut bytes,
            current.tileset_count(),
            2 * size_of::<TileSetContentReceipt>()
                + size_of::<AssetId>()
                + size_of::<AssetMutationBaseline<TileSet>>()
                + size_of::<&TileSetContentReceipt>()
                + BTREE_ENTRY_OVERHEAD,
        )?;
    }
    if let ResidencyInput::Source(snapshot) = content {
        if let Some(prefix) = snapshot.asset_resolution().scope().prefix() {
            add_entries(
                &mut bytes,
                1,
                prefix
                    .as_str()
                    .len()
                    .checked_add(crate::scene_retention::STRING_ALLOCATION_OVERHEAD)
                    .ok_or_else(metadata_overflow)?,
            )?;
        }
        // Existing database estimates include owned paths, case-folded keys and tree indices.
        // Four copies conservatively cover original/merged catalogs, target indexing, receipts
        // and real source aliases while those representations overlap during preparation.
        let existing = world.get_resource::<ProjectAssetDatabase>();
        for record in existing
            .into_iter()
            .flat_map(ProjectAssetDatabase::records)
            .chain(snapshot.asset_database().records())
        {
            let record_bytes = asset_record_retained_bytes(record)
                .map_err(|error| HostFault::new(error.diagnostics().clone()))?;
            add_entries(&mut bytes, 4, record_bytes)?;
        }
        for tileset in snapshot.tilesets() {
            if let nara_asset::AssetRef::Path(path) = &tileset.source().image {
                // Inputs and their prepared copies retain authored aliases. Their bound
                // canonical dependency is one shared allocation, independent of the Host scope.
                let alias = path
                    .as_str()
                    .len()
                    .checked_add(crate::scene_retention::STRING_ALLOCATION_OVERHEAD)
                    .ok_or_else(metadata_overflow)?;
                add_entries(&mut bytes, 3, alias)?;
                let canonical = snapshot
                    .asset_resolution()
                    .resolution_work_bytes(&tileset.source().image)
                    .ok_or_else(metadata_overflow)?;
                let canonical_len = if canonical == 0 {
                    path.as_str().len()
                } else {
                    canonical
                };
                add_entries(
                    &mut bytes,
                    1,
                    canonical_len
                        .checked_add(size_of::<nara_asset::AssetSourcePath>())
                        .and_then(|value| value.checked_add(ARC_CONTROL_BLOCK_BYTES))
                        .and_then(|value| {
                            value.checked_add(crate::scene_retention::STRING_ALLOCATION_OVERHEAD)
                        })
                        .ok_or_else(metadata_overflow)?,
                )?;
                // The temporary catalogue lookup overlaps those retained inputs.
                add_entries(&mut bytes, 1, canonical)?;
            }
        }
    } else {
        let ResidencyInput::Processed(content) = content else {
            unreachable!()
        };
        add_entries(
            &mut bytes,
            content.dependencies.len(),
            size_of::<RuntimeContentScene>(),
        )?;
        add_entries(
            &mut bytes,
            content.deferred_scene_products.len(),
            size_of::<AssetProductRef>(),
        )?;
        add_entries(
            &mut bytes,
            1,
            size_of::<RuntimeContentLease>() + ARC_CONTROL_BLOCK_BYTES,
        )?;
    }
    Ok(bytes)
}

fn add_entries(bytes: &mut usize, count: usize, per_entry: usize) -> Result<(), HostFault> {
    *bytes = count
        .checked_mul(per_entry)
        .and_then(|extra| bytes.checked_add(extra))
        .ok_or_else(metadata_overflow)?;
    Ok(())
}

fn metadata_overflow() -> HostFault {
    failure(
        "project.scene.residency-metadata-overflow",
        "Scene residency metadata size overflowed",
    )
}
