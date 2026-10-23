use std::{collections::BTreeMap, error::Error, fmt};

mod projection;
mod publication;
pub use projection::SceneProjection;
use publication::PreparedScenePublication;
pub use publication::PreparedSceneReplacement;

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{
    __private::LifecycleFreeInsertionPlan, __private::prepare_lifecycle_free_despawn,
    __private::validate_registered_persistent_component_apply, Component, Entity, World,
    bevy_ecs::component::ComponentId,
};
use nara_hierarchy::{
    __private::{prepare_construction_batch, prepare_mutation_baseline},
    Children, HierarchyRoots, Parent, validate_hierarchy,
};
use nara_identity::{
    __private::{
        AdditionalRetirementIdentityError, IdentitySupportTopologyError,
        PreparedSceneInstanceRegistration, PreparedSceneInstanceReplacement,
        PreparedSceneInstanceRetirement, prepare_exact_entity_retirement,
        prepare_exact_scene_instance_registration, prepare_exact_scene_instance_replacement,
        prepare_exact_scene_instance_retirement, prepare_scene_projection_epoch_retirement,
        validate_additional_retirement_identity_axes, validate_identity_support_topology,
    },
    IdentityDomainError, RuntimeEntityReference, SceneInstanceId, SpawnedSceneInstance,
    TombstoneCause, WorldEntityToken, WorldIdentityDomain, WorldIdentityDomainId,
    WorldIdentityDomainSettings, adopt_identity_entity,
};
use nara_reflect::{
    __private::{
        declare_persistent_apply_targets, validate_declared_persistent_apply_targets,
        validate_fresh_persistent_component_apply, validate_persistent_apply_support_topology,
    },
    ComponentCodecError, ComponentRegistry,
};

use crate::{
    ExpandedSceneCandidate, PrefabDocument, PrefabExpansionReport, PrefabProjectionPath,
    PrefabSourceResolver, SceneDocument, SceneEntityId, SceneEntityProvenanceMap,
    ScenePatchDocument,
    asset_bindings::SceneAssetBindings,
    candidate::validate_scene_provenance,
    diagnostics::{error as diagnostic_error, with_codec_error, with_public_locator},
    product_transaction::{
        PreparedSceneProductOverlay, SceneProductOverlayError, SceneProductOverlayWriter,
        SceneProductTransactionLimits,
    },
    runtime_hierarchy::{
        PreparedSceneEntityRetirement, build_scene_hierarchy_operations,
        combine_scene_hierarchy_operations, prepare_scene_hierarchy_retirement,
        validate_scene_entity_retirement_admission,
    },
    validation::{PreparedScene, preflight_scene_with_context},
};

#[derive(Debug)]
pub enum SceneEntityRetirementError {
    NotSceneEntity,
    HasChildren,
    Hierarchy,
    Lifecycle,
    Identity(IdentityDomainError),
}

impl fmt::Display for SceneEntityRetirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotSceneEntity => "target is not a scene-managed entity",
            Self::HasChildren => "scene entity retirement requires a leaf entity",
            Self::Hierarchy => "scene entity retirement could not update the runtime hierarchy",
            Self::Lifecycle => "scene entity retirement would run unsupported lifecycle work",
            Self::Identity(_) => "scene identity retirement failed",
        })
    }
}

impl Error for SceneEntityRetirementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Identity(error) => Some(error),
            Self::NotSceneEntity | Self::HasChildren | Self::Hierarchy | Self::Lifecycle => None,
        }
    }
}

/// Runtime provenance for an entity materialized from a persistent scene record.
#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct SceneEntitySource {
    /// Identity of the concrete materialized scene instance.
    pub instance_id: SceneInstanceId,
    /// Stable entity identity within the persistent scene document.
    pub entity_id: SceneEntityId,
    /// Opaque identity in the owning scene or prefab source document.
    pub source_entity_id: SceneEntityId,
    /// Structured prefab location, absent for scene-local entities.
    pub projection_path: Option<PrefabProjectionPath>,
}

/// Result of one scene materialization or replacement attempt.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneSpawnReport {
    /// Successful instance receipt, absent when materialization was rejected.
    pub instance: Option<SpawnedSceneInstance>,
    /// Structured diagnostics emitted while preparing or committing the operation.
    pub diagnostics: DiagnosticReport,
    retired_entities: usize,
}

/// Runtime component identities explicitly owned by native domains during Scene retirement.
///
/// Claims are an admission fact for one bounded retirement attempt. They do not authorize a
/// permanent lifecycle-hook whitelist: after domain owners have removed their components, the
/// ordinary strict Scene retirement validation runs again before any identity or entity commit.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneNativeRetirementClaims {
    component_ids: Vec<ComponentId>,
}

impl SceneNativeRetirementClaims {
    /// Creates claims from exact component identities registered in the target World.
    #[must_use]
    pub fn from_component_ids(component_ids: impl IntoIterator<Item = ComponentId>) -> Self {
        let mut claims = Self::default();
        for component_id in component_ids {
            claims.insert(component_id);
        }
        claims
    }

    /// Adds one exact runtime component identity, retaining first-registration order.
    pub fn insert(&mut self, component_id: ComponentId) {
        if !self.component_ids.contains(&component_id) {
            self.component_ids.push(component_id);
        }
    }

    /// Returns the exact claimed component identities.
    #[must_use]
    pub fn component_ids(&self) -> &[ComponentId] {
        &self.component_ids
    }

    /// Returns whether this claim set contains the component identity.
    #[must_use]
    pub fn contains(&self, component_id: ComponentId) -> bool {
        self.component_ids.contains(&component_id)
    }
}

