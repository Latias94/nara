use std::{collections::BTreeMap, error::Error, fmt};

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Component, Entity, Mut, World};
use nara_identity::{
    __private::{IdentitySupportTopologyError, validate_identity_support_topology},
    EntityLookup, IdentityDomainError, RuntimeEntityReference, SceneInstanceId,
    SpawnedSceneInstance, TombstoneCause, WorldEntityToken, WorldIdentityDomain,
    WorldIdentityDomainSettings,
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
    hierarchy::{Parent, sync_children},
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
        let current_instance = match commit {
            SceneIdentityCommit::Register => None,
            SceneIdentityCommit::Replace(current) => Some(current),
        };
        let current_targets =
            current_instance.map(|current| resolved_scene_targets(world, current));
        if let Err(error) =
            validate_scene_identity_support(world, current_targets.as_deref().unwrap_or_default())
        {
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
        if let Err(error) =
            validate_scene_persistent_apply(world, &preflight, current_targets.as_deref())
        {
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
        let mut parent_links = Vec::new();
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

            if let Some(parent_id) = entity.parent
                && let Some(parent_entity) = spawned_by_id.get(&parent_id)
            {
                parent_links.push((runtime_entity, *parent_entity));
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

        // No component-bearing operation runs between the fresh-target guard and this commit.
        // Parent and scene-source projections are installed only after persistent publication so
        // their runtime observers cannot invalidate the guarded persistent insertion.
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
        // Persistent components and private receipt state are complete before identity markers are
        // adopted. Both support topologies were checked before allocation, so the remaining
        // identity commit cannot install a new matching observer before retirement.
        let identity_commit =
            commit_scene_identity(world, &identity_tokens, commit, new_identity_domain);
        let (instance, retired) = match identity_commit {
            Ok(result) => result,
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

        let retired_entities = despawn_entities(world, &retired);
        if retired_entities != retired.len() {
            diagnostics.push(crate::diagnostics::warning(
                "scene.retired-entity-already-missing",
                "A retired scene entity was already absent",
            ));
        }
        for (entity, parent) in parent_links {
            world.entity_mut(entity).insert(Parent(parent));
        }
        for (entity_id, runtime_entity) in &spawned_by_id {
            world.entity_mut(*runtime_entity).insert(SceneEntitySource {
                instance_id: instance.instance_id(),
                entity_id: entity_id.clone(),
            });
        }
        sync_children(world);

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

pub(crate) fn resolved_scene_targets(world: &World, current: &SpawnedSceneInstance) -> Vec<Entity> {
    current
        .entity_ids()
        .iter()
        .filter_map(|entity_id| match current.resolve(world, entity_id) {
            EntityLookup::Resolved(entity) => Some(entity),
            _ => None,
        })
        .collect()
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
    for entity in spawned_entities.iter().rev().copied() {
        if world.get_entity(entity).is_ok() {
            world.despawn(entity);
        }
    }
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

fn commit_scene_identity(
    world: &mut World,
    identity_tokens: &BTreeMap<SceneEntityId, WorldEntityToken>,
    commit: SceneIdentityCommit<'_>,
    new_domain: Option<WorldIdentityDomain>,
) -> Result<(SpawnedSceneInstance, Vec<Entity>), IdentityDomainError> {
    if let Some(mut domain) = new_domain {
        debug_assert!(!world.contains_resource::<WorldIdentityDomain>());
        let result =
            commit_scene_identity_with_domain(&mut domain, world, identity_tokens, commit)?;
        world.insert_resource(domain);
        return Ok(result);
    }

    world.resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
        commit_scene_identity_with_domain(&mut domain, world, identity_tokens, commit)
    })
}

fn commit_scene_identity_with_domain(
    domain: &mut WorldIdentityDomain,
    world: &mut World,
    identity_tokens: &BTreeMap<SceneEntityId, WorldEntityToken>,
    commit: SceneIdentityCommit<'_>,
) -> Result<(SpawnedSceneInstance, Vec<Entity>), IdentityDomainError> {
    let entries = identity_tokens
        .iter()
        .map(|(entity_id, token)| (entity_id.clone(), *token))
        .collect::<Vec<_>>();
    match commit {
        SceneIdentityCommit::Register => domain
            .register_new_scene_instance(world, entries)
            .map(|instance| (instance, Vec::new())),
        SceneIdentityCommit::Replace(current) => {
            domain.replace_scene_instance(world, current, entries, TombstoneCause::Replaced)
        }
    }
}

fn despawn_entities(world: &mut World, entities: &[Entity]) -> usize {
    entities
        .iter()
        .copied()
        .filter(|entity| world.despawn(*entity))
        .count()
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
        .query::<&Parent>()
        .iter(world)
        .any(|parent| parent.0 == entity)
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
    sync_children(world);
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
