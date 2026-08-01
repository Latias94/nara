use std::{collections::BTreeMap, error::Error, fmt};

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{
    __private::{
        validate_lifecycle_free_relationship_despawn, validate_non_linked_relationship_insertion,
        validate_non_linked_relationship_teardown, validate_registered_persistent_component_apply,
    },
    Component, Entity, LifecycleFreeDespawnError, Mut, World, prepare_lifecycle_free_despawn,
};
use nara_hierarchy::{
    Children, HierarchyConstructionEdge, HierarchyConstructionWriter, Parent, validate_hierarchy,
};
use nara_identity::{
    __private::{
        IdentitySupportTopologyError, PreparedSceneInstanceRegistration,
        PreparedSceneInstanceReplacement, prepare_exact_scene_instance_registration,
        prepare_exact_scene_instance_replacement, prepare_exact_scene_instance_retirement,
        validate_identity_support_topology,
    },
    IdentityDomainError, RuntimeEntityReference, SceneInstanceId, SpawnedSceneInstance,
    TombstoneCause, WorldEntityToken, WorldIdentityDomain, WorldIdentityDomainSettings,
};
use nara_reflect::{
    __private::{
        declare_persistent_apply_targets, validate_declared_persistent_apply_targets,
        validate_fresh_persistent_component_apply, validate_persistent_apply_support_topology,
    },
    ComponentApplyBatch, ComponentCodecError, ComponentRegistry,
};

use crate::{
    PrefabDocument, PrefabExpansionReport, PrefabSourceResolver, SceneDocument, SceneEntityId,
    ScenePatchDocument,
    diagnostics::{error as diagnostic_error, with_codec_error, with_public_locator},
    validation::{PreparedScene, preflight_scene_with_context},
};

#[derive(Debug)]
pub enum SceneEntityRetirementError {
    NotSceneEntity,
    HasChildren,
    Identity(IdentityDomainError),
    DespawnFailed,
}

impl fmt::Display for SceneEntityRetirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotSceneEntity => "target is not a scene-managed entity",
            Self::HasChildren => "scene entity retirement requires a leaf entity",
            Self::Identity(_) => "scene identity retirement failed",
            Self::DespawnFailed => "scene entity disappeared after identity retirement",
        })
    }
}

