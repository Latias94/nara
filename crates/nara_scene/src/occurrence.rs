use std::{collections::BTreeMap, error::Error, fmt, mem::size_of, sync::Arc};

use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::{Resource, World, bevy_ecs::world::WorldId};
use nara_identity::{SceneInstanceId, SpawnedSceneInstance, WorldIdentityDomainId};

use crate::{SourceDocumentKey, provenance::provenance_string_bytes};

/// One explicitly declared logical placement in a receiving World.
///
/// Neither coordinate is derived from the source, an allocator, a pointer or a callsite. Reusing
/// an ended slot declares continuity; it does not retain deleted-object history.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneOccurrenceKey {
    namespace: Arc<str>,
    slot: Arc<str>,
}

impl SceneOccurrenceKey {
    pub fn new(
        namespace: impl Into<String>,
        slot: impl Into<String>,
    ) -> Result<Self, SceneOccurrenceKeyError> {
        let namespace = namespace.into();
        let slot = slot.into();
        if namespace.is_empty() {
            return Err(SceneOccurrenceKeyError::EmptyNamespace);
        }
        if slot.is_empty() {
            return Err(SceneOccurrenceKeyError::EmptySlot);
        }
        if namespace.contains('\0') || slot.contains('\0') {
            return Err(SceneOccurrenceKeyError::ContainsNull);
        }
        Ok(Self {
            namespace: Arc::from(namespace),
            slot: Arc::from(slot),
        })
    }

    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    #[must_use]
    pub fn slot(&self) -> &str {
        &self.slot
    }

    fn retained_bytes(&self) -> Option<usize> {
        size_of::<Self>()
            .checked_add(provenance_string_bytes(&self.namespace)?)?
            .checked_add(provenance_string_bytes(&self.slot)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneOccurrenceKeyError {
    EmptyNamespace,
    EmptySlot,
    ContainsNull,
}

impl fmt::Display for SceneOccurrenceKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyNamespace => "Scene occurrence namespace must not be empty",
            Self::EmptySlot => "Scene occurrence slot must not be empty",
            Self::ContainsNull => "Scene occurrence coordinates must not contain NUL",
        })
    }
}
impl Error for SceneOccurrenceKeyError {}

/// Admission ceilings for active Scene occurrences, not runtime Identity history.
///
/// Insert this resource before materialization to select different ceilings. Shrinking a ceiling
/// never evicts an active occurrence. Retained bytes model logical ownership, not allocator/RSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct SceneOccurrenceLimits {
    active_occurrences: usize,
    retained_bytes: usize,
}

