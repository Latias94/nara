use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet, HashSet},
};

use nara_core::ItemLimit;
use nara_ecs::{
    __private::LifecycleFreeInsertionPlan,
    Component, Entity, Resource, World,
    bevy_ecs::component::{ComponentId, Mutable},
};
use nara_hierarchy::{Children, Parent};
use nara_identity::SceneEntityId;
use nara_reflect::{ComponentRegistry, ComponentTypeId};

use crate::{spawn::SceneEntitySource, validation::PreparedScene};

/// Required item limits for one provisional product-scene replacement transaction.
///
/// Overlay writes count owned component insertions and product-resource replacements. The limits
/// bound retained entries, not the byte size of arbitrary Rust values. Engine ceilings are
/// enforced before the transaction creates scratch entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneProductTransactionLimits {
    overlay_writes: ItemLimit,
    additional_retirements: ItemLimit,
}

impl SceneProductTransactionLimits {
    /// Maximum overlay entries accepted by the engine for one transaction.
    pub const MAX_OVERLAY_WRITES: usize = 100_000;

    /// Maximum additional retirement tokens accepted by the engine for one transaction.
    pub const MAX_ADDITIONAL_RETIREMENTS: usize = 100_000;

    /// Creates the required caller limits for one transaction.
    #[must_use]
    pub const fn new(overlay_writes: ItemLimit, additional_retirements: ItemLimit) -> Self {
        Self {
            overlay_writes,
            additional_retirements,
        }
    }

    /// Returns the caller limit for component and resource overlay entries.
    #[must_use]
    pub(crate) const fn overlay_writes(self) -> ItemLimit {
        self.overlay_writes
    }

    /// Returns the caller limit for additional product-owned retirement tokens.
    #[must_use]
    pub(crate) const fn additional_retirements(self) -> ItemLimit {
        self.additional_retirements
    }
}

/// Exact replacement baseline owned by a concrete product resource.
///
/// A game or package implements this trait only for its own resource types. Rust's orphan rules
/// prevent downstream code from opting engine-owned resources into this provisional writer.
///
/// Capture and comparison must be bounded, pure observations: no mutation, I/O, waiting, or
/// external callbacks. The baseline must retain the resource incarnation and all content facts
/// relevant to replacement. Owners must distinguish newly installed resource incarnations and
/// writes that restore an earlier value (ABA), including writes within the same ECS change tick.
/// A newly constructed resource must have a distinct baseline even with identical content. Use
/// exact owner identities and mutation generations or equivalent per-slot/state facts; presence,
/// collection length, a content hash alone, and ECS change ticks are insufficient.
/// Temporarily removing and returning the same unchanged Rust owner preserves its logical
/// incarnation; the ECS resource slot is not an independent revision authority here.
///
/// The Scene transaction retains one baseline from the installed resource during preparation
/// and compares it again before publication. It does not infer resource ownership or revisions.
pub trait SceneProductResource: Resource<Mutability = Mutable> {
    /// Owned, immutable facts needed to admit replacement of this resource incarnation.
    type Baseline: Send + Sync + 'static;

    /// Captures the exact installed incarnation and replacement-relevant content facts.
    fn capture_baseline(&self) -> Self::Baseline;

    /// Whether this is still the captured incarnation with unchanged relevant content facts.
    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool;
}

pub(crate) enum SceneProductOverlayError {
    LimitExceeded,
    MissingTarget(SceneEntityId),
    ComponentUnregistered(SceneEntityId),
    DuplicateComponent(SceneEntityId),
    ExistingComponent(SceneEntityId),
    ReservedComponent(SceneEntityId),
    ResourceMissing,
    ResourceChanged,
    DuplicateResource,
}

struct PendingComponentWrite {
    target: SceneEntityId,
    lower: Box<LowerComponentWrite>,
}

struct PendingResourceWrite {
    validate: Box<ValidateResourceWrite>,
    commit: Box<CommitResourceWrite>,
}

type LowerComponentWrite = dyn FnOnce(Entity, &mut LifecycleFreeInsertionPlan) + Send + Sync;
type ValidateResourceWrite = dyn Fn(&World) -> Result<(), SceneProductOverlayError> + Send + Sync;
type CommitResourceWrite = dyn FnOnce(&mut World) + Send + Sync;

/// Scoped writer for runtime-only values attached to a replacement scene candidate.
///
/// The writer exposes stable scene IDs rather than candidate `Entity` values. It is constructed
/// only by the replacement transaction and lowers every accepted value before returning control to
/// the caller, so neither a candidate map nor a candidate-World handle can escape the call.
pub struct SceneProductOverlayWriter<'world> {
    world: &'world World,
    registry: &'world ComponentRegistry,
    authored_components: BTreeMap<SceneEntityId, BTreeSet<ComponentTypeId>>,
    limit: usize,
    writes: usize,
    seen_components: HashSet<(SceneEntityId, ComponentId)>,
    seen_resources: HashSet<TypeId>,
    components: Vec<PendingComponentWrite>,
    resources: Vec<PendingResourceWrite>,
    error: Option<SceneProductOverlayError>,
}

impl<'world> SceneProductOverlayWriter<'world> {
    pub(crate) fn new(
        world: &'world World,
        registry: &'world ComponentRegistry,
        scene: &PreparedScene,
        limit: ItemLimit,
    ) -> Self {
        let authored_components = scene
            .entities
            .iter()
            .map(|entity| {
                (
                    entity.id.clone(),
                    entity
                        .components
                        .iter()
                        .map(|component| component.id.clone())
                        .collect(),
                )
            })
            .collect();
        Self {
            world,
            registry,
            authored_components,
            limit: limit.get(),
            writes: 0,
            seen_components: HashSet::new(),
            seen_resources: HashSet::new(),
            components: Vec::new(),
            resources: Vec::new(),
            error: None,
        }
    }

