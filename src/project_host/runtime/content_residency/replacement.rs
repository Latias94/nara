//! Owned CPU residency and Scene candidate for managed travel.

use super::*;

pub(crate) struct PreparedProjectSceneReplacement {
    scene: Option<PreparedSceneReplacement>,
    expected: SpawnedSceneInstance,
    snapshot: ProjectContentSnapshot,
    original_catalog: ProjectAssetDatabase,
    staged_server: AssetServer,
    images: ImagePublications,
    inputs: Vec<TileSetPublicationInput>,
    retirements: Vec<TileSetOwnedSourceReceipt>,
    catalog: CatalogChange,
    lease: ProjectContentLease,
}

impl PreparedProjectSceneReplacement {
    pub(crate) fn prepare<F>(
        world: &mut World,
        expected: &SpawnedSceneInstance,
        snapshot: &ProjectContentSnapshot,
        claims: &SceneNativeRetirementClaims,
        limits: SceneProductTransactionLimits,
        additional_retirements: &[WorldEntityToken],
        configure: F,
    ) -> Result<Self, SceneSpawnReport>
    where
        F: FnOnce(&mut SceneProductOverlayWriter<'_>),
    {
        nara_hierarchy::__private::prepare_mutation_baseline(world);
        let registry = current_scene_registry(world, expected)?;
        validate_resources(world, snapshot).map_err(rejected)?;
        if !snapshot
            .shares_schema_snapshot(&registry.snapshot().expect("current registry is frozen"))
        {
            return Err(rejected(failure(
                "project.scene.schema-mismatch",
                "Prepared Scene uses another schema authority",
            )));
        }
        let current = world.resource::<ProjectSceneResidency>();
        let prepared = PreparedResidency::prepare(world, snapshot, current).map_err(rejected)?;
        let PreparedResidency {
            bindings,
            images,
            inputs,
            retirements,
            catalog,
            lease,
        } = prepared;
        let original_catalog = world.resource::<ProjectAssetDatabase>().clone();
        let staged_server = bindings.staged_server().clone();
        probe_tilesets(
            world,
            &inputs,
            &retirements,
            &staged_server,
            &catalog.database,
        )
        .map_err(rejected)?;
        let scene = prepare_expanded_scene_replacement_with_claims(
            world,
            &registry,
            snapshot.expanded_candidate(),
            Some(snapshot.asset_database()),
            expected,
            bindings,
            claims,
            limits,
            additional_retirements,
            configure,
        )?;
        Ok(Self {
            scene: Some(scene),
            expected: expected.clone(),
            snapshot: snapshot.clone(),
            original_catalog,
            staged_server,
            images,
            inputs,
            retirements,
            catalog,
            lease,
        })
    }

    pub(crate) fn validate_baseline(&self, world: &mut World) -> Result<(), SceneSpawnReport> {
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
            &self.catalog.database,
        )
        .map_err(rejected)
    }

    fn validate_content(&self, world: &World) -> Result<(), SceneSpawnReport> {
        current_scene_registry(world, &self.expected)?;
        validate_resources(world, &self.snapshot).map_err(rejected)?;
        if world.get_resource::<ProjectAssetDatabase>() != Some(&self.original_catalog) {
            return Err(rejected(failure(
                "project.scene.asset-catalog-changed",
                "Scene asset catalogue changed after preparation",
            )));
        }
        if !self.images.is_empty() {
            self.images
                .validate(
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
                &self.catalog.database,
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
        let images = if self.images.is_empty() {
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
            self.images
                .commit(&server, &mut values, &mut states, &mut events)
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
        world.insert_resource(self.catalog.database);
        world.insert_resource(self.snapshot.domain_assets().clone());
        world.resource_mut::<CurrentSceneOwner>().commit_source(
            self.snapshot.scene_source(),
            report
                .instance
                .as_ref()
                .expect("Scene commit returns its receipt")
                .clone(),
        );
        *world.resource_mut::<ProjectSceneResidency>() = ProjectSceneResidency {
            lineage: self.snapshot.lineage(),
            images,
            tilesets,
            catalog: self.catalog.owned,
            _lease: self.lease,
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
    retirements: &[TileSetOwnedSourceReceipt],
    server: &AssetServer,
    database: &ProjectAssetDatabase,
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

fn validate_resources(world: &World, snapshot: &ProjectContentSnapshot) -> Result<(), HostFault> {
    if !world
        .get_resource::<CurrentSceneOwner>()
        .is_some_and(CurrentSceneOwner::is_project_backed)
        || !world.contains_resource::<ProjectSceneResidency>()
        || !world.contains_resource::<ProjectAssetDatabase>()
        || !world.contains_resource::<crate::project_content::ProjectDomainAssets>()
    {
        return Err(resources_missing());
    }
    let uses_tilesets = !snapshot.tilesets().is_empty()
        || !world
            .resource::<ProjectSceneResidency>()
            .tilesets
            .is_empty();
    let uses_images = !snapshot.images().is_empty()
        || !world.resource::<ProjectSceneResidency>().images.is_empty();
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
    if nara_ecs::__private::validate_resource_scope::<ProjectSceneResidency>(world).is_err()
        || nara_ecs::__private::validate_resource_scope::<ProjectAssetDatabase>(world).is_err()
        || nara_ecs::__private::validate_resource_scope::<crate::project_content::ProjectDomainAssets>(world).is_err()
        || uses_tilesets && nara_ecs::__private::validate_resource_scope::<TileSetAssets>(world).is_err()
    {
        return Err(failure("project.scene.residency-lifecycle-unsupported", "Scene residency owners cannot run lifecycle effects during commit"));
    }
    Ok(())
}
