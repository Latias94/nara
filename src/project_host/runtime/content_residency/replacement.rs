//! Synchronous publication of a hosted primary Scene and its prepared content.

use std::borrow::Cow;

use nara_ecs::Mut;
use nara_scene::current_scene::stage_current_scene_replacement;

use super::*;

pub(in crate::project_host::runtime) fn replace_scene(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    content: &PreparedSceneContent,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[SceneProductRetirement<'_>],
    configure: impl FnOnce(&mut SceneProductOverlayWriter<'_>),
) -> SceneSpawnReport {
    try_replace(
        world,
        expected,
        content,
        limits,
        additional_retirements,
        configure,
    )
    .unwrap_or_else(|report| report)
}

fn try_replace(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    content: &PreparedSceneContent,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[SceneProductRetirement<'_>],
    configure: impl FnOnce(&mut SceneProductOverlayWriter<'_>),
) -> Result<SceneSpawnReport, SceneSpawnReport> {
    nara_scene::hierarchy::prepare_mutation_baseline(world);
    let component_bindings = current_scene_bindings(world, expected)?;
    let input = ResidencyInput::from(content);
    validate_resources(
        world,
        #[cfg(feature = "runtime-2d")]
        input,
    )
    .map_err(rejected)?;
    if !content.shares_component_bindings(&component_bindings) {
        return Err(rejected(failure(
            "project.scene.schema-mismatch",
            "Prepared Scene uses another schema authority",
        )));
    }
    world.resource_scope(|world, mut current: Mut<ProjectSceneResidency>| {
        let retirements =
            scene_retirements(world, &current, limits, additional_retirements).map_err(rejected)?;
        let prepared =
            PreparedResidency::prepare(world, input, Some(&current)).map_err(rejected)?;
        #[cfg(feature = "runtime-2d")]
        let uses_tilesets = prepared.stock.tilesets.has_publications();
        let replace =
            |world: &mut World,
             #[cfg(feature = "runtime-2d")] owner: Option<&mut TileSetAssets>| {
                let PreparedResidency {
                    catalog,
                    bindings,
                    #[cfg(feature = "runtime-2d")]
                    mut stock,
                } = prepared;
                #[cfg(feature = "runtime-2d")]
                let tilesets = owner
                    .map(|owner| {
                        stock::prepare_tilesets(
                            world,
                            owner,
                            std::mem::take(&mut stock.tilesets.inputs),
                            &stock.tilesets.retirements,
                            bindings.staged_server(),
                            catalog.asset_database(),
                        )
                    })
                    .transpose()
                    .map_err(rejected)?;
                let scene = stage_current_scene_replacement(
                    world,
                    expected,
                    Some(content.scene_source()),
                    bindings,
                    limits,
                    &retirements,
                    configure,
                )?;
                world.flush();
                let report = match catalog.validate(world).and_then(|()| {
                    #[cfg(feature = "runtime-2d")]
                    stock.validate(world, tilesets.as_ref())?;
                    Ok(())
                }) {
                    Ok(()) => scene.commit(world),
                    Err(fault) => Err((scene, rejected(fault))),
                };
                let report = match report {
                    Ok(report) => report,
                    Err((scene, mut report)) => {
                        if scene.discard(world).is_err() {
                            report.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::new("project.scene.candidate-cleanup-incomplete")
                                    .expect("engine diagnostic code is valid"),
                                SafeSummary::new(
                                    "Rejected Scene candidate could not be safely discarded",
                                )
                                .expect("static summary"),
                            ));
                        }
                        return Err(report);
                    }
                };
                #[cfg(feature = "runtime-2d")]
                let stock =
                    stock::commit_publications(world, stock.images, tilesets, stock.materials);
                let authority = catalog.commit(world);
                world.insert_resource(content.domain_assets().clone());
                Ok((
                    report,
                    ProjectSceneResidency {
                        additions: additive::Additions::default(),
                        #[cfg(feature = "runtime-2d")]
                        stock,
                        authority,
                    },
                ))
            };
        #[cfg(feature = "runtime-2d")]
        let (report, next) = if uses_tilesets {
            world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
                replace(world, Some(&mut owner))
            })
        } else {
            replace(world, None)
        }?;
        #[cfg(not(feature = "runtime-2d"))]
        let (report, next) = replace(world)?;
        *current = next;
        Ok(report)
    })
}

fn scene_retirements<'a>(
    world: &World,
    current: &'a ProjectSceneResidency,
    limits: SceneProductTransactionLimits,
    product: &'a [SceneProductRetirement<'a>],
) -> Result<Cow<'a, [SceneProductRetirement<'a>]>, HostFault> {
    if !current.has_addition() {
        return Ok(Cow::Borrowed(product));
    }
    let additions =
        current.additions.active.iter().filter(|addition| {
            !nara_scene::advanced::scene_instance_retired(world, &addition.receipt)
        });
    let entries = product.len().saturating_add(additions.clone().count());
    if entries
        > limits
            .additional_retirements()
            .get()
            .min(SceneProductTransactionLimits::MAX_ADDITIONAL_RETIREMENTS)
    {
        return Err(failure(
            "scene.product-retirement-limit-exceeded",
            "Travel exceeds its additional Scene retirement limit",
        ));
    }
    Ok(Cow::Owned(
        product
            .iter()
            .copied()
            .chain(additions.map(|addition| SceneProductRetirement::Instance(&addition.receipt)))
            .collect(),
    ))
}

fn validate_resources(
    world: &World,
    #[cfg(feature = "runtime-2d")] content: ResidencyInput<'_>,
) -> Result<(), HostFault> {
    if !world.contains_resource::<CurrentScene>()
        || !world.contains_resource::<ProjectSceneResidency>()
        || !world.contains_resource::<ProjectDomainAssets>()
    {
        return Err(resources_missing());
    }
    let unsupported = transaction::validate_resource_scope::<ProjectSceneResidency>(world).is_err()
        || transaction::validate_resource_scope::<ProjectDomainAssets>(world).is_err()
        || transaction::validate_resource_scope::<CurrentScene>(world).is_err();
    #[cfg(feature = "project-source")]
    let unsupported = if matches!(content, ResidencyInput::Source(_)) {
        if !world.contains_resource::<ProjectAssetDatabase>()
            || world
                .resource::<CurrentScene>()
                .source_content_stamp()
                .is_none()
        {
            return Err(resources_missing());
        }
        unsupported || transaction::validate_resource_scope::<ProjectAssetDatabase>(world).is_err()
    } else {
        unsupported
    };
    #[cfg(feature = "runtime-2d")]
    let unsupported = {
        let current = world.resource::<ProjectSceneResidency>();
        stock::validate_resources(world, content, Some(current))?;
        let uses_tilesets = content.tileset_count() != 0 || current.tileset_count() != 0;
        unsupported
            || uses_tilesets
                && transaction::validate_resource_scope::<TileSetAssets>(world).is_err()
    };
    if unsupported {
        return Err(lifecycle_unsupported());
    }
    Ok(())
}
