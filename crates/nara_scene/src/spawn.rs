use std::{collections::BTreeMap, error::Error, fmt, sync::Arc};

mod preparation;
mod projection;
mod publication;
use preparation::UnboundScenePublication;
pub use preparation::{PreparedScene, prepare_scene, prepare_scene_replacement};
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
        prepare_exact_scene_instance_registration, prepare_registered_scene_instance_replacement,
        prepare_registered_scene_instance_retirement,
        prepare_registered_scene_projection_epoch_retirement,
        validate_additional_retirement_identity_axes, validate_identity_support_topology,
    },
    IdentityDomainError, RuntimeEntityReference, SceneInstanceId, SpawnedSceneInstance,
    TombstoneCause, WorldEntityToken, WorldIdentityDomain, WorldIdentityDomainId,
    WorldIdentityDomainSettings, adopt_identity_entity,
};
use nara_reflect::{
    __private::{
        UnboundComponentApplyBatch, declare_persistent_apply_targets,
        validate_declared_persistent_apply_targets, validate_fresh_persistent_component_apply,
        validate_persistent_apply_support_topology, validate_registered_persistent_apply_targets,
    },
    ComponentCodecError, ComponentRegistry, ComponentTypeId, ComponentValueMaterializationLimits,
};

use crate::{
    ExpandedSceneCandidate, SceneComponentBindings, SceneEntityId, SceneEntityProvenance,
    SceneEntityProvenanceMap, SceneOccurrenceKey, SourceDocumentKey,
    asset_bindings::SceneAssetBindings,
    candidate::{SceneMaterializationInput, validate_scene_provenance},
    diagnostics::{error as diagnostic_error, with_codec_error, with_public_locator},
    occurrence::{
        OccurrenceError, PreparedSceneOccurrence, SceneSourceContext, occurrence_for_instance,
        release_occurrence,
    },
    product_transaction::{
        PreparedSceneProductOverlay, SceneProductOverlayError, SceneProductOverlayWriter,
        SceneProductTransactionLimits,
    },
    runtime_hierarchy::{
        PreparedSceneEntityRetirement, build_scene_hierarchy_operations,
        combine_scene_hierarchy_operations, prepare_scene_hierarchy_retirement,
        validate_scene_entity_retirement_admission,
    },
    validation::{
        PreparedScene as PreflightScene, preflight_scene_with_context,
        preflight_scene_with_context_and_bindings,
    },
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
    pub(crate) context: Arc<SceneSourceContext>,
    /// Identity of the concrete materialized scene instance.
    pub instance_id: SceneInstanceId,
    /// Entity locator within this materialized instance, not a durable source address.
    pub entity_id: SceneEntityId,
    pub(crate) provenance: SceneEntityProvenance,
}

impl SceneEntitySource {
    /// Returns the complete typed authored origin independently of the materialized locator.
    #[must_use]
    pub const fn provenance(&self) -> &SceneEntityProvenance {
        &self.provenance
    }

    /// Returns the required containing document source, independently of Prefab lookup requests.
    #[must_use]
    pub fn containing_source(&self) -> &SourceDocumentKey {
        &self.context.source
    }

    /// Returns the caller-declared logical placement, independently of this runtime activation.
    #[must_use]
    pub fn occurrence(&self) -> &SceneOccurrenceKey {
        &self.context.occurrence
    }
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
pub(crate) struct SceneSpawner;

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
        bound_domain: WorldIdentityDomainId,
        instance: SceneInstanceId,
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
            Self::Register { instance, .. } => *instance,
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

    pub(crate) fn sync(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        candidate: &ExpandedSceneCandidate,
        occurrence: &SceneOccurrenceKey,
        database: Option<&ProjectAssetDatabase>,
        current: Option<&SpawnedSceneInstance>,
    ) -> SceneSpawnReport {
        match preparation::prepare_ordinary_scene(
            world,
            registry,
            candidate,
            occurrence,
            database,
            current.map_or(SceneIdentityCommit::Register, SceneIdentityCommit::Replace),
        ) {
            Ok(prepared) => prepared.commit(world),
            Err(report) => report,
        }
    }

