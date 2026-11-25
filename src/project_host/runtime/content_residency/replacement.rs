//! Owned CPU residency and Scene candidate for synchronous travel.

use std::borrow::Cow;

use nara_ecs::Mut;
use nara_scene::advanced::SceneNativeRetirementClaims;

use super::*;

pub(super) struct StagedProjectSceneReplacement {
    scene: Option<StagedSceneReplacement>,
    expected: SpawnedSceneInstance,
    additions_revision: u64,
    content: PreparedSceneContent,
    #[cfg(feature = "runtime-2d")]
    staged_server: AssetServer,
    #[cfg(feature = "runtime-2d")]
    stock: PreparedStock,
    catalog: PreparedCatalog,
}

impl StagedProjectSceneReplacement {
    pub(super) fn stage<F>(
        world: &mut World,
        expected: &SpawnedSceneInstance,
        content: &PreparedSceneContent,
        limits: SceneProductTransactionLimits,
        additional_retirements: &[SceneProductRetirement<'_>],
        configure: F,
    ) -> Result<Self, SceneSpawnReport>
    where
        F: FnOnce(&mut SceneProductOverlayWriter<'_>),
    {
        nara_scene::hierarchy::prepare_mutation_baseline(world);
        let component_bindings = current_scene_bindings(world, expected)?;
        validate_resources(
            world,
            #[cfg(feature = "runtime-2d")]
            content.into(),
        )
        .map_err(rejected)?;
        if !content.shares_component_bindings(&component_bindings) {
            return Err(rejected(failure(
                "project.scene.schema-mismatch",
                "Prepared Scene uses another schema authority",
            )));
        }
        world.resource_scope(|world, current: Mut<ProjectSceneResidency>| {
            let additions_revision = current.additions.revision;
            let scene_retirements =
                scene_retirements(world, &current, limits, additional_retirements)
                    .map_err(rejected)?;
            let PreparedResidency {
                // Bind the reservation first so every fallible local drops its catalogue copies
                // and publication scratch before this metadata owner is released.
                catalog,
                bindings,
                #[cfg(feature = "runtime-2d")]
                stock,
            } = PreparedResidency::prepare(world, content.into(), Some(&current))
                .map_err(rejected)?;
            #[cfg(feature = "runtime-2d")]
            let staged_server = {
                let server = bindings.staged_server();
                stock::probe_tilesets(world, &stock.tilesets, server, &catalog)
                    .map_err(rejected)?;
                server.clone()
            };
            let scene = match content {
                #[cfg(feature = "project-source")]
                PreparedSceneContent::Source(snapshot) => component_bindings
                    .stage_scene_replacement_with_claims(
                        world,
                        snapshot.compiled_candidate(),
                        Some(snapshot.asset_resolution()),
                        expected,
                        bindings,
                        &SceneNativeRetirementClaims::default(),
                        limits,
                        &scene_retirements,
                        configure,
                    ),
                PreparedSceneContent::Processed(content) => component_bindings
                    .stage_runtime_scene_replacement_with_claims(
                        world,
                        content.scene.candidate(),
                        expected,
                        bindings,
                        &SceneNativeRetirementClaims::default(),
                        limits,
                        &scene_retirements,
                        configure,
                    ),
            }?;
            Ok(Self {
                scene: Some(scene),
                expected: expected.clone(),
                additions_revision,
                content: content.clone(),
                #[cfg(feature = "runtime-2d")]
                staged_server,
                #[cfg(feature = "runtime-2d")]
                stock,
                catalog,
            })
        })
    }

    fn validate_content(&self, world: &World) -> Result<(), SceneSpawnReport> {
        let bindings = current_scene_bindings(world, &self.expected)?;
        if !self.content.shares_component_bindings(&bindings) {
            return Err(rejected(failure(
                "project.scene.schema-mismatch",
                "Prepared Scene uses another schema authority",
            )));
        }
        validate_resources(
            world,
            #[cfg(feature = "runtime-2d")]
            (&self.content).into(),
        )
        .map_err(rejected)?;
        let residency = world.resource::<ProjectSceneResidency>();
        if residency.additions.revision != self.additions_revision {
            return Err(rejected(failure(
                "project.scene.additions-changed",
                "Active project Scene additions changed after travel preparation",
            )));
        }
        residency
            .authority
            .validate((&self.content).into())
            .map_err(rejected)?;
        self.catalog.validate(world).map_err(rejected)?;
        #[cfg(feature = "runtime-2d")]
        self.stock.validate_replacement(world).map_err(rejected)?;
        Ok(())
    }

    pub(super) fn commit(
        self,
        world: &mut World,
    ) -> Result<SceneSpawnReport, (Self, SceneSpawnReport)> {
        world.flush();
        if let Err(report) = self.validate_content(world) {
            return Err((self, report));
        }
        #[cfg(feature = "runtime-2d")]
        if self.stock.tilesets.has_publications() {
            let inputs = match self.stock.tilesets.share_inputs() {
                Ok(inputs) => inputs,
                Err(fault) => return Err((self, rejected(fault))),
            };
            return world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
                let tilesets = match stock::prepare_tilesets(
                    world,
                    &mut owner,
                    inputs,
                    &self.stock.tilesets.retirements,
                    &self.staged_server,
                    self.catalog.asset_database(),
                ) {
                    Ok(prepared) => prepared,
                    Err(fault) => return Err((self, rejected(fault))),
                };
                self.commit_scene(world, Some(tilesets))
            });
        }
        self.commit_scene(
            world,
            #[cfg(feature = "runtime-2d")]
            None,
        )
    }

