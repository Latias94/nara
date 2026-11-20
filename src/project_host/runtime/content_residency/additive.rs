//! Activation and retirement of the current Host's deferred companion Scenes.

#[cfg(feature = "runtime-2d")]
use std::collections::BTreeSet;

#[cfg(feature = "runtime-2d")]
use nara_asset::{AssetEvents, AssetStates, Assets, Handle};
use nara_asset::{AssetProductRef, ImportedProductId};
use nara_diagnostic::DiagnosticReport;
#[cfg(feature = "runtime-2d")]
use nara_image::ImageAsset;
use nara_scene::{SceneOccurrenceKey, retire_and_despawn_scene_instance};
#[cfg(feature = "runtime-2d")]
use nara_tilemap::{TileSet, TileSetContentReceipt};

use super::*;
#[cfg(feature = "runtime-2d")]
use super::{
    images::{ImageChange, ImagePublications},
    materials::{MaterialChange, MaterialPublications},
    tilesets::{TileSetChange, TileSetChanges},
};
use crate::startup_scene::SceneSource;

#[derive(Default)]
pub(super) struct Additions {
    pub(super) active: Vec<ActiveAddition>,
    #[cfg(feature = "runtime-2d")]
    pub(super) stock: StockResidency,
    #[cfg(feature = "runtime-2d")]
    pub(super) catalog: Vec<AssetRecord>,
    pub(super) revision: u64,
    pub(super) retiring: bool,
}

pub(super) struct ActiveAddition {
    pub(super) receipt: SpawnedSceneInstance,
    #[cfg(feature = "runtime-2d")]
    stock: StockUses,
    _authority: ResidencyAuthority,
    _source: SceneSource,
}

#[cfg(feature = "runtime-2d")]
struct StockUses {
    images: Vec<AssetProductRef>,
    materials: Vec<AssetProductRef>,
    tilesets: Vec<Handle<TileSet>>,
}

impl Additions {
    fn next_revision(&self) -> Result<u64, HostFault> {
        if self.retiring {
            return Err(failure(
                "project.scene.additions-retiring",
                "Managed travel is retiring the captured project Scene additions",
            ));
        }
        self.revision.checked_add(1).ok_or_else(|| {
            failure(
                "project.scene.addition-revision-exhausted",
                "Project Scene additions cannot admit another revision",
            )
        })
    }
}

pub(in crate::project_host::runtime) fn admits_processed_addition(
    residency: &ProjectSceneResidency,
    current: &CurrentSceneOwner,
    product: AssetProductRef,
) -> bool {
    let Some(root) = current.hosted_runtime_scene() else {
        return false;
    };
    let (base, deferred_scene_products) = match &residency.authority {
        #[cfg(feature = "runtime-2d")]
        ResidencyAuthority::Source { .. } => return false,
        ResidencyAuthority::Processed {
            base,
            deferred_scene_products,
            ..
        } => (base, deferred_scene_products),
    };
    product.product() == ImportedProductId::PRIMARY
        && base.same_base(root.base())
        && deferred_scene_products.binary_search(&product).is_ok()
}

pub(crate) fn has_active_project_scene_addition(world: &World) -> bool {
    world
        .get_resource::<ProjectSceneResidency>()
        .is_some_and(ProjectSceneResidency::has_addition)
}