    fn spawn_with_asset_context_and_product(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        candidate: &ExpandedSceneCandidate,
        occurrence: &SceneOccurrenceKey,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
        product: Option<SceneProductTransactionInput<'_>>,
    ) -> SceneSpawnReport {
        let bindings = SceneAssetBindings::from_world(world);
        self.spawn_with_asset_context_and_product_bindings(
            world, registry, candidate, occurrence, database, commit, product, bindings,
        )
    }

    fn spawn_with_asset_context_and_product_bindings(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        candidate: &ExpandedSceneCandidate,
        occurrence: &SceneOccurrenceKey,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
        product: Option<SceneProductTransactionInput<'_>>,
        bindings: SceneAssetBindings,
    ) -> SceneSpawnReport {
        match self.prepare_with_asset_context_and_product_bindings(
            world,
            registry,
            candidate,
            occurrence,
            database,
            commit,
            product,
            bindings,
            &SceneNativeRetirementClaims::default(),
        ) {
            Ok(prepared) => preparation::finish_scene_publication(world, prepared),
            Err(report) => report,
        }
    }

    fn prepare_with_asset_context_and_product_bindings(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        candidate: &ExpandedSceneCandidate,
        occurrence: &SceneOccurrenceKey,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
        product: Option<SceneProductTransactionInput<'_>>,
        bindings: SceneAssetBindings,
        claims: &SceneNativeRetirementClaims,
    ) -> Result<PreparedScenePublication, SceneSpawnReport> {
        let definition = preparation::ScenePreparationDefinition::capture(world, registry)?;
        if !matches!(commit, SceneIdentityCommit::Projection(_)) {
            definition.require_managed_definition()?;
        }
        self.prepare_unbound_with_asset_context_and_product_bindings(
            world,
            registry,
            definition,
            SceneMaterializationInput::Expanded(candidate),
            occurrence,
            database,
            commit,
            product,
            bindings,
            claims,
            ComponentValueMaterializationLimits::default(),
        )?
        .bind(world)
    }

