//! Closed content authority and admission for residency-owned metadata.

use std::{mem::size_of, sync::Arc};

use nara_asset::AssetProductRef;
#[cfg(feature = "runtime-2d")]
use nara_asset::{AssetId, AssetMutationBaseline};
#[cfg(feature = "project-source")]
use nara_asset::{AssetSourceKind, ImportedProductId};
#[cfg(feature = "runtime-2d")]
use nara_render::image::{ImageAsset, ImagePublicationInput};
#[cfg(feature = "project-source")]
use nara_render::material::{Material2dAsset, Material2dDomain};
#[cfg(feature = "runtime-2d")]
use nara_render::tilemap::{
    TileSet, TileSetContentReceipt, TileSetDescriptor, TileSetPublicationInput,
};

#[cfg(feature = "project-source")]
use super::catalog::CatalogChange;
#[cfg(feature = "runtime-2d")]
use super::images::OwnedImage;
use super::*;
#[cfg(feature = "project-source")]
use crate::{
    project_content::{ProjectContentLease, ProjectSourceContent, asset_record_retained_bytes},
    project_host::ProjectSettingsLineage,
};
#[cfg(feature = "runtime-2d")]
use crate::{runtime_content::RuntimeContentAsset, scene_retention::BTREE_ENTRY_OVERHEAD};
use crate::{
    runtime_content::{RuntimeContentBase, RuntimeContentLease, RuntimeContentScene},
    scene_retention::ARC_CONTROL_BLOCK_BYTES,
};

#[cfg(test)]
mod tests;

