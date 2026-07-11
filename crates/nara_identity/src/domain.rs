use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_core::ItemLimit;
use nara_ecs::{Component, Entity, Resource, World, world::WorldId};
use thiserror::Error;

use crate::{
    EntityReference, IdentityAllocationError, IdentityRemapError, PersistentRuntimeReference,
    RuntimeEntityReference, SceneEntityId, SceneInstanceId, WorldEntityLocator,
    WorldEntityLocatorRemap, WorldIdentityDomainId, allocator::MonotonicNonZeroU64Allocator,
};

const DEFAULT_LIFETIME_CLAIMS: usize = 1_048_576;
const DEFAULT_RECENT_TOMBSTONES: usize = 4_096;

static NEXT_WORLD_IDENTITY_DOMAIN_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldIdentityDomainSettings {
    lifetime_claims: ItemLimit,
    recent_tombstones: ItemLimit,
}

impl WorldIdentityDomainSettings {
    pub fn new(
        lifetime_claims: ItemLimit,
        recent_tombstones: ItemLimit,
    ) -> Result<Self, WorldIdentitySettingsError> {
        if recent_tombstones > lifetime_claims {
            return Err(WorldIdentitySettingsError::TombstonesExceedClaims {
                tombstones: recent_tombstones.get(),
                claims: lifetime_claims.get(),
            });
        }
        Ok(Self {
            lifetime_claims,
            recent_tombstones,
        })
    }

    #[must_use]
    pub const fn lifetime_claims(self) -> ItemLimit {
        self.lifetime_claims
    }

    #[must_use]
    pub const fn recent_tombstones(self) -> ItemLimit {
        self.recent_tombstones
    }
}

impl Default for WorldIdentityDomainSettings {
    fn default() -> Self {
        Self {
            lifetime_claims: ItemLimit::new(DEFAULT_LIFETIME_CLAIMS)
                .expect("default identity claim limit is non-zero"),
            recent_tombstones: ItemLimit::new(DEFAULT_RECENT_TOMBSTONES)
                .expect("default identity tombstone limit is non-zero"),
        }
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum WorldIdentitySettingsError {
    #[error("recent tombstone limit cannot exceed the lifetime claim limit")]
    TombstonesExceedClaims { tombstones: usize, claims: usize },
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
}

#[derive(Debug, Component)]
struct IdentityEntityMarker {
    domain: WorldIdentityDomainId,
}

impl WorldEntityToken {
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

    fn reference_count(&self) -> usize {
        self.entities
            .values()
            .map(|references| 1 + usize::from(references.persistent.is_some()))
            .fold(0, usize::saturating_add)
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
        self.contains(entity)
            .then(|| RuntimeEntityReference::scene(self.instance, entity.clone()))
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
    active_scene_instances: BTreeMap<SceneInstanceId, BTreeSet<SceneEntityId>>,
    claimed_references: BTreeSet<RuntimeEntityReference>,
    scene_entities: BTreeMap<RuntimeEntityReference, Entity>,
    persistent_entities: BTreeMap<RuntimeEntityReference, Entity>,
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
            registered_entities: self.identities_by_entity.len(),
            recent_tombstones: self.recent_tombstones.len(),
        }
    }

    pub fn register_new_scene_instance(
        &mut self,
        world: &World,
        entries: impl IntoIterator<Item = (SceneEntityId, WorldEntityToken)>,
    ) -> Result<SpawnedSceneInstance, IdentityDomainError> {
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
        let allocated = self
            .scene_instance_allocator
            .allocate()
            .expect("scene instance allocation was preflighted");
        debug_assert_eq!(allocated, instance.non_zero());
        Ok(self.commit_scene_instance(instance, entries))
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

    pub fn register_persistent(
        &mut self,
        world: &World,
        token: WorldEntityToken,
        persistent: PersistentRuntimeReference,
    ) -> Result<WorldEntityLocator, IdentityDomainError> {
        self.validate_live_token(world, token)?;
        let entity = token.entity();
        let reference = RuntimeEntityReference::persistent(persistent);
        if self.claimed_references.contains(&reference) {
            return Err(IdentityDomainError::ReferenceAlreadyClaimed { reference });
        }
        if self
            .identities_by_entity
            .get(&entity)
            .is_some_and(|identity| identity.persistent.is_some())
        {
            return Err(IdentityDomainError::EntityAxisAlreadyRegistered {
                axis: EntityIdentityAxis::Persistent,
            });
        }
        self.check_claim_capacity(1)?;

        self.claimed_references.insert(reference.clone());
        self.persistent_entities.insert(reference.clone(), entity);
        self.identities_by_entity
            .entry(entity)
            .or_default()
            .persistent = Some(reference.clone());
        Ok(WorldEntityLocator::new(self.id, reference))
    }

    #[must_use]
    pub fn lookup(&self, world: &World, reference: &RuntimeEntityReference) -> EntityLookup {
        if self.validate_world_binding(world).is_err() {
            return EntityLookup::WrongWorldBinding;
        }
        match self.lookup_registered(reference) {
            EntityLookup::Resolved(entity) if !self.is_live_owned_entity(world, entity) => {
                EntityLookup::StaleRegistration
            }
            result => result,
        }
    }

    fn lookup_registered(&self, reference: &RuntimeEntityReference) -> EntityLookup {
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
                self.lookup(
                    world,
                    &RuntimeEntityReference::scene(instance, entity.clone()),
                )
            }
            EntityReference::Persistent { entity } => {
                self.lookup(world, &RuntimeEntityReference::persistent(entity.clone()))
            }
        }
    }