pub(in crate::project_host::runtime) fn activate_addition(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    content: &PreparedSceneContent,
    occurrence: &SceneOccurrenceKey,
) -> Result<SceneSpawnReport, SceneSpawnReport> {
    nara_hierarchy::__private::prepare_mutation_baseline(world);
    let bindings = current_scene_bindings(world, expected)?;
    if !content.shares_component_bindings(&bindings) {
        return Err(rejected(failure(
            "project.scene.schema-mismatch",
            "Prepared Scene uses another schema authority",
        )));
    }
    let residency = world
        .get_resource::<ProjectSceneResidency>()
        .ok_or_else(|| rejected(resources_missing()))?;
    let revision = residency.additions.next_revision().map_err(rejected)?;
    let input = ResidencyInput::from(content);
    residency.authority.validate(input).map_err(rejected)?;
    if input
        .domain_assets()
        .contains_additive_unsupported_collections()
    {
        return Err(rejected(failure(
            "project.scene.addition-domain-unsupported",
            #[cfg(feature = "runtime-2d")]
            "Additive Scene residency supports Image, Material and TileSet asset closures",
            #[cfg(not(feature = "runtime-2d"))]
            "The selected asset collections are unavailable for additive Scene residency",
        )));
    }
    #[cfg(feature = "runtime-2d")]
    if let ResidencyInput::Source(snapshot) = input {
        let database = validate_catalogue(world).map_err(rejected)?;
        let incoming = snapshot.asset_database();
        if residency.owned_catalog_records().any(|record| {
            (incoming.record_for_path(record.path()).is_some()
                || incoming.record_for_stable_id(record.stable_id()).is_some())
                && database.record_for_path(record.path()) != Some(record)
        }) {
            return Err(rejected(catalog_changed()));
        }
    }
    #[cfg(feature = "runtime-2d")]
    let (material_change, material_custody) = {
        stock::validate_resources(world, input, Some(residency)).map_err(rejected)?;
        let change = MaterialChange::Add(residency);
        let custody = MaterialPublications::reserve(input, change).map_err(rejected)?;
        (change, custody)
    };
    let catalog = PreparedCatalog::prepare(
        world,
        input,
        #[cfg(feature = "runtime-2d")]
        None,
        Some(residency),
    )
    .map_err(rejected)?;
    let reservations = SceneAssetReservations::from_world(world);
    #[cfg(feature = "runtime-2d")]
    let (reservations, stock, staged_server) = {
        let mut reservations = reservations;
        let images = stock::prepare_images(
            world,
            input,
            ImageChange::Add(residency),
            &mut reservations,
            &catalog,
        )
        .map_err(rejected)?;
        let tilesets = tilesets::prepare(
            world,
            input,
            TileSetChange::Add(residency),
            &mut reservations,
            &images,
        )
        .map_err(rejected)?;
        if !tilesets.inputs.is_empty()
            && nara_ecs::__private::validate_resource_scope::<TileSetAssets>(world).is_err()
        {
            return Err(rejected(lifecycle_unsupported()));
        }
        let materials = MaterialPublications::prepare(
            world,
            input,
            material_change,
            &mut reservations,
            &images,
            material_custody,
        )
        .map_err(rejected)?;
        let staged_server = reservations.staged_server().clone();
        let stock = PreparedStock {
            images,
            materials,
            tilesets,
        };
        (reservations, stock, staged_server)
    };
    let scene = match content {
        #[cfg(feature = "runtime-2d")]
        PreparedSceneContent::Source(snapshot) => bindings.prepare_scene_with_reservations(
            world,
            snapshot.compiled_candidate(),
            occurrence,
            Some(snapshot.asset_resolution()),
            reservations,
        ),
        PreparedSceneContent::Processed(content) => bindings
            .prepare_runtime_scene_with_reservations(
                world,
                content.scene.candidate(),
                occurrence,
                reservations,
            ),
    }?;
    let candidate = PreparedAddition {
        scene,
        #[cfg(feature = "runtime-2d")]
        stock,
        catalog,
        source: content.scene_source(),
        revision,
    };
    #[cfg(feature = "runtime-2d")]
    if !candidate.stock.tilesets.inputs.is_empty() {
        let mut candidate = candidate;
        return world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
            let inputs = std::mem::take(&mut candidate.stock.tilesets.inputs);
            let prepared = stock::prepare_tilesets(
                world,
                &mut owner,
                inputs,
                &[],
                &staged_server,
                candidate
                    .catalog
                    .asset_resolution()
                    .map(|resolver| resolver.database()),
            )
            .map_err(rejected)?;
            candidate.commit(world, Some(prepared))
        });
    }
    candidate.commit(
        world,
        #[cfg(feature = "runtime-2d")]
        None,
    )
}

pub(super) struct PreparedAddition {
    scene: nara_scene::PreparedScene,
    #[cfg(feature = "runtime-2d")]
    stock: PreparedStock,
    catalog: PreparedCatalog,
    source: SceneSource,
    revision: u64,
}

