use std::sync::atomic::{AtomicU64, Ordering};

use nara_app::{FixedTime, RealTime, RuntimeGeneration};
use nara_ecs::{
    Entity, Res, Resource, SystemParam, World, bevy_ecs::entity::EntityHashMap,
    bevy_ecs::world::WorldId,
};

use crate::{GlobalTransform2d, Transform2d};

// Simulation and presentation share one identity space. Restoring an old private slot or
// rebuilding its owner must never mint a publication that another retained payload already has.
pub(crate) fn next_publication() -> Option<u64> {
    static NEXT_PUBLICATION: AtomicU64 = AtomicU64::new(1);
    NEXT_PUBLICATION
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .ok()
}

/// The owner boundary at which a spatial cut was published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialSnapshotPhase {
    Startup,
    Fixed,
    PostUpdate,
    Extract,
    Presented,
}

/// Identifies a publication. A copied stamp does not grant access to its payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialSnapshotStamp {
    pub world: WorldId,
    pub runtime_generation: Option<RuntimeGeneration>,
    pub phase: SpatialSnapshotPhase,
    pub tick: u64,
    pub frame: u64,
    pub publication: u64,
    pub hierarchy_generation: u64,
}

/// An unavailable publication is never replaced by a last-good or local-space value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SpatialSnapshotError {
    #[error("the requested spatial snapshot has not been published successfully")]
    Unavailable,
    #[error("the spatial snapshot belongs to a different World")]
    WrongWorld,
    #[error("the spatial snapshot belongs to a different runtime generation")]
    WrongRuntimeGeneration,
    #[error("the spatial snapshot was published at {actual:?}, expected {expected:?}")]
    WrongPhase {
        expected: SpatialSnapshotPhase,
        actual: SpatialSnapshotPhase,
    },
    #[error("the spatial snapshot does not describe this tick/frame")]
    WrongTime,
    #[error("the spatial snapshot clocks are unavailable")]
    MissingClock,
    #[error("entity {entity:?} is not a spatial participant in this snapshot")]
    MissingEntity { entity: Entity },
    #[error("entity {entity:?} is not a structural participant in this snapshot")]
    MissingStructuralEntity { entity: Entity },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpatialSource {
    pub local: Transform2d,
    pub parent: Option<Entity>,
}

#[derive(Default)]
pub(crate) struct SpatialPayload {
    pub sources: EntityHashMap<SpatialSource>,
    pub globals: EntityHashMap<GlobalTransform2d>,
    pub order: Vec<Entity>,
    pub structural_indices: EntityHashMap<usize>,
    pub spatial_order: Vec<Entity>,
}

#[derive(Default, Resource)]
pub(crate) struct SimulationSnapshot {
    pub stamp: Option<SpatialSnapshotStamp>,
    pub payload: SpatialPayload,
    pub hierarchy_publication: Option<nara_hierarchy::__private::HierarchyPublication>,
}

#[derive(Default, Resource)]
pub(crate) struct PresentationSnapshot {
    pub stamp: Option<SpatialSnapshotStamp>,
    pub source_publication: u64,
    /// False uses the exact simulation payload; true owns interpolated affine values.
    pub interpolated: bool,
    pub globals: EntityHashMap<GlobalTransform2d>,
}

/// A borrowed immutable publication. It deliberately implements neither Clone nor Resource.
pub struct SpatialSnapshot2d<'w> {
    stamp: SpatialSnapshotStamp,
    payload: &'w SpatialPayload,
    globals: &'w EntityHashMap<GlobalTransform2d>,
}

impl SpatialSnapshot2d<'_> {
    pub fn global(&self, entity: Entity) -> Result<GlobalTransform2d, SpatialSnapshotError> {
        self.globals
            .get(&entity)
            .copied()
            .ok_or(SpatialSnapshotError::MissingEntity { entity })
    }

    /// Returns the captured direct parent's frame, stopping at a non-spatial parent.
    pub fn effective_parent(
        &self,
        entity: Entity,
    ) -> Result<Option<GlobalTransform2d>, SpatialSnapshotError> {
        let source = self
            .payload
            .sources
            .get(&entity)
            .ok_or(SpatialSnapshotError::MissingEntity { entity })?;
        Ok(source
            .parent
            .and_then(|parent| self.globals.get(&parent).copied()))
    }

    /// The complete captured structural order, including non-spatial participants.
    pub fn order(&self) -> Result<&[Entity], SpatialSnapshotError> {
        Ok(&self.payload.order)
    }

    /// Returns the captured structural index without consulting the live hierarchy.
    pub fn structural_index(&self, entity: Entity) -> Result<usize, SpatialSnapshotError> {
        self.payload
            .structural_indices
            .get(&entity)
            .copied()
            .ok_or(SpatialSnapshotError::MissingStructuralEntity { entity })
    }

    pub fn stamp(&self) -> Result<SpatialSnapshotStamp, SpatialSnapshotError> {
        Ok(self.stamp)
    }
}

