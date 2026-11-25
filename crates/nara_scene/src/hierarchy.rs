//! Runtime-only structural hierarchy for Nara.
//!
//! The ordered root registry and failure-atomic placement writer use Bevy's native hierarchy.
//! The writer detaches linked relationships before scene-owned exact retirement.

mod error;
mod relation;
mod validation;
mod writer;

pub use error::HierarchyError;
pub use nara_ecs::bevy_ecs::hierarchy::ChildOf as Parent;
pub use nara_ecs::bevy_ecs::hierarchy::{ChildOf, Children};
pub use relation::{HierarchyOrder, HierarchyRoots};
pub use validation::validate_hierarchy;
pub use writer::{
    HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement, HierarchyPlacementAnchor,
    HierarchyPlacementOwner, HierarchyWriter,
};

use nara_app::{
    App, CoreStage, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
    PluginId, StartupStage,
};
use nara_ecs::{
    Component, Mut, Resource, SystemSet, World,
    bevy_ecs::{
        change_detection::Tick,
        error::BevyError,
        lifecycle::{HookContext, RemovedComponentEntity},
        message::{MessageCursor, Messages},
        world::{DeferredWorld, WorldId},
    },
    schedule::IntoScheduleConfigs,
};
use validation::{
    HierarchyValidationScratch, structural_entities, validate_hierarchy_with_scratch,
};

pub(crate) use writer::{
    PreparedHierarchyBatch, hierarchy_retirement_operations as retirement_operations,
    prepare_hierarchy_batch as prepare_construction_batch, validate_fresh_construction_topology,
    validate_hierarchy_retirement_despawn as validate_retirement_despawn,
    validate_hierarchy_retirement_despawn_with_claimed_components as validate_retirement_despawn_with_claimed_components,
};

/// Flushes deferred hierarchy commands before checking a scene transaction's live baseline.
#[doc(hidden)]
pub fn prepare_mutation_baseline(world: &mut World) {
    writer::prepare_mutation_baseline(world);
}

/// Capability token proving that the current runtime hierarchy generation completed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub(crate) struct CompletedHierarchyProjection {
    generation: u64,
    publication: Option<HierarchyPublication>,
}

/// Owner-created identity for one exact validated order, including its source World.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HierarchyPublication {
    world: WorldId,
    sequence: u64,
}

impl CompletedHierarchyProjection {
    const fn new(generation: u64, publication: HierarchyPublication) -> Self {
        Self {
            generation,
            publication: Some(publication),
        }
    }

    /// Returns the validated runtime hierarchy generation.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Internal schedule boundary after a dirty runtime hierarchy generation has been validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum HierarchySet {
    ValidateAndComplete,
}

/// Runs the original hierarchy validator at the transform owner's fixed-input boundary.
pub fn complete_fixed_input_hierarchy(
    world: &mut World,
) -> Result<(), nara_ecs::bevy_ecs::error::BevyError> {
    validate_dirty_hierarchy(world)
}

/// Returns the completed topology generation for the current World.
///
/// This is a provisional cross-domain completion fact for first-party projection modules. It
/// is deliberately outside the ordinary hierarchy API.
#[must_use]
pub fn completed_topology_generation(world: &World) -> Option<u64> {
    completed_topology_publication(world)?;
    Some(
        world
            .get_resource::<CompletedHierarchyProjection>()?
            .generation(),
    )
}

/// Returns an exact owner publication after validating all of its associated projections.
#[must_use]
pub(crate) fn completed_topology_publication(world: &World) -> Option<HierarchyPublication> {
    let completed = *world.get_resource::<CompletedHierarchyProjection>()?;
    let publication = completed.publication?;
    let state = world.get_resource::<HierarchyGenerationState>()?;
    let order = world.get_resource::<HierarchyOrder>()?;
    let roots = world.get_resource::<HierarchyRoots>()?;
    (publication.world == world.id()
        && state.baseline_world == Some(world.id())
        && state.completed_publication == Some(publication)
        && order.publication() == Some(publication)
        && state.completed_generation == Some(completed.generation())
        && state.topology_generation == completed.generation()
        && state.baseline_root_epoch == Some(roots.epoch())
        && order.root_epoch() == Some(roots.epoch())
        && !state.native_changes.has_changed(world))
    .then_some(())
    .filter(|_| order.generation() == completed.generation())
    .filter(|_| order.roots() == roots.as_slice())
    .map(|()| publication)
}

#[derive(Debug, Resource)]
struct HierarchyGenerationState {
    roots_required: bool,
    topology_generation: u64,
    completed_generation: Option<u64>,
    baseline_valid: bool,
    baseline_root_epoch: Option<u64>,
    baseline_world: Option<WorldId>,
    native_changes: NativeHierarchyChanges,
    completed_publication: Option<HierarchyPublication>,
    #[cfg(test)]
    validation_scans: u64,
}

