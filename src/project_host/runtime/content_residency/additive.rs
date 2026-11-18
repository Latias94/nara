//! Activation and retirement of the current Host's deferred companion Scenes.

use nara_asset::{AssetProductRef, ImportedProductId};
use nara_diagnostic::DiagnosticReport;
use nara_scene::{SceneOccurrenceKey, retire_and_despawn_scene_instance};

use super::*;
use crate::startup_scene::SceneSource;

#[derive(Default)]
pub(super) struct Additions {
    pub(super) active: Vec<ActiveAddition>,
    pub(super) images: Vec<OwnedImage>,
    pub(super) materials: MaterialResidency,
    pub(super) tilesets: Vec<TileSetContentReceipt>,
    pub(super) catalog: Vec<AssetRecord>,
    pub(super) revision: u64,
    pub(super) retiring: bool,
}

pub(super) struct ActiveAddition {
    pub(super) receipt: SpawnedSceneInstance,
    images: Vec<AssetProductRef>,
    materials: Vec<AssetProductRef>,
    tilesets: Vec<nara_asset::Handle<TileSet>>,
    _authority: ResidencyAuthority,
    source: SceneSource,
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
    let ResidencyAuthority::Processed {
        base,
        deferred_scene_products,
        ..
    } = &residency.authority
    else {
        return false;
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
            "Additive Scene residency supports Image, Material and TileSet asset closures",
        )));
    }
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
    replacement::validate_native_resources(world, input, Some(residency)).map_err(rejected)?;
    let material_change = MaterialChange::Add(residency);
    let material_custody =
        MaterialPublications::reserve(input, material_change).map_err(rejected)?;
    let catalog =
        PreparedCatalog::prepare(world, input, None, Some(residency)).map_err(rejected)?;
    let mut reservations = SceneAssetReservations::from_world(world);
    let images = prepare_images(
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
        return Err(rejected(failure(
            "project.scene.residency-lifecycle-unsupported",
            "Scene residency owners cannot run lifecycle effects during commit",
        )));
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
    let scene = match content {
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
        images,
        materials,
        catalog,
        source: content.scene_source(),
        revision,
        tileset_uses: tilesets.uses,
        new_handles: tilesets.new_handles,
    };
    if tilesets.inputs.is_empty() {
        return candidate.commit(world, None);
    }
    world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
        let prepared = owner
            .prepare_publications(
                tilesets.inputs,
                &[],
                &staged_server,
                candidate
                    .catalog
                    .asset_resolution()
                    .map(|resolver| resolver.database()),
                world.resource::<Assets<TileSet>>(),
                world.resource::<AssetStates>(),
            )
            .map_err(|error| rejected(tileset_failure(error)))?;
        candidate.commit(world, Some(prepared))
    })
}

pub(super) struct PreparedAddition {
    scene: nara_scene::PreparedScene,
    images: ImagePublications,
    materials: MaterialPublications,
    catalog: PreparedCatalog,
    source: SceneSource,
    revision: u64,
    tileset_uses: Vec<nara_asset::Handle<TileSet>>,
    new_handles: BTreeSet<nara_asset::AssetId>,
}