#[derive(Clone, Copy)]
struct ReadContext {
    world: WorldId,
    generation: Option<RuntimeGeneration>,
    tick: Option<u64>,
    frame: Option<u64>,
}

impl ReadContext {
    fn from_world(world: &World) -> Self {
        Self {
            world: world.id(),
            generation: world.get_resource::<RuntimeGeneration>().copied(),
            tick: world.get_resource::<FixedTime>().map(FixedTime::tick),
            frame: world.get_resource::<RealTime>().map(|time| time.frame),
        }
    }

    fn validate(self, stamp: SpatialSnapshotStamp) -> Result<(), SpatialSnapshotError> {
        if stamp.world != self.world {
            return Err(SpatialSnapshotError::WrongWorld);
        }
        if stamp.runtime_generation != self.generation {
            return Err(SpatialSnapshotError::WrongRuntimeGeneration);
        }
        Ok(())
    }

    fn validate_current(
        self,
        stamp: SpatialSnapshotStamp,
        phase: SpatialSnapshotPhase,
    ) -> Result<(), SpatialSnapshotError> {
        self.validate(stamp)?;
        if stamp.phase != phase {
            return Err(SpatialSnapshotError::WrongPhase {
                expected: phase,
                actual: stamp.phase,
            });
        }
        let (Some(tick), Some(frame)) = (self.tick, self.frame) else {
            return Err(SpatialSnapshotError::MissingClock);
        };
        if stamp.tick != tick || stamp.frame != frame {
            return Err(SpatialSnapshotError::WrongTime);
        }
        Ok(())
    }
}

fn simulation_read(
    context: ReadContext,
    slot: Option<&SimulationSnapshot>,
    fixed: bool,
) -> Result<SpatialSnapshot2d<'_>, SpatialSnapshotError> {
    let slot = slot.ok_or(SpatialSnapshotError::Unavailable)?;
    let stamp = slot.stamp.ok_or(SpatialSnapshotError::Unavailable)?;
    if fixed {
        context.validate_current(stamp, SpatialSnapshotPhase::Fixed)?;
    } else {
        context.validate(stamp)?;
    }
    Ok(SpatialSnapshot2d {
        stamp,
        payload: &slot.payload,
        globals: &slot.payload.globals,
    })
}

fn presentation_read<'w>(
    context: ReadContext,
    simulation: Option<&'w SimulationSnapshot>,
    presentation: Option<&'w PresentationSnapshot>,
) -> Result<SpatialSnapshot2d<'w>, SpatialSnapshotError> {
    let simulation = simulation.ok_or(SpatialSnapshotError::Unavailable)?;
    let presentation = presentation.ok_or(SpatialSnapshotError::Unavailable)?;
    let stamp = presentation
        .stamp
        .ok_or(SpatialSnapshotError::Unavailable)?;
    context.validate_current(stamp, SpatialSnapshotPhase::Presented)?;
    let source_stamp = simulation.stamp.ok_or(SpatialSnapshotError::Unavailable)?;
    context.validate(source_stamp)?;
    if source_stamp.publication != presentation.source_publication {
        return Err(SpatialSnapshotError::Unavailable);
    }
    Ok(SpatialSnapshot2d {
        stamp,
        payload: &simulation.payload,
        globals: if presentation.interpolated {
            &presentation.globals
        } else {
            &simulation.payload.globals
        },
    })
}

/// Reads the most recent simulation publication, retaining its original time and phase.
///
/// Source edits and entity retirement after publication do not change this cut. A subsequent
/// failed publication returns [`SpatialSnapshotError::Unavailable`], not the last-good cut.
pub fn historical_spatial_snapshot(
    world: &World,
) -> Result<SpatialSnapshot2d<'_>, SpatialSnapshotError> {
    simulation_read(ReadContext::from_world(world), world.get_resource(), false)
}

/// Reads this tick's completed fixed cut, including inside Finalize/Capture.
///
/// A later PostUpdate/Extract publication replaces the slot and has a different phase. Use
/// [`historical_spatial_snapshot`] when the reader intentionally accepts that later cut.
pub fn fixed_spatial_snapshot(
    world: &World,
) -> Result<SpatialSnapshot2d<'_>, SpatialSnapshotError> {
    simulation_read(ReadContext::from_world(world), world.get_resource(), true)
}

/// Reads the presentation completed for this frame.
pub fn presented_spatial_snapshot(
    world: &World,
) -> Result<SpatialSnapshot2d<'_>, SpatialSnapshotError> {
    presentation_read(
        ReadContext::from_world(world),
        world.get_resource(),
        world.get_resource(),
    )
}

// Bevy 0.20's WorldId param only fetches metadata, but lacks the read-only marker.
// Delegate its exact access registration and fetch rather than storing movable World identity.
struct SnapshotWorldId(WorldId);

// SAFETY: Every access is delegated to the upstream WorldId SystemParam unchanged.
unsafe impl SystemParam for SnapshotWorldId {
    type State = <WorldId as SystemParam>::State;
    type Item<'w, 's> = Self;