    fn prepare_unbound_with_asset_context_and_product_bindings(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        definition: preparation::ScenePreparationDefinition,
        candidate: SceneMaterializationInput<'_>,
        occurrence: &SceneOccurrenceKey,
        database: Option<&ProjectAssetDatabase>,
        commit: SceneIdentityCommit<'_>,
        product: Option<SceneProductTransactionInput<'_>>,
        bindings: SceneAssetBindings,
        claims: &SceneNativeRetirementClaims,
        value_limits: ComponentValueMaterializationLimits,
    ) -> Result<UnboundScenePublication, SceneSpawnReport> {
        if !definition.matches_registry(registry) {
            return Err(preparation::definition_rejection());
        }
        let (database, value_limits) = match candidate {
            SceneMaterializationInput::Expanded(_) => (database, value_limits),
            #[cfg(feature = "serde")]
            SceneMaterializationInput::Runtime(candidate) => {
                definition
                    .bindings()
                    .filter(|bindings| candidate.definitions().matches(bindings))
                    .ok_or_else(preparation::definition_rejection)?;
                definition.validate(world)?;
                let limits = candidate.limits().value_limits();
                // Cook/admit validated this immutable input against these exact definitions.
                // Every version remains current, so shared preflight cannot enter migration.
                // Runtime product identities resolve only through the receiving asset bindings.
                (None, limits)
            }
        };
        if registry
            .schema_for_type::<crate::SceneDataComponents>()
            .is_some()
        {
            return Err(preparation::data_binding_rejection());
        }
        let document = candidate.document();
        let provenance = candidate.provenance();
        if validate_scene_provenance(document, provenance).is_err() {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic_error(
                "scene.invalid-provenance",
                "Expanded scene source provenance is inconsistent",
            ));
            return Err(SceneSpawnReport {
                diagnostics,
                ..Default::default()
            });
        }
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
        let mut preflight = component_batch.with_decode_context(database, |context| {
            context.set_materialization_limits(value_limits);
            match definition.bindings() {
                None => preflight_scene_with_context(document, registry, context),
                Some(component_bindings) => preflight_scene_with_context_and_bindings(
                    document,
                    registry,
                    component_bindings,
                    context,
                ),
            }
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
        if preflight
            .entities
            .iter()
            .any(|entity| !entity.data_components.is_empty())
        {
            world.register_component::<crate::SceneDataComponents>();
            if validate_registered_persistent_component_apply::<crate::SceneDataComponents>(
                world, None,
            )
            .is_err()
            {
                return Err(preparation::data_lifecycle_rejection());
            }
        }
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
        diagnostics = validate_scene_predecessor(
            world,
            commit,
            prepared_product.as_ref(),
            claims,
            diagnostics,
        )?;
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
        let predecessor = match commit {
            SceneIdentityCommit::Register => None,
            SceneIdentityCommit::Replace(current) => Some(current),
            SceneIdentityCommit::Projection(current) => current,
        };
        let prepared_occurrence = PreparedSceneOccurrence::prepare(
            world,
            candidate.source_key(),
            occurrence,
            predecessor,
            matches!(commit, SceneIdentityCommit::Projection(_)),
        )
        .map_err(|error| occurrence_rejection(DiagnosticReport::default(), error))?;
        let _identity_admission =
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
        let mut component_locations = Vec::new();
        let mut components = Vec::new();
        for (entity_index, entity) in preflight.entities.iter_mut().enumerate() {
            for component in std::mem::take(&mut entity.components) {
                component_locations.push((entity_index, component.id));
                components.push(component.prepared);
            }
        }
        let prepared_values = match component_batch.prepare_unbound(components) {
            Ok(prepared) => prepared,
            Err((index, error)) => {
                let (entity_index, component_id) = &component_locations[index];
                let entity_id = &preflight.entities[*entity_index].id;
                diagnostics.push(with_codec_error(
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.component-apply-failed",
                                "Component apply failed",
                            ),
                            "entity-id",
                            entity_id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    &error,
                    registry.schema(component_id),
                ));
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };
        Ok(UnboundScenePublication {
            definition,
            preflight,
            component_locations,
            prepared_values,
            prepared_product,
            prepared_occurrence,
            provenance: provenance.clone(),
            target: preparation::ScenePublicationTarget::from_borrowed(commit),
            claims: claims.clone(),
            diagnostics,
        })
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
    let occurrence = match occurrence_for_instance(world, current) {
        Ok(occurrence) => occurrence.clone(),
        Err(error) => return occurrence_rejection(DiagnosticReport::default(), error),
    };
    SceneSpawner::new().spawn_with_asset_context_and_product_bindings(
        world,
        registry,
        candidate,
        &occurrence,
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
    let occurrence = match occurrence_for_instance(world, current) {
        Ok(occurrence) => occurrence.clone(),
        Err(error) => return Err(occurrence_rejection(DiagnosticReport::default(), error)),
    };
    SceneSpawner::new()
        .prepare_with_asset_context_and_product_bindings(
            world,
            registry,
            candidate,
            &occurrence,
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

impl SceneComponentBindings {
    /// Prepares a complete mixed replacement before product-owned native retirement starts.
    ///
    /// This advanced operation retains the existing claims, product writes and explicit
    /// commit/retry/discard custody of [`PreparedSceneReplacement`]. Data definitions are captured
    /// with the same managed authority as ordinary mixed preparation.
    #[doc(hidden)]
    pub fn prepare_scene_replacement_with_claims<F>(
        &self,
        world: &mut World,
        candidate: &ExpandedSceneCandidate,
        database: Option<&ProjectAssetDatabase>,
        current: &SpawnedSceneInstance,
        assets: SceneAssetBindings,
        claims: &SceneNativeRetirementClaims,
        limits: SceneProductTransactionLimits,
        additional_retirements: &[WorldEntityToken],
        configure: F,
    ) -> Result<PreparedSceneReplacement, SceneSpawnReport>
    where
        F: FnOnce(&mut SceneProductOverlayWriter<'_>),
    {
        self.prepare_selected_scene_replacement_with_claims(
            world,
            SceneMaterializationInput::Expanded(candidate),
            database,
            current,
            assets,
            claims,
            SceneProductTransactionInput {
                limits,
                additional_retirements,
                configure: Box::new(configure),
            },
        )
    }

    /// Prepares a processed native/data replacement before native-domain retirement starts.
    ///
    /// Product identities use the content owner's exact receiving asset bindings. The callback
    /// runs once during preparation; commit and retry retain the existing native retirement
    /// custody without rerunning codecs or consulting authoring sources.
    #[cfg(feature = "serde")]
    #[doc(hidden)]
    pub fn prepare_runtime_scene_replacement_with_claims<F>(
        &self,
        world: &mut World,
        candidate: &crate::runtime_scene::RuntimeSceneCandidate,
        current: &SpawnedSceneInstance,
        assets: SceneAssetBindings,
        claims: &SceneNativeRetirementClaims,
        limits: SceneProductTransactionLimits,
        additional_retirements: &[WorldEntityToken],
        configure: F,
    ) -> Result<PreparedSceneReplacement, SceneSpawnReport>
    where
        F: FnOnce(&mut SceneProductOverlayWriter<'_>),
    {
        self.prepare_selected_scene_replacement_with_claims(
            world,
            SceneMaterializationInput::Runtime(candidate),
            None,
            current,
            assets,
            claims,
            SceneProductTransactionInput {
                limits,
                additional_retirements,
                configure: Box::new(configure),
            },
        )
    }

    fn prepare_selected_scene_replacement_with_claims(
        &self,
        world: &mut World,
        candidate: SceneMaterializationInput<'_>,
        database: Option<&ProjectAssetDatabase>,
        current: &SpawnedSceneInstance,
        assets: SceneAssetBindings,
        claims: &SceneNativeRetirementClaims,
        product: SceneProductTransactionInput<'_>,
    ) -> Result<PreparedSceneReplacement, SceneSpawnReport> {
        let occurrence = occurrence_for_instance(world, current)
            .map_err(|error| occurrence_rejection(DiagnosticReport::default(), error))?
            .clone();
        let definition = preparation::ScenePreparationDefinition::capture_with_bindings(
            world,
            self.native_registry(),
            self,
        )?;
        SceneSpawner::new()
            .prepare_unbound_with_asset_context_and_product_bindings(
                world,
                self.native_registry(),
                definition,
                candidate,
                &occurrence,
                database,
                SceneIdentityCommit::Replace(current),
                Some(product),
                assets,
                claims,
                ComponentValueMaterializationLimits::default(),
            )?
            .bind(world)
            .map(PreparedSceneReplacement)
    }
}

fn validate_scene_predecessor(
    world: &World,
    commit: SceneIdentityCommit<'_>,
    prepared_product: Option<&PreparedSceneProductTransaction>,
    claims: &SceneNativeRetirementClaims,
    mut diagnostics: DiagnosticReport,
) -> Result<DiagnosticReport, SceneSpawnReport> {
    // Admission for retiring the old membership precedes scratch entity allocation. A global
    // despawn observer could also run while abandoning an otherwise empty candidate.
    if let SceneIdentityCommit::Replace(current) | SceneIdentityCommit::Projection(Some(current)) =
        commit
    {
        let retirement = match prepare_registered_scene_instance_retirement(
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
        if let Err(error) =
            validate_registered_persistent_apply_targets(world, old_entities.iter().copied())
        {
            return Err(persistent_apply_rejection(diagnostics, &error));
        }
        let mut all_entities = old_entities.to_vec();
        if let Some(product) = prepared_product {
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
    Ok(diagnostics)
}

fn prepare_scene_product_transaction(
    world: &World,
    registry: &ComponentRegistry,
    scene: &PreflightScene,
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
    validate_scene_product_current(world, &product.overlay, &product.additional_retirements)?;
    Ok(PreparedSceneProductTransaction {
        overlay: product.overlay,
        additional_retirements: product.additional_retirements,
    })
}

fn validate_scene_product_current(
    world: &World,
    overlay: &PreparedSceneProductOverlay,
    additional_retirements: &[WorldEntityToken],
) -> Result<(), PrepareSceneProductTransactionError> {
    validate_additional_retirement_identity_axes(world, additional_retirements.iter().copied())
        .map_err(PrepareSceneProductTransactionError::Identity)?;
    for token in additional_retirements {
        if world.get::<SceneEntitySource>(token.entity()).is_some() {
            return Err(PrepareSceneProductTransactionError::SceneOwnedRetirement);
        }
    }
    overlay
        .validate_resources(world)
        .map_err(PrepareSceneProductTransactionError::Overlay)?;
    Ok(())
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
        PrepareSceneProductTransactionError::Overlay(SceneProductOverlayError::ResourceChanged) => {
            (
                "scene.product-resource-changed",
                "Scene product resource incarnation or content changed after baseline capture",
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
    preflight: &PreflightScene,
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
    Occurrence(OccurrenceError),
    Identity(IdentityDomainError),
    IdentitySupport(IdentitySupportTopologyError),
    PersistentApply(ComponentCodecError),
    Hierarchy,
    Lifecycle,
}

/// Ends exactly one Scene activation and despawns its remaining runtime entities.
///
/// This also ends an empty instance, or one whose entities have already been individually retired.
/// A stale or foreign receipt rejects without ending another activation. Rejection preserves the
/// admitted Scene identities and entities; ordinary pending World commands are flushed first.
/// Native lifecycle work must be completed by its owning domain before this operation is eligible.
///
/// Returns the number of runtime entities removed. Identity history remains in the World domain;
/// this is not history eviction or a disposable projection epoch reset. Owners of a
/// [`SceneProjection`] must use [`SceneProjection::clear`] instead so their receipt and epoch are
/// retired together. Dropping a receipt alone never ends an instance.
pub fn retire_and_despawn_scene_instance(
    world: &mut World,
    current: &SpawnedSceneInstance,
) -> Result<usize, DiagnosticReport> {
    retire_scene_instance_exact(world, current, TombstoneCause::Unloaded)
        .map(|retired| retired.len())
        .map_err(|error| {
            let diagnostic = match error {
                SceneInstanceRetirementTransactionError::Occurrence(error) => {
                    return occurrence_rejection(DiagnosticReport::default(), error).diagnostics;
                }
                SceneInstanceRetirementTransactionError::Identity(error) => {
                    crate::diagnostics::with_identity_error(
                        diagnostic_error(
                            "scene.identity-retirement-failed",
                            "Scene identity retirement failed",
                        ),
                        &error,
                    )
                }
                SceneInstanceRetirementTransactionError::IdentitySupport(error) => {
                    crate::diagnostics::with_identity_support_error(
                        diagnostic_error(
                            "scene.identity-support-ineligible",
                            "Scene identity support is ineligible for retirement",
                        ),
                        &error,
                    )
                }
                SceneInstanceRetirementTransactionError::PersistentApply(error) => {
                    with_codec_error(
                        diagnostic_error(
                            "scene.persistent-apply-ineligible",
                            "Persistent scene components are ineligible for retirement",
                        ),
                        &error,
                        None,
                    )
                }
                SceneInstanceRetirementTransactionError::Hierarchy => diagnostic_error(
                    "scene.hierarchy-retirement-ineligible",
                    "Scene hierarchy is ineligible for exact retirement",
                ),
                SceneInstanceRetirementTransactionError::Lifecycle => diagnostic_error(
                    "scene.lifecycle-retirement-ineligible",
                    "Scene retirement would run unsupported lifecycle work",
                ),
            };
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic);
            diagnostics
        })
}

pub(crate) fn retire_scene_instance_exact(
    world: &mut World,
    current: &SpawnedSceneInstance,
    cause: TombstoneCause,
) -> Result<Vec<Entity>, SceneInstanceRetirementTransactionError> {
    prepare_mutation_baseline(world);
    let prepared = prepare_registered_scene_instance_retirement(world, current, cause)
        .map_err(SceneInstanceRetirementTransactionError::Identity)?;
    occurrence_for_instance(world, current)
        .map_err(SceneInstanceRetirementTransactionError::Occurrence)?;
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
    release_occurrence(world, current);
    Ok(retired)
}

fn occurrence_rejection(
    mut diagnostics: DiagnosticReport,
    error: OccurrenceError,
) -> SceneSpawnReport {
    let (code, message) = match error {
        OccurrenceError::Occupied => (
            "scene.occurrence-occupied",
            "The logical Scene occurrence is already active",
        ),
        OccurrenceError::Budget => (
            "scene.occurrence-budget-exceeded",
            "Scene occurrence admission exceeds its active ownership budget",
        ),
        OccurrenceError::WrongWorld => (
            "scene.occurrence-world-mismatch",
            "Scene occurrence ownership belongs to another World",
        ),
        OccurrenceError::Lifecycle => (
            "scene.occurrence-lifecycle-ineligible",
            "Scene occurrence publication would run unsupported lifecycle work",
        ),
        OccurrenceError::Missing | OccurrenceError::Changed => (
            "scene.occurrence-association-changed",
            "The exact Scene occurrence association is missing or changed",
        ),
    };
    diagnostics.push(diagnostic_error(code, message));
    SceneSpawnReport {
        diagnostics,
        ..Default::default()
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
    world: &World,
    entity_count: usize,
    commit: SceneIdentityCommit<'_>,
) -> Result<Option<WorldIdentityDomain>, IdentityDomainError> {
    if let SceneIdentityCommit::Projection(current) = commit {
        prepare_registered_scene_projection_epoch_retirement(world, current)?;
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
                    bound_domain: domain.id(),
                    domain: Some(domain),
                    instance: registration.instance_id(),
                });
            }
            let domain = world
                .get_resource::<WorldIdentityDomain>()
                .ok_or(IdentityDomainError::WorldDomainUnavailable)?;
            let registration = prepare_exact_scene_instance_registration(domain, world, &entries)?;
            Ok(PreparedSceneIdentityCommit::Register {
                domain: None,
                bound_domain: domain.id(),
                instance: registration.instance_id(),
            })
        }
        SceneIdentityCommit::Replace(current) => {
            debug_assert!(new_domain.is_none());
            let replacement = prepare_registered_scene_instance_replacement(
                world,
                current,
                &entries,
                TombstoneCause::Replaced,
            )?;
            Ok(PreparedSceneIdentityCommit::Replace { replacement })
        }
        SceneIdentityCommit::Projection(current) => {
            let retirement = prepare_registered_scene_projection_epoch_retirement(world, current)?;
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

/// Materializes a required source-bound candidate into one explicitly declared logical slot.
#[must_use]
pub fn spawn_scene(
    world: &mut World,
    registry: &ComponentRegistry,
    candidate: &ExpandedSceneCandidate,
    occurrence: &SceneOccurrenceKey,
    database: Option<&ProjectAssetDatabase>,
) -> SceneSpawnReport {
    match prepare_scene(world, registry, candidate, occurrence, database) {
        Ok(prepared) => prepared.commit(world),
        Err(report) => report,
    }
}

#[cfg(test)]
mod provenance_admission_tests {
    use super::*;
    use nara_ecs::Component;

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

    #[test]
    fn prepared_registration_rejects_intervening_domain_or_allocator_publication() {
        use crate::{SceneDocument, SourceDocumentKind};
        for installed in [false, true] {
            let mut world = World::new();
            let mut registry = ComponentRegistry::new();
            registry.freeze().unwrap();
            let candidate = ExpandedSceneCandidate::plain(
                SourceDocumentKey::named(
                    "56d73fb3-6fc0-435b-956b-1d4df1569e3a".parse().unwrap(),
                    "registration",
                    SourceDocumentKind::Scene,
                )
                .unwrap(),
                Arc::new(SceneDocument::default()),
            );
            if installed {
                let report = spawn_scene(
                    &mut world,
                    &registry,
                    &candidate,
                    &SceneOccurrenceKey::new("register", "baseline").unwrap(),
                    None,
                );
                assert!(report.instance.is_some());
            }
            let pending_slot = SceneOccurrenceKey::new("register", "pending").unwrap();
            let bindings = SceneAssetBindings::from_world(&world);
            let prepared = SceneSpawner::new()
                .prepare_with_asset_context_and_product_bindings(
                    &mut world,
                    &registry,
                    &candidate,
                    &pending_slot,
                    None,
                    SceneIdentityCommit::Register,
                    None,
                    bindings,
                    &SceneNativeRetirementClaims::default(),
                )
                .unwrap_or_else(|report| panic!("{:?}", report.diagnostics));
            let intervening = spawn_scene(
                &mut world,
                &registry,
                &candidate,
                &SceneOccurrenceKey::new("register", "intervening").unwrap(),
                None,
            )
            .instance
            .unwrap();
            let stats = world.resource::<WorldIdentityDomain>().stats();
            let (prepared, report) = match prepared.commit(&mut world) {
                Ok(_) => panic!("an old registration proof cannot survive another publication"),
                Err(rejected) => rejected,
            };
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.prepared-baseline-changed"
            }));
            assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
            assert!(prepared.discard(&mut world).is_ok());
            assert!(
                spawn_scene(&mut world, &registry, &candidate, &pending_slot, None)
                    .instance
                    .is_some()
            );
            retire_and_despawn_scene_instance(&mut world, &intervening).unwrap();
        }
    }
}