impl Error for SceneEntityRetirementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Identity(error) => Some(error),
            Self::NotSceneEntity | Self::HasChildren | Self::DespawnFailed => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct SceneEntitySource {
    pub instance_id: SceneInstanceId,
    pub entity_id: SceneEntityId,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneSpawnReport {
    pub instance: Option<SpawnedSceneInstance>,
    pub diagnostics: DiagnosticReport,
    retired_entities: usize,
}

impl SceneSpawnReport {
    #[must_use]
    pub(crate) const fn retired_entities(&self) -> usize {
        self.retired_entities
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SceneSpawner;

#[derive(Debug, Clone, Copy)]
enum SceneIdentityCommit<'a> {
    Register,
    Replace(&'a SpawnedSceneInstance),
}

enum PreparedSceneIdentityCommit {
    Register {
        domain: Option<WorldIdentityDomain>,
        registration: PreparedSceneInstanceRegistration,
    },
    Replace {
        replacement: PreparedSceneInstanceReplacement,
        hierarchy: PreparedSceneHierarchyDetach,
    },
}

impl PreparedSceneIdentityCommit {
    const fn instance_id(&self) -> SceneInstanceId {
        match self {
            Self::Register { registration, .. } => registration.instance_id(),
            Self::Replace { replacement, .. } => replacement.instance_id(),
        }
    }

    fn retiring_entities(&self) -> &[Entity] {
        match self {
            Self::Register { .. } => &[],
            Self::Replace { replacement, .. } => replacement.retiring_entities(),
        }
    }
}

enum PrepareSceneIdentityCommitError {
    Identity(IdentityDomainError),
    Hierarchy,
}

#[derive(Debug)]
enum SceneHierarchyDetachError {
    MissingEntity,
    Invalid,
}

struct PreparedSceneHierarchyDetach {
    detached_children: Vec<Entity>,
    affected_entities: Vec<Entity>,
}

impl PreparedSceneHierarchyDetach {
    fn affected_entities(&self) -> &[Entity] {
        &self.affected_entities
    }

    fn commit(self, world: &mut World) {
        let mut commands = world.commands();
        for child in self.detached_children {
            commands.entity(child).remove::<Parent>();
        }
    }
}

fn prepare_scene_hierarchy_detach(
    world: &World,
    retirements: &[Entity],
) -> Result<PreparedSceneHierarchyDetach, SceneHierarchyDetachError> {
    if retirements.is_empty() {
        return Ok(PreparedSceneHierarchyDetach {
            detached_children: Vec::new(),
            affected_entities: Vec::new(),
        });
    }
    if retirements
        .iter()
        .any(|entity| world.get_entity(*entity).is_err())
    {
        return Err(SceneHierarchyDetachError::MissingEntity);
    }
    validate_hierarchy(world).map_err(|_| SceneHierarchyDetachError::Invalid)?;

    let mut detached_children = Vec::new();
    let mut affected_entities = Vec::new();
    for entity in retirements.iter().copied() {
        if let Some(parent) = world.get::<Parent>(entity).map(Parent::parent) {
            detached_children.push(entity);
            affected_entities.push(entity);
            affected_entities.push(parent);
        }
        if let Some(children) = world.get::<Children>(entity) {
            for child in children.iter() {
                detached_children.push(child);
                affected_entities.push(child);
                affected_entities.push(entity);
            }
        }
    }

    detached_children.sort_unstable_by_key(|entity| entity.to_bits());
    detached_children.dedup();
    affected_entities.sort_unstable_by_key(|entity| entity.to_bits());
    affected_entities.dedup();
    Ok(PreparedSceneHierarchyDetach {
        detached_children,
        affected_entities,
    })
}

impl SceneSpawner {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn spawn(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
    ) -> SceneSpawnReport {
        self.spawn_with_asset_context(
            world,
            registry,
            document,
            None,
            SceneIdentityCommit::Register,
        )
    }

    pub fn spawn_with_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        self.spawn_with_asset_context(
            world,
            registry,
            document,
            Some(database),
            SceneIdentityCommit::Register,
        )
    }

    pub(crate) fn replace(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        current: &SpawnedSceneInstance,
    ) -> SceneSpawnReport {
        self.spawn_with_asset_context(
            world,
            registry,
            document,
            None,
            SceneIdentityCommit::Replace(current),
        )
    }

    pub(crate) fn replace_with_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        current: &SpawnedSceneInstance,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        self.spawn_with_asset_context(
            world,
            registry,
            document,
            Some(database),
            SceneIdentityCommit::Replace(current),
        )
    }

    pub fn spawn_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        resolver: &R,
    ) -> SceneSpawnReport {
        let expansion = document.expand_prefabs(registry, resolver);
        self.spawn_prefab_expansion(
            world,
            registry,
            expansion,
            None,
            SceneIdentityCommit::Register,
        )
    }

    pub fn spawn_with_prefab_resolver_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        let expansion = document.expand_prefabs_with_asset_database(registry, resolver, database);
        self.spawn_prefab_expansion(
            world,
            registry,
            expansion,
            Some(database),
            SceneIdentityCommit::Register,
        )
    }

    pub(crate) fn replace_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        current: &SpawnedSceneInstance,
        resolver: &R,
    ) -> SceneSpawnReport {
        let expansion = document.expand_prefabs(registry, resolver);
        self.spawn_prefab_expansion(
            world,
            registry,
            expansion,
            None,
            SceneIdentityCommit::Replace(current),
        )
    }

    pub(crate) fn replace_with_prefab_resolver_and_asset_database<
        R: PrefabSourceResolver + ?Sized,
    >(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        current: &SpawnedSceneInstance,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        let expansion = document.expand_prefabs_with_asset_database(registry, resolver, database);
        self.spawn_prefab_expansion(
            world,
            registry,
            expansion,
            Some(database),
            SceneIdentityCommit::Replace(current),
        )
    }

    fn spawn_with_asset_context(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
    ) -> SceneSpawnReport {
        let mut component_batch = ComponentApplyBatch::from_world(world);
        let mut preflight = component_batch.with_decode_context(database, |context| {
            preflight_scene_with_context(document, registry, context)
        });
        if preflight.diagnostics.has_errors() {
            return SceneSpawnReport {
                instance: None,
                diagnostics: preflight.diagnostics,
                retired_entities: 0,
            };
        }

        let mut diagnostics = std::mem::take(&mut preflight.diagnostics);
        let has_targets = !preflight.entities.is_empty();
        let has_persistent_components = preflight
            .entities
            .iter()
            .any(|entity| !entity.components.is_empty());
        if let Err(error) = validate_persistent_apply_support_topology(
            world,
            &component_batch,
            has_targets,
            has_persistent_components,
        ) {
            return persistent_apply_rejection(diagnostics, &error);
        }
        if let Err(error) = validate_scene_identity_support(world, &[]) {
            diagnostics.push(crate::diagnostics::with_identity_support_error(
                diagnostic_error(
                    "scene.identity-support-ineligible",
                    "Scene identity support is ineligible for target-World apply",
                ),
                &error,
            ));
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }
        if let Err(error) = validate_scene_persistent_apply(world, &preflight, None) {
            return persistent_apply_rejection(diagnostics, &error);
        }
        if let Err(error) = component_batch.validate_commit(world) {
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.component-apply-commit-failed",
                    "Prepared scene components could not be committed",
                ),
                &error,
            ));
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }
        let new_identity_domain =
            match prepare_scene_identity(world, preflight.entities.len(), commit) {
                Ok(domain) => domain,
                Err(error) => {
                    diagnostics.push(crate::diagnostics::with_identity_error(
                        match commit {
                            SceneIdentityCommit::Register => diagnostic_error(
                                "scene.identity-registration-failed",
                                "Scene identity registration failed",
                            ),
                            SceneIdentityCommit::Replace(_) => diagnostic_error(
                                "scene.identity-replacement-failed",
                                "Scene identity replacement failed",
                            ),
                        },
                        &error,
                    ));
                    return SceneSpawnReport {
                        instance: None,
                        diagnostics,
                        retired_entities: 0,
                    };
                }
            };
        let mut spawned_by_id = BTreeMap::new();
        let mut spawned_entities = Vec::new();
        let mut hierarchy_edges = Vec::new();
        for entity in &preflight.entities {
            let runtime_entity = world.spawn_empty().id();
            spawned_entities.push(runtime_entity);
            if spawned_by_id
                .insert(entity.id.clone(), runtime_entity)
                .is_some()
            {
                diagnostics.push(with_public_locator(
                    diagnostic_error(
                        "scene.internal-duplicate-entity",
                        "Scene spawn produced a duplicate entity identity",
                    ),
                    "entity-id",
                    entity.id.as_str(),
                ));
                break;
            }
        }

        if diagnostics.has_errors() {
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        for entity in preflight.entities {
            let Some(runtime_entity) = spawned_by_id.get(&entity.id).copied() else {
                diagnostics.push(with_public_locator(
                    diagnostic_error(
                        "scene.internal-missing-entity",
                        "Spawned scene entity is missing",
                    ),
                    "entity-id",
                    entity.id.as_str(),
                ));
                continue;
            };

            for component in entity.components {
                let component_id = component.id;
                component_batch = match component_batch.stage(runtime_entity, component.prepared) {
                    Ok(batch) => batch,
                    Err(error) => {
                        diagnostics.push(with_codec_error(
                            with_public_locator(
                                with_public_locator(
                                    diagnostic_error(
                                        "scene.component-apply-failed",
                                        "Component apply failed",
                                    ),
                                    "entity-id",
                                    entity.id.as_str(),
                                ),
                                "component-id",
                                component_id.as_str(),
                            ),
                            &error,
                        ));
                        rollback_spawn_transaction(world, &spawned_entities);
                        return SceneSpawnReport {
                            instance: None,
                            diagnostics,
                            retired_entities: 0,
                        };
                    }
                };
            }

            if let Some(parent_id) = entity.parent {
                match spawned_by_id.get(&parent_id).copied() {
                    Some(parent_entity) => hierarchy_edges.push(HierarchyConstructionEdge::new(
                        runtime_entity,
                        parent_entity,
                    )),
                    None => {
                        diagnostics.push(with_public_locator(
                            diagnostic_error(
                                "scene.internal-missing-parent-remap",
                                "Validated scene parent was not mapped to a runtime entity",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ));
                    }
                }
            }
        }

        if diagnostics.has_errors() {
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        if let Err(error) = component_batch.validate_commit(world) {
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.component-apply-commit-failed",
                    "Prepared scene components could not be committed",
                ),
                &error,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        if let Err(error) =
            declare_persistent_apply_targets(world, spawned_entities.iter().copied())
        {
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.component-apply-commit-failed",
                    "Prepared scene components could not be committed",
                ),
                &error,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }
        let identity_tokens = match prepare_scene_identity_tokens(
            world,
            &spawned_by_id,
            new_identity_domain.as_ref(),
        ) {
            Ok(tokens) => tokens,
            Err(error) => {
                diagnostics.push(crate::diagnostics::with_identity_error(
                    match commit {
                        SceneIdentityCommit::Register => diagnostic_error(
                            "scene.identity-registration-failed",
                            "Scene identity registration failed",
                        ),
                        SceneIdentityCommit::Replace(_) => diagnostic_error(
                            "scene.identity-replacement-failed",
                            "Scene identity replacement failed",
                        ),
                    },
                    &error,
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                };
            }
        };

        let prepared_identity = match prepare_scene_identity_commit(
            world,
            &identity_tokens,
            commit,
            new_identity_domain,
        ) {
            Ok(prepared) => prepared,
            Err(PrepareSceneIdentityCommitError::Identity(error)) => {
                diagnostics.push(crate::diagnostics::with_identity_error(
                    match commit {
                        SceneIdentityCommit::Register => diagnostic_error(
                            "scene.identity-registration-failed",
                            "Scene identity registration failed",
                        ),
                        SceneIdentityCommit::Replace(_) => diagnostic_error(
                            "scene.identity-replacement-failed",
                            "Scene identity replacement failed",
                        ),
                    },
                    &error,
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                };
            }
            Err(PrepareSceneIdentityCommitError::Hierarchy) => {
                diagnostics.push(diagnostic_error(
                    "scene.hierarchy-retirement-ineligible",
                    "Scene hierarchy could not be prepared for exact retirement",
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                };
            }
        };

        let retired_entities = prepared_identity.retiring_entities().to_vec();
        if let Err(error) = validate_scene_identity_support(world, &retired_entities) {
            diagnostics.push(crate::diagnostics::with_identity_support_error(
                diagnostic_error(
                    "scene.identity-support-ineligible",
                    "Scene identity support is ineligible for target-World apply",
                ),
                &error,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }
        if let Err(error) = validate_existing_scene_persistent_apply(world, &retired_entities) {
            let report = persistent_apply_rejection(diagnostics, &error);
            rollback_spawn_transaction(world, &spawned_entities);
            return report;
        }

        let mut candidate_hierarchy_targets = hierarchy_edges
            .iter()
            .flat_map(|edge| [edge.child(), edge.parent()])
            .collect::<Vec<_>>();
        candidate_hierarchy_targets.sort_unstable_by_key(|entity| entity.to_bits());
        candidate_hierarchy_targets.dedup();
        let hierarchy_projection_ineligible = !hierarchy_edges.is_empty()
            && (validate_non_linked_relationship_insertion::<Parent>(
                world,
                hierarchy_edges
                    .iter()
                    .map(|edge| (edge.child(), edge.parent())),
            )
            .is_err()
                || validate_non_linked_relationship_teardown::<Parent>(
                    world,
                    &candidate_hierarchy_targets,
                )
                .is_err());
        if hierarchy_projection_ineligible
            || validate_scene_source_support(world, &spawned_entities).is_err()
        {
            diagnostics.push(diagnostic_error(
                "scene.runtime-projection-ineligible",
                "Scene runtime projections would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        if let PreparedSceneIdentityCommit::Replace {
            replacement,
            hierarchy,
        } = &prepared_identity
            && validate_prepared_scene_retirement(world, replacement.retiring_entities(), hierarchy)
                .is_err()
        {
            diagnostics.push(diagnostic_error(
                "scene.identity-replacement-ineligible",
                "Scene replacement would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        if HierarchyConstructionWriter::new(world)
            .attach_batch(&hierarchy_edges)
            .is_err()
        {
            diagnostics.push(diagnostic_error(
                "scene.hierarchy-publication-failed",
                "Validated scene hierarchy could not be published",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }
        world.flush();

        if let Err(error) = component_batch.commit(world) {
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.component-apply-commit-failed",
                    "Prepared scene components could not be committed",
                ),
                &error,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        let instance_id = prepared_identity.instance_id();
        for (entity_id, runtime_entity) in &spawned_by_id {
            world.entity_mut(*runtime_entity).insert(SceneEntitySource {
                instance_id,
                entity_id: entity_id.clone(),
            });
        }

        let (instance, retired) = match prepared_identity {
            PreparedSceneIdentityCommit::Register {
                domain,
                registration,
            } => {
                let instance = match domain {
                    Some(mut domain) => {
                        let instance = registration.commit(&mut domain);
                        world.insert_resource(domain);
                        instance
                    }
                    None => world.resource_scope(|_world, mut domain: Mut<WorldIdentityDomain>| {
                        registration.commit(&mut domain)
                    }),
                };
                (instance, Vec::new())
            }
            PreparedSceneIdentityCommit::Replace {
                replacement,
                hierarchy,
            } => {
                hierarchy.commit(world);
                let mut domain = world
                    .remove_resource::<WorldIdentityDomain>()
                    .expect("prepared scene replacement requires its identity domain");
                let retirement = prepare_lifecycle_free_despawn(world, &retired_entities).expect(
                    "prevalidated hierarchy detach must leave exact retirement lifecycle-free",
                );
                let result = replacement.commit(&mut domain);
                let world = retirement.commit();
                world.insert_resource(domain);
                result
            }
        };

        let retired_entities = retired.len();

        SceneSpawnReport {
            instance: Some(instance),
            diagnostics,
            retired_entities,
        }
    }

    fn spawn_prefab_expansion(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        expansion: PrefabExpansionReport,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
    ) -> SceneSpawnReport {
        let mut diagnostics = expansion.diagnostics;
        if diagnostics.has_errors() {
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        let document = expansion
            .document
            .expect("successful prefab expansion should include document");
        let mut report =
            self.spawn_with_asset_context(world, registry, &document, database, commit);
        diagnostics.extend(report.diagnostics);
        report.diagnostics = diagnostics;
        report
    }

    pub fn spawn_prefab(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
    ) -> SceneSpawnReport {
        self.spawn_prefab_with_patch(world, registry, prefab, &ScenePatchDocument::default())
    }

    pub fn spawn_prefab_with_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        self.spawn_prefab_with_patch_and_asset_database(
            world,
            registry,
            prefab,
            &ScenePatchDocument::default(),
            database,
        )
    }

    pub fn spawn_prefab_with_patch(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
        patch: &ScenePatchDocument,
    ) -> SceneSpawnReport {
        let instantiate = prefab.instantiate_with_patch(registry, patch);
        let mut diagnostics = instantiate.diagnostics;
        if diagnostics.has_errors() {
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        let document = instantiate
            .document
            .expect("successful prefab patch instantiation should include document");
        let mut report = self.spawn(world, registry, &document);
        diagnostics.extend(report.diagnostics);
        report.diagnostics = diagnostics;
        report
    }

    pub fn spawn_prefab_with_patch_and_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
        patch: &ScenePatchDocument,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        let instantiate =
            prefab.instantiate_with_patch_and_asset_database(registry, patch, database);
        let mut diagnostics = instantiate.diagnostics;
        if diagnostics.has_errors() {
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        let document = instantiate
            .document
            .expect("successful prefab patch instantiation should include document");
        let mut report = self.spawn_with_asset_database(world, registry, &document, database);
        diagnostics.extend(report.diagnostics);
        report.diagnostics = diagnostics;
        report
    }
}

fn validate_scene_persistent_apply(
    world: &mut World,
    preflight: &PreparedScene,
    current_targets: Option<&[Entity]>,
) -> Result<(), ComponentCodecError> {
    if let Some(current_targets) = current_targets {
        validate_existing_scene_persistent_apply(world, current_targets)?;
    }
    validate_fresh_persistent_component_apply(
        world,
        preflight.entities.iter().flat_map(|entity| {
            entity
                .components
                .iter()
                .map(|component| &component.prepared)
        }),
    )
}

pub(crate) fn validate_existing_scene_persistent_apply(
    world: &mut World,
    targets: &[Entity],
) -> Result<(), ComponentCodecError> {
    validate_declared_persistent_apply_targets(world, targets.iter().copied())
}

pub(crate) fn validate_scene_identity_support(
    world: &mut World,
    targets: &[Entity],
) -> Result<(), IdentitySupportTopologyError> {
    validate_identity_support_topology(world, targets.iter().copied())
}

pub(crate) enum SceneInstanceRetirementTransactionError {
    Identity(IdentityDomainError),
    IdentitySupport(IdentitySupportTopologyError),
    PersistentApply(ComponentCodecError),
    Hierarchy,
    Lifecycle,
}

pub(crate) fn retire_scene_instance_exact(
    world: &mut World,
    current: &SpawnedSceneInstance,
    cause: TombstoneCause,
) -> Result<Vec<Entity>, SceneInstanceRetirementTransactionError> {
    let prepared = prepare_exact_scene_instance_retirement(world, current, cause)
        .map_err(SceneInstanceRetirementTransactionError::Identity)?;
    let retirements = prepared.retiring_entities().to_vec();
    validate_scene_identity_support(world, &retirements)
        .map_err(SceneInstanceRetirementTransactionError::IdentitySupport)?;
    validate_existing_scene_persistent_apply(world, &retirements)
        .map_err(SceneInstanceRetirementTransactionError::PersistentApply)?;
    let hierarchy = prepare_scene_hierarchy_detach(world, &retirements)
        .map_err(|_| SceneInstanceRetirementTransactionError::Hierarchy)?;
    validate_prepared_scene_retirement(world, &retirements, &hierarchy)
        .map_err(|_| SceneInstanceRetirementTransactionError::Lifecycle)?;
    hierarchy.commit(world);
    let mut domain = world
        .remove_resource::<WorldIdentityDomain>()
        .expect("prepared scene retirement requires its identity domain");
    let retirement = prepare_lifecycle_free_despawn(world, &retirements)
        .expect("prevalidated hierarchy detach must leave exact retirement lifecycle-free");
    let retired = prepared.commit(&mut domain);
    let world = retirement.commit();
    world.insert_resource(domain);
    Ok(retired)
}

fn validate_prepared_scene_retirement(
    world: &mut World,
    retirements: &[Entity],
    hierarchy: &PreparedSceneHierarchyDetach,
) -> Result<(), LifecycleFreeDespawnError> {
    if hierarchy.affected_entities().is_empty() {
        let _ = prepare_lifecycle_free_despawn(world, retirements)?.cancel();
        Ok(())
    } else {
        validate_lifecycle_free_relationship_despawn::<Parent>(
            world,
            retirements,
            hierarchy.affected_entities(),
        )
    }
}

fn persistent_apply_rejection(
    mut diagnostics: DiagnosticReport,
    error: &ComponentCodecError,
) -> SceneSpawnReport {
    diagnostics.push(with_codec_error(
        diagnostic_error(
            "scene.persistent-apply-ineligible",
            "Persistent scene components are ineligible for target-World apply",
        ),
        error,
    ));
    SceneSpawnReport {
        instance: None,
        diagnostics,
        retired_entities: 0,
    }
}

fn rollback_spawn_transaction(world: &mut World, spawned_entities: &[Entity]) {
    let live_entities = spawned_entities
        .iter()
        .copied()
        .filter(|entity| world.get_entity(*entity).is_ok())
        .collect::<Vec<_>>();
    if live_entities.is_empty() {
        return;
    }

    let has_relationship = live_entities.iter().copied().any(|entity| {
        world.get::<Parent>(entity).is_some() || world.get::<Children>(entity).is_some()
    });
    if has_relationship {
        let hierarchy = prepare_scene_hierarchy_detach(world, &live_entities)
            .expect("scene candidate hierarchy rollback must remain prevalidated");
        validate_lifecycle_free_relationship_despawn::<Parent>(
            world,
            &live_entities,
            hierarchy.affected_entities(),
        )
        .expect("scene candidate relationship rollback must remain lifecycle-free");
        hierarchy.commit(world);
    }
    let retirement = prepare_lifecycle_free_despawn(world, &live_entities)
        .expect("scene candidate rollback must remain lifecycle-free");
    let _ = retirement.commit();
}

fn validate_scene_source_support(
    world: &mut World,
    targets: &[Entity],
) -> Result<(), nara_ecs::__private::PersistentComponentMetadataError> {
    world.register_component::<SceneEntitySource>();
    world.flush();
    for target in targets {
        validate_registered_persistent_component_apply::<SceneEntitySource>(world, Some(*target))?;
    }
    Ok(())
}

fn prepare_scene_identity_tokens(
    world: &mut World,
    spawned_by_id: &BTreeMap<SceneEntityId, Entity>,
    new_domain: Option<&WorldIdentityDomain>,
) -> Result<BTreeMap<SceneEntityId, WorldEntityToken>, IdentityDomainError> {
    if let Some(domain) = new_domain {
        return adopt_scene_identity_tokens(domain, world, spawned_by_id);
    }
    if !world.contains_resource::<WorldIdentityDomain>() {
        return Err(IdentityDomainError::WorldDomainUnavailable);
    }
    world.resource_scope(|world, domain: Mut<WorldIdentityDomain>| {
        adopt_scene_identity_tokens(&domain, world, spawned_by_id)
    })
}

fn adopt_scene_identity_tokens(
    domain: &WorldIdentityDomain,
    world: &mut World,
    spawned_by_id: &BTreeMap<SceneEntityId, Entity>,
) -> Result<BTreeMap<SceneEntityId, WorldEntityToken>, IdentityDomainError> {
    spawned_by_id
        .iter()
        .map(|(entity_id, entity)| {
            domain
                .adopt_entity(world, *entity)
                .map(|token| (entity_id.clone(), token))
        })
        .collect()
}

fn prepare_scene_identity(
    world: &World,
    entity_count: usize,
    commit: SceneIdentityCommit<'_>,
) -> Result<Option<WorldIdentityDomain>, IdentityDomainError> {
    if let Some(domain) = world.get_resource::<WorldIdentityDomain>() {
        match commit {
            SceneIdentityCommit::Register => {
                domain.preflight_scene_instance_registration(world, entity_count)?;
            }
            SceneIdentityCommit::Replace(current) => {
                domain.preflight_scene_instance_replacement(
                    world,
                    current,
                    entity_count,
                    TombstoneCause::Replaced,
                )?;
            }
        }
        return Ok(None);
    }
    if matches!(commit, SceneIdentityCommit::Replace(_)) {
        return Err(IdentityDomainError::WorldDomainUnavailable);
    }

    let domain = WorldIdentityDomain::new(world, WorldIdentityDomainSettings::default())?;
    domain.preflight_scene_instance_registration(world, entity_count)?;
    Ok(Some(domain))
}

fn prepare_scene_identity_commit(
    world: &mut World,
    identity_tokens: &BTreeMap<SceneEntityId, WorldEntityToken>,
    commit: SceneIdentityCommit<'_>,
    new_domain: Option<WorldIdentityDomain>,
) -> Result<PreparedSceneIdentityCommit, PrepareSceneIdentityCommitError> {
    let entries = identity_tokens
        .iter()
        .map(|(entity_id, token)| (entity_id.clone(), *token))
        .collect::<Vec<_>>();
    match commit {
        SceneIdentityCommit::Register => {
            if let Some(domain) = new_domain {
                debug_assert!(!world.contains_resource::<WorldIdentityDomain>());
                let registration =
                    prepare_exact_scene_instance_registration(&domain, world, &entries)
                        .map_err(PrepareSceneIdentityCommitError::Identity)?;
                return Ok(PreparedSceneIdentityCommit::Register {
                    domain: Some(domain),
                    registration,
                });
            }
            world.resource_scope(|world, domain: Mut<WorldIdentityDomain>| {
                let registration =
                    prepare_exact_scene_instance_registration(&domain, world, &entries)
                        .map_err(PrepareSceneIdentityCommitError::Identity)?;
                Ok(PreparedSceneIdentityCommit::Register {
                    domain: None,
                    registration,
                })
            })
        }
        SceneIdentityCommit::Replace(current) => {
            debug_assert!(new_domain.is_none());
            let replacement = prepare_exact_scene_instance_replacement(
                world,
                current,
                &entries,
                TombstoneCause::Replaced,
            )
            .map_err(PrepareSceneIdentityCommitError::Identity)?;
            let hierarchy = prepare_scene_hierarchy_detach(world, replacement.retiring_entities())
                .map_err(|_| PrepareSceneIdentityCommitError::Hierarchy)?;
            Ok(PreparedSceneIdentityCommit::Replace {
                replacement,
                hierarchy,
            })
        }
    }
}

/// Retires one scene-managed stable identity before removing its runtime entity.
pub fn retire_and_despawn_scene_entity(
    world: &mut World,
    entity: Entity,
) -> Result<(), SceneEntityRetirementError> {
    let Some(source) = world.get::<SceneEntitySource>(entity).cloned() else {
        return Err(SceneEntityRetirementError::NotSceneEntity);
    };
    if world
        .get::<Children>(entity)
        .is_some_and(|children| !children.is_empty())
    {
        return Err(SceneEntityRetirementError::HasChildren);
    }
    if !world.contains_resource::<WorldIdentityDomain>() {
        return Err(SceneEntityRetirementError::Identity(
            IdentityDomainError::WorldDomainUnavailable,
        ));
    }
    let reference = RuntimeEntityReference::scene(source.instance_id, source.entity_id);
    world
        .resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
            domain.retire_entity_with_reference(
                world,
                entity,
                &reference,
                TombstoneCause::Despawned,
            )
        })
        .map_err(SceneEntityRetirementError::Identity)?;
    if !world.despawn(entity) {
        return Err(SceneEntityRetirementError::DespawnFailed);
    }
    Ok(())
}

#[must_use]
pub fn spawn_scene(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn(world, registry, document)
}

#[must_use]
pub fn spawn_scene_with_asset_database(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
    database: &ProjectAssetDatabase,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_with_asset_database(world, registry, document, database)
}

#[must_use]
pub fn spawn_scene_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
    resolver: &R,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_with_prefab_resolver(world, registry, document, resolver)
}

#[must_use]
pub fn spawn_scene_with_prefab_resolver_and_asset_database<R: PrefabSourceResolver + ?Sized>(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
    resolver: &R,
    database: &ProjectAssetDatabase,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_with_prefab_resolver_and_asset_database(
        world, registry, document, resolver, database,
    )
}

#[must_use]
pub fn spawn_prefab(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_prefab(world, registry, prefab)
}

#[must_use]
pub fn spawn_prefab_with_asset_database(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
    database: &ProjectAssetDatabase,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_prefab_with_asset_database(world, registry, prefab, database)
}

#[must_use]
pub fn spawn_prefab_with_patch(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
    patch: &ScenePatchDocument,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_prefab_with_patch(world, registry, prefab, patch)
}

#[must_use]
pub fn spawn_prefab_with_patch_and_asset_database(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
    patch: &ScenePatchDocument,
    database: &ProjectAssetDatabase,
) -> SceneSpawnReport {
    SceneSpawner::new()
        .spawn_prefab_with_patch_and_asset_database(world, registry, prefab, patch, database)
}