    fn init_state(world: &mut World) -> Self::State {
        <WorldId as SystemParam>::init_state(world)
    }

    fn init_access(
        state: &Self::State,
        meta: &mut nara_ecs::bevy_ecs::system::SystemMeta,
        access: &mut nara_ecs::bevy_ecs::system::SystemAccess,
        world: &mut World,
    ) {
        <WorldId as SystemParam>::init_access(state, meta, access, world);
    }

    unsafe fn get_param<'w, 's>(
        state: &'s mut Self::State,
        meta: &nara_ecs::bevy_ecs::system::SystemMeta,
        world: nara_ecs::bevy_ecs::world::unsafe_world_cell::UnsafeWorldCell<'w>,
        tick: nara_ecs::bevy_ecs::change_detection::Tick,
    ) -> Result<Self, nara_ecs::bevy_ecs::system::SystemParamValidationError> {
        // SAFETY: The caller supplies the same registered access and initialized World.
        unsafe { <WorldId as SystemParam>::get_param(state, meta, world, tick).map(Self) }
    }
}

// SAFETY: The upstream fetch only calls UnsafeWorldCell::id and does not mutate the World.
unsafe impl nara_ecs::bevy_ecs::system::ReadOnlySystemParam for SnapshotWorldId {}

#[derive(SystemParam)]
struct SnapshotContext<'w> {
    world: SnapshotWorldId,
    generation: Option<Res<'w, RuntimeGeneration>>,
    fixed: Option<Res<'w, FixedTime>>,
    time: Option<Res<'w, RealTime>>,
}

impl SnapshotContext<'_> {
    fn read(&self) -> ReadContext {
        ReadContext {
            world: self.world.0,
            generation: self.generation.as_deref().copied(),
            tick: self.fixed.as_deref().map(FixedTime::tick),
            frame: self.time.as_deref().map(|time| time.frame),
        }
    }
}

macro_rules! simulation_param {
    ($name:ident, $fixed:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(SystemParam)]
        pub struct $name<'w> {
            context: SnapshotContext<'w>,
            slot: Option<Res<'w, SimulationSnapshot>>,
        }

        impl $name<'_> {
            fn read(&self) -> Result<SpatialSnapshot2d<'_>, SpatialSnapshotError> {
                simulation_read(self.context.read(), self.slot.as_deref(), $fixed)
            }

            pub fn global(
                &self,
                entity: Entity,
            ) -> Result<GlobalTransform2d, SpatialSnapshotError> {
                self.read()?.global(entity)
            }

            pub fn effective_parent(
                &self,
                entity: Entity,
            ) -> Result<Option<GlobalTransform2d>, SpatialSnapshotError> {
                self.read()?.effective_parent(entity)
            }

            pub fn order(&self) -> Result<&[Entity], SpatialSnapshotError> {
                Ok(&self.read()?.payload.order)
            }

            pub fn stamp(&self) -> Result<SpatialSnapshotStamp, SpatialSnapshotError> {
                self.read()?.stamp()
            }

            pub fn structural_index(&self, entity: Entity) -> Result<usize, SpatialSnapshotError> {
                self.read()?.structural_index(entity)
            }
        }
    };
}

simulation_param!(
    HistoricalSpatialSnapshot2d,
    false,
    "Read-only access to the last simulation publication, including during mutation."
);
simulation_param!(
    FixedSpatialSnapshot2d,
    true,
    "Read-only access to this tick's completed fixed publication."
);

/// Read-only access to this frame's completed presentation publication.
#[derive(SystemParam)]
pub struct PresentedSpatialSnapshot2d<'w> {
    context: SnapshotContext<'w>,
    simulation: Option<Res<'w, SimulationSnapshot>>,
    presentation: Option<Res<'w, PresentationSnapshot>>,
}

impl PresentedSpatialSnapshot2d<'_> {
    fn read(&self) -> Result<SpatialSnapshot2d<'_>, SpatialSnapshotError> {
        presentation_read(
            self.context.read(),
            self.simulation.as_deref(),
            self.presentation.as_deref(),
        )
    }

    pub fn global(&self, entity: Entity) -> Result<GlobalTransform2d, SpatialSnapshotError> {
        self.read()?.global(entity)
    }

    pub fn effective_parent(
        &self,
        entity: Entity,
    ) -> Result<Option<GlobalTransform2d>, SpatialSnapshotError> {
        self.read()?.effective_parent(entity)
    }

    pub fn order(&self) -> Result<&[Entity], SpatialSnapshotError> {
        Ok(&self.read()?.payload.order)
    }

    pub fn stamp(&self) -> Result<SpatialSnapshotStamp, SpatialSnapshotError> {
        self.read()?.stamp()
    }

    pub fn structural_index(&self, entity: Entity) -> Result<usize, SpatialSnapshotError> {
        self.read()?.structural_index(entity)
    }
}
