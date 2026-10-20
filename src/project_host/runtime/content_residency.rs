//! Concrete CPU content ownership for one committed project Scene.

mod catalog;
mod images;

use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
};

use nara_asset::{
    AssetEvents, AssetRecord, AssetRef, AssetServer, AssetSourceKind, AssetStates, Assets,
    ProjectAssetDatabase, SourceHash,
};
use nara_ecs::{Mut, Res, ResMut, Resource, World, system::SystemState};
use nara_identity::{SpawnedSceneInstance, WorldEntityToken};
use nara_image::ImageAsset;
use nara_scene::{
    SceneSpawnReport,
    advanced::{
        SceneAssetBindings, SceneProductOverlayWriter, SceneProductTransactionLimits,
        replace_expanded_scene_with_product_and_bindings,
    },
};
use nara_tilemap::{
    __private::{PreparedTileSetPublications, TileSetOwnedSourceReceipt, TileSetPublicationInput},
    TileSet, TileSetAssetError, TileSetAssets,
};

use super::{
    HostFault, ProjectContentSnapshot, ProjectSettingsLineage,
    publish_snapshot_assets_with_database, single_error,
};
use crate::{
    project_content::{ProjectContentLease, asset_record_retained_bytes},
    startup_scene::{CurrentSceneOwner, current_scene_registry},
};
use catalog::CatalogChange;
use images::{ImagePublications, OwnedImage};

#[derive(Resource)]
struct ProjectSceneResidency {
    lineage: ProjectSettingsLineage,
    images: Vec<OwnedImage>,
    tilesets: Vec<TileSetOwnedSourceReceipt>,
    catalog: Vec<AssetRecord>,
    _lease: ProjectContentLease,
}

/// Startup is unpublished, so its existing incremental publisher remains legal. Capture actual
/// publications only after it succeeds, and preserve registrations supplied by other plugins.
pub(super) fn publish_startup(
    world: &mut World,
    snapshot: &ProjectContentSnapshot,
) -> Result<(), HostFault> {
    let catalog = CatalogChange::prepare(
        &world
            .get_resource::<ProjectAssetDatabase>()
            .cloned()
            .unwrap_or_default(),
        &[],
        snapshot.asset_database().records().cloned(),
    )?;
    let lease = reserve_metadata(snapshot, &catalog)?;
    if !snapshot.images().is_empty() {
        images::validate_startup_images(
            snapshot,
            world
                .get_resource::<AssetServer>()
                .ok_or_else(resources_missing)?,
            world
                .get_resource::<Assets<ImageAsset>>()
                .ok_or_else(resources_missing)?,
            world
                .get_resource::<AssetStates>()
                .ok_or_else(resources_missing)?,
        )?;
    }
    publish_snapshot_assets_with_database(world, snapshot, &catalog.database)?;
    let mut bindings = SceneAssetBindings::from_world(world);
    let mut images = Vec::with_capacity(snapshot.images().len());
    for content in snapshot.images() {
        let source = content.image().source();
        let record = AssetRecord::new(
            source.stable_id(),
            source.path().clone(),
            AssetSourceKind::Image,
        );
        let handle = bindings
            .reserve::<ImageAsset>(
                &AssetRef::StableId(record.stable_id()),
                snapshot.asset_database(),
                &AssetSourceKind::Image,
            )
            .expect("startup published every admitted Image identity");
        let values = world.resource::<Assets<ImageAsset>>();
        let states = world.resource::<AssetStates>();
        assert!(values.get(handle).is_some());
        images.push(OwnedImage::capture(handle, record, values, states));
    }
    let mut tilesets = Vec::with_capacity(snapshot.tilesets().len());
    for content in snapshot.tilesets() {
        let handle = bindings
            .reserve::<TileSet>(
                &AssetRef::StableId(content.stable_id()),
                snapshot.asset_database(),
                &tileset_kind(),
            )
            .expect("startup published every admitted TileSet identity");
        let receipt = world
            .resource::<TileSetAssets>()
            .owned_source_receipt(
                handle,
                world.resource::<Assets<TileSet>>(),
                world.resource::<AssetStates>(),
            )
            .expect("startup retains each successfully installed TileSet source");
        tilesets.push(receipt);
    }
    world.insert_resource(catalog.database);
    world.insert_resource(ProjectSceneResidency {
        lineage: snapshot.lineage(),
        images,
        tilesets,
        catalog: catalog.owned,
        _lease: lease,
    });
    Ok(())
}

struct PreparedResidency {
    bindings: SceneAssetBindings,
    images: ImagePublications,
    inputs: Vec<TileSetPublicationInput>,
    retirements: Vec<TileSetOwnedSourceReceipt>,
    catalog: CatalogChange,
    lease: ProjectContentLease,
}

