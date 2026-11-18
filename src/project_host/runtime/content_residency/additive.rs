//! Activation and retirement of the current Host's deferred companion Scene.

use nara_asset::{AssetProductRef, ImportedProductId};
use nara_diagnostic::DiagnosticReport;
use nara_scene::{SceneOccurrenceKey, retire_and_despawn_scene_instance};

use super::*;
use crate::startup_scene::SceneSource;

pub(super) struct ActiveAddition {
    receipt: SpawnedSceneInstance,
    images: Vec<OwnedImage>,
    authority: ResidencyAuthority,
    _source: SceneSource,
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

pub(super) fn replacement_blocked() -> HostFault {
    failure(
        "project.scene.addition-active",
        "Unload the active project Scene addition before replacing the primary Scene",
    )
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
    if residency.has_addition() {
        return Err(rejected(replacement_blocked()));
    }
    let input = ResidencyInput::from(content);
    residency.authority.validate(input).map_err(rejected)?;
    if input.tileset_count() != 0 || input.domain_assets().contains_non_image_collections() {
        return Err(rejected(failure(
            "project.scene.addition-domain-unsupported",
            "Additive Scene residency requires an Image-only asset closure",
        )));
    }
    if matches!(input, ResidencyInput::Source(_)) {
        validate_catalogue(world).map_err(rejected)?;
    }
    let shared = if input.image_count() == 0 {
        &[][..]
    } else {
        residency.images.as_slice()
    };
    let catalog =
        PreparedCatalog::prepare(world, input, None, Some(shared.len())).map_err(rejected)?;
    let mut reservations = SceneAssetReservations::from_world(world);
    let images = prepare_images(
        world,
        input,
        ImageChange::Add(shared),
        &mut reservations,
        &catalog,
    )
    .map_err(rejected)?;
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
    catalog.validate(world).map_err(rejected)?;
    validate_images(world, &images).map_err(rejected)?;
    let report = scene.commit(world);
    if let Some(receipt) = &report.instance {
        let images = replacement::commit_images(world, images);
        let authority = catalog.commit(world);
        world.resource_mut::<ProjectSceneResidency>().addition = Some(ActiveAddition {
            receipt: receipt.clone(),
            images,
            authority,
            _source: content.scene_source(),
        });
    }
    Ok(report)
}

/// Unloads exactly the active additive Scene returned by its Host's activation operation.
///
/// Call at an exclusive simulation boundary. The primary Scene and any Images it owns stay
/// live. Previously retained render inputs keep their original pixels. A stale or foreign
/// receipt, unsafe hierarchy teardown or overwritten publication refuses before mutation.
pub fn unload_project_scene_addition(
    world: &mut World,
    expected: &SpawnedSceneInstance,
) -> Result<usize, DiagnosticReport> {
    nara_hierarchy::__private::prepare_mutation_baseline(world);
    let active = world
        .get_resource::<ProjectSceneResidency>()
        .and_then(|residency| residency.addition.as_ref())
        .filter(|active| active.receipt == *expected)
        .ok_or_else(|| {
            failure(
                "project.scene.addition-mismatch",
                "The receipt does not name the original active project Scene addition",
            )
            .diagnostics
        })?;
    let retirement = prepare_retirement(world, active).map_err(|fault| fault.diagnostics)?;
    let count = if nara_scene::__private::scene_instance_retired(world, expected) {
        0
    } else {
        retire_and_despawn_scene_instance(world, expected)?
    };
    replacement::commit_images(world, retirement);
    let active = world
        .resource_mut::<ProjectSceneResidency>()
        .addition
        .take()
        .expect("the retiring addition is retained until its entities and Images are retired");
    if let ResidencyAuthority::Source { catalog, .. } = &active.authority {
        let mut database = world.resource_mut::<ProjectAssetDatabase>();
        for record in catalog {
            assert!(database.remove_exact(record));
        }
    }
    Ok(count)
}

fn prepare_retirement(
    world: &World,
    active: &ActiveAddition,
) -> Result<ImagePublications, HostFault> {
    if let ResidencyAuthority::Source { catalog, .. } = &active.authority {
        let database = validate_catalogue(world)?;
        if catalog
            .iter()
            .any(|record| database.record_for_path(record.path()) != Some(record))
        {
            return Err(failure(
                "project.scene.asset-catalog-changed",
                "Scene asset catalogue changed before additive retirement",
            ));
        }
    }
    if active.images.is_empty() {
        return Ok(ImagePublications::empty());
    }
    let states = world
        .get_resource::<AssetStates>()
        .ok_or_else(resources_missing)?;
    if !world.contains_resource::<AssetEvents>() {
        return Err(resources_missing());
    }
    ImagePublications::retire(
        &active.images,
        world
            .get_resource::<AssetServer>()
            .ok_or_else(resources_missing)?,
        world
            .get_resource::<Assets<ImageAsset>>()
            .ok_or_else(resources_missing)?,
        states,
    )
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
