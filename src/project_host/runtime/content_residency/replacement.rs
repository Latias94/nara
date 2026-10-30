//! Owned CPU residency and Scene candidate for managed travel.

use super::*;

pub(crate) struct PreparedProjectSceneReplacement {
    scene: Option<PreparedSceneReplacement>,
    expected: SpawnedSceneInstance,
    content: PreparedSceneContent,
    staged_server: AssetServer,
    images: ImagePublications,
    inputs: Vec<TileSetPublicationInput>,
    retirements: Vec<TileSetContentReceipt>,
    tileset_slots: Vec<AssetMutationBaseline<TileSet>>,
    catalog: PreparedCatalog,
}

impl PreparedProjectSceneReplacement {
    pub(crate) fn prepare<F>(
        world: &mut World,
        expected: &SpawnedSceneInstance,
        content: &PreparedSceneContent,
        claims: &SceneNativeRetirementClaims,
        limits: SceneProductTransactionLimits,
        additional_retirements: &[WorldEntityToken],
        configure: F,
    ) -> Result<Self, SceneSpawnReport>
    where
        F: FnOnce(&mut SceneProductOverlayWriter<'_>),
    {
        nara_hierarchy::__private::prepare_mutation_baseline(world);
        let component_bindings = current_scene_bindings(world, expected)?;
        validate_resources(world, content).map_err(rejected)?;
        if !content.shares_component_bindings(&component_bindings) {
            return Err(rejected(failure(
                "project.scene.schema-mismatch",
                "Prepared Scene uses another schema authority",
            )));
        }
        let current = world.resource::<ProjectSceneResidency>();
        let prepared =
            PreparedResidency::prepare(world, content.into(), Some(current)).map_err(rejected)?;
        let PreparedResidency {
            // Bind the reservation first so every fallible local drops its catalogue copies
            // and publication scratch before this metadata owner is released.
            catalog,
            bindings,
            staged_server,
            images,
            inputs,
            retirements,
            tileset_slots,
        } = prepared;
        probe_tilesets(
            world,
            &inputs,
            &retirements,
            &staged_server,
            catalog
                .asset_resolution()
                .map(|resolver| resolver.database()),
        )
        .map_err(rejected)?;
        let scene = match content {
            PreparedSceneContent::Source(snapshot) => component_bindings
                .prepare_scene_replacement_with_claims(
                    world,
                    snapshot.expanded_candidate(),
                    Some(snapshot.asset_resolution()),
                    expected,
                    bindings,
                    claims,
                    limits,
                    additional_retirements,
                    configure,
                ),
            PreparedSceneContent::Processed(content) => component_bindings
                .prepare_runtime_scene_replacement_with_claims(
                    world,
                    content.scene.candidate(),
                    expected,
                    bindings,
                    claims,
                    limits,
                    additional_retirements,
                    configure,
                ),
        }?;
        Ok(Self {
            scene: Some(scene),
            expected: expected.clone(),
            content: content.clone(),
            staged_server,
            images,
            inputs,
            retirements,
            tileset_slots,
            catalog,
        })
    }

    pub(crate) fn validate_baseline(&self, world: &mut World) -> Result<(), SceneSpawnReport> {
        world.flush();
        self.validate_content(world)?;
        self.scene
            .as_ref()
            .expect("unpublished replacement retains its Scene candidate")
            .validate_baseline(world)?;
        probe_tilesets(
            world,
            &self.inputs,
            &self.retirements,
            &self.staged_server,
            self.catalog
                .asset_resolution()
                .map(|resolver| resolver.database()),
        )
        .map_err(rejected)
    }

    fn validate_content(&self, world: &World) -> Result<(), SceneSpawnReport> {
        let bindings = current_scene_bindings(world, &self.expected)?;
        if !self.content.shares_component_bindings(&bindings) {
            return Err(rejected(failure(
                "project.scene.schema-mismatch",
                "Prepared Scene uses another schema authority",
            )));
        }
        validate_resources(world, &self.content).map_err(rejected)?;
        world
            .resource::<ProjectSceneResidency>()
            .authority
            .validate((&self.content).into())
            .map_err(rejected)?;
        self.catalog.validate(world).map_err(rejected)?;
        if !self.tileset_slots.is_empty() {
            for baseline in &self.tileset_slots {
                baseline
                    .validate(
                        world.resource::<Assets<TileSet>>(),
                        world.resource::<AssetStates>(),
                    )
                    .map_err(|_| rejected(tileset_failure(TileSetAssetError::Superseded)))?;
            }
        }
        if !self.images.is_empty() {
            self.images
                .validate(
                    world.resource::<AssetServer>(),
                    world.resource::<Assets<ImageAsset>>(),
                    world.resource::<AssetStates>(),
                )
                .map_err(rejected)?;
        }
        Ok(())
    }

    pub(crate) fn commit(
        self,
        world: &mut World,
    ) -> Result<SceneSpawnReport, (Self, SceneSpawnReport)> {
        // Domain retirement hooks may defer World writes. Apply them before checking the
        // publication revisions so a stale asset guard rejects the commit instead of panicking
        // inside the infallible publication tail.
        world.flush();
        if let Err(report) = self.validate_content(world) {
            return Err((self, report));
        }
        if self.inputs.is_empty() && self.retirements.is_empty() {
            return self.commit_with_tilesets(world, None);
        }
        let inputs = match share_inputs(&self.inputs) {
            Ok(inputs) => inputs,
            Err(fault) => return Err((self, rejected(fault))),
        };
        world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
            let tilesets = match owner.prepare_publications(
                inputs,
                &self.retirements,
                &self.staged_server,
                self.catalog
                    .asset_resolution()
                    .map(|resolver| resolver.database()),
                world.resource::<Assets<TileSet>>(),
                world.resource::<AssetStates>(),
            ) {
                Ok(prepared) => prepared,
                Err(error) => return Err((self, rejected(tileset_failure(error)))),
            };
            self.commit_with_tilesets(world, Some(tilesets))
        })
    }

    fn commit_with_tilesets(
        mut self,
        world: &mut World,
        tilesets: Option<PreparedTileSetPublications<'_>>,
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
        let (images, tilesets) = commit_publications(world, self.images, tilesets);
        let authority = self.catalog.commit(world);
        world.insert_resource(self.content.domain_assets().clone());
        world.resource_mut::<CurrentSceneOwner>().commit_source(
            self.content.scene_source(),
            report
                .instance
                .as_ref()
                .expect("Scene commit returns its receipt")
                .clone(),
        );
        *world.resource_mut::<ProjectSceneResidency>() = ProjectSceneResidency {
            images,
            tilesets,
            authority,
        };
        Ok(report)
    }

    pub(crate) fn discard(mut self, world: &mut World) -> Result<(), Self> {
        if let Some(scene) = self.scene.take() {
            if let Err(scene) = scene.discard(world) {
                self.scene = Some(scene);
                return Err(self);
            }
        }
        Ok(())
    }
}