impl Default for SceneOccurrenceLimits {
    fn default() -> Self {
        Self {
            active_occurrences: 4096,
            retained_bytes: 64 * 1024 * 1024,
        }
    }
}
impl SceneOccurrenceLimits {
    #[must_use]
    pub const fn with_active_occurrences(mut self, limit: ItemLimit) -> Self {
        self.active_occurrences = limit.get();
        self
    }
    #[must_use]
    pub const fn with_retained_bytes(mut self, limit: ByteLimit) -> Self {
        self.retained_bytes = limit.get();
        self
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SceneSourceContext {
    pub(crate) source: SourceDocumentKey,
    pub(crate) occurrence: SceneOccurrenceKey,
}

#[derive(Debug)]
struct ActiveOccurrence {
    context: Arc<SceneSourceContext>,
    domain: WorldIdentityDomainId,
    instance: SceneInstanceId,
    retained_bytes: usize,
}

#[derive(Resource)]
struct ActiveSceneOccurrences {
    world: WorldId,
    entries: BTreeMap<SceneOccurrenceKey, ActiveOccurrence>,
    retained_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OccurrenceError {
    WrongWorld,
    Occupied,
    Missing,
    Changed,
    Budget,
    Lifecycle,
}

pub(crate) struct PreparedSceneOccurrence {
    world: WorldId,
    pub(crate) context: Arc<SceneSourceContext>,
    predecessor: Option<(
        WorldIdentityDomainId,
        SceneInstanceId,
        Arc<SceneSourceContext>,
    )>,
    retained_bytes: usize,
    projection: bool,
}

fn validate_state(world: &World) -> Result<Option<&ActiveSceneOccurrences>, OccurrenceError> {
    let state = world.get_resource::<ActiveSceneOccurrences>();
    if state.is_some_and(|state| state.world != world.id()) {
        return Err(OccurrenceError::WrongWorld);
    }
    Ok(state)
}

pub(crate) fn occurrence_for_instance<'a>(
    world: &'a World,
    receipt: &SpawnedSceneInstance,
) -> Result<&'a SceneOccurrenceKey, OccurrenceError> {
    validate_state(world)?
        .ok_or(OccurrenceError::Missing)?
        .entries
        .iter()
        .find(|(_, entry)| {
            entry.domain == receipt.domain_id() && entry.instance == receipt.instance_id()
        })
        .map(|(key, _)| key)
        .ok_or(OccurrenceError::Missing)
}

pub(crate) fn validate_projection_occurrence(
    world: &World,
    current: Option<&SpawnedSceneInstance>,
) -> Result<(), OccurrenceError> {
    let state = validate_state(world)?;
    match current {
        None if state.is_none_or(|state| state.entries.is_empty()) => Ok(()),
        Some(receipt)
            if state.is_some_and(|state| state.entries.len() == 1)
                && occurrence_for_instance(world, receipt).is_ok() =>
        {
            Ok(())
        }
        _ => Err(OccurrenceError::Changed),
    }
}

impl PreparedSceneOccurrence {
    pub(crate) fn prepare(
        world: &mut World,
        source: &SourceDocumentKey,
        occurrence: &SceneOccurrenceKey,
        current: Option<&SpawnedSceneInstance>,
        projection: bool,
    ) -> Result<Self, OccurrenceError> {
        world.register_component::<ActiveSceneOccurrences>();
        if projection {
            validate_projection_occurrence(world, current)?;
        }
        let state = validate_state(world)?;
        let predecessor = match current {
            Some(receipt) => {
                let entry = match state
                    .ok_or(OccurrenceError::Missing)?
                    .entries
                    .get(occurrence)
                {
                    Some(entry)
                        if entry.domain == receipt.domain_id()
                            && entry.instance == receipt.instance_id() =>
                    {
                        entry
                    }
                    _ => {
                        occurrence_for_instance(world, receipt)?;
                        return Err(OccurrenceError::Changed);
                    }
                };
                Some((entry.domain, entry.instance, Arc::clone(&entry.context)))
            }
            None => None,
        };
        let retained_bytes = size_of::<(SceneOccurrenceKey, ActiveOccurrence)>()
            .checked_add(6 * size_of::<usize>())
            .and_then(|bytes| {
                bytes.checked_add(size_of::<SceneSourceContext>() + 2 * size_of::<usize>())
            })
            .and_then(|bytes| bytes.checked_add(source.checked_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(occurrence.retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(occurrence.retained_bytes()?))
            .ok_or(OccurrenceError::Budget)?;
        Self::validate_admission(world, occurrence, predecessor.as_ref(), retained_bytes)?;
        Ok(Self {
            world: world.id(),
            context: Arc::new(SceneSourceContext {
                source: source.clone(),
                occurrence: occurrence.clone(),
            }),
            predecessor,
            retained_bytes,
            projection,
        })
    }

    fn validate_admission(
        world: &World,
        occurrence: &SceneOccurrenceKey,
        predecessor: Option<&(
            WorldIdentityDomainId,
            SceneInstanceId,
            Arc<SceneSourceContext>,
        )>,
        new_bytes: usize,
    ) -> Result<(), OccurrenceError> {
        let state = validate_state(world)?;
        let existing = state.and_then(|state| state.entries.get(occurrence));
        match (existing, predecessor) {
            (None, None) => {}
            (Some(entry), Some((domain, instance, context)))
                if entry.domain == *domain
                    && entry.instance == *instance
                    && Arc::ptr_eq(&entry.context, context) => {}
            (Some(_), None) => return Err(OccurrenceError::Occupied),
            _ => return Err(OccurrenceError::Changed),
        }
        let limits = world
            .get_resource::<SceneOccurrenceLimits>()
            .copied()
            .unwrap_or_default();
        let count = state
            .map_or(0, |state| state.entries.len())
            .checked_add(usize::from(existing.is_none()))
            .ok_or(OccurrenceError::Budget)?;
        let bytes = state
            .map_or(0, |state| state.retained_bytes)
            .checked_sub(existing.map_or(0, |entry| entry.retained_bytes))
            .and_then(|bytes| bytes.checked_add(new_bytes))
            .ok_or(OccurrenceError::Budget)?;
        if count > limits.active_occurrences || bytes > limits.retained_bytes {
            return Err(OccurrenceError::Budget);
        }
        if state.is_none() {
            nara_ecs::__private::validate_resource_insertion::<ActiveSceneOccurrences>(world)
                .map_err(|_| OccurrenceError::Lifecycle)?;
        }
        Ok(())
    }

    pub(crate) fn validate(
        &self,
        world: &World,
        current: Option<&SpawnedSceneInstance>,
    ) -> Result<(), OccurrenceError> {
        if world.id() != self.world {
            return Err(OccurrenceError::WrongWorld);
        }
        if self.projection {
            validate_projection_occurrence(world, current)?;
        }
        Self::validate_admission(
            world,
            &self.context.occurrence,
            self.predecessor.as_ref(),
            self.retained_bytes,
        )
    }

    // Only after final admission, inside the same exclusive, callback-free publication tail.
    pub(crate) fn commit(self, world: &mut World, receipt: &SpawnedSceneInstance) {
        if !world.contains_resource::<ActiveSceneOccurrences>() {
            world.insert_resource(ActiveSceneOccurrences {
                world: world.id(),
                entries: BTreeMap::new(),
                retained_bytes: 0,
            });
        }
        let mut state = world.resource_mut::<ActiveSceneOccurrences>();
        let entry = ActiveOccurrence {
            context: Arc::clone(&self.context),
            domain: receipt.domain_id(),
            instance: receipt.instance_id(),
            retained_bytes: self.retained_bytes,
        };
        let old = state.entries.insert(self.context.occurrence.clone(), entry);
        state.retained_bytes = state.retained_bytes - old.map_or(0, |entry| entry.retained_bytes)
            + self.retained_bytes;
    }
}

pub(crate) fn release_occurrence(world: &mut World, receipt: &SpawnedSceneInstance) {
    let key = occurrence_for_instance(world, receipt)
        .expect("exact occurrence retirement was admitted")
        .clone();
    let mut state = world.resource_mut::<ActiveSceneOccurrences>();
    let entry = state
        .entries
        .remove(&key)
        .expect("admitted occurrence stays installed");
    state.retained_bytes -= entry.retained_bytes;
}