pub(super) enum ResidencyAuthority {
    #[cfg(feature = "project-source")]
    Source {
        lineage: ProjectSettingsLineage,
        catalog: Vec<AssetRecord>,
        sources: Arc<ProjectSourceContent>,
        scope: nara_asset::AssetPathScope,
        _lease: ProjectContentLease,
    },
    Processed {
        #[cfg(feature = "runtime-2d")]
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
            #[cfg(feature = "project-source")]
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
            #[cfg(feature = "project-source")]
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
    #[cfg(feature = "project-source")]
    Source {
        original: Option<ProjectAssetDatabase>,
        change: CatalogChange,
        lineage: ProjectSettingsLineage,
        sources: Arc<ProjectSourceContent>,
        scope: nara_asset::AssetPathScope,
        lease: ProjectContentLease,
    },
    Processed {
        #[cfg(feature = "runtime-2d")]
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
            #[cfg(feature = "project-source")]
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
                    sources: Arc::clone(snapshot.retained_sources()),
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
                    #[cfg(feature = "runtime-2d")]
                    tilesets: content.tilesets.clone(),
                    scenes: content.dependencies.clone(),
                    deferred_scene_products: content.deferred_scene_products.clone(),
                    lease: Arc::new(lease),
                    base: content.base().clone(),
                })
            }
        }
    }

    #[cfg(feature = "runtime-2d")]
    pub(super) fn asset_database(&self) -> Option<&ProjectAssetDatabase> {
        match self {
            #[cfg(feature = "project-source")]
            Self::Source { change, .. } => Some(&change.database),
            Self::Processed { .. } => None,
        }
    }

    #[cfg(feature = "project-source")]
    pub(super) fn stage_source_relocations(
        &self,
        snapshot: &ProjectContentSnapshot,
        bindings: &mut SceneAssetReservations,
    ) -> Result<(), HostFault> {
        let Self::Source {
            original,
            change,
            scope,
            ..
        } = self
        else {
            return Ok(());
        };
        if !snapshot.has_complete_reference_catalogue() {
            return Ok(());
        }
        let materials = snapshot.domain_assets().collection::<Material2dDomain>();
        let published_paths = snapshot.images().iter().map(|image| image.path()).chain(
            materials
                .iter()
                .flat_map(|collection| collection.source_records().map(AssetRecord::path)),
        );
        for record in published_paths.filter_map(|path| change.owned_record(path)) {
            let primary = AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
                .map_err(|_| binding_changed())?;
            let server = bindings.staged_server();
            let id = match record.source_kind() {
                AssetSourceKind::Image => server
                    .get_product_handle::<ImageAsset>(primary)
                    .map(|handle| handle.id()),
                AssetSourceKind::Other(kind) if kind == Material2dDomain::SOURCE_KIND => server
                    .get_product_handle::<Material2dAsset>(primary)
                    .map(|handle| handle.id()),
                _ => continue,
            };
            let Some(previous) = id.and_then(|id| server.path(id)) else {
                continue;
            };
            if previous == record.path() {
                continue;
            }
            if scope.relative_path(previous).is_none()
                || scope.relative_path(record.path()).is_none()
                || change.database.record_for_path(previous).is_some()
                || original
                    .as_ref()
                    .and_then(|db| db.record_for_path(previous))
                    .is_some_and(|old| {
                        old.stable_id() != record.stable_id()
                            || old.source_kind() != record.source_kind()
                    })
            {
                return Err(binding_changed());
            }
            let previous = previous.clone();
            bindings
                .relocate_primary_source(&previous, record)
                .map_err(|_| binding_changed())?;
        }
        Ok(())
    }

    #[cfg(feature = "runtime-2d")]
    pub(super) fn runtime_lease(&self) -> Option<&Arc<RuntimeContentLease>> {
        match self {
            #[cfg(feature = "project-source")]
            Self::Source { .. } => None,
            Self::Processed { lease, .. } => Some(lease),
        }
    }

    pub(super) fn validate(&self, _world: &World) -> Result<(), HostFault> {
        #[cfg(feature = "project-source")]
        if let Self::Source { original, .. } = self {
            if _world.get_resource::<ProjectAssetDatabase>() != original.as_ref() {
                return Err(failure(
                    "project.scene.asset-catalog-changed",
                    "Scene asset catalogue changed after preparation",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn commit(self, _world: &mut World) -> ResidencyAuthority {
        match self {
            #[cfg(feature = "project-source")]
            Self::Source {
                original: _,
                change,
                lineage,
                sources,
                scope,
                lease,
            } => {
                _world.insert_resource(change.database);
                ResidencyAuthority::Source {
                    lineage,
                    catalog: change.owned,
                    sources,
                    scope,
                    _lease: lease,
                }
            }
            Self::Processed {
                #[cfg(feature = "runtime-2d")]
                tilesets,
                scenes,
                deferred_scene_products,
                lease,
                base,
            } => ResidencyAuthority::Processed {
                #[cfg(feature = "runtime-2d")]
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
        + size_of::<nara_scene::current_scene::StagedCurrentSceneReplacement>();
    // SceneAssetReservations owns the original baseline and the staged catalogue. Startup
    // and 2D additive activation also clone the staged catalogue before publication.
    let catalogue_bytes = match world.get_resource::<AssetServer>() {
        Some(server) => server
            .checked_retained_bytes()
            .ok_or_else(metadata_overflow)?,
        None => size_of::<AssetServer>(),
    };
    let staged_copies = match (current, addition_to) {
        (Some(_), _) => 1,
        (None, Some(_)) if !cfg!(feature = "runtime-2d") => 1,
        _ => 2,
    };
    add_entries(&mut bytes, 1 + staged_copies, catalogue_bytes)?;
    if addition_to.is_some() {
        add_entries(&mut bytes, 1, size_of::<additive::ActiveAddition>())?;
        add_entries(&mut bytes, 1, size_of::<additive::PreparedAddition>())?;
    }
    #[cfg(feature = "runtime-2d")]
    stock_metadata_bytes(&mut bytes, content, current, addition_to, staged_copies)?;
    let content = match content {
        #[cfg(feature = "project-source")]
        ResidencyInput::Source(snapshot) => {
            source_metadata_bytes(&mut bytes, world, snapshot)?;
            return Ok(bytes);
        }
        ResidencyInput::Processed(content) => content,
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
    Ok(bytes)
}

#[cfg(feature = "runtime-2d")]
fn stock_metadata_bytes(
    bytes: &mut usize,
    content: ResidencyInput<'_>,
    current: Option<&ProjectSceneResidency>,
    addition_to: Option<&ProjectSceneResidency>,
    staged_copies: usize,
) -> Result<(), HostFault> {
    // Only the staged copies grow. A complete new binding is a conservative upper bound
    // even when the target reuses an existing identity; current-only bindings are already above.
    match content {
        #[cfg(feature = "project-source")]
        ResidencyInput::Source(snapshot) => {
            for record in snapshot.asset_database().records() {
                let growth =
                    AssetServer::checked_product_binding_retained_bytes(Some(record.path()))
                        .ok_or_else(metadata_overflow)?;
                add_entries(bytes, staged_copies, growth)?;
            }
        }
        ResidencyInput::Processed(content) => {
            let growth = AssetServer::checked_product_binding_retained_bytes(None)
                .ok_or_else(metadata_overflow)?;
            let products = content
                .images
                .len()
                .checked_add(content.tilesets.len())
                .and_then(|products| products.checked_add(content.materials.len()))
                .ok_or_else(metadata_overflow)?;
            add_entries(
                bytes,
                products,
                growth
                    .checked_mul(staged_copies)
                    .ok_or_else(metadata_overflow)?,
            )?;
        }
    }
    if addition_to.is_some() {
        add_entries(
            bytes,
            content
                .image_count()
                .checked_add(content.material_count())
                .ok_or_else(metadata_overflow)?,
            size_of::<AssetProductRef>(),
        )?;
        add_entries(
            bytes,
            content.tileset_count(),
            size_of::<nara_asset::Handle<TileSet>>(),
        )?;
    }
    let image_metadata = 2 * size_of::<ImagePublicationInput>()
        + size_of::<AssetMutationBaseline<ImageAsset>>()
        + size_of::<OwnedImage>()
        + size_of::<AssetProductRef>()
        + 3 * BTREE_ENTRY_OVERHEAD
        + 5 * ARC_CONTROL_BLOCK_BYTES
        + size_of::<RuntimeContentAsset<ImageAsset>>();
    add_entries(bytes, content.image_count(), image_metadata)?;
    let tileset_metadata = 3 * size_of::<TileSetPublicationInput>()
        + 3 * size_of::<TileSetContentReceipt>()
        + size_of::<RuntimeContentAsset<TileSetDescriptor>>()
        + size_of::<AssetMutationBaseline<TileSet>>()
        + size_of::<AssetId>()
        + 3 * BTREE_ENTRY_OVERHEAD;
    add_entries(bytes, content.tileset_count(), tileset_metadata)?;
    if let Some(current) = current.or(addition_to) {
        add_entries(
            bytes,
            current.image_count(),
            2 * size_of::<OwnedImage>()
                + size_of::<AssetId>()
                + size_of::<&OwnedImage>()
                + BTREE_ENTRY_OVERHEAD,
        )?;
        add_entries(
            bytes,
            current.tileset_count(),
            2 * size_of::<TileSetContentReceipt>()
                + size_of::<AssetId>()
                + size_of::<AssetMutationBaseline<TileSet>>()
                + size_of::<&TileSetContentReceipt>()
                + BTREE_ENTRY_OVERHEAD,
        )?;
    }
    Ok(())
}

#[cfg(feature = "project-source")]
fn source_metadata_bytes(
    bytes: &mut usize,
    world: &World,
    snapshot: &ProjectContentSnapshot,
) -> Result<(), HostFault> {
    if let Some(prefix) = snapshot.asset_resolution().scope().prefix() {
        add_entries(
            bytes,
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
        add_entries(bytes, 4, record_bytes)?;
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
            add_entries(bytes, 3, alias)?;
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
                bytes,
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
            add_entries(bytes, 1, canonical)?;
        }
    }
    Ok(())
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