pub(super) fn commit_publications(
    world: &mut World,
    images: ImagePublications,
    tilesets: Option<PreparedTileSetPublications<'_>>,
) -> (Vec<OwnedImage>, Vec<TileSetContentReceipt>) {
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
    (images, tilesets)
}

fn share_inputs(
    inputs: &[TileSetPublicationInput],
) -> Result<Vec<TileSetPublicationInput>, HostFault> {
    inputs
        .iter()
        .map(|input| input.share_retained().ok_or_else(resources_missing))
        .collect()
}

fn probe_tilesets(
    world: &mut World,
    inputs: &[TileSetPublicationInput],
    retirements: &[TileSetContentReceipt],
    server: &AssetServer,
    database: Option<&ProjectAssetDatabase>,
) -> Result<(), HostFault> {
    if inputs.is_empty() && retirements.is_empty() {
        return Ok(());
    }
    let inputs = share_inputs(inputs)?;
    world.resource_scope(|world, mut owner: Mut<TileSetAssets>| {
        owner
            .prepare_publications(
                inputs,
                retirements,
                server,
                database,
                world.resource::<Assets<TileSet>>(),
                world.resource::<AssetStates>(),
            )
            .map(|_| ())
            .map_err(tileset_failure)
    })
}

fn validate_resources(world: &World, content: &PreparedSceneContent) -> Result<(), HostFault> {
    if !world.contains_resource::<CurrentSceneOwner>()
        || !world.contains_resource::<ProjectSceneResidency>()
        || !world.contains_resource::<crate::project_content::ProjectDomainAssets>()
    {
        return Err(resources_missing());
    }
    let source = matches!(content, PreparedSceneContent::Source(_));
    if source
        && (!world.contains_resource::<ProjectAssetDatabase>()
            || !world.resource::<CurrentSceneOwner>().is_project_backed())
    {
        return Err(resources_missing());
    }
    validate_native_resources(
        world,
        content.into(),
        Some(world.resource::<ProjectSceneResidency>()),
    )?;
    let uses_tilesets = ResidencyInput::from(content).tileset_count() != 0
        || !world
            .resource::<ProjectSceneResidency>()
            .tilesets
            .is_empty();
    if nara_ecs::__private::validate_resource_scope::<ProjectSceneResidency>(world).is_err()
        || source && nara_ecs::__private::validate_resource_scope::<ProjectAssetDatabase>(world).is_err()
        || nara_ecs::__private::validate_resource_scope::<crate::project_content::ProjectDomainAssets>(world).is_err()
        || nara_ecs::__private::validate_resource_scope::<CurrentSceneOwner>(world).is_err()
        || uses_tilesets && nara_ecs::__private::validate_resource_scope::<TileSetAssets>(world).is_err()
    {
        return Err(failure("project.scene.residency-lifecycle-unsupported", "Scene residency owners cannot run lifecycle effects during commit"));
    }
    Ok(())
}

pub(super) fn validate_native_resources(
    world: &World,
    content: ResidencyInput<'_>,
    current: Option<&ProjectSceneResidency>,
) -> Result<(), HostFault> {
    let uses_tilesets =
        content.tileset_count() != 0 || current.is_some_and(|current| !current.tilesets.is_empty());
    let uses_images = uses_tilesets
        || content.image_count() != 0
        || current.is_some_and(|current| !current.images.is_empty());
    if uses_images
        && (!world.contains_resource::<AssetServer>()
            || !world.contains_resource::<Assets<ImageAsset>>()
            || !world.contains_resource::<AssetStates>()
            || !world.contains_resource::<AssetEvents>())
    {
        return Err(resources_missing());
    }
    if uses_tilesets
        && (!world.contains_resource::<TileSetAssets>()
            || !world.contains_resource::<Assets<TileSet>>()
            || !world.contains_resource::<AssetStates>()
            || !world.contains_resource::<AssetEvents>())
    {
        return Err(resources_missing());
    }
    Ok(())
}