impl SceneSpawnReport {
    #[must_use]
    pub(crate) const fn retired_entities(&self) -> usize {
        self.retired_entities
    }
}

/// Stateless entry point for scene materialization operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct SceneSpawner;

struct SceneProductTransactionInput<'a> {
    limits: SceneProductTransactionLimits,
    additional_retirements: &'a [WorldEntityToken],
    configure: Box<dyn FnOnce(&mut SceneProductOverlayWriter<'_>) + 'a>,
}

struct PendingSceneProductTransaction {
    overlay: PreparedSceneProductOverlay,
    additional_retirements: Vec<WorldEntityToken>,
}

struct PreparedSceneProductTransaction {
    overlay: PreparedSceneProductOverlay,
    additional_retirements: Vec<WorldEntityToken>,
}

enum PrepareSceneProductTransactionError {
    OverlayCeilingExceeded,
    AdditionalRetirementCeilingExceeded,
    AdditionalRetirementLimitExceeded,
    Overlay(SceneProductOverlayError),
    Identity(AdditionalRetirementIdentityError),
    SceneOwnedRetirement,
}

#[derive(Debug, Clone, Copy)]
enum SceneIdentityCommit<'a> {
    Register,
    Replace(&'a SpawnedSceneInstance),
    Projection(Option<&'a SpawnedSceneInstance>),
}

enum PreparedSceneIdentityCommit {
    Register {
        domain: Option<WorldIdentityDomain>,
        registration: PreparedSceneInstanceRegistration,
    },
    Replace {
        replacement: PreparedSceneInstanceReplacement,
    },
    Projection {
        domain: WorldIdentityDomain,
        predecessor: Option<WorldIdentityDomainId>,
        retirement: Option<PreparedSceneInstanceRetirement>,
        registration: PreparedSceneInstanceRegistration,
    },
}

impl PreparedSceneIdentityCommit {
    const fn instance_id(&self) -> SceneInstanceId {
        match self {
            Self::Register { registration, .. } => registration.instance_id(),
            Self::Replace { replacement, .. } => replacement.instance_id(),
            Self::Projection { registration, .. } => registration.instance_id(),
        }
    }

    fn retiring_entities(&self) -> &[Entity] {
        match self {
            Self::Register { .. } => &[],
            Self::Replace { replacement, .. } => replacement.retiring_entities(),
            Self::Projection { retirement, .. } => retirement
                .as_ref()
                .map_or(&[], |retirement| retirement.retiring_entities()),
        }
    }

    fn replaces_instance(&self) -> bool {
        matches!(
            self,
            Self::Replace { .. }
                | Self::Projection {
                    retirement: Some(_),
                    ..
                }
        )
    }
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
        self.spawn_with_asset_context_and_product(
            world, registry, document, None, database, commit, None,
        )
    }

    fn spawn_with_asset_context_and_product(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        provenance: Option<&SceneEntityProvenanceMap>,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
        product: Option<SceneProductTransactionInput<'_>>,
    ) -> SceneSpawnReport {
        let bindings = SceneAssetBindings::from_world(world);
        self.spawn_with_asset_context_and_product_bindings(
            world, registry, document, provenance, database, commit, product, bindings,
        )
    }

    fn spawn_with_asset_context_and_product_bindings(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        provenance: Option<&SceneEntityProvenanceMap>,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
        product: Option<SceneProductTransactionInput<'_>>,
        bindings: SceneAssetBindings,
    ) -> SceneSpawnReport {
        match self.prepare_with_asset_context_and_product_bindings(
            world,
            registry,
            document,
            provenance,
            database,
            commit,
            product,
            bindings,
            &SceneNativeRetirementClaims::default(),
        ) {
            Ok(prepared) => match prepared.commit(world) {
                Ok(report) => report,
                Err((prepared, report)) => {
                    if prepared.discard(world).is_err() {
                        let mut report = report;
                        report.diagnostics.push(diagnostic_error(
                            "scene.candidate-cleanup-incomplete",
                            "Rejected Scene candidate could not be safely discarded",
                        ));
                        return report;
                    }
                    report
                }
            },
            Err(report) => report,
        }
    }

    fn prepare_with_asset_context_and_product_bindings(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        provenance: Option<&SceneEntityProvenanceMap>,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
        product: Option<SceneProductTransactionInput<'_>>,
        bindings: SceneAssetBindings,
        claims: &SceneNativeRetirementClaims,
    ) -> Result<PreparedScenePublication, SceneSpawnReport> {
        let mut component_batch = match bindings.into_batch(world) {
            Ok(batch) => batch,
            Err(error) => {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(with_codec_error(
                    diagnostic_error(
                        "scene.asset-bindings-invalid",
                        "Prepared Scene asset bindings no longer match the target World",
                    ),
                    &error,
                    None,
                ));
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };
        if provenance
            .is_some_and(|provenance| validate_scene_provenance(document, provenance).is_err())
        {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic_error(
                "scene.invalid-provenance",
                "Expanded scene source provenance is inconsistent",
            ));
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        let mut preflight = component_batch.with_decode_context(database, |context| {
            preflight_scene_with_context(document, registry, context)
        });
        if preflight.diagnostics.has_errors() {
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics: preflight.diagnostics,
                retired_entities: 0,
            });
        }

        let mut diagnostics = std::mem::take(&mut preflight.diagnostics);
        let pending_product = match product {
            Some(product) => {
                match prepare_scene_product_transaction(world, registry, &preflight, product) {
                    Ok(product) => Some(product),
                    Err(error) => return Err(scene_product_rejection(diagnostics, error)),
                }
            }
            None => None,
        };
        // Product configuration and decoding precede the shared semantic World baseline.
        prepare_mutation_baseline(world);
        if validate_hierarchy(world).is_err() {
            diagnostics.push(diagnostic_error(
                "scene.hierarchy-invalid",
                "The existing runtime hierarchy is invalid",
            ));
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        let prepared_product = match pending_product {
            Some(product) => match validate_scene_product_baseline(world, product) {
                Ok(product) => Some(product),
                Err(error) => return Err(scene_product_rejection(diagnostics, error)),
            },
            None => None,
        };
        // Admission for retiring the old membership precedes scratch entity allocation. A global
        // despawn observer could also run while abandoning an otherwise empty candidate.
        if let SceneIdentityCommit::Replace(current)
        | SceneIdentityCommit::Projection(Some(current)) = commit
        {
            let retirement = match prepare_exact_scene_instance_retirement(
                world,
                current,
                TombstoneCause::Replaced,
            ) {
                Ok(retirement) => retirement,
                Err(error) => {
                    diagnostics.push(crate::diagnostics::with_identity_error(
                        diagnostic_error(
                            "scene.identity-replacement-failed",
                            "Scene identity replacement failed",
                        ),
                        &error,
                    ));
                    return Err(SceneSpawnReport {
                        instance: None,
                        diagnostics,
                        retired_entities: 0,
                    });
                }
            };
            let old_entities = retirement.retiring_entities();
            if let Err(error) = validate_existing_scene_persistent_apply(world, old_entities) {
                return Err(persistent_apply_rejection(diagnostics, &error));
            }
            let mut all_entities = old_entities.to_vec();
            if let Some(product) = &prepared_product {
                all_entities.extend(
                    product
                        .additional_retirements
                        .iter()
                        .map(|token| token.entity()),
                );
            }
            let eligible =
                prepare_scene_hierarchy_retirement(world, &all_entities).is_ok_and(|hierarchy| {
                    validate_scene_entity_retirement_admission(
                        world,
                        old_entities,
                        &hierarchy,
                        claims.component_ids(),
                    )
                    .is_ok()
                });
            if !eligible {
                diagnostics.push(diagnostic_error(
                    "scene.identity-replacement-ineligible",
                    "Scene replacement would run unsupported lifecycle work",
                ));
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        }
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
            return Err(persistent_apply_rejection(diagnostics, &error));
        }
        if let Err(error) = validate_scene_identity_support(world, &[]) {
            diagnostics.push(crate::diagnostics::with_identity_support_error(
                diagnostic_error(
                    "scene.identity-support-ineligible",
                    "Scene identity support is ineligible for target-World apply",
                ),
                &error,
            ));
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        if let Err(error) = validate_scene_persistent_apply(world, &preflight, None) {
            return Err(persistent_apply_rejection(diagnostics, &error));
        }
        if let Err(error) = component_batch.validate_commit(world) {
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.component-apply-commit-failed",
                    "Prepared scene components could not be committed",
                ),
                &error,
                None,
            ));
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
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
                            SceneIdentityCommit::Projection(_) => diagnostic_error(
                                "scene.projection-epoch-ineligible",
                                "Scene projection epoch could not be prepared",
                            ),
                        },
                        &error,
                    ));
                    return Err(SceneSpawnReport {
                        instance: None,
                        diagnostics,
                        retired_entities: 0,
                    });
                }
            };
        let mut spawned_by_id = BTreeMap::new();
        let mut spawned_entities = Vec::new();
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
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }

        let new_hierarchy_operations =
            match build_scene_hierarchy_operations(&preflight, &spawned_by_id) {
                Ok(operations) => operations,
                Err(entity_id) => {
                    diagnostics.push(with_public_locator(
                        diagnostic_error(
                            "scene.internal-missing-placement-remap",
                            "Validated scene placement was not mapped to a runtime entity",
                        ),
                        "entity-id",
                        entity_id.as_str(),
                    ));
                    rollback_spawn_transaction(world, &spawned_entities);
                    return Err(SceneSpawnReport {
                        instance: None,
                        diagnostics,
                        retired_entities: 0,
                    });
                }
            };
        let (product_insertion, product_resources, additional_retirements) = match prepared_product
        {
            Some(product) => {
                let (insertion, resources) = product.overlay.lower_components(&spawned_by_id);
                (insertion, Some(resources), product.additional_retirements)
            }
            None => (LifecycleFreeInsertionPlan::new(), None, Vec::new()),
        };
        let additional_retirement_entities = additional_retirements
            .iter()
            .map(|token| token.entity())
            .collect::<Vec<_>>();

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
                            registry.schema(&component_id),
                        ));
                        rollback_spawn_transaction(world, &spawned_entities);
                        return Err(SceneSpawnReport {
                            instance: None,
                            diagnostics,
                            retired_entities: 0,
                        });
                    }
                };
            }
        }

        if diagnostics.has_errors() {
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }

        if let Err(error) = component_batch.validate_commit(world) {
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.component-apply-commit-failed",
                    "Prepared scene components could not be committed",
                ),
                &error,
                None,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
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
                None,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
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
                        SceneIdentityCommit::Projection(_) => diagnostic_error(
                            "scene.projection-epoch-ineligible",
                            "Scene projection epoch could not be prepared",
                        ),
                    },
                    &error,
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        let prepared_identity = match prepare_scene_identity_commit(
            world,
            &identity_tokens,
            commit,
            new_identity_domain,
        ) {
            Ok(prepared) => prepared,
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
                        SceneIdentityCommit::Projection(_) => diagnostic_error(
                            "scene.projection-epoch-ineligible",
                            "Scene projection epoch could not be prepared",
                        ),
                    },
                    &error,
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        let scene_retirements = prepared_identity.retiring_entities().to_vec();
        let mut retired_entities = scene_retirements.clone();
        retired_entities.extend(additional_retirements.iter().map(|token| token.entity()));
        let prepared_hierarchy_retirement = if prepared_identity.replaces_instance() {
            match prepare_scene_hierarchy_retirement(world, &retired_entities) {
                Ok(retirement) => Some(retirement),
                Err(_) => {
                    diagnostics.push(diagnostic_error(
                        "scene.hierarchy-retirement-ineligible",
                        "Scene hierarchy could not be prepared for exact retirement",
                    ));
                    rollback_spawn_transaction(world, &spawned_entities);
                    return Err(SceneSpawnReport {
                        instance: None,
                        diagnostics,
                        retired_entities: 0,
                    });
                }
            }
        } else {
            None
        };
        let hierarchy_operations = if prepared_identity.replaces_instance() {
            combine_scene_hierarchy_operations(
                world,
                prepared_hierarchy_retirement
                    .as_ref()
                    .expect("prepared scene replacement requires hierarchy retirement"),
                &new_hierarchy_operations,
                &scene_retirements,
            )
        } else {
            new_hierarchy_operations.clone()
        };
        if !additional_retirements.is_empty()
            && PreparedSceneEntityRetirement::prepare(
                world,
                &additional_retirement_entities,
                prepared_hierarchy_retirement
                    .as_ref()
                    .expect("product replacement requires hierarchy retirement"),
            )
            .is_err()
        {
            diagnostics.push(diagnostic_error(
                "scene.product-retirement-lifecycle-active",
                "Scene product retirement would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        if let Err(error) = validate_scene_identity_support(world, &retired_entities) {
            diagnostics.push(crate::diagnostics::with_identity_support_error(
                diagnostic_error(
                    "scene.identity-support-ineligible",
                    "Scene identity support is ineligible for target-World apply",
                ),
                &error,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        if let Err(error) = validate_existing_scene_persistent_apply(world, &scene_retirements) {
            let report = persistent_apply_rejection(diagnostics, &error);
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(report);
        }

        if validate_scene_source_support(world, &spawned_entities).is_err() {
            diagnostics.push(diagnostic_error(
                "scene.runtime-projection-ineligible",
                "Scene runtime projections would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }

        if prepared_identity.replaces_instance()
            && validate_scene_entity_retirement_admission(
                world,
                &retired_entities,
                prepared_hierarchy_retirement
                    .as_ref()
                    .expect("prepared scene replacement requires hierarchy retirement"),
                claims.component_ids(),
            )
            .is_err()
        {
            diagnostics.push(diagnostic_error(
                "scene.identity-replacement-ineligible",
                "Scene replacement would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }

        let prepared_components = match component_batch.prepare(world) {
            Ok(prepared) => prepared,
            Err(error) => {
                diagnostics.push(with_codec_error(
                    diagnostic_error(
                        "scene.component-apply-commit-failed",
                        "Prepared scene components could not be committed",
                    ),
                    &error,
                    None,
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        let _prepared_hierarchy = match prepare_construction_batch(world, &hierarchy_operations) {
            Ok(prepared) => prepared,
            Err(error) => {
                diagnostics.push(if error == nara_hierarchy::HierarchyError::LifecycleWork {
                    diagnostic_error(
                        "scene.runtime-projection-ineligible",
                        "Scene runtime projections would run unsupported lifecycle work",
                    )
                } else {
                    diagnostic_error(
                        "scene.hierarchy-publication-failed",
                        "Validated scene hierarchy could not be prepared",
                    )
                });
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        if product_insertion.validate(world).is_err() {
            diagnostics.push(diagnostic_error(
                "scene.product-overlay-lifecycle-ineligible",
                "Scene product overlay would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        Ok(PreparedScenePublication {
            prepared_identity,
            identity_tokens,
            expected: match commit {
                SceneIdentityCommit::Register => None,
                SceneIdentityCommit::Replace(current) => Some(current.clone()),
                SceneIdentityCommit::Projection(current) => current.cloned(),
            },
            prepared_components,
            product_insertion,
            product_resources,
            hierarchy_operations,
            scene_retirements,
            additional_retirement_tokens: additional_retirements,
            retired_entities,
            spawned_by_id,
            provenance: provenance.cloned(),
            claims: claims.clone(),
            diagnostics,
        })
    }

    fn spawn_prefab_expansion(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        expansion: PrefabExpansionReport,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
    ) -> SceneSpawnReport {
        let PrefabExpansionReport {
            candidate,
            mut diagnostics,
        } = expansion;
        if diagnostics.has_errors() {
            return SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            };
        }

        let candidate = candidate.expect("successful prefab expansion should include candidate");
        let mut report = self.spawn_with_asset_context_and_product(
            world,
            registry,
            candidate.document(),
            Some(candidate.provenance()),
            database,
            commit,
            None,
        );
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

/// Replaces one active Scene Instance with bounded product-owned runtime values.
///
/// This provisional advanced path keeps the candidate World and stable-ID-to-Entity mapping
/// private. The callback can only enqueue owned typed values through its scoped writer. All limits
/// and additional retirement tokens are validated before candidate entities are allocated. Each
/// token binds its entity to the active World identity domain; a bare ECS `Entity` is insufficient
/// authority for product-owned retirement. Receipt-owning product state can be held outside the
/// World with `World::resource_scope` and updated from the successful report before the exclusive
/// caller returns; a rejected replacement leaves that state unchanged.
/// The asset database must come from the admitted source, as with initial spawning; `None` is
/// appropriate only for sources that do not require project stable-asset resolution.
#[must_use]
pub fn replace_scene_with_product<F>(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
    database: Option<&ProjectAssetDatabase>,
    current: &SpawnedSceneInstance,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    SceneSpawner::new().spawn_with_asset_context_and_product(
        world,
        registry,
        document,
        None,
        database,
        SceneIdentityCommit::Replace(current),
        Some(SceneProductTransactionInput {
            limits,
            additional_retirements,
            configure: Box::new(configure),
        }),
    )
}

/// Replaces one active Scene Instance while preserving the source provenance of an already
/// expanded document and its admitted asset-resolution context.
#[must_use]
pub fn replace_expanded_scene_with_product<F>(
    world: &mut World,
    registry: &ComponentRegistry,
    candidate: &ExpandedSceneCandidate,
    database: Option<&ProjectAssetDatabase>,
    current: &SpawnedSceneInstance,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    let bindings = SceneAssetBindings::from_world(world);
    replace_expanded_scene_with_product_and_bindings(
        world,
        registry,
        candidate,
        database,
        current,
        bindings,
        limits,
        additional_retirements,
        configure,
    )
}

/// Replaces an expanded Scene using the exact asset identities prepared by its content owner.
///
/// Bindings must originate in this World and its unchanged live AssetServer. Component decoding
/// extends their one scratch catalogue; successful Scene commit publishes that same catalogue.
/// Rejection publishes neither the catalogue nor candidate components and leaves the prior receipt
/// authoritative. No temporary entities escape this call.
///
/// This is a data transaction, not asset residency publication. A concrete content owner must
/// preflight its resource writes and retirement before calling, then perform only infallible commits
/// after a successful report. The supplied database must be the candidate's admitted source context.
#[must_use]
pub fn replace_expanded_scene_with_product_and_bindings<F>(
    world: &mut World,
    registry: &ComponentRegistry,
    candidate: &ExpandedSceneCandidate,
    database: Option<&ProjectAssetDatabase>,
    current: &SpawnedSceneInstance,
    bindings: SceneAssetBindings,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    SceneSpawner::new().spawn_with_asset_context_and_product_bindings(
        world,
        registry,
        candidate.document(),
        Some(candidate.provenance()),
        database,
        SceneIdentityCommit::Replace(current),
        Some(SceneProductTransactionInput {
            limits,
            additional_retirements,
            configure: Box::new(configure),
        }),
        bindings,
    )
}

/// Prepares every data and product write before a concrete native-domain owner retires handles.
/// No codec or product callback is retained across the preparation boundary.
#[doc(hidden)]
pub fn prepare_expanded_scene_replacement_with_claims<F>(
    world: &mut World,
    registry: &ComponentRegistry,
    candidate: &ExpandedSceneCandidate,
    database: Option<&ProjectAssetDatabase>,
    current: &SpawnedSceneInstance,
    bindings: SceneAssetBindings,
    claims: &SceneNativeRetirementClaims,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> Result<PreparedSceneReplacement, SceneSpawnReport>
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    SceneSpawner::new()
        .prepare_with_asset_context_and_product_bindings(
            world,
            registry,
            candidate.document(),
            Some(candidate.provenance()),
            database,
            SceneIdentityCommit::Replace(current),
            Some(SceneProductTransactionInput {
                limits,
                additional_retirements,
                configure: Box::new(configure),
            }),
            bindings,
            claims,
        )
        .map(PreparedSceneReplacement)
}

fn prepare_scene_product_transaction(
    world: &World,
    registry: &ComponentRegistry,
    scene: &PreparedScene,
    product: SceneProductTransactionInput<'_>,
) -> Result<PendingSceneProductTransaction, PrepareSceneProductTransactionError> {
    if product.limits.overlay_writes().get() > SceneProductTransactionLimits::MAX_OVERLAY_WRITES {
        return Err(PrepareSceneProductTransactionError::OverlayCeilingExceeded);
    }
    if product.limits.additional_retirements().get()
        > SceneProductTransactionLimits::MAX_ADDITIONAL_RETIREMENTS
    {
        return Err(PrepareSceneProductTransactionError::AdditionalRetirementCeilingExceeded);
    }
    if product.additional_retirements.len() > product.limits.additional_retirements().get() {
        return Err(PrepareSceneProductTransactionError::AdditionalRetirementLimitExceeded);
    }

    let mut overlay =
        SceneProductOverlayWriter::new(world, registry, scene, product.limits.overlay_writes());
    (product.configure)(&mut overlay);
    let overlay = overlay
        .finish()
        .map_err(PrepareSceneProductTransactionError::Overlay)?;
    Ok(PendingSceneProductTransaction {
        overlay,
        additional_retirements: product.additional_retirements.to_vec(),
    })
}

fn validate_scene_product_baseline(
    world: &mut World,
    product: PendingSceneProductTransaction,
) -> Result<PreparedSceneProductTransaction, PrepareSceneProductTransactionError> {
    validate_additional_retirement_identity_axes(
        world,
        product.additional_retirements.iter().copied(),
    )
    .map_err(PrepareSceneProductTransactionError::Identity)?;
    for token in &product.additional_retirements {
        if world.get::<SceneEntitySource>(token.entity()).is_some() {
            return Err(PrepareSceneProductTransactionError::SceneOwnedRetirement);
        }
    }
    product
        .overlay
        .validate_resources(world)
        .map_err(PrepareSceneProductTransactionError::Overlay)?;

    Ok(PreparedSceneProductTransaction {
        overlay: product.overlay,
        additional_retirements: product.additional_retirements,
    })
}

fn scene_product_rejection(
    mut diagnostics: DiagnosticReport,
    error: PrepareSceneProductTransactionError,
) -> SceneSpawnReport {
    let (code, summary, entity_id) = match error {
        PrepareSceneProductTransactionError::OverlayCeilingExceeded => (
            "scene.product-overlay-ceiling-exceeded",
            "Scene product overlay exceeds the engine item ceiling",
            None,
        ),
        PrepareSceneProductTransactionError::AdditionalRetirementCeilingExceeded => (
            "scene.product-retirement-ceiling-exceeded",
            "Scene product retirement exceeds the engine item ceiling",
            None,
        ),
        PrepareSceneProductTransactionError::AdditionalRetirementLimitExceeded => (
            "scene.product-retirement-limit-exceeded",
            "Scene product retirement exceeds its transaction item limit",
            None,
        ),
        PrepareSceneProductTransactionError::Overlay(SceneProductOverlayError::LimitExceeded) => (
            "scene.product-overlay-limit-exceeded",
            "Scene product overlay exceeds its transaction item limit",
            None,
        ),
        PrepareSceneProductTransactionError::Overlay(SceneProductOverlayError::MissingTarget(
            entity,
        )) => (
            "scene.product-overlay-target-missing",
            "Scene product overlay target is absent from the candidate scene",
            Some(entity),
        ),
        PrepareSceneProductTransactionError::Overlay(
            SceneProductOverlayError::ComponentUnregistered(entity),
        ) => (
            "scene.product-overlay-component-unregistered",
            "Scene product overlay component is not registered in the target World",
            Some(entity),
        ),
        PrepareSceneProductTransactionError::Overlay(
            SceneProductOverlayError::DuplicateComponent(entity),
        ) => (
            "scene.product-overlay-component-duplicate",
            "Scene product overlay writes the same component more than once",
            Some(entity),
        ),
        PrepareSceneProductTransactionError::Overlay(
            SceneProductOverlayError::ExistingComponent(entity),
        ) => (
            "scene.product-overlay-component-existing",
            "Scene product overlay component already exists in authored scene data",
            Some(entity),
        ),
        PrepareSceneProductTransactionError::Overlay(
            SceneProductOverlayError::ReservedComponent(entity),
        ) => (
            "scene.product-overlay-component-reserved",
            "Scene product overlay cannot write an engine-owned structural component",
            Some(entity),
        ),
        PrepareSceneProductTransactionError::Overlay(SceneProductOverlayError::ResourceMissing) => {
            (
                "scene.product-resource-missing",
                "Scene product resource replacement target is not installed",
                None,
            )
        }
        PrepareSceneProductTransactionError::Overlay(
            SceneProductOverlayError::DuplicateResource,
        ) => (
            "scene.product-resource-duplicate",
            "Scene product overlay replaces the same resource more than once",
            None,
        ),
        PrepareSceneProductTransactionError::Identity(
            AdditionalRetirementIdentityError::WorldDomainUnavailable,
        ) => (
            "scene.product-retirement-identity-unavailable",
            "Scene product retirement requires the active World identity domain",
            None,
        ),
        PrepareSceneProductTransactionError::Identity(
            AdditionalRetirementIdentityError::WorldBindingMismatch,
        )
        | PrepareSceneProductTransactionError::Identity(
            AdditionalRetirementIdentityError::TokenWrongDomain { .. },
        ) => (
            "scene.product-retirement-identity-wrong-world",
            "Scene product retirement identity authority belongs to another World",
            None,
        ),
        PrepareSceneProductTransactionError::Identity(
            AdditionalRetirementIdentityError::EntityMissing { .. },
        ) => (
            "scene.product-retirement-entity-missing",
            "Scene product retirement contains a missing entity",
            None,
        ),
        PrepareSceneProductTransactionError::Identity(
            AdditionalRetirementIdentityError::EntityNotOwned { .. },
        ) => (
            "scene.product-retirement-identity-unowned",
            "Scene product retirement token no longer owns its entity",
            None,
        ),
        PrepareSceneProductTransactionError::Identity(
            AdditionalRetirementIdentityError::DuplicateEntity { .. },
        ) => (
            "scene.product-retirement-entity-duplicate",
            "Scene product retirement contains a duplicate entity",
            None,
        ),
        PrepareSceneProductTransactionError::Identity(
            AdditionalRetirementIdentityError::ActiveSceneAxis { .. },
        )
        | PrepareSceneProductTransactionError::SceneOwnedRetirement => (
            "scene.product-retirement-scene-owned",
            "Scene product retirement contains a scene-owned entity",
            None,
        ),
        PrepareSceneProductTransactionError::Identity(
            AdditionalRetirementIdentityError::PersistentAxis { .. },
        ) => (
            "scene.product-retirement-persistent-owned",
            "Scene product retirement contains a persistent-identity-owned entity",
            None,
        ),
    };
    let diagnostic = match entity_id {
        Some(entity_id) => with_public_locator(
            diagnostic_error(code, summary),
            "entity-id",
            entity_id.as_str(),
        ),
        None => diagnostic_error(code, summary),
    };
    diagnostics.push(diagnostic);
    SceneSpawnReport {
        instance: None,
        diagnostics,
        retired_entities: 0,
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
    prepare_mutation_baseline(world);
    let prepared = prepare_exact_scene_instance_retirement(world, current, cause)
        .map_err(SceneInstanceRetirementTransactionError::Identity)?;
    let retirements = prepared.retiring_entities().to_vec();
    validate_scene_identity_support(world, &retirements)
        .map_err(SceneInstanceRetirementTransactionError::IdentitySupport)?;
    validate_existing_scene_persistent_apply(world, &retirements)
        .map_err(SceneInstanceRetirementTransactionError::PersistentApply)?;
    let hierarchy = prepare_scene_hierarchy_retirement(world, &retirements)
        .map_err(|_| SceneInstanceRetirementTransactionError::Hierarchy)?;
    let retirement = PreparedSceneEntityRetirement::prepare(world, &retirements, &hierarchy)
        .map_err(|_| SceneInstanceRetirementTransactionError::Lifecycle)?;
    let prepared_hierarchy = prepare_construction_batch(world, hierarchy.operations())
        .map_err(|_| SceneInstanceRetirementTransactionError::Hierarchy)?;
    prepared_hierarchy.commit(world);
    let mut domain = world
        .remove_resource::<WorldIdentityDomain>()
        .expect("prepared scene retirement requires its identity domain");
    let retired = prepared.commit(&mut domain);
    retirement.commit(world);
    world.insert_resource(domain);
    Ok(retired)
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
        None,
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
        world.get::<Parent>(entity).is_some()
            || world.get::<Children>(entity).is_some()
            || world
                .get_resource::<HierarchyRoots>()
                .is_some_and(|roots| roots.contains(entity))
    });
    if has_relationship {
        let hierarchy = prepare_scene_hierarchy_retirement(world, &live_entities)
            .expect("scene candidate hierarchy rollback must remain valid");
        let retirement = PreparedSceneEntityRetirement::prepare(world, &live_entities, &hierarchy)
            .expect("scene candidate hierarchy rollback must remain lifecycle-free");
        prepare_construction_batch(world, hierarchy.operations())
            .expect("scene candidate hierarchy rollback must remain prevalidated")
            .commit(world);
        retirement.commit(world);
        return;
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
    spawned_by_id
        .iter()
        .map(|(entity_id, entity)| {
            adopt_identity_entity(world, *entity).map(|token| (entity_id.clone(), token))
        })
        .collect()
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
    world: &mut World,
    entity_count: usize,
    commit: SceneIdentityCommit<'_>,
) -> Result<Option<WorldIdentityDomain>, IdentityDomainError> {
    if let SceneIdentityCommit::Projection(current) = commit {
        prepare_scene_projection_epoch_retirement(world, current)?;
        let settings = world.get_resource::<WorldIdentityDomain>().map_or_else(
            WorldIdentityDomainSettings::default,
            WorldIdentityDomain::settings,
        );
        let domain = WorldIdentityDomain::new(world, settings)?;
        domain.preflight_scene_instance_registration(world, entity_count)?;
        return Ok(Some(domain));
    }
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
            SceneIdentityCommit::Projection(_) => {
                unreachable!("projection prepares a detached domain")
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
) -> Result<PreparedSceneIdentityCommit, IdentityDomainError> {
    let entries = identity_tokens
        .iter()
        .map(|(entity_id, token)| (entity_id.clone(), *token))
        .collect::<Vec<_>>();
    match commit {
        SceneIdentityCommit::Register => {
            if let Some(domain) = new_domain {
                debug_assert!(!world.contains_resource::<WorldIdentityDomain>());
                let registration =
                    prepare_exact_scene_instance_registration(&domain, world, &entries)?;
                return Ok(PreparedSceneIdentityCommit::Register {
                    domain: Some(domain),
                    registration,
                });
            }
            let domain = world
                .get_resource::<WorldIdentityDomain>()
                .ok_or(IdentityDomainError::WorldDomainUnavailable)?;
            let registration = prepare_exact_scene_instance_registration(domain, world, &entries)?;
            Ok(PreparedSceneIdentityCommit::Register {
                domain: None,
                registration,
            })
        }
        SceneIdentityCommit::Replace(current) => {
            debug_assert!(new_domain.is_none());
            let replacement = prepare_exact_scene_instance_replacement(
                world,
                current,
                &entries,
                TombstoneCause::Replaced,
            )?;
            Ok(PreparedSceneIdentityCommit::Replace { replacement })
        }
        SceneIdentityCommit::Projection(current) => {
            let retirement = prepare_scene_projection_epoch_retirement(world, current)?;
            let predecessor = world
                .get_resource::<WorldIdentityDomain>()
                .map(WorldIdentityDomain::id);
            let domain =
                new_domain.expect("projection identity is prepared in a fresh detached domain");
            let registration = prepare_exact_scene_instance_registration(&domain, world, &entries)?;
            Ok(PreparedSceneIdentityCommit::Projection {
                domain,
                predecessor,
                retirement,
                registration,
            })
        }
    }
}

/// Retires one scene-managed stable identity before removing its runtime entity.
pub fn retire_and_despawn_scene_entity(
    world: &mut World,
    entity: Entity,
) -> Result<(), SceneEntityRetirementError> {
    prepare_mutation_baseline(world);
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
    let domain = world
        .get_resource::<WorldIdentityDomain>()
        .expect("scene entity retirement requires its identity domain");
    let locators = domain
        .locators_for_entity(world, entity)
        .map_err(SceneEntityRetirementError::Identity)?
        .ok_or(SceneEntityRetirementError::Identity(
            IdentityDomainError::EntityNotRegistered,
        ))?;
    let reference = locators
        .scene()
        .filter(|locator| {
            matches!(
                locator.entity(),
                RuntimeEntityReference::Scene { instance, entity, .. }
                    if *instance == source.instance_id && *entity == source.entity_id
            )
        })
        .ok_or(SceneEntityRetirementError::Identity(
            IdentityDomainError::StaleRegistration,
        ))?
        .entity();
    let prepared_identity =
        prepare_exact_entity_retirement(world, entity, reference, TombstoneCause::Despawned)
            .map_err(SceneEntityRetirementError::Identity)?;
    let hierarchy = prepare_scene_hierarchy_retirement(world, &[entity])
        .map_err(|_| SceneEntityRetirementError::Hierarchy)?;
    let retirement = PreparedSceneEntityRetirement::prepare(world, &[entity], &hierarchy)
        .map_err(|_| SceneEntityRetirementError::Lifecycle)?;
    let prepared_hierarchy = prepare_construction_batch(world, hierarchy.operations())
        .map_err(|_| SceneEntityRetirementError::Hierarchy)?;
    let mut domain = world
        .remove_resource::<WorldIdentityDomain>()
        .expect("prepared scene entity retirement requires its identity domain");
    prepared_identity.commit(&mut domain);
    prepared_hierarchy.commit(world);
    retirement.commit(world);
    world.insert_resource(domain);
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

/// Materializes an immutable expanded scene with the source association admitted by its owner.
#[must_use]
pub fn spawn_expanded_scene(
    world: &mut World,
    registry: &ComponentRegistry,
    candidate: &ExpandedSceneCandidate,
    database: Option<&ProjectAssetDatabase>,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_with_asset_context_and_product(
        world,
        registry,
        candidate.document(),
        Some(candidate.provenance()),
        database,
        SceneIdentityCommit::Register,
        None,
    )
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

#[cfg(test)]
mod provenance_admission_tests {
    use super::*;
    use crate::{SceneEntityProvenance, SceneEntityRecord};
    use nara_core::ItemLimit;
    use nara_ecs::Component;
    use nara_identity::EntityLookup;

    #[derive(Component)]
    struct ClaimedRuntimeState;

    #[test]
    fn native_retirement_claims_are_exact_and_deduplicated() {
        let mut world = World::new();
        let component_id = world.register_component::<ClaimedRuntimeState>();
        let claims = SceneNativeRetirementClaims::from_component_ids([component_id, component_id]);
        assert_eq!(claims.component_ids(), &[component_id]);
        assert!(claims.contains(component_id));
    }

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    fn invalid_sources(document: &SceneDocument) -> Vec<SceneEntityProvenanceMap> {
        let candidate = ExpandedSceneCandidate::plain(std::sync::Arc::new(document.clone()));
        let sources = candidate.provenance();
        let mut missing = sources.clone();
        missing.remove(&id("left"));
        let mut extra = sources.clone();
        extra.insert(id("extra"), SceneEntityProvenance::local(id("extra")));
        let mut rekeyed = sources.clone();
        let source = rekeyed.remove(&id("left")).unwrap();
        rekeyed.insert(id("stale"), source);
        let swapped = [
            (id("left"), sources[&id("right")].clone()),
            (id("right"), sources[&id("left")].clone()),
        ]
        .into_iter()
        .collect();
        vec![missing, extra, rekeyed, swapped]
    }

    #[test]
    fn invalid_provenance_rejects_before_scene_allocation_or_product_configuration() {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("left")),
            SceneEntityRecord::new(id("right")),
        ]);
        for sources in invalid_sources(&document) {
            let mut world = World::new();
            let before_entities = world
                .iter_entities()
                .map(|entity| entity.id())
                .collect::<Vec<_>>();
            let report = SceneSpawner::new().spawn_with_asset_context_and_product(
                &mut world,
                &registry,
                &document,
                Some(&sources),
                None,
                SceneIdentityCommit::Register,
                Some(SceneProductTransactionInput {
                    limits: SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
                    additional_retirements: &[],
                    configure: Box::new(|_| {
                        panic!("invalid provenance must reject before product configuration")
                    }),
                }),
            );
            assert!(report.instance.is_none());
            assert_eq!(
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<Vec<_>>(),
                before_entities
            );
            assert!(!world.contains_resource::<WorldIdentityDomain>());
            assert_eq!(
                report.diagnostics.iter().next().unwrap().code().as_str(),
                "scene.invalid-provenance"
            );
        }
    }

    #[test]
    fn invalid_provenance_replacement_preserves_old_entities_identity_and_sources() {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("left")),
            SceneEntityRecord::new(id("right")),
        ]);
        let mut world = World::new();
        let initial = spawn_scene(&mut world, &registry, &document);
        assert!(
            !initial.diagnostics.has_errors(),
            "{:#?}",
            initial.diagnostics
        );
        let current = initial
            .instance
            .expect("the baseline plain scene must materialize");
        let before = world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>();
        let stats = world.resource::<WorldIdentityDomain>().stats();
        let old_sources = before
            .iter()
            .filter_map(|entity| {
                world
                    .get::<SceneEntitySource>(*entity)
                    .cloned()
                    .map(|source| (*entity, source))
            })
            .collect::<Vec<_>>();
        assert_eq!(old_sources.len(), document.entities.len());
        for sources in invalid_sources(&document) {
            let report = SceneSpawner::new().spawn_with_asset_context_and_product(
                &mut world,
                &registry,
                &document,
                Some(&sources),
                None,
                SceneIdentityCommit::Replace(&current),
                None,
            );
            assert!(report.instance.is_none());
            assert_eq!(report.retired_entities(), 0);
            assert_eq!(
                report.diagnostics.iter().next().unwrap().code().as_str(),
                "scene.invalid-provenance"
            );
            assert_eq!(
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<Vec<_>>(),
                before
            );
            assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
            for (entity, source) in &old_sources {
                assert_eq!(world.get::<SceneEntitySource>(*entity), Some(source));
                assert_eq!(
                    current.resolve(&world, &source.entity_id),
                    EntityLookup::Resolved(*entity)
                );
            }
        }
    }
}
