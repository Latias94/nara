//! Concrete CPU content ownership for one committed project Scene.

mod catalog;
mod images;
mod metadata;
mod replacement;
pub(super) use replacement::PreparedProjectSceneReplacement;

use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
};

use nara_asset::{
    AssetEvents, AssetMutationBaseline, AssetRecord, AssetRef, AssetServer, AssetSourceKind,
    AssetStates, Assets, ProjectAssetDatabase, SourceHash,
};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use nara_ecs::{Mut, Res, ResMut, Resource, World, bevy_ecs::system::SystemState};
use nara_identity::{SpawnedSceneInstance, WorldEntityToken};
use nara_image::ImageAsset;
use nara_scene::{
    SceneSpawnReport,
    advanced::{
        PreparedSceneReplacement, SceneAssetBindings, SceneNativeRetirementClaims,
        SceneProductOverlayWriter, SceneProductTransactionLimits,
    },
};
use nara_tilemap::{
    PreparedTileSetPublications, TileSet, TileSetAssetError, TileSetAssets, TileSetContentReceipt,
    TileSetPublicationInput,
};

use super::content::PreparedSceneContent;
use super::{HostFault, ProjectContentSnapshot, ProjectSettingsLineage, single_error};
use crate::{
    project_content::{ProjectContentLease, asset_record_retained_bytes},
    runtime_content::{ProcessedSceneContent, RuntimeContentBase, RuntimeContentLease},
    startup_scene::{CurrentSceneOwner, current_scene_bindings},
};
use catalog::CatalogChange;
use images::{ImagePublications, OwnedImage};
use metadata::{PreparedCatalog, ResidencyAuthority};

#[derive(Resource)]
struct ProjectSceneResidency {
    images: Vec<OwnedImage>,
    tilesets: Vec<TileSetContentReceipt>,
    // Resource values and their receipts retire before their metadata reservation and base.
    authority: ResidencyAuthority,
}

#[derive(Clone, Copy)]
enum ResidencyInput<'a> {
    Source(&'a ProjectContentSnapshot),
    Processed(&'a ProcessedSceneContent),
}

impl<'a> From<&'a PreparedSceneContent> for ResidencyInput<'a> {
    fn from(content: &'a PreparedSceneContent) -> Self {
        match content {
            PreparedSceneContent::Source(snapshot) => Self::Source(snapshot),
            PreparedSceneContent::Processed(content) => Self::Processed(content),
        }
    }
}

impl ResidencyInput<'_> {
    fn image_count(self) -> usize {
        match self {
            Self::Source(snapshot) => snapshot.images().len(),
            Self::Processed(content) => content.images.len(),
        }
    }

    fn tileset_count(self) -> usize {
        match self {
            Self::Source(snapshot) => snapshot.tilesets().len(),
            Self::Processed(content) => content.tilesets.len(),
        }
    }
}

/// Source startup uses the same concrete resource preparation and publication as later travel.
pub(super) fn publish_startup(
    world: &mut World,
    snapshot: &ProjectContentSnapshot,
) -> Result<(), HostFault> {
    publish_startup_content(
        world,
        ResidencyInput::Source(snapshot),
        snapshot.domain_assets(),
    )
}

/// Publishes into the unpublished startup World before its processed Scene is materialized.
pub(super) fn publish_processed_startup(
    world: &mut World,
    content: &ProcessedSceneContent,
) -> Result<(), HostFault> {
    publish_startup_content(world, ResidencyInput::Processed(content), &content.domains)
}

fn publish_startup_content(
    world: &mut World,
    content: ResidencyInput<'_>,
    domains: &crate::project_content::ProjectDomainAssets,
) -> Result<(), HostFault> {
    prepare_startup_resources(world, content)?;
    if world.contains_resource::<ProjectSceneResidency>() {
        return Err(resources_missing());
    }
    let prepared = PreparedResidency::prepare(world, content, None)?;
    prepared
        .bindings
        .validate(world)
        .map_err(|_| binding_changed())?;
    prepared.catalog.validate(world)?;
    if prepared.inputs.is_empty() && prepared.retirements.is_empty() {
        prepared.commit_startup(world, domains, None);
    } else {
        // This owner borrow fences capacity and claims through the same synchronous tail used
        // by Scene replacement. Startup has no CurrentSceneOwner or live Scene to retire yet.
        world.resource_scope(
            |world, mut owner: Mut<TileSetAssets>| -> Result<(), HostFault> {
                prepared.publish_startup(world, domains, &mut owner)
            },
        )?;
    }
    Ok(())
}

