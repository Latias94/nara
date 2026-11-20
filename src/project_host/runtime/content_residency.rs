//! Concrete CPU content ownership for the committed project Scene group.

mod additive;
#[cfg(feature = "runtime-2d")]
mod catalog;
#[cfg(feature = "runtime-2d")]
mod images;
mod instances;
#[cfg(feature = "runtime-2d")]
mod materials;
mod metadata;
mod replacement;
#[cfg(feature = "runtime-2d")]
mod stock;
#[cfg(feature = "runtime-2d")]
mod tilesets;
pub(super) use additive::activate_addition;
pub(crate) use additive::has_active_project_scene_addition;
pub use additive::unload_project_scene_addition;
pub use instances::{inspect_current_scene_dependency, instantiate_current_scene_dependency};
pub(super) use replacement::StagedProjectSceneReplacement;

use nara_asset::AssetServer;
#[cfg(feature = "runtime-2d")]
use nara_asset::{AssetRecord, ProjectAssetDatabase};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
#[cfg(feature = "runtime-2d")]
use nara_ecs::Mut;
use nara_ecs::{Resource, World};
use nara_identity::SpawnedSceneInstance;
use nara_scene::{
    SceneSpawnReport,
    advanced::{
        SceneAssetReservations, SceneNativeRetirementClaims, SceneProductOverlayWriter,
        SceneProductRetirement, SceneProductTransactionLimits, StagedSceneReplacement,
    },
};
#[cfg(feature = "runtime-2d")]
use nara_tilemap::{PreparedTileSetPublications, TileSetAssets};

use super::content::PreparedSceneContent;
use super::{HostFault, single_error};
#[cfg(feature = "runtime-2d")]
use crate::project_content::ProjectContentSnapshot;
use crate::{
    project_assets::ProjectDomainAssets,
    runtime_content::ProcessedSceneContent,
    startup_scene::{CurrentSceneOwner, current_scene_bindings},
};
use metadata::{PreparedCatalog, ResidencyAuthority};
#[cfg(feature = "runtime-2d")]
use stock::{PreparedStock, StockResidency};

#[derive(Resource)]
pub(super) struct ProjectSceneResidency {
    additions: additive::Additions,
    #[cfg(feature = "runtime-2d")]
    stock: StockResidency,
    // Resource values and their receipts retire before their metadata reservation and base.
    authority: ResidencyAuthority,
}

impl ProjectSceneResidency {
    pub(super) fn has_addition(&self) -> bool {
        !self.additions.active.is_empty()
    }

    #[cfg(feature = "runtime-2d")]
    fn owned_catalog_records(&self) -> impl Iterator<Item = &AssetRecord> {
        let primary = match &self.authority {
            ResidencyAuthority::Source { catalog, .. } => catalog.as_slice(),
            ResidencyAuthority::Processed { .. } => &[],
        };
        primary.iter().chain(&self.additions.catalog)
    }

    pub(super) fn admits_processed_addition(
        &self,
        current: &CurrentSceneOwner,
        product: nara_asset::AssetProductRef,
    ) -> bool {
        additive::admits_processed_addition(self, current, product)
    }
}

#[derive(Clone, Copy)]
enum ResidencyInput<'a> {
    #[cfg(feature = "runtime-2d")]
    Source(&'a ProjectContentSnapshot),
    Processed(&'a ProcessedSceneContent),
}

impl<'a> From<&'a PreparedSceneContent> for ResidencyInput<'a> {
    fn from(content: &'a PreparedSceneContent) -> Self {
        match content {
            #[cfg(feature = "runtime-2d")]
            PreparedSceneContent::Source(snapshot) => Self::Source(snapshot),
            PreparedSceneContent::Processed(content) => Self::Processed(content),
        }
    }
}

impl<'a> ResidencyInput<'a> {
    fn domain_assets(self) -> &'a ProjectDomainAssets {
        match self {
            #[cfg(feature = "runtime-2d")]
            Self::Source(snapshot) => snapshot.domain_assets(),
            Self::Processed(content) => &content.domains,
        }
    }
}

/// Source startup uses the same concrete resource preparation and publication as later travel.
#[cfg(feature = "runtime-2d")]
pub(super) fn publish_startup(
    world: &mut World,
    snapshot: &ProjectContentSnapshot,
) -> Result<(), HostFault> {
    publish_startup_content(world, ResidencyInput::Source(snapshot))
}

/// Publishes into the unpublished startup World before its processed Scene is materialized.
pub(super) fn publish_processed_startup(
    world: &mut World,
    content: &ProcessedSceneContent,
) -> Result<(), HostFault> {
    publish_startup_content(world, ResidencyInput::Processed(content))
}