impl PreparedAddition {
    fn commit(
        self,
        world: &mut World,
        #[cfg(feature = "runtime-2d")] tilesets: Option<PreparedTileSetPublications<'_>>,
    ) -> Result<SceneSpawnReport, SceneSpawnReport> {
        self.catalog.validate(world).map_err(rejected)?;
        #[cfg(feature = "runtime-2d")]
        let (images, materials, uses, new_handles) = {
            let PreparedStock {
                images,
                materials,
                tilesets:
                    TileSetChanges {
                        uses, new_handles, ..
                    },
            } = self.stock;
            stock::validate_images(world, &images).map_err(rejected)?;
            materials.validate(world).map_err(rejected)?;
            let uses = StockUses {
                images: images.additive_products(),
                materials: materials.products(),
                tilesets: uses,
            };
            (images, materials, uses, new_handles)
        };
        let report = self.scene.commit(world);
        if let Some(receipt) = &report.instance {
            #[cfg(feature = "runtime-2d")]
            let stock = stock::commit_publications(world, images, tilesets, materials);
            let authority = self.catalog.commit(world);
            #[cfg(feature = "runtime-2d")]
            let mut authority = authority;
            let mut residency = world.resource_mut::<ProjectSceneResidency>();
            let additions = &mut residency.additions;
            #[cfg(feature = "runtime-2d")]
            {
                if let ResidencyAuthority::Source { catalog, .. } = &mut authority {
                    additions.catalog.append(catalog);
                }
                additions.stock.images.extend(stock.images);
                additions.stock.materials.append(stock.materials);
                additions.stock.tilesets.extend(
                    stock
                        .tilesets
                        .into_iter()
                        .filter(|receipt| new_handles.contains(&receipt.handle().id())),
                );
            }
            additions.active.push(ActiveAddition {
                receipt: receipt.clone(),
                #[cfg(feature = "runtime-2d")]
                stock: uses,
                _authority: authority,
                _source: self.source,
            });
            additions.revision = self.revision;
        }
        Ok(report)
    }
}

/// Unloads exactly the active additive Scene returned by its Host's activation operation.
///
/// Call at an exclusive simulation boundary. Other Scenes and their shared Images stay live.
/// Previously retained render inputs keep their original pixels. A stale or foreign
/// receipt, unavailable publication resources, unsafe hierarchy teardown or overwritten
/// publication refuses before retirement.
pub fn unload_project_scene_addition(
    world: &mut World,
    expected: &SpawnedSceneInstance,
) -> Result<usize, DiagnosticReport> {
    nara_hierarchy::__private::prepare_mutation_baseline(world);
    let additions = &world
        .get_resource::<ProjectSceneResidency>()
        .ok_or_else(addition_mismatch)?
        .additions;
    let index = additions
        .active
        .iter()
        .position(|active| active.receipt == *expected)
        .ok_or_else(addition_mismatch)?;
    let revision = additions
        .next_revision()
        .map_err(|fault| fault.diagnostics)?;
    let prepared = PreparedUnload {
        #[cfg(feature = "runtime-2d")]
        retirement: prepare_retirement(world, additions, index)
            .map_err(|fault| fault.diagnostics)?,
        index,
        revision,
    };
    #[cfg(feature = "runtime-2d")]
    if !prepared.retirement.tilesets.is_empty() {
        stock::validate_tileset_resources(world).map_err(|fault| fault.diagnostics)?;
        if nara_ecs::__private::validate_resource_scope::<TileSetAssets>(world).is_err() {
            return Err(lifecycle_unsupported().diagnostics);
        }
        return world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
            let publications = stock::prepare_tilesets(
                world,
                &mut owner,
                Vec::new(),
                &prepared.retirement.tilesets,
                world.resource::<AssetServer>(),
                world.get_resource::<ProjectAssetDatabase>(),
            )
            .map_err(|fault| fault.diagnostics)?;
            prepared.commit(world, expected, Some(publications))
        });
    }
    prepared.commit(
        world,
        expected,
        #[cfg(feature = "runtime-2d")]
        None,
    )
}

struct PreparedUnload {
    #[cfg(feature = "runtime-2d")]
    retirement: StockRetirement,
    index: usize,
    revision: u64,
}

impl PreparedUnload {
    fn commit(
        self,
        world: &mut World,
        expected: &SpawnedSceneInstance,
        #[cfg(feature = "runtime-2d")] publications: Option<PreparedTileSetPublications<'_>>,
    ) -> Result<usize, DiagnosticReport> {
        #[cfg(feature = "runtime-2d")]
        let StockRetirement {
            images,
            materials,
            tilesets,
            catalog,
        } = self.retirement;
        #[cfg(feature = "runtime-2d")]
        let (retiring_images, retiring_materials, retiring_tilesets) = (
            images.retiring_products().collect::<BTreeSet<_>>(),
            materials.retiring_products().collect::<BTreeSet<_>>(),
            tilesets
                .iter()
                .map(|receipt| receipt.handle().id())
                .collect::<BTreeSet<_>>(),
        );
        let count = if nara_scene::__private::scene_instance_retired(world, expected) {
            0
        } else {
            retire_and_despawn_scene_instance(world, expected)?
        };
        #[cfg(feature = "runtime-2d")]
        {
            stock::commit_publications(world, images, publications, materials);
            if !catalog.is_empty() {
                let mut database = world.resource_mut::<ProjectAssetDatabase>();
                for record in &catalog {
                    assert!(database.remove_exact(record));
                }
            }
        }
        let mut residency = world.resource_mut::<ProjectSceneResidency>();
        let additions = &mut residency.additions;
        additions.active.remove(self.index);
        #[cfg(feature = "runtime-2d")]
        {
            additions
                .stock
                .images
                .retain(|image| !retiring_images.contains(&image.product));
            additions
                .stock
                .materials
                .retain_products(|product| !retiring_materials.contains(&product));
            additions
                .stock
                .tilesets
                .retain(|receipt| !retiring_tilesets.contains(&receipt.handle().id()));
            additions.catalog.retain(|record| !catalog.contains(record));
        }
        additions.revision = self.revision;
        Ok(count)
    }
}