impl PreparedAddition {
    fn commit(
        self,
        world: &mut World,
        tilesets: Option<PreparedTileSetPublications<'_>>,
    ) -> Result<SceneSpawnReport, SceneSpawnReport> {
        self.catalog.validate(world).map_err(rejected)?;
        validate_images(world, &self.images).map_err(rejected)?;
        self.materials.validate(world).map_err(rejected)?;
        let image_products = self.images.additive_products();
        let material_products = self.materials.products();
        let report = self.scene.commit(world);
        if let Some(receipt) = &report.instance {
            let (images, receipts, materials) =
                replacement::commit_publications(world, self.images, tilesets, self.materials);
            let mut authority = self.catalog.commit(world);
            let mut residency = world.resource_mut::<ProjectSceneResidency>();
            let additions = &mut residency.additions;
            if let ResidencyAuthority::Source { catalog, .. } = &mut authority {
                additions.catalog.append(catalog);
            }
            additions.images.extend(images);
            additions.materials.append(materials);
            additions.tilesets.extend(
                receipts
                    .into_iter()
                    .filter(|receipt| self.new_handles.contains(&receipt.handle().id())),
            );
            additions.active.push(ActiveAddition {
                receipt: receipt.clone(),
                images: image_products,
                materials: material_products,
                tilesets: self.tileset_uses,
                _authority: authority,
                source: self.source,
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
/// receipt, unsafe hierarchy teardown or overwritten publication refuses before mutation.
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
    let (retirement, materials, tilesets, catalog) =
        prepare_retirement(world, additions, index).map_err(|fault| fault.diagnostics)?;
    if !tilesets.is_empty()
        && nara_ecs::__private::validate_resource_scope::<TileSetAssets>(world).is_err()
    {
        return Err(failure(
            "project.scene.residency-lifecycle-unsupported",
            "Scene residency owners cannot run lifecycle effects during commit",
        )
        .diagnostics);
    }
    let prepared = PreparedUnload {
        images: retirement,
        materials,
        catalog,
        index,
        revision,
    };
    if tilesets.is_empty() {
        return prepared.commit(world, expected, None, &[]);
    }
    world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
        let publications = owner
            .prepare_publications(
                Vec::new(),
                &tilesets,
                world.resource::<AssetServer>(),
                world.get_resource::<ProjectAssetDatabase>(),
                world.resource::<Assets<TileSet>>(),
                world.resource::<AssetStates>(),
            )
            .map_err(|error| tileset_failure(error).diagnostics)?;
        prepared.commit(world, expected, Some(publications), &tilesets)
    })
}

struct PreparedUnload {
    images: ImagePublications,
    materials: MaterialPublications,
    catalog: Vec<AssetRecord>,
    index: usize,
    revision: u64,
}

impl PreparedUnload {
    fn commit(
        self,
        world: &mut World,
        expected: &SpawnedSceneInstance,
        tileset_publications: Option<PreparedTileSetPublications<'_>>,
        tilesets: &[TileSetContentReceipt],
    ) -> Result<usize, DiagnosticReport> {
        let Self {
            images: retirement,
            materials,
            catalog,
            index,
            revision,
        } = self;
        let retiring_tilesets = tilesets
            .iter()
            .map(|receipt| receipt.handle().id())
            .collect::<BTreeSet<_>>();
        let retiring_images = retirement.retiring_products().collect::<BTreeSet<_>>();
        let retiring_materials = materials.retiring_products().collect::<BTreeSet<_>>();
        let count = if nara_scene::__private::scene_instance_retired(world, expected) {
            0
        } else {
            retire_and_despawn_scene_instance(world, expected)?
        };
        replacement::commit_publications(world, retirement, tileset_publications, materials);
        if !catalog.is_empty() {
            let mut database = world.resource_mut::<ProjectAssetDatabase>();
            for record in &catalog {
                assert!(database.remove_exact(record));
            }
        }
        let mut residency = world.resource_mut::<ProjectSceneResidency>();
        let additions = &mut residency.additions;
        additions.active.remove(index);
        additions
            .images
            .retain(|image| !retiring_images.contains(&image.product));
        additions
            .materials
            .retain_products(|product| !retiring_materials.contains(&product));
        additions
            .tilesets
            .retain(|receipt| !retiring_tilesets.contains(&receipt.handle().id()));
        additions.catalog.retain(|record| !catalog.contains(record));
        additions.revision = revision;
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

fn prepare_retirement(
    world: &World,
    additions: &Additions,
    index: usize,
) -> Result<
    (
        ImagePublications,
        MaterialPublications,
        Vec<TileSetContentReceipt>,
        Vec<AssetRecord>,
    ),
    HostFault,
> {
    let active = &additions.active[index];
    let remaining = additions.active[..index]
        .iter()
        .chain(&additions.active[index + 1..])
        .filter(|other| !nara_scene::__private::scene_instance_retired(world, &other.receipt))
        .collect::<Vec<_>>();
    let mut retiring_catalog = Vec::new();
    if let Some(source) = active.source.asset_resolution() {
        let database = validate_catalogue(world)?;
        for record in &additions.catalog {
            if source.database().record_for_path(record.path()) != Some(record) {
                continue;
            }
            if database.record_for_path(record.path()) != Some(record) {
                return Err(catalog_changed());
            }
            if !remaining.iter().any(|other| {
                other.source.asset_resolution().is_some_and(|resolver| {
                    resolver.database().record_for_path(record.path()) == Some(record)
                })
            }) {
                retiring_catalog.push(record.clone());
            }
        }
    }
    let materials = MaterialPublications::retire(
        additions.materials.iter().filter(|material| {
            active.materials.contains(&material.product)
                && !remaining
                    .iter()
                    .any(|other| other.materials.contains(&material.product))
        }),
        world,
    )?;
    let tilesets = additions
        .tilesets
        .iter()
        .filter(|receipt| {
            active.tilesets.contains(&receipt.handle())
                && !remaining
                    .iter()
                    .any(|other| other.tilesets.contains(&receipt.handle()))
        })
        .cloned()
        .collect();
    let mut retiring_images = additions
        .images
        .iter()
        .filter(|image| {
            active.images.contains(&image.product)
                && !remaining
                    .iter()
                    .any(|other| other.images.contains(&image.product))
        })
        .peekable();
    if retiring_images.peek().is_none() {
        return Ok((
            ImagePublications::empty(),
            materials,
            tilesets,
            retiring_catalog,
        ));
    }
    let states = world
        .get_resource::<AssetStates>()
        .ok_or_else(resources_missing)?;
    if !world.contains_resource::<AssetEvents>() {
        return Err(resources_missing());
    }
    let retirement = ImagePublications::retire(
        retiring_images,
        world
            .get_resource::<AssetServer>()
            .ok_or_else(resources_missing)?,
        world
            .get_resource::<Assets<ImageAsset>>()
            .ok_or_else(resources_missing)?,
        states,
    )?;
    Ok((retirement, materials, tilesets, retiring_catalog))
}

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

fn catalog_changed() -> HostFault {
    failure(
        "project.scene.asset-catalog-changed",
        "An owned Scene asset catalog record changed before additive publication",
    )
}

fn validate_images(world: &World, images: &ImagePublications) -> Result<(), HostFault> {
    if images.is_empty() {
        return Ok(());
    }
    if !world.contains_resource::<AssetEvents>() {
        return Err(resources_missing());
    }
    images.validate(
        world
            .get_resource::<AssetServer>()
            .ok_or_else(resources_missing)?,
        world
            .get_resource::<Assets<ImageAsset>>()
            .ok_or_else(resources_missing)?,
        world
            .get_resource::<AssetStates>()
            .ok_or_else(resources_missing)?,
    )
}