    fn commit_scene(
        mut self,
        world: &mut World,
        #[cfg(feature = "runtime-2d")] tilesets: Option<PreparedTileSetPublications<'_>>,
    ) -> Result<SceneSpawnReport, (Self, SceneSpawnReport)> {
        let scene = self
            .scene
            .take()
            .expect("unpublished replacement retains its Scene candidate");
        let report = match scene.commit(world) {
            Ok(report) => report,
            Err((scene, report)) => {
                self.scene = Some(scene);
                return Err((self, report));
            }
        };
        #[cfg(feature = "runtime-2d")]
        let stock =
            stock::commit_publications(world, self.stock.images, tilesets, self.stock.materials);
        let authority = self.catalog.commit(world);
        world.insert_resource(self.content.domain_assets().clone());
        world.resource_mut::<CurrentScene>().commit_source(
            self.content.scene_source(),
            report
                .instance
                .as_ref()
                .expect("Scene commit returns its receipt")
                .clone(),
        );
        *world.resource_mut::<ProjectSceneResidency>() = ProjectSceneResidency {
            additions: additive::Additions::default(),
            #[cfg(feature = "runtime-2d")]
            stock,
            authority,
        };
        Ok(report)
    }

    pub(super) fn discard(mut self, world: &mut World) -> Result<(), Self> {
        if let Some(scene) = self.scene.take() {
            if let Err(scene) = scene.discard(world) {
                self.scene = Some(scene);
                return Err(self);
            }
        }
        Ok(())
    }
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
    let source = {
        let source = matches!(content, ResidencyInput::Source(_));
        if source
            && (!world.contains_resource::<ProjectAssetDatabase>()
                || !world.resource::<CurrentScene>().is_project_backed())
        {
            return Err(resources_missing());
        }
        source
    };
    #[cfg(feature = "runtime-2d")]
    let unsupported = {
        let current = world.resource::<ProjectSceneResidency>();
        stock::validate_resources(world, content, Some(current))?;
        let uses_tilesets = content.tileset_count() != 0 || current.tileset_count() != 0;
        #[cfg(feature = "project-source")]
        let unsupported = unsupported
            || source
                && transaction::validate_resource_scope::<ProjectAssetDatabase>(world).is_err();
        unsupported
            || uses_tilesets
                && transaction::validate_resource_scope::<TileSetAssets>(world).is_err()
    };
    if unsupported {
        return Err(lifecycle_unsupported());
    }
    Ok(())
}