fn addition_mismatch() -> DiagnosticReport {
    failure(
        "project.scene.addition-mismatch",
        "The receipt does not name an active project Scene addition",
    )
    .diagnostics
}

#[cfg(feature = "runtime-2d")]
struct StockRetirement {
    images: ImagePublications,
    materials: MaterialPublications,
    tilesets: Vec<TileSetContentReceipt>,
    catalog: Vec<AssetRecord>,
}

#[cfg(feature = "runtime-2d")]
fn prepare_retirement(
    world: &World,
    additions: &Additions,
    index: usize,
) -> Result<StockRetirement, HostFault> {
    let active = &additions.active[index];
    let remaining = additions.active[..index]
        .iter()
        .chain(&additions.active[index + 1..])
        .filter(|other| !nara_scene::__private::scene_instance_retired(world, &other.receipt))
        .collect::<Vec<_>>();
    let mut retiring_catalog = Vec::new();
    if let Some(source) = active._source.asset_resolution() {
        let database = validate_catalogue(world)?;
        for record in &additions.catalog {
            if source.database().record_for_path(record.path()) != Some(record) {
                continue;
            }
            if database.record_for_path(record.path()) != Some(record) {
                return Err(catalog_changed());
            }
            if !remaining.iter().any(|other| {
                other._source.asset_resolution().is_some_and(|resolver| {
                    resolver.database().record_for_path(record.path()) == Some(record)
                })
            }) {
                retiring_catalog.push(record.clone());
            }
        }
    }
    let materials = MaterialPublications::retire(
        additions.stock.materials.iter().filter(|material| {
            active.stock.materials.contains(&material.product)
                && !remaining
                    .iter()
                    .any(|other| other.stock.materials.contains(&material.product))
        }),
        world,
    )?;
    let tilesets = additions
        .stock
        .tilesets
        .iter()
        .filter(|receipt| {
            active.stock.tilesets.contains(&receipt.handle())
                && !remaining
                    .iter()
                    .any(|other| other.stock.tilesets.contains(&receipt.handle()))
        })
        .cloned()
        .collect();
    let mut retiring_images = additions
        .stock
        .images
        .iter()
        .filter(|image| {
            active.stock.images.contains(&image.product)
                && !remaining
                    .iter()
                    .any(|other| other.stock.images.contains(&image.product))
        })
        .peekable();
    let images = if retiring_images.peek().is_none() {
        ImagePublications::empty()
    } else {
        let states = world
            .get_resource::<AssetStates>()
            .ok_or_else(resources_missing)?;
        if !world.contains_resource::<AssetEvents>() {
            return Err(resources_missing());
        }
        ImagePublications::retire(
            retiring_images,
            world
                .get_resource::<AssetServer>()
                .ok_or_else(resources_missing)?,
            world
                .get_resource::<Assets<ImageAsset>>()
                .ok_or_else(resources_missing)?,
            states,
        )?
    };
    Ok(StockRetirement {
        images,
        materials,
        tilesets,
        catalog: retiring_catalog,
    })
}

#[cfg(feature = "runtime-2d")]
fn validate_catalogue(world: &World) -> Result<&ProjectAssetDatabase, HostFault> {
    let database = world
        .get_resource::<ProjectAssetDatabase>()
        .ok_or_else(resources_missing)?;
    if nara_ecs::__private::validate_resource_scope::<ProjectAssetDatabase>(world).is_err() {
        return Err(failure(
            "project.scene.residency-lifecycle-unsupported",
            "Scene asset catalogue cannot run lifecycle effects during publication",
        ));
    }
    Ok(database)
}

#[cfg(feature = "runtime-2d")]
fn catalog_changed() -> HostFault {
    failure(
        "project.scene.asset-catalog-changed",
        "An owned Scene asset catalog record changed before additive publication",
    )
}