fn prepare_startup_resources(
    world: &mut World,
    content: ResidencyInput<'_>,
) -> Result<(), HostFault> {
    world.register_component::<AssetServer>();
    world.register_component::<ProjectSceneResidency>();
    world.register_component::<crate::project_content::ProjectDomainAssets>();
    if matches!(content, ResidencyInput::Source(_)) {
        world.register_component::<ProjectAssetDatabase>();
    }
    world.flush();
    let unsupported = nara_ecs::__private::validate_resource_insertion::<AssetServer>(world)
        .is_err()
        || nara_ecs::__private::validate_resource_insertion::<ProjectSceneResidency>(world)
            .is_err()
        || nara_ecs::__private::validate_resource_insertion::<
            crate::project_content::ProjectDomainAssets,
        >(world)
        .is_err()
        || matches!(content, ResidencyInput::Source(_))
            && nara_ecs::__private::validate_resource_insertion::<ProjectAssetDatabase>(world)
                .is_err()
        || content.tileset_count() != 0
            && nara_ecs::__private::validate_resource_scope::<TileSetAssets>(world).is_err();
    if unsupported {
        return Err(failure(
            "project.scene.residency-lifecycle-unsupported",
            "Scene residency owners cannot run lifecycle effects during commit",
        ));
    }
    Ok(())
}

struct PreparedResidency {
    bindings: SceneAssetBindings,
    staged_server: AssetServer,
    images: ImagePublications,
    inputs: Vec<TileSetPublicationInput>,
    retirements: Vec<TileSetContentReceipt>,
    tileset_slots: Vec<AssetMutationBaseline<TileSet>>,
    catalog: PreparedCatalog,
}

impl PreparedResidency {
    fn publish_startup(
        mut self,
        world: &mut World,
        domains: &crate::project_content::ProjectDomainAssets,
        owner: &mut TileSetAssets,
    ) -> Result<(), HostFault> {
        // Keep every catalogue copy and its reservation in this aggregate through refusal.
        // The closure must capture this complete owner, rather than moving individual fields.
        let inputs = std::mem::take(&mut self.inputs);
        let tilesets = owner
            .prepare_publications(
                inputs,
                &self.retirements,
                &self.staged_server,
                self.catalog
                    .asset_resolution()
                    .map(|resolver| resolver.database()),
                world.resource::<Assets<TileSet>>(),
                world.resource::<AssetStates>(),
            )
            .map_err(tileset_failure)?;
        self.commit_startup(world, domains, Some(tilesets));
        Ok(())
    }

    fn commit_startup(
        self,
        world: &mut World,
        domains: &crate::project_content::ProjectDomainAssets,
        tilesets: Option<PreparedTileSetPublications<'_>>,
    ) {
        let Self {
            catalog,
            bindings,
            staged_server,
            images,
            inputs,
            retirements,
            tileset_slots,
        } = self;
        drop((bindings, inputs, retirements, tileset_slots));
        world.insert_resource(staged_server);
        let (images, tilesets) = replacement::commit_publications(world, images, tilesets);
        let authority = catalog.commit(world);
        world.insert_resource(domains.clone());
        world.insert_resource(ProjectSceneResidency {
            images,
            tilesets,
            authority,
        });
    }