    pub fn locators_for_token(
        &self,
        world: &World,
        token: WorldEntityToken,
    ) -> Result<Option<WorldEntityLocators>, IdentityDomainError> {
        self.validate_live_token(world, token)?;
        let entity = token.entity();
        let Some(identity) = self.identities_by_entity.get(&entity) else {
            return Ok(None);
        };
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
                .iter()
                .zip(instance.entity_ids.iter())
                .any(|(active, expected)| active != expected)
        {
            return Err(IdentityDomainError::SceneInstanceMembershipMismatch {
                instance: instance.instance,
            });
        }

        let mut entities = BTreeMap::new();
        for entity_id in active_ids {
            let scene = RuntimeEntityReference::scene(instance.instance, entity_id.clone());
            let entity = self.scene_entities.get(&scene).copied().ok_or(
                IdentityDomainError::SceneInstanceMembershipMismatch {
                    instance: instance.instance,
                },
            )?;
            if !self.is_live_owned_entity(world, entity) {
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
        world: &World,
        instance: &SpawnedSceneInstance,
        cause: TombstoneCause,
    ) -> Result<Vec<Entity>, IdentityDomainError> {
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
            .iter()
            .any(|entity_id| !instance.contains(entity_id))
        {
            return Err(IdentityDomainError::SceneInstanceMembershipMismatch {
                instance: instance.instance,
            });
        }

        let entities = active_ids
            .iter()
            .map(|entity_id| {
                let reference = RuntimeEntityReference::scene(instance.instance, entity_id.clone());
                self.scene_entities.get(&reference).copied().ok_or(
                    IdentityDomainError::SceneInstanceMembershipMismatch {
                        instance: instance.instance,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.retire_entities_and_instance(Some(instance.instance), entities, cause)
    }

    pub fn retire_entity(
        &mut self,
        world: &World,
        token: WorldEntityToken,
        cause: TombstoneCause,
    ) -> Result<(), IdentityDomainError> {
        self.validate_world_binding(world)?;
        self.validate_token_domain(token)?;
        self.retire_entities_and_instance(None, vec![token.entity()], cause)?;
        Ok(())
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
            let reference = RuntimeEntityReference::scene(instance, entity_id.clone());
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
        let additional = source.reference_count().checked_add(1).ok_or(
            IdentityDomainError::LifetimeClaimLimit {
                requested: usize::MAX,
                maximum: self.settings.lifetime_claims.get(),
            },
        )?;
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

            let scene_reference = RuntimeEntityReference::scene(instance, entity_id.clone());
            if self.claimed_references.contains(&scene_reference) {
                return Err(IdentityDomainError::ReferenceAlreadyClaimed {
                    reference: scene_reference,
                });
            }
            if let Some(persistent) = &persistent {
                let reference = RuntimeEntityReference::persistent(persistent.clone());
                if self.claimed_references.contains(&reference)
                    || !persistent_references.insert(reference.clone())
                {
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

        let target_snapshot = SceneIdentitySnapshot {
            domain: self.id,
            instance,
            entities: by_id
                .values()
                .map(|entry| {
                    (
                        entry.entity_id.clone(),
                        SceneIdentityReferences {
                            scene: RuntimeEntityReference::scene(instance, entry.entity_id.clone()),
                            persistent: entry
                                .persistent
                                .clone()
                                .map(RuntimeEntityReference::persistent),
                        },
                    )
                })
                .collect(),
        };
        let remap = WorldEntityLocatorRemap::between_scene_snapshots(source, &target_snapshot)
            .map_err(IdentityDomainError::InvalidSceneRemap)?;
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

    fn commit_scene_instance(
        &mut self,
        instance: SceneInstanceId,
        entries: Vec<(SceneEntityId, WorldEntityToken)>,
    ) -> SpawnedSceneInstance {
        self.claimed_scene_instances.insert(instance);
        let mut entity_ids = Vec::with_capacity(entries.len());
        let mut active_ids = BTreeSet::new();
        for (entity_id, token) in entries {
            let entity = token.entity();
            let reference = RuntimeEntityReference::scene(instance, entity_id.clone());
            self.claimed_references.insert(reference.clone());
            self.scene_entities.insert(reference.clone(), entity);
            self.identities_by_entity.entry(entity).or_default().scene = Some(reference);
            active_ids.insert(entity_id.clone());
            entity_ids.push(entity_id);
        }
        self.active_scene_instances.insert(instance, active_ids);
        SpawnedSceneInstance {
            domain: self.id,
            instance,
            entity_ids,
        }
    }

    fn commit_scene_identity_group(
        &mut self,
        instance: SceneInstanceId,
        entries: Vec<SceneIdentityGroupEntry>,
    ) -> SpawnedSceneInstance {
        self.claimed_scene_instances.insert(instance);
        let mut entity_ids = Vec::with_capacity(entries.len());
        let mut active_ids = BTreeSet::new();
        for entry in entries {
            let entity = entry.token.entity();
            let scene_reference = RuntimeEntityReference::scene(instance, entry.entity_id.clone());
            self.claimed_references.insert(scene_reference.clone());
            self.scene_entities.insert(scene_reference.clone(), entity);
            let identity = self.identities_by_entity.entry(entity).or_default();
            identity.scene = Some(scene_reference);

            if let Some(persistent) = entry.persistent {
                let persistent_reference = RuntimeEntityReference::persistent(persistent);
                self.claimed_references.insert(persistent_reference.clone());
                self.persistent_entities
                    .insert(persistent_reference.clone(), entity);
                identity.persistent = Some(persistent_reference);
            }
            active_ids.insert(entry.entity_id.clone());
            entity_ids.push(entry.entity_id);
        }
        self.active_scene_instances.insert(instance, active_ids);
        SpawnedSceneInstance {
            domain: self.id,
            instance,
            entity_ids,
        }
    }

    fn retire_entities_and_instance(
        &mut self,
        instance: Option<SceneInstanceId>,
        mut entities: Vec<Entity>,
        cause: TombstoneCause,
    ) -> Result<Vec<Entity>, IdentityDomainError> {
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
                    } = reference
                    && let Some(active_ids) = self.active_scene_instances.get_mut(&instance)
                {
                    active_ids.remove(&entity_id);
                }
            }
            if let Some(reference) = identity.persistent {
                self.persistent_entities.remove(&reference);
            }
            self.identities_by_entity.remove(&entity);
        }
        self.retirement_sequence = sequence_allocator;
        self.record_tombstones(tombstones);
        Ok(entities)
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
        if !self.is_live_owned_entity(world, token.entity()) {
            return Err(IdentityDomainError::EntityTokenNotOwned);
        }
        Ok(())
    }

    fn is_live_owned_entity(&self, world: &World, entity: Entity) -> bool {
        world.get_entity(entity).is_ok()
            && world
                .get::<IdentityEntityMarker>(entity)
                .is_some_and(|marker| marker.domain == self.id)
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
    let domain = {
        let domain = world
            .get_resource::<WorldIdentityDomain>()
            .ok_or(IdentityDomainError::WorldDomainUnavailable)?;
        domain.validate_world_binding(world)?;
        domain.id()
    };
    let entity = world.spawn(IdentityEntityMarker { domain }).id();
    Ok(WorldEntityToken { domain, entity })
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

pub(crate) fn allocate_world_identity_domain_id_from(
    allocator: &AtomicU64,
) -> Result<WorldIdentityDomainId, IdentityDomainError> {
    let raw = allocator
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
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
    InvalidSceneRemap(#[source] IdentityRemapError),
    #[error("identity lifetime claim limit was exceeded")]
    LifetimeClaimLimit { requested: usize, maximum: usize },
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
}

impl From<IdentityAllocationError> for IdentityDomainError {
    fn from(_: IdentityAllocationError) -> Self {
        Self::SceneInstanceExhausted
    }
}
