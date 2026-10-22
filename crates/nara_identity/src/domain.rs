use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_core::ItemLimit;
use nara_ecs::{
    __private::{
        validate_registered_persistent_component_apply, validate_resource_insertion,
        validate_resource_scope,
    },
    Component, Entity, Resource, World, prepare_lifecycle_free_despawn,
    world::WorldId,
};
use thiserror::Error;

use crate::{
    EntityReference, IdentityAllocationError, IdentityRemapError, PersistentRuntimeReference,
    RuntimeEntityIncarnation, RuntimeEntityReference, SceneEntityId, SceneInstanceId,
    WorldEntityLocator, WorldEntityLocatorRemap, WorldIdentityDomainId,
    allocator::MonotonicNonZeroU64Allocator,
};

const DEFAULT_LIFETIME_CLAIMS: usize = 1_048_576;
const DEFAULT_PERSISTENT_LOGICAL_RECORDS: usize = 1_048_576;
const DEFAULT_RECENT_TOMBSTONES: usize = 4_096;

static NEXT_WORLD_IDENTITY_DOMAIN_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_RUNTIME_ENTITY_INCARNATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldIdentityDomainSettings {
    lifetime_claims: ItemLimit,
    persistent_logical_records: ItemLimit,
    recent_tombstones: ItemLimit,
}

impl WorldIdentityDomainSettings {
    pub const fn new(
        lifetime_claims: ItemLimit,
        persistent_logical_records: ItemLimit,
        recent_tombstones: ItemLimit,
    ) -> Self {
        Self {
            lifetime_claims,
            persistent_logical_records,
            recent_tombstones,
        }
    }

    #[must_use]
    pub const fn lifetime_claims(self) -> ItemLimit {
        self.lifetime_claims
    }

    #[must_use]
    pub const fn recent_tombstones(self) -> ItemLimit {
        self.recent_tombstones
    }

    #[must_use]
    pub const fn persistent_logical_records(self) -> ItemLimit {
        self.persistent_logical_records
    }
}