impl PreparedResidency {
    fn prepare(
        world: &World,
        snapshot: &ProjectContentSnapshot,
        current: &ProjectSceneResidency,
    ) -> Result<Self, HostFault> {
        if snapshot.lineage() != current.lineage {
            return Err(failure(
                "project.scene.lineage-mismatch",
                "Prepared Scene belongs to another project",
            ));
        }
        let mut bindings = SceneAssetBindings::from_world(world);
        let no_content = current.images.is_empty() && snapshot.images().is_empty();
        if !no_content && !world.contains_resource::<AssetEvents>() {
            return Err(resources_missing());
        }
        let images = if no_content {
            // A data-only replacement has no Image resource dependency.
            ImagePublications::empty()
        } else {
            ImagePublications::prepare(
                snapshot,
                &current.images,
                &mut bindings,
                world
                    .get_resource::<AssetServer>()
                    .ok_or_else(resources_missing)?,
                world
                    .get_resource::<Assets<ImageAsset>>()
                    .ok_or_else(resources_missing)?,
                world
                    .get_resource::<AssetStates>()
                    .ok_or_else(resources_missing)?,
            )?
        };
        let old_tilesets = current
            .tilesets
            .iter()
            .map(|receipt| (receipt.handle().id(), receipt))
            .collect::<BTreeMap<_, _>>();
        let mut inputs = Vec::with_capacity(snapshot.tilesets().len());
        let mut next_ids = BTreeSet::new();
        for content in snapshot.tilesets() {
            let handle = bindings
                .reserve::<TileSet>(
                    &AssetRef::StableId(content.stable_id()),
                    snapshot.asset_database(),
                    &tileset_kind(),
                )
                .map_err(|_| {
                    failure(
                        "project.scene.asset-identity-conflict",
                        "Prepared Scene asset identity conflicts with the runtime",
                    )
                })?;
            let (image_handle, image) = images
                .image_for_path(content.image_path())
                .ok_or_else(resources_missing)?;
            let record =
                AssetRecord::new(content.stable_id(), content.path().clone(), tileset_kind());
            let mut input = TileSetPublicationInput::new(
                handle,
                record,
                content.source().clone(),
                SourceHash::from_digest(*content.source_hash().as_bytes()),
                image_handle,
                image,
            );
            if let Some(receipt) = old_tilesets.get(&handle.id()) {
                input = input.with_owned_source((*receipt).clone());
            }
            next_ids.insert(handle.id());
            inputs.push(input);
        }
        let retirements = current
            .tilesets
            .iter()
            .filter(|receipt| !next_ids.contains(&receipt.handle().id()))
            .cloned()
            .collect();
        let catalog = CatalogChange::prepare(
            &world
                .get_resource::<ProjectAssetDatabase>()
                .cloned()
                .unwrap_or_default(),
            &current.catalog,
            snapshot.asset_database().records().cloned(),
        )?;
        let lease = reserve_metadata(snapshot, &catalog)?;
        Ok(Self {
            bindings,
            images,
            inputs,
            retirements,
            catalog,
            lease,
        })
    }
}