    fn prepare(
        world: &World,
        content: ResidencyInput<'_>,
        current: Option<&ProjectSceneResidency>,
    ) -> Result<Self, HostFault> {
        if let Some(current) = current {
            current.authority.validate(content)?;
        }
        replacement::validate_native_resources(world, content, current)?;
        // Admission precedes all vectors, indices, catalogue copies and retained Image wrappers.
        let catalog = PreparedCatalog::prepare(world, content, current)?;
        let mut bindings = SceneAssetBindings::from_world(world);
        let owned_images = current.map_or(&[][..], |current| current.images.as_slice());
        let no_content = owned_images.is_empty() && content.image_count() == 0;
        let images = if no_content {
            // A data-only replacement has no Image resource dependency.
            ImagePublications::empty()
        } else {
            ImagePublications::prepare(
                content,
                owned_images,
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
                catalog.runtime_lease(),
            )?
        };
        let owned_tilesets = current.map_or(&[][..], |current| current.tilesets.as_slice());
        let old_tilesets = owned_tilesets
            .iter()
            .map(|receipt| (receipt.handle().id(), receipt))
            .collect::<BTreeMap<_, _>>();
        let mut inputs = Vec::with_capacity(content.tileset_count());
        let mut tileset_slots = Vec::with_capacity(content.tileset_count() + owned_tilesets.len());
        let mut next_ids = BTreeSet::new();
        let mut insert = |handle: nara_asset::Handle<TileSet>,
                          mut input: TileSetPublicationInput| {
            if let Some(receipt) = old_tilesets.get(&handle.id()) {
                input = input.with_receipt((*receipt).clone());
            }
            if !next_ids.insert(handle.id()) {
                return Err(binding_changed());
            }
            tileset_slots.push(
                world
                    .resource::<Assets<TileSet>>()
                    .prepare_mutation(handle, world.resource::<AssetStates>())
                    .map_err(|_| tileset_failure(TileSetAssetError::Superseded))?,
            );
            inputs.push(input);
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
                    let image_record = snapshot
                        .asset_resolution()
                        .resolve_ref(&content.source().image)
                        .map_err(|_| resources_missing())?;
                    let image_product = nara_asset::AssetProductRef::new(
                        image_record.stable_id(),
                        nara_asset::ImportedProductId::PRIMARY,
                    )
                    .map_err(|_| resources_missing())?;
                    let (image_handle, image) = images
                        .image_for_product(image_product)
                        .ok_or_else(resources_missing)?;
                    let record = AssetRecord::new(
                        content.stable_id(),
                        content.path().clone(),
                        tileset_kind(),
                    );
                    insert(
                        handle,
                        TileSetPublicationInput::from_source(
                            handle,
                            record,
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
                        .image_for_product(descriptor.image_product())
                        .ok_or_else(resources_missing)?;
                    insert(
                        handle,
                        TileSetPublicationInput::from_product(
                            handle,
                            descriptor,
                            image_handle,
                            image,
                        ),
                    )?;
                }
            }
        }
        let retirements: Vec<_> = owned_tilesets
            .iter()
            .filter(|receipt| !next_ids.contains(&receipt.handle().id()))
            .cloned()
            .collect();
        for receipt in &retirements {
            tileset_slots.push(
                world
                    .resource::<Assets<TileSet>>()
                    .prepare_mutation(receipt.handle(), world.resource::<AssetStates>())
                    .map_err(|_| tileset_failure(TileSetAssetError::Superseded))?,
            );
        }
        let staged_server = bindings.staged_server().clone();
        Ok(Self {
            bindings,
            staged_server,
            images,
            inputs,
            retirements,
            tileset_slots,
            catalog,
        })
    }
}

/// The admitted Host owns this synchronous convenience wrapper. Managed travel retains the same
/// preparation token while independent native domains retire and reconcile their state.
pub(super) fn replace_scene<F>(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    content: &PreparedSceneContent,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    match PreparedProjectSceneReplacement::prepare(
        world,
        expected,
        content,
        &SceneNativeRetirementClaims::default(),
        limits,
        additional_retirements,
        configure,
    ) {
        Ok(prepared) => match prepared.commit(world) {
            Ok(report) => report,
            Err((prepared, report)) => match prepared.discard(world) {
                Ok(()) => report,
                Err(_prepared) => {
                    let mut report = report;
                    report.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::new("project.scene.candidate-cleanup-incomplete")
                            .expect("engine diagnostic code is valid"),
                        SafeSummary::new("Rejected Scene candidate could not be safely discarded")
                            .expect("static summary"),
                    ));
                    report
                }
            },
        },
        Err(report) => report,
    }
}

fn binding_changed() -> HostFault {
    failure(
        "project.scene.asset-identity-conflict",
        "Prepared Scene asset identity conflicts with the runtime",
    )
}

fn tileset_kind() -> AssetSourceKind {
    AssetSourceKind::Other(String::from("tileset"))
}
fn tileset_failure(error: TileSetAssetError) -> HostFault {
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