impl Default for WorldIdentityDomainSettings {
    fn default() -> Self {
        Self {
            lifetime_claims: ItemLimit::new(DEFAULT_LIFETIME_CLAIMS)
                .expect("default identity claim limit is non-zero"),
            persistent_logical_records: ItemLimit::new(DEFAULT_PERSISTENT_LOGICAL_RECORDS)
                .expect("default persistent logical record limit is non-zero"),
            recent_tombstones: ItemLimit::new(DEFAULT_RECENT_TOMBSTONES)
                .expect("default identity tombstone limit is non-zero"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityIdentityAxis {
    Scene,
    Persistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TombstoneCause {
    Despawned,
    Unloaded,
    Replaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentRetirementDisposition {
    Unload,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdentityTombstoneSubject {
    SceneInstance(SceneInstanceId),
    Entity(RuntimeEntityReference),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityTombstone {
    sequence: NonZeroU64,
    cause: TombstoneCause,
}

impl IdentityTombstone {
    #[must_use]
    pub const fn sequence(&self) -> NonZeroU64 {
        self.sequence
    }

    #[must_use]
    pub const fn cause(&self) -> TombstoneCause {
        self.cause
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityLookup {
    Resolved(Entity),
    Tombstoned(Option<IdentityTombstone>),
    Unloaded,
    Deleted,
    StaleRuntime,
    Missing,
    ContextRequired,
    DomainUnavailable,
    WrongWorldBinding,
    WrongDomain {
        expected: WorldIdentityDomainId,
        actual: WorldIdentityDomainId,
    },
    StaleRegistration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityDomainStats {
    pub lifetime_claims: usize,
    pub claimed_scene_instances: usize,
    pub active_scene_instances: usize,
    pub active_scene_entities: usize,
    pub active_persistent_entities: usize,
    pub persistent_logical_records: usize,
    pub unloaded_persistent_entities: usize,
    pub deleted_persistent_entities: usize,
    pub registered_entities: usize,
    pub recent_tombstones: usize,
}

/// An opaque runtime entity capability minted by the identity domain installed in one world.
///
/// The token is intentionally not serializable. Mutation APIs accept it instead of a bare Bevy
/// `Entity`, whose allocator bits do not encode world ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldEntityToken {
    domain: WorldIdentityDomainId,
    entity: Entity,
    incarnation: RuntimeEntityIncarnation,
}

#[derive(Debug, Component)]
struct IdentityEntityMarker {
    domain: WorldIdentityDomainId,
    incarnation: RuntimeEntityIncarnation,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum IdentitySupportTopologyError {
    #[error("identity support lifecycle topology is incompatible with scene publication")]
    LifecycleConflict,
    #[error("identity support target no longer exists")]
    TargetMissing,
}

/// Failure to prove that an exact runtime retirement set has no semantic identity axes.
#[doc(hidden)]
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum AdditionalRetirementIdentityError {
    #[error("world has no identity domain")]
    WorldDomainUnavailable,
    #[error("world identity domain resource is bound to a different world")]
    WorldBindingMismatch,
    #[error("additional retirement contains a missing entity: {entity:?}")]
    EntityMissing { entity: Entity },
    #[error("additional retirement contains a duplicate entity: {entity:?}")]
    DuplicateEntity { entity: Entity },
    #[error("additional retirement token belongs to a different identity domain: {entity:?}")]
    TokenWrongDomain { entity: Entity },
    #[error("additional retirement token no longer owns its runtime entity: {entity:?}")]
    EntityNotOwned { entity: Entity },
    #[error("additional retirement entity has an active scene identity axis: {entity:?}")]
    ActiveSceneAxis { entity: Entity },
    #[error("additional retirement entity has a persistent identity axis: {entity:?}")]
    PersistentAxis { entity: Entity },
}

#[doc(hidden)]
pub fn validate_identity_support_topology(
    world: &mut World,
    targets: impl IntoIterator<Item = Entity>,
) -> Result<(), IdentitySupportTopologyError> {
    let targets = targets.into_iter().collect::<BTreeSet<_>>();
    world.register_component::<IdentityEntityMarker>();
    world.register_component::<WorldIdentityDomain>();
    world.flush();
    validate_registered_persistent_component_apply::<IdentityEntityMarker>(world, None)
        .and_then(|()| {
            if world.contains_resource::<WorldIdentityDomain>() {
                validate_resource_scope::<WorldIdentityDomain>(world)
            } else {
                validate_resource_insertion::<WorldIdentityDomain>(world)
            }
        })
        .map_err(|_| IdentitySupportTopologyError::LifecycleConflict)?;
    for target in targets {
        world
            .get_entity(target)
            .map_err(|_| IdentitySupportTopologyError::TargetMissing)?;
        validate_registered_persistent_component_apply::<IdentityEntityMarker>(world, Some(target))
            .map_err(|_| IdentitySupportTopologyError::LifecycleConflict)?;
    }
    Ok(())
}

#[cfg(test)]
mod identity_support_topology_tests {
    use nara_ecs::{
        Resource,
        lifecycle::{Add, Despawn, Insert},
        observer::{Observer, On},
        system::ResMut,
    };

    use super::*;

    #[derive(Resource, Default)]
    struct SupportCanary(u32);

    #[test]
    fn target_validation_rejects_identity_marker_observer_before_retirement() {
        let mut world = World::new();
        world.init_resource::<SupportCanary>();
        let domain = WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default())
            .expect("identity domain must initialize");
        let domain_id = domain.id();
        world.insert_resource(domain);
        let target = world
            .spawn(IdentityEntityMarker {
                domain: domain_id,
                incarnation: allocate_runtime_entity_incarnation().unwrap(),
            })
            .id();
        let marker = world.component_id::<IdentityEntityMarker>().unwrap();
        world.spawn(
            Observer::new(|_: On<Despawn<()>>, mut canary: ResMut<SupportCanary>| canary.0 += 1)
                .with_entity(target)
                .with_component(marker),
        );
        world.flush();

        let error = validate_identity_support_topology(&mut world, [target]).unwrap_err();

        assert_eq!(error, IdentitySupportTopologyError::LifecycleConflict);
        assert!(world.get_entity(target).is_ok());
        assert_eq!(world.resource::<SupportCanary>().0, 0);
    }

    #[test]
    fn identity_spawn_rejects_event_global_add_observer_before_allocation() {
        let mut world = World::new();
        world.init_resource::<SupportCanary>();
        let domain =
            WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
        world.insert_resource(domain);
        world.add_observer(|_: On<Add<()>>, mut canary: ResMut<SupportCanary>| canary.0 += 1);
        world.flush();
        let baseline_entities = world.iter_entities().count();

        assert_eq!(
            spawn_identity_entity(&mut world),
            Err(IdentityDomainError::LifecycleConflict)
        );

        assert_eq!(world.iter_entities().count(), baseline_entities);
        assert_eq!(world.resource::<SupportCanary>().0, 0);
    }

    #[test]
    fn first_adoption_rejects_target_add_and_insert_observers_before_marker_insertion() {
        for insert in [false, true] {
            let mut world = World::new();
            world.init_resource::<SupportCanary>();
            let domain =
                WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
            world.insert_resource(domain);
            let target = world.spawn_empty().id();
            let marker = world.register_component::<IdentityEntityMarker>();
            let observer = if insert {
                Observer::new(|_: On<Insert<()>>, mut canary: ResMut<SupportCanary>| {
                    canary.0 += 1;
                })
                .with_entity(target)
                .with_component(marker)
            } else {
                Observer::new(|_: On<Add<()>>, mut canary: ResMut<SupportCanary>| {
                    canary.0 += 1;
                })
                .with_entity(target)
            };
            world.spawn(observer);
            world.flush();
            let before = world.resource::<WorldIdentityDomain>().stats();
            let entities = world.iter_entities().count();

            assert_eq!(
                adopt_identity_entity(&mut world, target),
                Err(IdentityDomainError::LifecycleConflict)
            );

            assert!(world.get::<IdentityEntityMarker>(target).is_none());
            assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
            assert_eq!(world.iter_entities().count(), entities);
            assert_eq!(world.resource::<SupportCanary>().0, 0);
        }
    }
}

impl WorldEntityToken {
    #[must_use]
    pub const fn incarnation(self) -> RuntimeEntityIncarnation {
        self.incarnation
    }
    #[must_use]
    pub const fn domain_id(self) -> WorldIdentityDomainId {
        self.domain
    }

    #[must_use]
    pub const fn entity(self) -> Entity {
        self.entity
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldEntityLocators {
    scene: Option<WorldEntityLocator>,
    persistent: Option<WorldEntityLocator>,
}

impl WorldEntityLocators {
    #[must_use]
    pub const fn scene(&self) -> Option<&WorldEntityLocator> {
        self.scene.as_ref()
    }

    #[must_use]
    pub const fn persistent(&self) -> Option<&WorldEntityLocator> {
        self.persistent.as_ref()
    }

    pub fn iter(&self) -> impl Iterator<Item = &WorldEntityLocator> {
        self.scene.iter().chain(self.persistent.iter())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnedSceneInstance {
    domain: WorldIdentityDomainId,
    instance: SceneInstanceId,
    entity_ids: Vec<SceneEntityId>,
    incarnations: BTreeMap<SceneEntityId, RuntimeEntityIncarnation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneIdentitySnapshot {
    domain: WorldIdentityDomainId,
    instance: SceneInstanceId,
    entities: BTreeMap<SceneEntityId, SceneIdentityReferences>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SceneIdentityReferences {
    pub(crate) scene: RuntimeEntityReference,
    pub(crate) persistent: Option<RuntimeEntityReference>,
}

impl SceneIdentitySnapshot {
    #[must_use]
    pub const fn domain_id(&self) -> WorldIdentityDomainId {
        self.domain
    }

    #[must_use]
    pub const fn instance_id(&self) -> SceneInstanceId {
        self.instance
    }

    pub fn entity_ids(&self) -> impl Iterator<Item = &SceneEntityId> {
        self.entities.keys()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub(crate) fn references(&self, entity: &SceneEntityId) -> Option<&SceneIdentityReferences> {
        self.entities.get(entity)
    }
}

impl SpawnedSceneInstance {
    #[must_use]
    pub const fn domain_id(&self) -> WorldIdentityDomainId {
        self.domain
    }

    #[must_use]
    pub const fn instance_id(&self) -> SceneInstanceId {
        self.instance
    }

    #[must_use]
    pub fn entity_ids(&self) -> &[SceneEntityId] {
        &self.entity_ids
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entity_ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entity_ids.is_empty()
    }

    #[must_use]
    pub fn contains(&self, entity: &SceneEntityId) -> bool {
        self.entity_ids.binary_search(entity).is_ok()
    }

    #[must_use]
    pub fn runtime_reference(&self, entity: &SceneEntityId) -> Option<RuntimeEntityReference> {
        self.incarnations.get(entity).map(|incarnation| {
            RuntimeEntityReference::scene(self.instance, entity.clone(), *incarnation)
        })
    }

    #[must_use]
    pub fn locator(&self, entity: &SceneEntityId) -> Option<WorldEntityLocator> {
        self.runtime_reference(entity)
            .map(|reference| WorldEntityLocator::new(self.domain, reference))
    }

    #[must_use]
    pub fn resolve(&self, world: &World, entity: &SceneEntityId) -> EntityLookup {
        let Some(domain) = world.get_resource::<WorldIdentityDomain>() else {
            return EntityLookup::DomainUnavailable;
        };
        if domain.validate_world_binding(world).is_err() {
            return EntityLookup::WrongWorldBinding;
        }
        if self.domain != domain.id {
            return EntityLookup::WrongDomain {
                expected: domain.id,
                actual: self.domain,
            };
        }
        self.runtime_reference(entity)
            .map_or(EntityLookup::Missing, |reference| {
                domain.lookup(world, &reference)
            })
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct EntityIdentityAxes {
    scene: Option<RuntimeEntityReference>,
    persistent: Option<RuntimeEntityReference>,
}

struct PreparedRetirement {
    instance: Option<SceneInstanceId>,
    entities: Vec<Entity>,
    identities: Vec<EntityIdentityAxes>,
    sequence_allocator: MonotonicNonZeroU64Allocator,
    tombstones: Vec<(IdentityTombstoneSubject, IdentityTombstone)>,
    persistent_disposition: PersistentRetirementDisposition,
}

#[derive(Debug, Clone, Copy)]
enum PersistentLogicalRecord {
    Active {
        entity: Entity,
        incarnation: RuntimeEntityIncarnation,
    },
    Unloaded {
        incarnation: RuntimeEntityIncarnation,
    },
    Deleted {
        incarnation: RuntimeEntityIncarnation,
    },
}

impl PersistentLogicalRecord {
    fn incarnation(self) -> RuntimeEntityIncarnation {
        match self {
            Self::Active { incarnation, .. }
            | Self::Unloaded { incarnation }
            | Self::Deleted { incarnation } => incarnation,
        }
    }
}

/// Fully prevalidated identity registration for one unpublished Scene Instance candidate.
#[doc(hidden)]
pub struct PreparedSceneInstanceRegistration {
    domain: WorldIdentityDomainId,
    instance: SceneInstanceId,
    entries: Vec<(SceneEntityId, WorldEntityToken)>,
}

/// Fully prevalidated identity replacement whose commit mutates only the detached domain state.
///
/// The scene owner must retain exclusive World access between preparation and commit. This type is
/// workspace-internal because it deliberately does not define a general scene-session API.
#[doc(hidden)]
pub struct PreparedSceneInstanceReplacement {
    domain: WorldIdentityDomainId,
    instance: SceneInstanceId,
    entries: Vec<(SceneEntityId, WorldEntityToken)>,
    retirement: PreparedRetirement,
}

/// Fully prevalidated exact Scene Instance retirement used by the scene owner for unload.
#[doc(hidden)]
pub struct PreparedSceneInstanceRetirement {
    domain: WorldIdentityDomainId,
    retirement: PreparedRetirement,
}

/// Fully prevalidated retirement of one entity under an exact semantic reference.
#[doc(hidden)]
pub struct PreparedEntityRetirement {
    domain: WorldIdentityDomainId,
    retirement: PreparedRetirement,
}

impl PreparedEntityRetirement {
    /// Commits the already-prevalidated single-entity retirement.
    pub fn commit(self, domain: &mut WorldIdentityDomain) {
        assert_eq!(
            domain.id, self.domain,
            "prepared entity retirement must commit to the same identity domain"
        );
        let retired = domain.commit_retirement(self.retirement);
        debug_assert_eq!(retired.len(), 1);
    }
}

impl PreparedSceneInstanceRetirement {
    #[must_use]
    pub fn retiring_entities(&self) -> &[Entity] {
        &self.retirement.entities
    }

    #[must_use]
    pub fn commit(self, domain: &mut WorldIdentityDomain) -> Vec<Entity> {
        assert_eq!(
            domain.id, self.domain,
            "prepared scene retirement must commit to the same identity domain"
        );
        domain.commit_retirement(self.retirement)
    }
}

impl PreparedSceneInstanceRegistration {
    /// Returns the Scene Instance identity reserved by this preparation.
    #[must_use]
    pub const fn instance_id(&self) -> SceneInstanceId {
        self.instance
    }

    /// Publishes the fully prevalidated Scene Instance to the same identity domain.
    #[must_use]
    pub fn commit(self, domain: &mut WorldIdentityDomain) -> SpawnedSceneInstance {
        assert_eq!(
            domain.id, self.domain,
            "prepared scene registration must commit to the same identity domain"
        );
        let allocated = domain
            .scene_instance_allocator
            .allocate()
            .expect("scene registration allocation was prevalidated");
        assert_eq!(
            allocated,
            self.instance.non_zero(),
            "identity domain changed after scene registration preparation"
        );
        domain.commit_scene_instance(self.instance, self.entries)
    }
}

impl PreparedSceneInstanceReplacement {
    /// Returns the Scene Instance identity reserved for the replacement candidate.
    #[must_use]
    pub const fn instance_id(&self) -> SceneInstanceId {
        self.instance
    }

    /// Returns the exact active Scene Instance membership proven during preparation.
    #[must_use]
    pub fn retiring_entities(&self) -> &[Entity] {
        &self.retirement.entities
    }

    /// Commits the already-prevalidated allocator, retirement, and replacement state.
    ///
    /// No recoverable validation remains. The caller owns the exclusive World transaction and
    /// reinserts the domain after the exact old membership is despawned.
    #[must_use]
    pub fn commit(self, domain: &mut WorldIdentityDomain) -> (SpawnedSceneInstance, Vec<Entity>) {
        assert_eq!(
            domain.id, self.domain,
            "prepared scene replacement must commit to the same identity domain"
        );
        let allocated = domain
            .scene_instance_allocator
            .allocate()
            .expect("scene replacement allocation was prevalidated");
        assert_eq!(
            allocated,
            self.instance.non_zero(),
            "identity domain changed after scene replacement preparation"
        );
        let retired = domain.commit_retirement(self.retirement);
        let replacement = domain.commit_scene_instance(self.instance, self.entries);
        (replacement, retired)
    }
}

#[derive(Debug)]
struct SceneIdentityGroupEntry {
    entity_id: SceneEntityId,
    token: WorldEntityToken,
    persistent: Option<PersistentRuntimeReference>,
}

#[derive(Debug, Resource)]
pub struct WorldIdentityDomain {
    id: WorldIdentityDomainId,
    world_id: WorldId,
    settings: WorldIdentityDomainSettings,
    scene_instance_allocator: MonotonicNonZeroU64Allocator,
    retirement_sequence: MonotonicNonZeroU64Allocator,
    claimed_scene_instances: BTreeSet<SceneInstanceId>,
    active_scene_instances:
        BTreeMap<SceneInstanceId, BTreeMap<SceneEntityId, RuntimeEntityIncarnation>>,
    claimed_references: BTreeSet<RuntimeEntityReference>,
    scene_entities: BTreeMap<RuntimeEntityReference, Entity>,
    persistent_entities: BTreeMap<RuntimeEntityReference, Entity>,
    persistent_records: BTreeMap<PersistentRuntimeReference, PersistentLogicalRecord>,
    identities_by_entity: BTreeMap<Entity, EntityIdentityAxes>,
    recent_tombstones: BTreeMap<IdentityTombstoneSubject, IdentityTombstone>,
    tombstone_order: VecDeque<IdentityTombstoneSubject>,
}

impl WorldIdentityDomain {
    pub fn new(
        world: &World,
        settings: WorldIdentityDomainSettings,
    ) -> Result<Self, IdentityDomainError> {
        Ok(Self {
            id: allocate_world_identity_domain_id()?,
            world_id: world.id(),
            settings,
            scene_instance_allocator: MonotonicNonZeroU64Allocator::from_next_raw(1)
                .expect("one is a valid identity allocation start"),
            retirement_sequence: MonotonicNonZeroU64Allocator::from_next_raw(1)
                .expect("one is a valid retirement sequence start"),
            claimed_scene_instances: BTreeSet::new(),
            active_scene_instances: BTreeMap::new(),
            claimed_references: BTreeSet::new(),
            scene_entities: BTreeMap::new(),
            persistent_entities: BTreeMap::new(),
            persistent_records: BTreeMap::new(),
            identities_by_entity: BTreeMap::new(),
            recent_tombstones: BTreeMap::new(),
            tombstone_order: VecDeque::new(),
        })
    }

    #[must_use]
    pub const fn id(&self) -> WorldIdentityDomainId {
        self.id
    }

    #[must_use]
    pub const fn settings(&self) -> WorldIdentityDomainSettings {
        self.settings
    }

    #[must_use]
    pub fn stats(&self) -> IdentityDomainStats {
        IdentityDomainStats {
            lifetime_claims: self.claimed_scene_instances.len() + self.claimed_references.len(),
            claimed_scene_instances: self.claimed_scene_instances.len(),
            active_scene_instances: self.active_scene_instances.len(),
            active_scene_entities: self.scene_entities.len(),
            active_persistent_entities: self.persistent_entities.len(),
            persistent_logical_records: self.persistent_records.len(),
            unloaded_persistent_entities: self
                .persistent_records
                .values()
                .filter(|record| matches!(record, PersistentLogicalRecord::Unloaded { .. }))
                .count(),
            deleted_persistent_entities: self
                .persistent_records
                .values()
                .filter(|record| matches!(record, PersistentLogicalRecord::Deleted { .. }))
                .count(),
            registered_entities: self.identities_by_entity.len(),
            recent_tombstones: self.recent_tombstones.len(),
        }
    }

    /// Adopts an entity for a detached domain, such as an unpublished Scene candidate.
    ///
    /// Use [`adopt_identity_entity`] when the domain is installed in the World.
    pub fn adopt_entity(
        &self,
        world: &mut World,
        entity: Entity,
    ) -> Result<WorldEntityToken, IdentityDomainError> {
        self.validate_world_binding(world)?;
        validate_identity_adoption(world, entity)?;
        adopt_entity_in_domain(world, self.id, entity)
    }

    /// Registers a Scene instance in a detached domain after complete preflight.
    ///
    /// Use [`register_scene_instance`] when the domain is installed in the World.
    pub fn register_new_scene_instance(
        &mut self,
        world: &World,
        entries: impl IntoIterator<Item = (SceneEntityId, WorldEntityToken)>,
    ) -> Result<SpawnedSceneInstance, IdentityDomainError> {
        let prepared = self.prepare_scene_instance_registration(world, entries)?;
        Ok(prepared.commit(self))
    }

    pub fn preflight_scene_instance_registration(
        &self,
        world: &World,
        entity_count: usize,
    ) -> Result<(), IdentityDomainError> {
        self.validate_world_binding(world)?;
        let instance = SceneInstanceId::from_non_zero(
            self.scene_instance_allocator
                .peek()
                .map_err(|_| IdentityDomainError::SceneInstanceExhausted)?,
        );
        self.preflight_scene_entry_count(instance, entity_count)
    }

    /// Prepares the complete identity publication for one unpublished Scene Instance candidate.
    #[doc(hidden)]
    pub(crate) fn prepare_exact_scene_instance_registration(
        &self,
        world: &World,
        entries: &[(SceneEntityId, WorldEntityToken)],
    ) -> Result<PreparedSceneInstanceRegistration, IdentityDomainError> {
        self.prepare_scene_instance_registration(world, entries.iter().cloned())
    }

    fn prepare_scene_instance_registration(
        &self,
        world: &World,
        entries: impl IntoIterator<Item = (SceneEntityId, WorldEntityToken)>,
    ) -> Result<PreparedSceneInstanceRegistration, IdentityDomainError> {
        self.validate_world_binding(world)?;
        let instance = SceneInstanceId::from_non_zero(
            self.scene_instance_allocator
                .peek()
                .map_err(|_| IdentityDomainError::SceneInstanceExhausted)?,
        );
        let maximum_entries = self.scene_entry_capacity(instance)?;
        let entries = collect_scene_entries(
            entries,
            maximum_entries,
            self.settings.lifetime_claims.get(),
        )?;
        self.preflight_scene_instance(world, instance, &entries)?;
        Ok(PreparedSceneInstanceRegistration {
            domain: self.id,
            instance,
            entries,
        })
    }

    /// Returns a validated handle for one scene instance that is currently active in this World.
    ///
    /// The returned membership reflects the entities that remain active at the time of capture.
    /// Callers that need an immutable original roster must retain the handle before retiring any
    /// members from the instance.
    pub fn active_scene_instance(
        &self,
        world: &World,
        instance: SceneInstanceId,
    ) -> Result<SpawnedSceneInstance, IdentityDomainError> {
        self.validate_world_binding(world)?;
        let entity_ids = self
            .active_scene_instances
            .get(&instance)
            .ok_or(IdentityDomainError::SceneInstanceNotActive { instance })?;
        for (entity_id, incarnation) in entity_ids {
            let reference =
                RuntimeEntityReference::scene(instance, entity_id.clone(), *incarnation);
            let entity = self
                .scene_entities
                .get(&reference)
                .copied()
                .ok_or(IdentityDomainError::SceneInstanceMembershipMismatch { instance })?;
            if !self.is_live_owned_incarnation(world, entity, *incarnation) {
                return Err(IdentityDomainError::StaleRegistration);
            }
        }
        Ok(SpawnedSceneInstance {
            domain: self.id,
            instance,
            entity_ids: entity_ids.keys().cloned().collect(),
            incarnations: entity_ids.clone(),
        })
    }

    pub fn preflight_scene_instance_replacement(
        &self,
        world: &World,
        current: &SpawnedSceneInstance,
        entity_count: usize,
        cause: TombstoneCause,
    ) -> Result<SceneInstanceId, IdentityDomainError> {
        self.preflight_scene_instance_registration(world, entity_count)?;
        self.prepare_scene_instance_retirement(world, current, cause)?;
        self.scene_instance_allocator
            .peek()
            .map(SceneInstanceId::from_non_zero)
            .map_err(|_| IdentityDomainError::SceneInstanceExhausted)
    }

    /// Prepares the complete identity half of one exact Scene Instance replacement.
    ///
    /// This validates the concrete entries and current live membership rather than only their
    /// counts. It leaves the installed domain authoritative until the scene owner enters its
    /// relationship-detach commit tail.
    #[doc(hidden)]
    pub(crate) fn prepare_exact_scene_instance_replacement(
        world: &mut World,
        current: &SpawnedSceneInstance,
        entries: &[(SceneEntityId, WorldEntityToken)],
        cause: TombstoneCause,
    ) -> Result<PreparedSceneInstanceReplacement, IdentityDomainError> {
        validate_identity_support_topology(world, entries.iter().map(|(_, token)| token.entity()))
            .map_err(|_| IdentityDomainError::LifecycleConflict)?;
        let domain = installed_domain(world)?;
        let instance = SceneInstanceId::from_non_zero(
            domain
                .scene_instance_allocator
                .peek()
                .map_err(|_| IdentityDomainError::SceneInstanceExhausted)?,
        );
        let maximum_entries = domain.scene_entry_capacity(instance)?;
        let entries = collect_scene_entries(
            entries.iter().cloned(),
            maximum_entries,
            domain.settings.lifetime_claims.get(),
        )?;
        domain.preflight_scene_instance(world, instance, &entries)?;
        let retirement = domain.prepare_scene_instance_retirement(world, current, cause)?;
        Ok(PreparedSceneInstanceReplacement {
            domain: domain.id,
            instance,
            entries,
            retirement,
        })
    }

    /// Prepares the complete identity half of one exact Scene Instance unload.
    #[doc(hidden)]
    pub(crate) fn prepare_exact_scene_instance_retirement(
        world: &mut World,
        current: &SpawnedSceneInstance,
        cause: TombstoneCause,
    ) -> Result<PreparedSceneInstanceRetirement, IdentityDomainError> {
        validate_identity_support_topology(world, std::iter::empty())
            .map_err(|_| IdentityDomainError::LifecycleConflict)?;
        let domain = installed_domain(world)?;
        let retirement = domain.prepare_scene_instance_retirement(world, current, cause)?;
        Ok(PreparedSceneInstanceRetirement {
            domain: domain.id,
            retirement,
        })
    }

    /// Replaces one scene identity group and retires its prior runtime entities atomically.
    ///
    /// The operation rejects lifecycle hooks or observers that could mutate the replacement while
    /// the prior entities are despawned. The detached identity authority is published only after
    /// every retirement completes without lifecycle work.
    pub fn replace_scene_instance_and_despawn(
        world: &mut World,
        current: &SpawnedSceneInstance,
        entries: &[(SceneEntityId, WorldEntityToken)],
        retirements: &[Entity],
        cause: TombstoneCause,
    ) -> Result<(SpawnedSceneInstance, Vec<Entity>), IdentityDomainError> {
        validate_identity_support_topology(world, entries.iter().map(|(_, token)| token.entity()))
            .map_err(|_| IdentityDomainError::LifecycleConflict)?;
        let Some(mut domain) = world.remove_resource::<Self>() else {
            return Err(IdentityDomainError::WorldDomainUnavailable);
        };
        let scene_retirement = match domain.prepare_scene_instance_retirement(world, current, cause)
        {
            Ok(retirement) => retirement,
            Err(error) => {
                world.insert_resource(domain);
                return Err(error);
            }
        };
        if scene_retirement
            .entities
            .iter()
            .any(|entity| !retirements.contains(entity))
        {
            world.insert_resource(domain);
            return Err(IdentityDomainError::LifecycleConflict);
        }
        let retirement_set = retirements.iter().copied().collect::<BTreeSet<_>>();
        if retirement_set.len() != retirements.len()
            || entries
                .iter()
                .any(|(_, token)| retirement_set.contains(&token.entity()))
        {
            world.insert_resource(domain);
            return Err(IdentityDomainError::LifecycleConflict);
        }
        let scene_retirement_set = scene_retirement
            .entities
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if retirement_set.iter().any(|entity| {
            !scene_retirement_set.contains(entity)
                && domain.identities_by_entity.contains_key(entity)
        }) {
            world.insert_resource(domain);
            return Err(IdentityDomainError::LifecycleConflict);
        }
        let retirement = match prepare_lifecycle_free_despawn(world, retirements) {
            Ok(retirement) => retirement,
            Err(_) => {
                world.insert_resource(domain);
                return Err(IdentityDomainError::LifecycleConflict);
            }
        };
        let replacement = (|| {
            let prepared =
                domain.prepare_exact_scene_instance_registration(retirement.world(), entries)?;
            let scene_retirement =
                domain.prepare_scene_instance_retirement(retirement.world(), current, cause)?;
            let retired = domain.commit_retirement(scene_retirement);
            Ok::<_, IdentityDomainError>((prepared.commit(&mut domain), retired))
        })();
        match replacement {
            Ok(replacement) => {
                retirement.commit().insert_resource(domain);
                Ok(replacement)
            }
            Err(error) => {
                retirement.cancel().insert_resource(domain);
                Err(error)
            }
        }
    }

    pub fn replace_scene_instance(
        &mut self,
        world: &mut World,
        current: &SpawnedSceneInstance,
        entries: impl IntoIterator<Item = (SceneEntityId, WorldEntityToken)>,
        cause: TombstoneCause,
    ) -> Result<(SpawnedSceneInstance, Vec<Entity>), IdentityDomainError> {
        self.validate_world_binding(world)?;
        let instance = SceneInstanceId::from_non_zero(
            self.scene_instance_allocator
                .peek()
                .map_err(|_| IdentityDomainError::SceneInstanceExhausted)?,
        );
        let maximum_entries = self.scene_entry_capacity(instance)?;
        let entries = collect_scene_entries(
            entries,
            maximum_entries,
            self.settings.lifetime_claims.get(),
        )?;
        self.preflight_scene_instance(world, instance, &entries)?;
        let retirement = self.prepare_scene_instance_retirement(world, current, cause)?;
        if entries
            .iter()
            .any(|(_, token)| retirement.entities.contains(&token.entity()))
        {
            return Err(IdentityDomainError::LifecycleConflict);
        }
        self.revoke_retirement_markers(world, &retirement)?;

        let allocated = self
            .scene_instance_allocator
            .allocate()
            .expect("replacement scene instance allocation was preflighted");
        debug_assert_eq!(allocated, instance.non_zero());
        let retired = self.commit_retirement(retirement);
        let replacement = self.commit_scene_instance(instance, entries);
        Ok((replacement, retired))
    }

    pub fn register_restored_scene_instance(
        &mut self,
        world: &World,
        source: &SceneIdentitySnapshot,
        entries: impl IntoIterator<
            Item = (
                SceneEntityId,
                WorldEntityToken,
                Option<PersistentRuntimeReference>,
            ),
        >,
    ) -> Result<(SpawnedSceneInstance, WorldEntityLocatorRemap), IdentityDomainError> {
        self.validate_world_binding(world)?;
        let instance = source.instance_id();
        let (entries, remap) =
            self.preflight_scene_identity_group(world, source, instance, entries)?;
        self.scene_instance_allocator.reserve(instance.non_zero());
        let target = self.commit_scene_identity_group(instance, entries);
        Ok((target, remap))
    }

    pub fn register_parallel_scene_fork(
        &mut self,
        world: &World,
        source: &SceneIdentitySnapshot,
        entries: impl IntoIterator<
            Item = (
                SceneEntityId,
                WorldEntityToken,
                Option<PersistentRuntimeReference>,
            ),
        >,
    ) -> Result<(SpawnedSceneInstance, WorldEntityLocatorRemap), IdentityDomainError> {
        self.validate_world_binding(world)?;
        let instance = SceneInstanceId::from_non_zero(
            self.scene_instance_allocator
                .peek()
                .map_err(|_| IdentityDomainError::SceneInstanceExhausted)?,
        );
        let (entries, remap) =
            self.preflight_scene_identity_group(world, source, instance, entries)?;
        let allocated = self
            .scene_instance_allocator
            .allocate()
            .expect("scene instance allocation was preflighted");
        debug_assert_eq!(allocated, instance.non_zero());
        let target = self.commit_scene_identity_group(instance, entries);
        Ok((target, remap))
    }

    /// Registers a persistent axis in a detached domain.
    ///
    /// Use [`register_persistent_identity`] when the domain is installed in the World.
    pub fn register_persistent(
        &mut self,
        world: &World,
        token: WorldEntityToken,
        persistent: PersistentRuntimeReference,
    ) -> Result<WorldEntityLocator, IdentityDomainError> {
        let reference = self.prepare_persistent_registration(world, token, &persistent)?;
        Ok(self.commit_persistent_registration(token, persistent, reference))
    }

    fn prepare_persistent_registration(
        &self,
        world: &World,
        token: WorldEntityToken,
        persistent: &PersistentRuntimeReference,
    ) -> Result<RuntimeEntityReference, IdentityDomainError> {
        self.validate_live_token(world, token)?;
        let entity = token.entity();
        let reference = RuntimeEntityReference::persistent(persistent.clone(), token.incarnation());
        self.preflight_persistent_binding(persistent, &reference)?;
        if self
            .identities_by_entity
            .get(&entity)
            .is_some_and(|identity| identity.persistent.is_some())
        {
            return Err(IdentityDomainError::EntityAxisAlreadyRegistered {
                axis: EntityIdentityAxis::Persistent,
            });
        }
        Ok(reference)
    }

    fn commit_persistent_registration(
        &mut self,
        token: WorldEntityToken,
        persistent: PersistentRuntimeReference,
        reference: RuntimeEntityReference,
    ) -> WorldEntityLocator {
        let entity = token.entity();
        self.persistent_records.insert(
            persistent,
            PersistentLogicalRecord::Active {
                entity,
                incarnation: token.incarnation(),
            },
        );
        self.persistent_entities.insert(reference.clone(), entity);
        self.identities_by_entity
            .entry(entity)
            .or_default()
            .persistent = Some(reference.clone());
        WorldEntityLocator::new(self.id, reference)
    }

    #[must_use]
    pub fn lookup(&self, world: &World, reference: &RuntimeEntityReference) -> EntityLookup {
        if self.validate_world_binding(world).is_err() {
            return EntityLookup::WrongWorldBinding;
        }
        match self.lookup_registered(reference) {
            EntityLookup::Resolved(entity)
                if !self.is_live_owned_incarnation(world, entity, reference.incarnation()) =>
            {
                EntityLookup::StaleRegistration
            }
            result => result,
        }
    }

    fn lookup_registered(&self, reference: &RuntimeEntityReference) -> EntityLookup {
        if let RuntimeEntityReference::Persistent {
            entity,
            incarnation,
        } = reference
        {
            return match self.persistent_records.get(entity).copied() {
                Some(record) if record.incarnation() != *incarnation => EntityLookup::StaleRuntime,
                Some(PersistentLogicalRecord::Active { entity, .. }) => {
                    EntityLookup::Resolved(entity)
                }
                Some(PersistentLogicalRecord::Unloaded { .. }) => EntityLookup::Unloaded,
                Some(PersistentLogicalRecord::Deleted { .. }) => EntityLookup::Deleted,
                None => EntityLookup::Missing,
            };
        }
        if let RuntimeEntityReference::Scene {
            instance,
            entity,
            incarnation,
        } = reference
            && let Some(current) = self
                .active_scene_instances
                .get(instance)
                .and_then(|entities| entities.get(entity))
            && current != incarnation
        {
            return EntityLookup::StaleRuntime;
        }
        if let Some(entity) = self.active_entity(reference) {
            return EntityLookup::Resolved(entity);
        }
        if self.claimed_references.contains(reference) {
            let detail = self
                .recent_tombstones
                .get(&IdentityTombstoneSubject::Entity(reference.clone()))
                .cloned();
            return EntityLookup::Tombstoned(detail);
        }
        EntityLookup::Missing
    }

    #[must_use]
    pub fn resolve_entity_reference(
        &self,
        world: &World,
        reference: &EntityReference,
        scene_context: Option<SceneInstanceId>,
    ) -> EntityLookup {
        if self.validate_world_binding(world).is_err() {
            return EntityLookup::WrongWorldBinding;
        }
        match reference {
            EntityReference::SceneLocal { entity } => {
                let Some(instance) = scene_context else {
                    return EntityLookup::ContextRequired;
                };
                if let Some(incarnation) = self
                    .active_scene_instances
                    .get(&instance)
                    .and_then(|entities| entities.get(entity))
                {
                    return self.lookup(
                        world,
                        &RuntimeEntityReference::scene(instance, entity.clone(), *incarnation),
                    );
                }
                let first = RuntimeEntityReference::scene(
                    instance,
                    entity.clone(),
                    RuntimeEntityIncarnation::from_non_zero(NonZeroU64::MIN),
                );
                self.claimed_references.range(first..).next().filter(|reference| matches!(reference, RuntimeEntityReference::Scene { instance: candidate, entity: candidate_entity, .. } if *candidate == instance && candidate_entity == entity)).map_or(EntityLookup::Missing, |reference| self.lookup(world, reference))
            }
            EntityReference::Persistent { entity } => {
                self.persistent_records
                    .get(entity)
                    .map_or(EntityLookup::Missing, |record| {
                        self.lookup(
                            world,
                            &RuntimeEntityReference::persistent(
                                entity.clone(),
                                record.incarnation(),
                            ),
                        )
                    })
            }
        }
    }

    pub fn locators_for_token(
        &self,
        world: &World,
        token: WorldEntityToken,
    ) -> Result<Option<WorldEntityLocators>, IdentityDomainError> {
        self.validate_live_token(world, token)?;
        self.locators_for_entity(world, token.entity())
    }

    pub fn locators_for_entity(
        &self,
        world: &World,
        entity: Entity,
    ) -> Result<Option<WorldEntityLocators>, IdentityDomainError> {
        self.validate_world_binding(world)?;
        let Some(identity) = self.identities_by_entity.get(&entity) else {
            return Ok(None);
        };
        if identity
            .scene
            .iter()
            .chain(identity.persistent.iter())
            .any(|reference| {
                !self.is_live_owned_incarnation(world, entity, reference.incarnation())
            })
        {
            return Err(IdentityDomainError::StaleRegistration);
        }
        Ok(Some(WorldEntityLocators {
            scene: identity
                .scene
                .clone()
                .map(|reference| WorldEntityLocator::new(self.id, reference)),
            persistent: identity
                .persistent
                .clone()
                .map(|reference| WorldEntityLocator::new(self.id, reference)),
        }))
    }

    pub fn registered_locators(
        &self,
        world: &World,
    ) -> Result<impl Iterator<Item = WorldEntityLocator> + '_, IdentityDomainError> {
        self.validate_world_binding(world)?;
        Ok(self
            .scene_entities
            .keys()
            .chain(self.persistent_entities.keys())
            .cloned()
            .map(|reference| WorldEntityLocator::new(self.id, reference)))
    }

    pub fn scene_identity_snapshot(
        &self,
        world: &World,
        instance: &SpawnedSceneInstance,
    ) -> Result<SceneIdentitySnapshot, IdentityDomainError> {
        self.validate_world_binding(world)?;
        if instance.domain_id() != self.id {
            return Err(IdentityDomainError::WrongDomain {
                expected: self.id,
                actual: instance.domain_id(),
            });
        }
        let Some(active_ids) = self.active_scene_instances.get(&instance.instance) else {
            return Err(IdentityDomainError::SceneInstanceNotActive {
                instance: instance.instance,
            });
        };
        if active_ids.len() != instance.entity_ids.len()
            || active_ids
                .keys()
                .zip(instance.entity_ids.iter())
                .any(|(active, expected)| active != expected)
        {
            return Err(IdentityDomainError::SceneInstanceMembershipMismatch {
                instance: instance.instance,
            });
        }

        let mut entities = BTreeMap::new();
        for (entity_id, incarnation) in active_ids {
            let scene =
                RuntimeEntityReference::scene(instance.instance, entity_id.clone(), *incarnation);
            let entity = self.scene_entities.get(&scene).copied().ok_or(
                IdentityDomainError::SceneInstanceMembershipMismatch {
                    instance: instance.instance,
                },
            )?;
            if !self.is_live_owned_incarnation(world, entity, *incarnation) {
                return Err(IdentityDomainError::StaleRegistration);
            }
            let identity = self.identities_by_entity.get(&entity).ok_or(
                IdentityDomainError::SceneInstanceMembershipMismatch {
                    instance: instance.instance,
                },
            )?;
            entities.insert(
                entity_id.clone(),
                SceneIdentityReferences {
                    scene,
                    persistent: identity.persistent.clone(),
                },
            );
        }
        Ok(SceneIdentitySnapshot {
            domain: self.id,
            instance: instance.instance,
            entities,
        })
    }

    pub fn retire_scene_instance(
        &mut self,
        world: &mut World,
        instance: &SpawnedSceneInstance,
        cause: TombstoneCause,
    ) -> Result<Vec<Entity>, IdentityDomainError> {
        let retirement = self.prepare_scene_instance_retirement(world, instance, cause)?;
        self.revoke_retirement_markers(world, &retirement)?;
        Ok(self.commit_retirement(retirement))
    }

    fn prepare_scene_instance_retirement(
        &self,
        world: &World,
        instance: &SpawnedSceneInstance,
        cause: TombstoneCause,
    ) -> Result<PreparedRetirement, IdentityDomainError> {
        self.validate_world_binding(world)?;
        if instance.domain_id() != self.id {
            return Err(IdentityDomainError::WrongDomain {
                expected: self.id,
                actual: instance.domain_id(),
            });
        }
        let Some(active_ids) = self.active_scene_instances.get(&instance.instance) else {
            return Err(IdentityDomainError::SceneInstanceNotActive {
                instance: instance.instance,
            });
        };
        if active_ids
            .keys()
            .any(|entity_id| !instance.contains(entity_id))
        {
            return Err(IdentityDomainError::SceneInstanceMembershipMismatch {
                instance: instance.instance,
            });
        }

        let entities = active_ids
            .iter()
            .map(|(entity_id, incarnation)| {
                let reference = RuntimeEntityReference::scene(
                    instance.instance,
                    entity_id.clone(),
                    *incarnation,
                );
                self.scene_entities.get(&reference).copied().ok_or(
                    IdentityDomainError::SceneInstanceMembershipMismatch {
                        instance: instance.instance,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.prepare_retirement(Some(instance.instance), entities, cause)
    }

    /// Retires an entity in a detached domain.
    ///
    /// Use [`retire_identity_entity`] when the domain is installed in the World.
    pub fn retire_entity(
        &mut self,
        world: &mut World,
        token: WorldEntityToken,
        cause: TombstoneCause,
    ) -> Result<(), IdentityDomainError> {
        self.retire_entity_with_disposition(
            world,
            token,
            cause,
            PersistentRetirementDisposition::Unload,
        )
    }

    /// Retires an entity only when the caller's semantic reference still names it.
    pub fn retire_entity_with_reference(
        &mut self,
        world: &mut World,
        entity: Entity,
        reference: &RuntimeEntityReference,
        cause: TombstoneCause,
    ) -> Result<(), IdentityDomainError> {
        let retirement =
            self.prepare_entity_retirement_with_reference(world, entity, reference, cause)?;
        self.revoke_retirement_markers(world, &retirement)?;
        self.commit_retirement(retirement);
        Ok(())
    }

    fn prepare_entity_retirement_with_reference(
        &self,
        world: &World,
        entity: Entity,
        reference: &RuntimeEntityReference,
        cause: TombstoneCause,
    ) -> Result<PreparedRetirement, IdentityDomainError> {
        self.validate_world_binding(world)?;
        let identity = self
            .identities_by_entity
            .get(&entity)
            .ok_or(IdentityDomainError::EntityNotRegistered)?;
        if identity.scene.as_ref() != Some(reference)
            && identity.persistent.as_ref() != Some(reference)
        {
            return Err(IdentityDomainError::StaleRegistration);
        }
        if !matches!(
            self.lookup(world, reference),
            EntityLookup::Resolved(found) if found == entity
        ) {
            return Err(IdentityDomainError::StaleRegistration);
        }
        self.prepare_retirement(None, vec![entity], cause)
    }

    fn preflight_scene_instance(
        &self,
        world: &World,
        instance: SceneInstanceId,
        entries: &[(SceneEntityId, WorldEntityToken)],
    ) -> Result<(), IdentityDomainError> {
        if self.claimed_scene_instances.contains(&instance) {
            return Err(IdentityDomainError::SceneInstanceAlreadyClaimed { instance });
        }
        self.check_claim_capacity(entries.len().checked_add(1).ok_or(
            IdentityDomainError::LifetimeClaimLimit {
                requested: usize::MAX,
                maximum: self.settings.lifetime_claims.get(),
            },
        )?)?;

        for (entity_id, token) in entries {
            self.validate_live_token(world, *token)?;
            let entity = token.entity();
            let reference =
                RuntimeEntityReference::scene(instance, entity_id.clone(), token.incarnation());
            if self.claimed_references.contains(&reference) {
                return Err(IdentityDomainError::ReferenceAlreadyClaimed { reference });
            }
            if self
                .identities_by_entity
                .get(&entity)
                .is_some_and(|identity| identity.scene.is_some())
            {
                return Err(IdentityDomainError::EntityAxisAlreadyRegistered {
                    axis: EntityIdentityAxis::Scene,
                });
            }
        }
        Ok(())
    }

    fn preflight_scene_identity_group(
        &self,
        world: &World,
        source: &SceneIdentitySnapshot,
        instance: SceneInstanceId,
        entries: impl IntoIterator<
            Item = (
                SceneEntityId,
                WorldEntityToken,
                Option<PersistentRuntimeReference>,
            ),
        >,
    ) -> Result<(Vec<SceneIdentityGroupEntry>, WorldEntityLocatorRemap), IdentityDomainError> {
        if self.claimed_scene_instances.contains(&instance) {
            return Err(IdentityDomainError::SceneInstanceAlreadyClaimed { instance });
        }
        let additional =
            source
                .len()
                .checked_add(1)
                .ok_or(IdentityDomainError::LifetimeClaimLimit {
                    requested: usize::MAX,
                    maximum: self.settings.lifetime_claims.get(),
                })?;
        self.check_claim_capacity(additional)?;

        let mut by_id = BTreeMap::new();
        let mut entities = BTreeSet::new();
        let mut persistent_references = BTreeSet::new();
        for (entity_id, token, persistent) in entries {
            if by_id.len() >= source.len() {
                return Err(IdentityDomainError::IncompleteSceneFork);
            }
            let Some(source_references) = source.references(&entity_id) else {
                return Err(IdentityDomainError::IncompleteSceneFork);
            };
            if source_references.persistent.is_some() != persistent.is_some() {
                return Err(IdentityDomainError::IncompleteSceneForkIdentityAxes {
                    entity: entity_id,
                });
            }
            self.validate_live_token(world, token)?;
            let entity = token.entity();
            if !entities.insert(entity) {
                return Err(IdentityDomainError::DuplicateRuntimeEntity);
            }
            if let Some(identity) = self.identities_by_entity.get(&entity) {
                if identity.scene.is_some() {
                    return Err(IdentityDomainError::EntityAxisAlreadyRegistered {
                        axis: EntityIdentityAxis::Scene,
                    });
                }
                if identity.persistent.is_some() {
                    return Err(IdentityDomainError::EntityAxisAlreadyRegistered {
                        axis: EntityIdentityAxis::Persistent,
                    });
                }
            }

            let scene_reference =
                RuntimeEntityReference::scene(instance, entity_id.clone(), token.incarnation());
            if self.claimed_references.contains(&scene_reference) {
                return Err(IdentityDomainError::ReferenceAlreadyClaimed {
                    reference: scene_reference,
                });
            }
            if let Some(persistent) = &persistent {
                let reference =
                    RuntimeEntityReference::persistent(persistent.clone(), token.incarnation());
                self.preflight_persistent_binding(persistent, &reference)?;
                if !persistent_references.insert(persistent.clone()) {
                    return Err(IdentityDomainError::ReferenceAlreadyClaimed { reference });
                }
            }

            let entry = SceneIdentityGroupEntry {
                entity_id: entity_id.clone(),
                token,
                persistent,
            };
            if by_id.insert(entity_id.clone(), entry).is_some() {
                return Err(IdentityDomainError::DuplicateSceneEntityId { entity: entity_id });
            }
        }
        if !by_id.keys().eq(source.entity_ids()) {
            return Err(IdentityDomainError::IncompleteSceneFork);
        }
        let new_persistent_records = persistent_references
            .iter()
            .filter(|reference| !self.persistent_records.contains_key(*reference))
            .count();
        self.check_persistent_capacity(new_persistent_records)?;

        let target_snapshot = SceneIdentitySnapshot {
            domain: self.id,
            instance,
            entities: by_id
                .values()
                .map(|entry| {
                    (
                        entry.entity_id.clone(),
                        SceneIdentityReferences {
                            scene: RuntimeEntityReference::scene(
                                instance,
                                entry.entity_id.clone(),
                                entry.token.incarnation(),
                            ),
                            persistent: entry.persistent.clone().map(|persistent| {
                                RuntimeEntityReference::persistent(
                                    persistent,
                                    entry.token.incarnation(),
                                )
                            }),
                        },
                    )
                })
                .collect(),
        };
        let remap = WorldEntityLocatorRemap::between_scene_snapshots(source, &target_snapshot)
            .map_err(|error| IdentityDomainError::InvalidSceneRemap(Box::new(error)))?;
        Ok((by_id.into_values().collect(), remap))
    }

    fn scene_entry_capacity(
        &self,
        instance: SceneInstanceId,
    ) -> Result<usize, IdentityDomainError> {
        if self.claimed_scene_instances.contains(&instance) {
            return Err(IdentityDomainError::SceneInstanceAlreadyClaimed { instance });
        }
        let retained = self
            .claimed_scene_instances
            .len()
            .saturating_add(self.claimed_references.len());
        let requested = retained.saturating_add(1);
        let maximum = self.settings.lifetime_claims.get();
        if requested > maximum {
            return Err(IdentityDomainError::LifetimeClaimLimit { requested, maximum });
        }
        Ok(maximum - requested)
    }

    fn preflight_scene_entry_count(
        &self,
        instance: SceneInstanceId,
        entity_count: usize,
    ) -> Result<(), IdentityDomainError> {
        let maximum_entries = self.scene_entry_capacity(instance)?;
        if entity_count > maximum_entries {
            let retained = self
                .claimed_scene_instances
                .len()
                .saturating_add(self.claimed_references.len());
            return Err(IdentityDomainError::LifetimeClaimLimit {
                requested: retained.saturating_add(1).saturating_add(entity_count),
                maximum: self.settings.lifetime_claims.get(),
            });
        }
        Ok(())
    }

    fn commit_scene_instance(
        &mut self,
        instance: SceneInstanceId,
        entries: Vec<(SceneEntityId, WorldEntityToken)>,
    ) -> SpawnedSceneInstance {
        self.claimed_scene_instances.insert(instance);
        let mut entity_ids = Vec::with_capacity(entries.len());
        let mut active_ids = BTreeMap::new();
        for (entity_id, token) in entries {
            let entity = token.entity();
            let reference =
                RuntimeEntityReference::scene(instance, entity_id.clone(), token.incarnation());
            self.claimed_references.insert(reference.clone());
            self.scene_entities.insert(reference.clone(), entity);
            self.identities_by_entity.entry(entity).or_default().scene = Some(reference);
            active_ids.insert(entity_id.clone(), token.incarnation());
            entity_ids.push(entity_id);
        }
        self.active_scene_instances
            .insert(instance, active_ids.clone());
        SpawnedSceneInstance {
            domain: self.id,
            instance,
            entity_ids,
            incarnations: active_ids,
        }
    }

    fn commit_scene_identity_group(
        &mut self,
        instance: SceneInstanceId,
        entries: Vec<SceneIdentityGroupEntry>,
    ) -> SpawnedSceneInstance {
        self.claimed_scene_instances.insert(instance);
        let mut entity_ids = Vec::with_capacity(entries.len());
        let mut active_ids = BTreeMap::new();
        for entry in entries {
            let entity = entry.token.entity();
            let scene_reference = RuntimeEntityReference::scene(
                instance,
                entry.entity_id.clone(),
                entry.token.incarnation(),
            );
            self.claimed_references.insert(scene_reference.clone());
            self.scene_entities.insert(scene_reference.clone(), entity);
            let identity = self.identities_by_entity.entry(entity).or_default();
            identity.scene = Some(scene_reference);

            if let Some(persistent) = entry.persistent {
                let persistent_reference = RuntimeEntityReference::persistent(
                    persistent.clone(),
                    entry.token.incarnation(),
                );
                self.persistent_records.insert(
                    persistent,
                    PersistentLogicalRecord::Active {
                        entity,
                        incarnation: entry.token.incarnation(),
                    },
                );
                self.persistent_entities
                    .insert(persistent_reference.clone(), entity);
                identity.persistent = Some(persistent_reference);
            }
            active_ids.insert(entry.entity_id.clone(), entry.token.incarnation());
            entity_ids.push(entry.entity_id);
        }
        self.active_scene_instances
            .insert(instance, active_ids.clone());
        SpawnedSceneInstance {
            domain: self.id,
            instance,
            entity_ids,
            incarnations: active_ids,
        }
    }

    fn prepare_retirement(
        &self,
        instance: Option<SceneInstanceId>,
        mut entities: Vec<Entity>,
        cause: TombstoneCause,
    ) -> Result<PreparedRetirement, IdentityDomainError> {
        entities.sort_by_key(|entity| entity.to_bits());
        entities.dedup();

        let identities = entities
            .iter()
            .map(|entity| {
                self.identities_by_entity
                    .get(entity)
                    .cloned()
                    .ok_or(IdentityDomainError::EntityNotRegistered)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut subjects = BTreeSet::new();
        if let Some(instance) = instance {
            subjects.insert(IdentityTombstoneSubject::SceneInstance(instance));
        }
        for identity in &identities {
            if let Some(reference) = &identity.scene {
                subjects.insert(IdentityTombstoneSubject::Entity(reference.clone()));
            }
            if let Some(reference) = &identity.persistent {
                subjects.insert(IdentityTombstoneSubject::Entity(reference.clone()));
            }
        }

        let mut sequence_allocator = self.retirement_sequence.clone();
        let tombstones = subjects
            .into_iter()
            .map(|subject| {
                let sequence = sequence_allocator
                    .allocate()
                    .map_err(|_| IdentityDomainError::RetirementSequenceExhausted)?;
                Ok((subject, IdentityTombstone { sequence, cause }))
            })
            .collect::<Result<Vec<_>, IdentityDomainError>>()?;

        Ok(PreparedRetirement {
            instance,
            entities,
            identities,
            sequence_allocator,
            tombstones,
            persistent_disposition: PersistentRetirementDisposition::Unload,
        })
    }

    fn commit_retirement(&mut self, retirement: PreparedRetirement) -> Vec<Entity> {
        let PreparedRetirement {
            instance,
            entities,
            identities,
            sequence_allocator,
            tombstones,
            persistent_disposition,
        } = retirement;
        if let Some(instance) = instance {
            self.active_scene_instances.remove(&instance);
        }
        for (entity, identity) in entities.iter().copied().zip(identities) {
            if let Some(reference) = identity.scene {
                self.scene_entities.remove(&reference);
                if instance.is_none()
                    && let RuntimeEntityReference::Scene {
                        instance,
                        entity: entity_id,
                        ..
                    } = reference
                    && let Some(active_ids) = self.active_scene_instances.get_mut(&instance)
                {
                    active_ids.remove(&entity_id);
                }
            }
            if let Some(reference) = identity.persistent {
                self.persistent_entities.remove(&reference);
                if let RuntimeEntityReference::Persistent {
                    entity,
                    incarnation,
                } = reference
                {
                    let record = match persistent_disposition {
                        PersistentRetirementDisposition::Unload => {
                            PersistentLogicalRecord::Unloaded { incarnation }
                        }
                        PersistentRetirementDisposition::Delete => {
                            PersistentLogicalRecord::Deleted { incarnation }
                        }
                    };
                    self.persistent_records.insert(entity, record);
                }
            }
            self.identities_by_entity.remove(&entity);
        }
        self.retirement_sequence = sequence_allocator;
        self.record_tombstones(tombstones);
        entities
    }

    fn record_tombstones(
        &mut self,
        tombstones: Vec<(IdentityTombstoneSubject, IdentityTombstone)>,
    ) {
        for (subject, tombstone) in tombstones {
            self.recent_tombstones.insert(subject.clone(), tombstone);
            self.tombstone_order.push_back(subject);
        }
        let maximum = self.settings.recent_tombstones.get();
        while self.tombstone_order.len() > maximum {
            let subject = self
                .tombstone_order
                .pop_front()
                .expect("tombstone order is non-empty above its limit");
            self.recent_tombstones.remove(&subject);
        }
    }

    fn check_claim_capacity(&self, additional: usize) -> Result<(), IdentityDomainError> {
        let retained = self
            .claimed_scene_instances
            .len()
            .saturating_add(self.claimed_references.len());
        let requested = retained.saturating_add(additional);
        let maximum = self.settings.lifetime_claims.get();
        if requested > maximum {
            return Err(IdentityDomainError::LifetimeClaimLimit { requested, maximum });
        }
        Ok(())
    }

    fn check_persistent_capacity(&self, additional: usize) -> Result<(), IdentityDomainError> {
        let requested = self.persistent_records.len().saturating_add(additional);
        let maximum = self.settings.persistent_logical_records.get();
        if requested > maximum {
            return Err(IdentityDomainError::PersistentLogicalRecordLimit { requested, maximum });
        }
        Ok(())
    }

    fn preflight_persistent_binding(
        &self,
        persistent: &PersistentRuntimeReference,
        reference: &RuntimeEntityReference,
    ) -> Result<(), IdentityDomainError> {
        match self.persistent_records.get(persistent) {
            Some(PersistentLogicalRecord::Active { .. }) => {
                Err(IdentityDomainError::ReferenceAlreadyClaimed {
                    reference: reference.clone(),
                })
            }
            Some(PersistentLogicalRecord::Deleted { .. }) => {
                Err(IdentityDomainError::PersistentIdentityDeleted {
                    reference: persistent.clone(),
                })
            }
            Some(PersistentLogicalRecord::Unloaded { .. }) => Ok(()),
            None => self.check_persistent_capacity(1),
        }
    }

    fn revoke_retirement_markers(
        &self,
        world: &mut World,
        retirement: &PreparedRetirement,
    ) -> Result<(), IdentityDomainError> {
        let live_entities = self.validate_retirement_markers(world, retirement)?;
        revoke_retirement_markers(world, &live_entities)
    }

    fn validate_retirement_markers(
        &self,
        world: &World,
        retirement: &PreparedRetirement,
    ) -> Result<Vec<Entity>, IdentityDomainError> {
        let live_entities = retirement
            .entities
            .iter()
            .copied()
            .filter(|entity| world.get_entity(*entity).is_ok())
            .collect::<Vec<_>>();
        for (entity, identity) in retirement.entities.iter().zip(&retirement.identities) {
            if world.get_entity(*entity).is_ok()
                && identity
                    .scene
                    .iter()
                    .chain(identity.persistent.iter())
                    .any(|reference| {
                        !self.is_live_owned_incarnation(world, *entity, reference.incarnation())
                    })
            {
                return Err(IdentityDomainError::StaleRegistration);
            }
        }
        Ok(live_entities)
    }

    /// Retires an entity in a detached domain with an explicit logical disposition.
    ///
    /// Use [`retire_identity_entity_with_disposition`] for the installed domain.
    pub fn retire_entity_with_disposition(
        &mut self,
        world: &mut World,
        token: WorldEntityToken,
        cause: TombstoneCause,
        disposition: PersistentRetirementDisposition,
    ) -> Result<(), IdentityDomainError> {
        let retirement =
            self.prepare_entity_retirement_with_disposition(world, token, cause, disposition)?;
        self.revoke_retirement_markers(world, &retirement)?;
        self.commit_retirement(retirement);
        Ok(())
    }

    fn prepare_entity_retirement_with_disposition(
        &self,
        world: &World,
        token: WorldEntityToken,
        cause: TombstoneCause,
        disposition: PersistentRetirementDisposition,
    ) -> Result<PreparedRetirement, IdentityDomainError> {
        self.validate_world_binding(world)?;
        self.validate_token_domain(token)?;
        let identity = self
            .identities_by_entity
            .get(&token.entity())
            .ok_or(IdentityDomainError::EntityNotRegistered)?;
        if identity
            .scene
            .iter()
            .chain(identity.persistent.iter())
            .any(|reference| reference.incarnation() != token.incarnation())
        {
            return Err(IdentityDomainError::StaleRegistration);
        }
        let mut retirement = self.prepare_retirement(None, vec![token.entity()], cause)?;
        retirement.persistent_disposition = disposition;
        Ok(retirement)
    }

    pub fn delete_unloaded_persistent(
        &mut self,
        world: &World,
        reference: &PersistentRuntimeReference,
    ) -> Result<(), IdentityDomainError> {
        self.validate_world_binding(world)?;
        match self.persistent_records.get(reference).copied() {
            Some(PersistentLogicalRecord::Unloaded { incarnation }) => {
                self.persistent_records.insert(
                    reference.clone(),
                    PersistentLogicalRecord::Deleted { incarnation },
                );
                Ok(())
            }
            Some(PersistentLogicalRecord::Deleted { .. }) => Ok(()),
            Some(PersistentLogicalRecord::Active { .. }) => {
                Err(IdentityDomainError::PersistentIdentityActive {
                    reference: reference.clone(),
                })
            }
            None => Err(IdentityDomainError::PersistentIdentityMissing {
                reference: reference.clone(),
            }),
        }
    }

    fn active_entity(&self, reference: &RuntimeEntityReference) -> Option<Entity> {
        match reference {
            RuntimeEntityReference::Scene { .. } => self.scene_entities.get(reference).copied(),
            RuntimeEntityReference::Persistent { .. } => {
                self.persistent_entities.get(reference).copied()
            }
        }
    }

    fn validate_token_domain(&self, token: WorldEntityToken) -> Result<(), IdentityDomainError> {
        if token.domain_id() != self.id {
            return Err(IdentityDomainError::WrongDomain {
                expected: self.id,
                actual: token.domain_id(),
            });
        }
        Ok(())
    }

    fn validate_world_binding(&self, world: &World) -> Result<(), IdentityDomainError> {
        if world.id() != self.world_id {
            return Err(IdentityDomainError::WorldBindingMismatch);
        }
        Ok(())
    }

    fn validate_live_token(
        &self,
        world: &World,
        token: WorldEntityToken,
    ) -> Result<(), IdentityDomainError> {
        self.validate_world_binding(world)?;
        self.validate_token_domain(token)?;
        if world.get_entity(token.entity()).is_err() {
            return Err(IdentityDomainError::EntityTokenNotAlive);
        }
        if !self.is_live_owned_incarnation(world, token.entity(), token.incarnation()) {
            return Err(IdentityDomainError::EntityTokenNotOwned);
        }
        Ok(())
    }

    fn is_live_owned_incarnation(
        &self,
        world: &World,
        entity: Entity,
        incarnation: RuntimeEntityIncarnation,
    ) -> bool {
        world.get_entity(entity).is_ok()
            && world
                .get::<IdentityEntityMarker>(entity)
                .is_some_and(|marker| marker.domain == self.id && marker.incarnation == incarnation)
    }

    #[cfg(test)]
    pub(crate) fn set_next_scene_instance_for_test(
        &mut self,
        raw: u64,
    ) -> Result<(), IdentityAllocationError> {
        self.scene_instance_allocator = MonotonicNonZeroU64Allocator::from_next_raw(raw)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_next_retirement_sequence_for_test(
        &mut self,
        raw: u64,
    ) -> Result<(), IdentityAllocationError> {
        self.retirement_sequence = MonotonicNonZeroU64Allocator::from_next_raw(raw)?;
        Ok(())
    }
}

/// Workspace-internal preparation for one unpublished Scene Instance registration.
#[doc(hidden)]
pub fn prepare_exact_scene_instance_registration(
    domain: &WorldIdentityDomain,
    world: &World,
    entries: &[(SceneEntityId, WorldEntityToken)],
) -> Result<PreparedSceneInstanceRegistration, IdentityDomainError> {
    domain.prepare_exact_scene_instance_registration(world, entries)
}

/// Workspace-internal preparation for one exact Scene Instance replacement.
#[doc(hidden)]
pub fn prepare_exact_scene_instance_replacement(
    world: &mut World,
    current: &SpawnedSceneInstance,
    entries: &[(SceneEntityId, WorldEntityToken)],
    cause: TombstoneCause,
) -> Result<PreparedSceneInstanceReplacement, IdentityDomainError> {
    WorldIdentityDomain::prepare_exact_scene_instance_replacement(world, current, entries, cause)
}

/// Workspace-internal preparation for one exact Scene Instance retirement.
#[doc(hidden)]
pub fn prepare_exact_scene_instance_retirement(
    world: &mut World,
    current: &SpawnedSceneInstance,
    cause: TombstoneCause,
) -> Result<PreparedSceneInstanceRetirement, IdentityDomainError> {
    WorldIdentityDomain::prepare_exact_scene_instance_retirement(world, current, cause)
}

/// Workspace-internal preparation for one exact semantic entity retirement.
#[doc(hidden)]
pub fn prepare_exact_entity_retirement(
    world: &mut World,
    entity: Entity,
    reference: &RuntimeEntityReference,
    cause: TombstoneCause,
) -> Result<PreparedEntityRetirement, IdentityDomainError> {
    validate_identity_support_topology(world, [entity])
        .map_err(|_| IdentityDomainError::LifecycleConflict)?;
    let domain = installed_domain(world)?;
    let retirement =
        domain.prepare_entity_retirement_with_reference(world, entity, reference, cause)?;
    Ok(PreparedEntityRetirement {
        domain: domain.id,
        retirement,
    })
}

/// Validates that every additional runtime retirement token belongs to this World and has no
/// semantic identity axis.
///
/// Domain adoption alone is not a semantic identity axis, so an adopted runtime-only entity is
/// eligible when neither a Scene nor Persistent axis is registered for it.
///
/// This workspace-internal preflight is read-only. The scene owner remains responsible for the
/// bounded retirement set and for committing entity teardown under exclusive World access.
#[doc(hidden)]
pub fn validate_additional_retirement_identity_axes(
    world: &World,
    tokens: impl IntoIterator<Item = WorldEntityToken>,
) -> Result<(), AdditionalRetirementIdentityError> {
    let domain = world
        .get_resource::<WorldIdentityDomain>()
        .ok_or(AdditionalRetirementIdentityError::WorldDomainUnavailable)?;
    domain
        .validate_world_binding(world)
        .map_err(|_| AdditionalRetirementIdentityError::WorldBindingMismatch)?;

    let mut seen = BTreeSet::new();
    for token in tokens {
        let entity = token.entity();
        if !seen.insert(entity) {
            return Err(AdditionalRetirementIdentityError::DuplicateEntity { entity });
        }
        if token.domain_id() != domain.id() {
            return Err(AdditionalRetirementIdentityError::TokenWrongDomain { entity });
        }
        world
            .get_entity(entity)
            .map_err(|_| AdditionalRetirementIdentityError::EntityMissing { entity })?;
        if !domain.is_live_owned_incarnation(world, entity, token.incarnation()) {
            return Err(AdditionalRetirementIdentityError::EntityNotOwned { entity });
        }
        let Some(identity) = domain.identities_by_entity.get(&entity) else {
            continue;
        };
        if identity.scene.is_some() {
            return Err(AdditionalRetirementIdentityError::ActiveSceneAxis { entity });
        }
        if identity.persistent.is_some() {
            return Err(AdditionalRetirementIdentityError::PersistentAxis { entity });
        }
    }
    Ok(())
}

#[must_use]
pub fn resolve_in_world(world: &World, locator: &WorldEntityLocator) -> EntityLookup {
    let Some(domain) = world.get_resource::<WorldIdentityDomain>() else {
        return EntityLookup::DomainUnavailable;
    };
    if domain.validate_world_binding(world).is_err() {
        return EntityLookup::WrongWorldBinding;
    }
    if locator.domain_id() != domain.id {
        return EntityLookup::WrongDomain {
            expected: domain.id,
            actual: locator.domain_id(),
        };
    }
    domain.lookup(world, locator.entity())
}

pub fn spawn_identity_entity(world: &mut World) -> Result<WorldEntityToken, IdentityDomainError> {
    validate_identity_support_topology(world, std::iter::empty())
        .map_err(|_| IdentityDomainError::LifecycleConflict)?;
    let domain = {
        let domain = world
            .get_resource::<WorldIdentityDomain>()
            .ok_or(IdentityDomainError::WorldDomainUnavailable)?;
        domain.validate_world_binding(world)?;
        domain.id()
    };
    let incarnation = allocate_runtime_entity_incarnation()?;
    let entity = world
        .spawn(IdentityEntityMarker {
            domain,
            incarnation,
        })
        .id();
    Ok(WorldEntityToken {
        domain,
        entity,
        incarnation,
    })
}

/// Adopts a live entity using the identity domain installed in `world`.
///
/// The domain remains installed throughout admission and marker insertion. Adopting an entity
/// that already belongs to the domain returns its current token without minting an incarnation.
pub fn adopt_identity_entity(
    world: &mut World,
    entity: Entity,
) -> Result<WorldEntityToken, IdentityDomainError> {
    validate_identity_adoption(world, entity)?;
    let domain = installed_domain(world)?.id;
    adopt_entity_in_domain(world, domain, entity)
}

fn validate_identity_adoption(
    world: &mut World,
    entity: Entity,
) -> Result<(), IdentityDomainError> {
    validate_identity_support_topology(world, [entity]).map_err(|error| match error {
        IdentitySupportTopologyError::TargetMissing => IdentityDomainError::EntityTokenNotAlive,
        IdentitySupportTopologyError::LifecycleConflict => IdentityDomainError::LifecycleConflict,
    })
}

fn adopt_entity_in_domain(
    world: &mut World,
    domain: WorldIdentityDomainId,
    entity: Entity,
) -> Result<WorldEntityToken, IdentityDomainError> {
    let mut target = world.entity_mut(entity);
    let incarnation = if let Some(marker) = target.get::<IdentityEntityMarker>() {
        if marker.domain != domain {
            return Err(IdentityDomainError::EntityTokenNotOwned);
        }
        marker.incarnation
    } else {
        let incarnation = allocate_runtime_entity_incarnation()?;
        target.insert(IdentityEntityMarker {
            domain,
            incarnation,
        });
        incarnation
    };
    Ok(WorldEntityToken {
        domain,
        entity,
        incarnation,
    })
}

/// Registers a fresh Scene instance without extracting the installed domain.
pub fn register_scene_instance(
    world: &mut World,
    entries: impl IntoIterator<Item = (SceneEntityId, WorldEntityToken)>,
) -> Result<SpawnedSceneInstance, IdentityDomainError> {
    let prepared = installed_domain(world)?.prepare_scene_instance_registration(world, entries)?;
    Ok(prepared.commit(&mut world.resource_mut::<WorldIdentityDomain>()))
}

/// Registers a persistent axis after shared World/domain preflight, then publishes only indexes.
pub fn register_persistent_identity(
    world: &mut World,
    token: WorldEntityToken,
    persistent: PersistentRuntimeReference,
) -> Result<WorldEntityLocator, IdentityDomainError> {
    let reference =
        installed_domain(world)?.prepare_persistent_registration(world, token, &persistent)?;
    Ok(world
        .resource_mut::<WorldIdentityDomain>()
        .commit_persistent_registration(token, persistent, reference))
}

/// Retires every semantic axis and revokes the token, leaving the runtime entity alive.
pub fn retire_identity_entity(
    world: &mut World,
    token: WorldEntityToken,
    cause: TombstoneCause,
) -> Result<(), IdentityDomainError> {
    retire_identity_entity_with_disposition(
        world,
        token,
        cause,
        PersistentRetirementDisposition::Unload,
    )
}

/// Retires every semantic axis with an explicit persistent logical disposition.
///
/// Preparation borrows the installed domain immutably. Lifecycle admission precedes marker
/// removal, followed by one short mutable borrow to publish the prepared index transition.
pub fn retire_identity_entity_with_disposition(
    world: &mut World,
    token: WorldEntityToken,
    cause: TombstoneCause,
    disposition: PersistentRetirementDisposition,
) -> Result<(), IdentityDomainError> {
    world.flush();
    let domain = installed_domain(world)?;
    let retirement =
        domain.prepare_entity_retirement_with_disposition(world, token, cause, disposition)?;
    let live_entities = domain.validate_retirement_markers(world, &retirement)?;
    revoke_retirement_markers(world, &live_entities)?;
    world
        .resource_mut::<WorldIdentityDomain>()
        .commit_retirement(retirement);
    Ok(())
}

fn revoke_retirement_markers(
    world: &mut World,
    live_entities: &[Entity],
) -> Result<(), IdentityDomainError> {
    validate_identity_support_topology(world, live_entities.iter().copied())
        .map_err(|_| IdentityDomainError::LifecycleConflict)?;
    for &entity in live_entities {
        world.entity_mut(entity).remove::<IdentityEntityMarker>();
    }
    Ok(())
}

fn installed_domain(world: &World) -> Result<&WorldIdentityDomain, IdentityDomainError> {
    let domain = world
        .get_resource::<WorldIdentityDomain>()
        .ok_or(IdentityDomainError::WorldDomainUnavailable)?;
    domain.validate_world_binding(world)?;
    Ok(domain)
}

fn collect_scene_entries(
    entries: impl IntoIterator<Item = (SceneEntityId, WorldEntityToken)>,
    maximum_entries: usize,
    maximum_claims: usize,
) -> Result<Vec<(SceneEntityId, WorldEntityToken)>, IdentityDomainError> {
    let mut by_id = BTreeMap::new();
    let mut entities = BTreeSet::new();
    for (entity_id, token) in entries {
        if by_id.len() >= maximum_entries {
            return Err(IdentityDomainError::LifetimeClaimLimit {
                requested: maximum_claims.saturating_add(1),
                maximum: maximum_claims,
            });
        }
        if by_id.insert(entity_id.clone(), token).is_some() {
            return Err(IdentityDomainError::DuplicateSceneEntityId { entity: entity_id });
        }
        if !entities.insert(token.entity()) {
            return Err(IdentityDomainError::DuplicateRuntimeEntity);
        }
    }
    Ok(by_id.into_iter().collect())
}

fn allocate_world_identity_domain_id() -> Result<WorldIdentityDomainId, IdentityDomainError> {
    allocate_world_identity_domain_id_from(&NEXT_WORLD_IDENTITY_DOMAIN_ID)
}

fn allocate_runtime_entity_incarnation() -> Result<RuntimeEntityIncarnation, IdentityDomainError> {
    allocate_runtime_entity_incarnation_from(&NEXT_RUNTIME_ENTITY_INCARNATION)
}

pub(crate) fn allocate_runtime_entity_incarnation_from(
    allocator: &AtomicU64,
) -> Result<RuntimeEntityIncarnation, IdentityDomainError> {
    let raw = allocator
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            (current != 0).then(|| current.checked_add(1).unwrap_or(0))
        })
        .map_err(|_| IdentityDomainError::RuntimeEntityIncarnationExhausted)?;
    Ok(RuntimeEntityIncarnation::from_non_zero(
        NonZeroU64::new(raw).expect("incarnation allocation never returns zero"),
    ))
}

pub(crate) fn allocate_world_identity_domain_id_from(
    allocator: &AtomicU64,
) -> Result<WorldIdentityDomainId, IdentityDomainError> {
    let raw = allocator
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            if current == 0 {
                None
            } else {
                Some(current.checked_add(1).unwrap_or(0))
            }
        })
        .map_err(|_| IdentityDomainError::WorldDomainIdExhausted)?;
    let raw = NonZeroU64::new(raw).expect("world identity domain allocator never returns zero");
    Ok(WorldIdentityDomainId::from_non_zero(raw))
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentityDomainError {
    #[error("runtime entity incarnation allocation is exhausted")]
    RuntimeEntityIncarnationExhausted,
    #[error("world identity domain id allocation is exhausted")]
    WorldDomainIdExhausted,
    #[error("world has no identity domain")]
    WorldDomainUnavailable,
    #[error("world identity domain resource is bound to a different world")]
    WorldBindingMismatch,
    #[error("scene instance id allocation is exhausted")]
    SceneInstanceExhausted,
    #[error("scene instance identity was already claimed")]
    SceneInstanceAlreadyClaimed { instance: SceneInstanceId },
    #[error("scene instance is not active")]
    SceneInstanceNotActive { instance: SceneInstanceId },
    #[error("scene instance membership does not match the identity domain")]
    SceneInstanceMembershipMismatch { instance: SceneInstanceId },
    #[error("entity reference identity was already claimed")]
    ReferenceAlreadyClaimed { reference: RuntimeEntityReference },
    #[error("runtime entity already has an identity on this axis")]
    EntityAxisAlreadyRegistered { axis: EntityIdentityAxis },
    #[error("world entity token no longer names a live entity")]
    EntityTokenNotAlive,
    #[error("world entity token is not owned by the target world identity domain")]
    EntityTokenNotOwned,
    #[error("scene identity registration contains a duplicate scene entity id")]
    DuplicateSceneEntityId { entity: SceneEntityId },
    #[error("scene identity registration contains a duplicate runtime entity")]
    DuplicateRuntimeEntity,
    #[error("scene fork does not cover the complete source entity group")]
    IncompleteSceneFork,
    #[error("scene fork does not preserve the source identity axes")]
    IncompleteSceneForkIdentityAxes { entity: SceneEntityId },
    #[error("scene identity remap validation failed")]
    InvalidSceneRemap(#[source] Box<IdentityRemapError>),
    #[error("identity lifetime claim limit was exceeded")]
    LifetimeClaimLimit { requested: usize, maximum: usize },
    #[error("persistent logical record limit was exceeded")]
    PersistentLogicalRecordLimit { requested: usize, maximum: usize },
    #[error("persistent identity has been deleted")]
    PersistentIdentityDeleted {
        reference: PersistentRuntimeReference,
    },
    #[error("persistent identity is still active")]
    PersistentIdentityActive {
        reference: PersistentRuntimeReference,
    },
    #[error("persistent identity is not registered")]
    PersistentIdentityMissing {
        reference: PersistentRuntimeReference,
    },
    #[error("identity operation targets the wrong world domain")]
    WrongDomain {
        expected: WorldIdentityDomainId,
        actual: WorldIdentityDomainId,
    },
    #[error("runtime entity has no registered semantic identity")]
    EntityNotRegistered,
    #[error("registered runtime entity is stale or no longer owned by the identity domain")]
    StaleRegistration,
    #[error("identity retirement sequence is exhausted")]
    RetirementSequenceExhausted,
    #[error("identity lifecycle topology is incompatible with atomic publication or retirement")]
    LifecycleConflict,
}

impl From<IdentityAllocationError> for IdentityDomainError {
    fn from(_: IdentityAllocationError) -> Self {
        Self::SceneInstanceExhausted
    }
}