impl Default for HierarchyGenerationState {
    fn default() -> Self {
        Self {
            roots_required: false,
            topology_generation: 0,
            completed_generation: None,
            baseline_valid: false,
            baseline_root_epoch: None,
            baseline_world: None,
            native_changes: NativeHierarchyChanges::default(),
            completed_publication: None,
            #[cfg(test)]
            validation_scans: 0,
        }
    }
}

#[derive(Debug, Default, Resource)]
struct RetainedHierarchyValidationScratch(HierarchyValidationScratch);

#[derive(Debug, Default)]
struct NativeHierarchyChanges {
    since: Tick,
    parents: MessageCursor<RemovedComponentEntity>,
    children: MessageCursor<RemovedComponentEntity>,
}

impl NativeHierarchyChanges {
    fn capture(world: &mut World) -> Self {
        Self {
            // Separate later exclusive writes even when no schedule runs between them.
            since: world.increment_change_tick(),
            parents: relationship_removals::<ChildOf>(world)
                .map_or_else(MessageCursor::default, Messages::get_cursor_current),
            children: relationship_removals::<Children>(world)
                .map_or_else(MessageCursor::default, Messages::get_cursor_current),
        }
    }

    fn has_changed(&self, world: &World) -> bool {
        self.component_changed::<ChildOf>(world, &self.parents)
            || self.component_changed::<Children>(world, &self.children)
    }

    fn component_changed<T: Component>(
        &self,
        world: &World,
        removals: &MessageCursor<RemovedComponentEntity>,
    ) -> bool {
        if relationship_removals::<T>(world).is_some_and(|messages| {
            !removals.is_empty(messages) || removals.missed_messages(messages) != 0
        }) {
            return true;
        }
        let now = world.read_change_tick();
        structural_entities::<T>(world).any(|entity| {
            world
                .get_entity(entity)
                .ok()
                .and_then(|entity| entity.get_change_ticks::<T>())
                .is_some_and(|ticks| ticks.is_changed(self.since, now))
        })
    }
}

fn relationship_removals<T: Component>(world: &World) -> Option<&Messages<RemovedComponentEntity>> {
    world.removed_components().get(world.component_id::<T>()?)
}

impl HierarchyGenerationState {
    fn mark_dirty(&mut self) {
        self.topology_generation = self.topology_generation.saturating_add(1);
        self.completed_generation = None;
        self.completed_publication = None;
        self.baseline_valid = false;
    }

    fn needs_validation(&self) -> bool {
        self.completed_generation != Some(self.topology_generation)
    }
}

pub(crate) fn hierarchy_baseline_is_valid(world: &World) -> bool {
    let Some(state) = world.get_resource::<HierarchyGenerationState>() else {
        return false;
    };
    state.baseline_valid
        && state.baseline_world == Some(world.id())
        && world
            .get_resource::<HierarchyRoots>()
            .is_some_and(|roots| Some(roots.epoch()) == state.baseline_root_epoch)
        && !state.native_changes.has_changed(world)
}

pub(crate) fn mark_hierarchy_baseline_valid(world: &mut World) {
    let world_id = world.id();
    let root_epoch = world
        .get_resource::<HierarchyRoots>()
        .map(HierarchyRoots::epoch);
    let native_changes = NativeHierarchyChanges::capture(world);
    let mut state = world.get_resource_or_init::<HierarchyGenerationState>();
    state.baseline_valid = true;
    state.baseline_root_epoch = root_epoch;
    state.baseline_world = Some(world_id);
    state.native_changes = native_changes;
}

pub const HIERARCHY_PLUGIN_ID: PluginId = PluginId::new("nara.hierarchy");
pub const HIERARCHY_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(HIERARCHY_PLUGIN_ID, PluginCategory::Core);

/// Installs runtime hierarchy validation and its semantic completion boundary.
#[derive(Debug, Default, Clone, Copy)]
pub struct HierarchyPlugin;