fn publish_startup_content(
    world: &mut World,
    content: ResidencyInput<'_>,
) -> Result<(), HostFault> {
    prepare_startup_resources(
        world,
        #[cfg(feature = "runtime-2d")]
        content,
    )?;
    if world.contains_resource::<ProjectSceneResidency>() {
        return Err(resources_missing());
    }
    let prepared = PreparedResidency::prepare(world, content, None)?;
    prepared
        .bindings
        .validate_baseline(world)
        .map_err(|_| binding_changed())?;
    prepared.catalog.validate(world)?;
    #[cfg(feature = "runtime-2d")]
    {
        prepared.stock.materials.validate(world)?;
        // This owner borrow fences capacity and claims through the same synchronous tail used
        // by Scene replacement. Startup has no CurrentSceneOwner or live Scene to retire yet.
        if prepared.stock.tilesets.has_publications() {
            return world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
                let mut prepared = prepared;
                let inputs = std::mem::take(&mut prepared.stock.tilesets.inputs);
                let tilesets = stock::prepare_tilesets(
                    world,
                    &mut owner,
                    inputs,
                    &prepared.stock.tilesets.retirements,
                    prepared.bindings.staged_server(),
                    prepared
                        .catalog
                        .asset_resolution()
                        .map(|resolver| resolver.database()),
                )?;
                prepared.commit_startup(world, content.domain_assets(), Some(tilesets));
                Ok(())
            });
        }
    }
    prepared.commit_startup(
        world,
        content.domain_assets(),
        #[cfg(feature = "runtime-2d")]
        None,
    );
    Ok(())
}

fn prepare_startup_resources(
    world: &mut World,
    #[cfg(feature = "runtime-2d")] content: ResidencyInput<'_>,
) -> Result<(), HostFault> {
    world.register_component::<AssetServer>();
    world.register_component::<ProjectSceneResidency>();
    world.register_component::<ProjectDomainAssets>();
    #[cfg(feature = "runtime-2d")]
    if matches!(content, ResidencyInput::Source(_)) {
        world.register_component::<ProjectAssetDatabase>();
    }
    world.flush();
    let unsupported = nara_ecs::__private::validate_resource_insertion::<AssetServer>(world)
        .is_err()
        || nara_ecs::__private::validate_resource_insertion::<ProjectSceneResidency>(world)
            .is_err()
        || nara_ecs::__private::validate_resource_insertion::<ProjectDomainAssets>(world).is_err();
    #[cfg(feature = "runtime-2d")]
    let unsupported = unsupported
        || matches!(content, ResidencyInput::Source(_))
            && nara_ecs::__private::validate_resource_insertion::<ProjectAssetDatabase>(world)
                .is_err()
        || content.tileset_count() != 0
            && nara_ecs::__private::validate_resource_scope::<TileSetAssets>(world).is_err();
    if unsupported {
        return Err(lifecycle_unsupported());
    }
    Ok(())
}

struct PreparedResidency {
    bindings: SceneAssetReservations,
    #[cfg(feature = "runtime-2d")]
    stock: PreparedStock,
    catalog: PreparedCatalog,
}

impl PreparedResidency {
    fn commit_startup(
        self,
        world: &mut World,
        domains: &ProjectDomainAssets,
        #[cfg(feature = "runtime-2d")] tilesets: Option<PreparedTileSetPublications<'_>>,
    ) {
        let Self {
            catalog,
            bindings,
            #[cfg(feature = "runtime-2d")]
            stock,
        } = self;
        let staged_server = bindings.staged_server().clone();
        drop(bindings);
        world.insert_resource(staged_server);
        #[cfg(feature = "runtime-2d")]
        let stock = stock::commit_publications(world, stock.images, tilesets, stock.materials);
        let authority = catalog.commit(world);
        world.insert_resource(domains.clone());
        world.insert_resource(ProjectSceneResidency {
            additions: additive::Additions::default(),
            #[cfg(feature = "runtime-2d")]
            stock,
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
        #[cfg(feature = "runtime-2d")]
        let (material_change, material_custody) = {
            stock::validate_resources(world, content, current)?;
            let change = materials::MaterialChange::Replace(current);
            let custody = materials::MaterialPublications::reserve(content, change)?;
            (change, custody)
        };
        // Admission precedes all vectors, indices, catalogue copies and retained Image wrappers.
        let catalog = PreparedCatalog::prepare(
            world,
            content,
            #[cfg(feature = "runtime-2d")]
            current,
            None,
        )?;
        let bindings = SceneAssetReservations::from_world(world);
        #[cfg(feature = "runtime-2d")]
        let (bindings, stock) = {
            let mut bindings = bindings;
            if let (Some(_), ResidencyInput::Source(snapshot)) = (current, content) {
                catalog.stage_source_relocations(snapshot, &mut bindings)?;
            }
            let images = stock::prepare_images(
                world,
                content,
                images::ImageChange::Replace(current),
                &mut bindings,
                &catalog,
            )?;
            let materials = materials::MaterialPublications::prepare(
                world,
                content,
                material_change,
                &mut bindings,
                &images,
                material_custody,
            )?;
            let tilesets = tilesets::prepare(
                world,
                content,
                tilesets::TileSetChange::Replace(current),
                &mut bindings,
                &images,
            )?;
            let stock = PreparedStock {
                images,
                materials,
                tilesets,
            };
            (bindings, stock)
        };
        Ok(Self {
            bindings,
            #[cfg(feature = "runtime-2d")]
            stock,
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
    additional_retirements: &[SceneProductRetirement<'_>],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    match StagedProjectSceneReplacement::stage(
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

fn failure(code: &'static str, summary: &'static str) -> HostFault {
    HostFault::new(single_error(code, summary))
}

fn lifecycle_unsupported() -> HostFault {
    failure(
        "project.scene.residency-lifecycle-unsupported",
        "Scene residency owners cannot run lifecycle effects during commit",
    )
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