    /// Adds one owned runtime component to a candidate selected by stable scene ID.
    ///
    /// The component type must already be registered in the target World. Invalid writes are
    /// retained as a transaction rejection; no candidate or registry mutation occurs here.
    /// Ordinary product composition is responsible for registering accepted runtime-only types
    /// during plugin build and seal; this provisional advanced writer does not infer provenance
    /// from an existing ECS component ID.
    pub fn insert_component<T>(&mut self, target: SceneEntityId, component: T) -> &mut Self
    where
        T: Component,
    {
        if self.error.is_some() || !self.reserve_write() {
            return self;
        }
        if !self.authored_components.contains_key(&target) {
            self.error = Some(SceneProductOverlayError::MissingTarget(target));
            return self;
        }

        let Some(component_id) = self.world.component_id::<T>() else {
            self.error = Some(SceneProductOverlayError::ComponentUnregistered(target));
            return self;
        };
        if is_reserved_component::<T>() {
            self.error = Some(SceneProductOverlayError::ReservedComponent(target));
            return self;
        }
        if self.registry.schema_for_type::<T>().is_some_and(|schema| {
            self.authored_components
                .get(&target)
                .is_some_and(|components| components.contains(schema.id()))
        }) {
            self.error = Some(SceneProductOverlayError::ExistingComponent(target));
            return self;
        }
        if !self.seen_components.insert((target.clone(), component_id)) {
            self.error = Some(SceneProductOverlayError::DuplicateComponent(target));
            return self;
        }

        self.components.push(PendingComponentWrite {
            target,
            lower: Box::new(move |entity, plan| plan.push(entity, component)),
        });
        self
    }

    /// Replaces one existing product-owned resource during the scene commit tail.
    ///
    /// Resource insertion is deliberately unsupported: plugin composition must install the
    /// resource before runtime publication, and the owner trait limits this path to downstream
    /// product-owned types. The installed resource supplies an exact baseline; a missing or
    /// changed resource rejects publication before any Scene, resource, or binding commit.
    /// A receipt-owning run resource that needs the resulting `SpawnedSceneInstance` remains
    /// outside this writer: hold it through an exclusive `World::resource_scope`, then update it
    /// without failure after a successful replacement report and before the exclusive system returns.
    pub fn replace_resource<R>(&mut self, replacement: R) -> &mut Self
    where
        R: SceneProductResource,
    {
        if self.error.is_some() || !self.reserve_write() {
            return self;
        }
        let Some(installed) = self.world.get_resource::<R>() else {
            self.error = Some(SceneProductOverlayError::ResourceMissing);
            return self;
        };
        if !self.seen_resources.insert(TypeId::of::<R>()) {
            self.error = Some(SceneProductOverlayError::DuplicateResource);
            return self;
        }

        let baseline = installed.capture_baseline();
        self.resources.push(PendingResourceWrite {
            validate: Box::new(move |world| {
                let installed = world
                    .get_resource::<R>()
                    .ok_or(SceneProductOverlayError::ResourceMissing)?;
                if installed.matches_baseline(&baseline) {
                    Ok(())
                } else {
                    Err(SceneProductOverlayError::ResourceChanged)
                }
            }),
            commit: Box::new(move |world| {
                *world
                    .get_resource_mut::<R>()
                    .expect("prepared product resource replacement must remain installed") =
                    replacement;
            }),
        });
        self
    }

    fn reserve_write(&mut self) -> bool {
        if self.writes >= self.limit {
            self.error = Some(SceneProductOverlayError::LimitExceeded);
            return false;
        }
        self.writes += 1;
        true
    }

    pub(crate) fn finish(self) -> Result<PreparedSceneProductOverlay, SceneProductOverlayError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(PreparedSceneProductOverlay {
            components: self.components,
            resources: self.resources,
        })
    }
}

pub(crate) struct PreparedSceneProductOverlay {
    components: Vec<PendingComponentWrite>,
    resources: Vec<PendingResourceWrite>,
}

impl PreparedSceneProductOverlay {
    pub(crate) fn validate_resources(&self, world: &World) -> Result<(), SceneProductOverlayError> {
        self.resources
            .iter()
            .try_for_each(|write| (write.validate)(world))
    }

    pub(crate) fn lower_components(
        self,
        spawned_by_id: &BTreeMap<SceneEntityId, Entity>,
    ) -> (LifecycleFreeInsertionPlan, PreparedSceneProductResources) {
        let mut plan = LifecycleFreeInsertionPlan::new();
        for component in self.components {
            let target = spawned_by_id
                .get(&component.target)
                .copied()
                .expect("validated product overlay target must exist in the candidate map");
            (component.lower)(target, &mut plan);
        }
        (
            plan,
            PreparedSceneProductResources {
                writes: self.resources,
            },
        )
    }
}

pub(crate) struct PreparedSceneProductResources {
    writes: Vec<PendingResourceWrite>,
}

impl PreparedSceneProductResources {
    pub(crate) fn validate(&self, world: &World) -> Result<(), SceneProductOverlayError> {
        self.writes
            .iter()
            .try_for_each(|write| (write.validate)(world))
    }

    pub(crate) fn commit(self, world: &mut World) {
        for write in self.writes {
            (write.commit)(world);
        }
    }
}

fn is_reserved_component<T: Component>() -> bool {
    let type_id = TypeId::of::<T>();
    type_id == TypeId::of::<Parent>()
        || type_id == TypeId::of::<Children>()
        || type_id == TypeId::of::<SceneEntitySource>()
        || type_id == TypeId::of::<crate::SceneDataComponents>()
}