/// The owned Host preparation entry is the only caller of this crate-private operation. It must
/// deliver an admitted snapshot from its original loader; no public project-source bypass exists.
pub(super) fn replace_project_scene<F>(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    snapshot: &ProjectContentSnapshot,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    nara_hierarchy::__private::prepare_mutation_baseline(world);
    let registry = match current_scene_registry(world, expected) {
        Ok(registry) => registry,
        Err(report) => return report,
    };
    if !world.resource::<CurrentSceneOwner>().is_project_backed()
        || !world.contains_resource::<ProjectSceneResidency>()
        || !world.contains_resource::<ProjectAssetDatabase>()
    {
        return rejected(resources_missing());
    }
    if nara_ecs::__private::validate_resource_scope::<ProjectSceneResidency>(world).is_err()
        || nara_ecs::__private::validate_resource_scope::<ProjectAssetDatabase>(world).is_err()
    {
        return rejected(failure(
            "project.scene.residency-lifecycle-unsupported",
            "Scene residency owners cannot run lifecycle effects during commit",
        ));
    }
    let uses_tilesets = !snapshot.tilesets().is_empty()
        || !world
            .resource::<ProjectSceneResidency>()
            .tilesets
            .is_empty();
    if uses_tilesets
        && nara_ecs::__private::validate_resource_scope::<TileSetAssets>(world).is_err()
    {
        return rejected(failure(
            "project.scene.residency-lifecycle-unsupported",
            "Scene residency owners cannot run lifecycle effects during commit",
        ));
    }
    if !snapshot.shares_schema_snapshot(&registry.snapshot().expect("current registry is frozen")) {
        return rejected(failure(
            "project.scene.schema-mismatch",
            "Prepared Scene uses another schema authority",
        ));
    }
    world.resource_scope(|world, mut current: Mut<ProjectSceneResidency>| {
        let mut prepared = match PreparedResidency::prepare(world, snapshot, &current) {
            Ok(prepared) => prepared,
            Err(error) => return rejected(error),
        };
        if prepared.inputs.is_empty() && prepared.retirements.is_empty() {
            return commit(
                world,
                &mut current,
                prepared,
                None,
                expected,
                snapshot,
                limits,
                additional_retirements,
                configure,
            );
        }
        if !world.contains_resource::<TileSetAssets>()
            || !world.contains_resource::<Assets<TileSet>>()
            || !world.contains_resource::<AssetStates>()
        {
            return rejected(resources_missing());
        }
        world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
            let tilesets = match owner.prepare_publications(
                std::mem::take(&mut prepared.inputs),
                &prepared.retirements,
                prepared.bindings.staged_server(),
                &prepared.catalog.database,
                world.resource::<Assets<TileSet>>(),
                world.resource::<AssetStates>(),
            ) {
                Ok(tilesets) => tilesets,
                Err(error) => return rejected(tileset_failure(error)),
            };
            commit(
                world,
                &mut current,
                prepared,
                Some(tilesets),
                expected,
                snapshot,
                limits,
                additional_retirements,
                configure,
            )
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn commit<F>(
    world: &mut World,
    current: &mut ProjectSceneResidency,
    prepared: PreparedResidency,
    tilesets: Option<PreparedTileSetPublications<'_>>,
    expected: &SpawnedSceneInstance,
    snapshot: &ProjectContentSnapshot,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    let registry = match current_scene_registry(world, expected) {
        Ok(registry) => registry,
        Err(report) => return report,
    };
    let PreparedResidency {
        bindings,
        images,
        catalog,
        lease,
        ..
    } = prepared;
    let report = replace_expanded_scene_with_product_and_bindings(
        world,
        &registry,
        snapshot.expanded_candidate(),
        Some(snapshot.asset_database()),
        expected,
        bindings,
        limits,
        additional_retirements,
        configure,
    );
    if report.diagnostics.has_errors() || report.instance.is_none() {
        return report;
    }
    let images = if images.is_empty() {
        Vec::new()
    } else {
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
    };
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
    world.insert_resource(catalog.database);
    world.resource_mut::<CurrentSceneOwner>().commit_source(
        snapshot.scene_source(),
        report.instance.as_ref().unwrap().clone(),
    );
    *current = ProjectSceneResidency {
        lineage: snapshot.lineage(),
        images,
        tilesets,
        catalog: catalog.owned,
        _lease: lease,
    };
    report
}

fn reserve_metadata(
    snapshot: &ProjectContentSnapshot,
    catalog: &CatalogChange,
) -> Result<ProjectContentLease, HostFault> {
    // Logical owned metadata, including the merged catalogue copy; decoded pixels keep their own
    // original content lease. Checked arithmetic preserves the existing aggregate content ceiling.
    let mut bytes = size_of::<ProjectSceneResidency>()
        .checked_add(
            snapshot
                .images()
                .len()
                .checked_mul(size_of::<OwnedImage>())
                .ok_or_else(resources_missing)?,
        )
        .and_then(|bytes| {
            bytes.checked_add(
                snapshot
                    .tilesets()
                    .len()
                    .checked_mul(size_of::<TileSetOwnedSourceReceipt>())?,
            )
        })
        .ok_or_else(resources_missing)?;
    // Reuse the catalogue's conservative path/case-fold/index estimate. Applying it to plain
    // owned records overcounts storage but preserves one checked, qualified policy.
    let image_records = snapshot
        .images()
        .iter()
        .map(|content| {
            let source = content.image().source();
            AssetRecord::new(
                source.stable_id(),
                source.path().clone(),
                AssetSourceKind::Image,
            )
        })
        .collect::<Vec<_>>();
    for record in catalog
        .database
        .records()
        .chain(catalog.owned.iter())
        .chain(image_records.iter())
    {
        let record_bytes = asset_record_retained_bytes(record)
            .map_err(|error| HostFault::new(error.diagnostics().clone()))?;
        bytes = bytes
            .checked_add(record_bytes)
            .ok_or_else(resources_missing)?;
    }
    snapshot
        .reserve_consumer_retained(bytes)
        .map_err(|error| HostFault::new(error.diagnostics().clone()))
}

fn tileset_kind() -> AssetSourceKind {
    AssetSourceKind::Other(String::from("tileset"))
}
fn tileset_failure(error: TileSetAssetError) -> HostFault {
    let summary = match error {
        TileSetAssetError::InvalidSourceKind => "Prepared Scene TileSet source kind is invalid",
        TileSetAssetError::InvalidSource => "Prepared Scene TileSet source is invalid",
        TileSetAssetError::Capacity => "Prepared Scene exceeds TileSet source capacity",
        TileSetAssetError::RetentionCapacity => {
            "Prepared Scene exceeds retained TileSet Image capacity"
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
fn failure(code: &'static str, summary: &'static str) -> HostFault {
    HostFault::new(single_error(code, summary))
}
fn resources_missing() -> HostFault {
    failure(
        "project.scene.residency-unavailable",
        "Project Scene residency resources are unavailable",
    )
}
fn rejected(fault: HostFault) -> SceneSpawnReport {
    let mut report = SceneSpawnReport::default();
    report.diagnostics = fault.diagnostics;
    report
}