impl Plugin for HierarchyPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &HIERARCHY_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if !app.world().contains_resource::<HierarchyRoots>() {
            app.insert_resource(HierarchyRoots::new())?;
        }
        app.init_resource::<HierarchyOrder>()?;
        app.world_mut()?
            .get_resource_or_init::<HierarchyGenerationState>()
            .roots_required = true;
        app.init_resource::<CompletedHierarchyProjection>()?;
        app.init_resource::<RetainedHierarchyValidationScratch>()?;
        app.add_systems(
            StartupStage::Tooling,
            complete_scheduled_hierarchy.in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.configure_sets(
            CoreStage::FixedUpdate,
            HierarchySet::ValidateAndComplete
                .after(FixedUpdateSet::Simulate)
                .before(FixedUpdateSet::Finalize),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            complete_scheduled_hierarchy.in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.add_systems(
            CoreStage::PostUpdate,
            complete_scheduled_hierarchy.in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.add_systems(
            CoreStage::Extract,
            complete_scheduled_hierarchy.in_set(HierarchySet::ValidateAndComplete),
        )?;
        Ok(())
    }
}

fn mark_topology_dirty(world: &mut World) {
    world
        .get_resource_or_init::<HierarchyGenerationState>()
        .mark_dirty();
    world.remove_resource::<CompletedHierarchyProjection>();
}

fn hierarchy_component_changed(mut world: DeferredWorld<'_>, _context: HookContext) {
    if let Some(mut state) = world.get_resource_mut::<HierarchyGenerationState>() {
        state.mark_dirty();
    }
}

fn complete_scheduled_hierarchy(world: &mut World) -> Result<(), BevyError> {
    if let Err(error) = validate_dirty_hierarchy(world) {
        crate::fixed_spatial::invalidate_spatial_snapshots(world);
        return Err(error);
    }
    Ok(())
}

fn validate_dirty_hierarchy(world: &mut World) -> Result<(), BevyError> {
    let Some(root_epoch) = world
        .get_resource::<HierarchyRoots>()
        .map(HierarchyRoots::epoch)
    else {
        mark_topology_dirty(world);
        return Err(HierarchyError::MissingRoots.into_execution_error());
    };

    world.get_resource_or_init::<HierarchyGenerationState>();
    let order_epoch = world
        .get_resource::<HierarchyOrder>()
        .and_then(HierarchyOrder::root_epoch);
    let publication_valid = completed_topology_publication(world).is_some();
    let world_id = world.id();
    {
        let mut state = world.resource_mut::<HierarchyGenerationState>();
        if state.baseline_valid
            && (state.baseline_root_epoch != Some(root_epoch)
                || state.baseline_world != Some(world_id))
        {
            state.mark_dirty();
        } else if state.completed_generation == Some(state.topology_generation)
            && (order_epoch != Some(root_epoch) || !publication_valid)
        {
            state.mark_dirty();
        }
    }

    let generation = {
        let state = world.resource::<HierarchyGenerationState>();
        if !state.needs_validation() {
            return Ok(());
        }
        state.topology_generation
    };
    // A failed validation must not leave the previous completion fact observable.
    world.remove_resource::<CompletedHierarchyProjection>();

    #[cfg(test)]
    {
        world
            .resource_mut::<HierarchyGenerationState>()
            .validation_scans += 1;
    }

    world.init_resource::<RetainedHierarchyValidationScratch>();
    let validation = world.resource_scope(
        |world, mut scratch: Mut<RetainedHierarchyValidationScratch>| {
            validate_hierarchy_with_scratch(world, &mut scratch.0)
        },
    );
    validation.map_err(BevyError::error)?;
    world.init_resource::<HierarchyOrder>();
    let roots = world
        .get_resource::<HierarchyRoots>()
        .ok_or_else(|| HierarchyError::MissingRoots.into_execution_error())?;
    let root_epoch = roots.epoch();
    let roots = roots.as_slice().to_vec();
    static NEXT_PUBLICATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let publication = HierarchyPublication {
        world: world_id,
        sequence: NEXT_PUBLICATION
            .try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |sequence| sequence.checked_add(1),
            )
            .expect("hierarchy publication identity space exhausted"),
    };
    let mut entities = Vec::new();
    let mut pending = roots.iter().rev().copied().collect::<Vec<_>>();
    while let Some(entity) = pending.pop() {
        entities.push(entity);
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter().rev().copied());
        }
    }
    world.resource_scope(|_world, mut order: Mut<HierarchyOrder>| {
        order.replace(generation, Some(root_epoch), roots, entities, publication);
    });
    {
        let mut state = world.resource_mut::<HierarchyGenerationState>();
        state.completed_generation = Some(generation);
        state.completed_publication = Some(publication);
    }
    mark_hierarchy_baseline_valid(world);
    world.insert_resource(CompletedHierarchyProjection::new(generation, publication));
    Ok(())
}

pub mod prelude {
    pub use crate::hierarchy::{
        ChildOf, Children, HierarchyCommandsExt, HierarchyOperation, HierarchyOrder,
        HierarchyPlacement, HierarchyPlacementAnchor, HierarchyPlacementOwner, HierarchyPlugin,
        HierarchyRoots, HierarchyWriter, Parent,
    };
}

#[cfg(test)]
mod tests;
