//! Runtime-only structural hierarchy for Nara.
//!
//! The module owns one non-linked parent relationship, its ordered root and child projections,
//! bounded validation, and failure-atomic placement writers. It does not own persistence,
//! transform math, visibility, UI layout, prefab provenance, or entity lifetime.

mod error;
mod relation;
mod validation;
mod writer;

pub use error::HierarchyError;
pub use relation::{Children, HierarchyOrder, HierarchyRoots, Parent};
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
    Mut, Resource, World, error::BevyError, lifecycle::HookContext, schedule::IntoScheduleConfigs,
    world::DeferredWorld,
};
use validation::{HierarchyValidationScratch, validate_hierarchy_with_scratch};

#[doc(hidden)]
pub mod __private {
    use nara_ecs::{Resource, SystemSet, World};

    use super::{
        HierarchyError, HierarchyGenerationState, HierarchyOperation, HierarchyOrder,
        HierarchyRoots,
        writer::{
            hierarchy_retirement_operations, prepare_hierarchy_batch,
            validate_hierarchy_retirement_despawn,
            validate_hierarchy_retirement_despawn_with_claimed_components,
        },
    };

    pub use super::writer::PreparedHierarchyBatch;

    /// Materializes deferred work before an enclosing owner takes any semantic World proofs.
    pub fn prepare_mutation_baseline(world: &mut World) {
        super::writer::prepare_mutation_baseline(world);
    }

    /// Capability token proving that the current runtime hierarchy generation completed.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
    pub struct CompletedHierarchyProjection {
        generation: u64,
    }

    impl CompletedHierarchyProjection {
        pub(super) const fn new(generation: u64) -> Self {
            Self { generation }
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

    /// Returns the completed topology generation for the current World.
    ///
    /// This is a provisional cross-domain completion fact for first-party projection modules. It
    /// is deliberately outside the ordinary hierarchy API.
    #[must_use]
    pub fn completed_topology_generation(world: &World) -> Option<u64> {
        let completed = *world.get_resource::<CompletedHierarchyProjection>()?;
        let state = world.get_resource::<HierarchyGenerationState>()?;
        let order = world.get_resource::<HierarchyOrder>()?;
        let roots = world.get_resource::<HierarchyRoots>()?;
        (state.completed_generation == Some(completed.generation())
            && state.topology_generation == completed.generation()
            && state.baseline_root_epoch == Some(roots.epoch())
            && order.root_epoch() == Some(roots.epoch()))
        .then_some(())
        .filter(|_| order.generation() == completed.generation())
        .filter(|_| order.roots() == roots.as_slice())
        .map(|()| completed.generation())
    }

    /// Prepares construction edges against the enclosing owner's already materialized baseline.
    ///
    /// This does not execute deferred work; preparation and commit require exclusive World access.
    pub fn prepare_construction_batch(
        world: &mut World,
        operations: &[HierarchyOperation],
    ) -> Result<PreparedHierarchyBatch, HierarchyError> {
        prepare_hierarchy_batch(world, operations)
    }

    /// Derives the ordered transaction that removes an exact runtime entity set.
    pub fn retirement_operations(
        world: &World,
        entities: &[nara_ecs::Entity],
    ) -> Result<Vec<HierarchyOperation>, HierarchyError> {
        hierarchy_retirement_operations(world, entities)
    }

    /// Validates exact entity retirement while allowing only hierarchy-owned component hooks.
    pub fn validate_retirement_despawn(
        world: &World,
        entities: &[nara_ecs::Entity],
        affected_entities: &[nara_ecs::Entity],
    ) -> Result<(), nara_ecs::LifecycleFreeDespawnError> {
        validate_hierarchy_retirement_despawn(world, entities, affected_entities)
    }

    /// Validates exact entity retirement while admitting component lifecycle effects explicitly
    /// claimed by a managed native domain. The claim is only valid until the final strict check.
    pub fn validate_retirement_despawn_with_claimed_components(
        world: &World,
        entities: &[nara_ecs::Entity],
        affected_entities: &[nara_ecs::Entity],
        claimed_component_ids: &[nara_ecs::component::ComponentId],
    ) -> Result<(), nara_ecs::LifecycleFreeDespawnError> {
        validate_hierarchy_retirement_despawn_with_claimed_components(
            world,
            entities,
            affected_entities,
            claimed_component_ids,
        )
    }
}

use __private::HierarchySet;

#[derive(Debug, Resource)]
struct HierarchyGenerationState {
    topology_generation: u64,
    completed_generation: Option<u64>,
    baseline_valid: bool,
    baseline_root_epoch: Option<u64>,
    #[cfg(test)]
    validation_scans: u64,
}

impl Default for HierarchyGenerationState {
    fn default() -> Self {
        Self {
            topology_generation: 0,
            completed_generation: None,
            baseline_valid: false,
            baseline_root_epoch: None,
            #[cfg(test)]
            validation_scans: 0,
        }
    }
}

#[derive(Debug, Default, Resource)]
struct RetainedHierarchyValidationScratch(HierarchyValidationScratch);

impl HierarchyGenerationState {
    fn mark_dirty(&mut self) {
        self.topology_generation = self.topology_generation.saturating_add(1);
        self.completed_generation = None;
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
        && world
            .get_resource::<HierarchyRoots>()
            .is_some_and(|roots| Some(roots.epoch()) == state.baseline_root_epoch)
}

pub(crate) fn mark_hierarchy_baseline_valid(world: &mut World) {
    let root_epoch = world
        .get_resource::<HierarchyRoots>()
        .map(HierarchyRoots::epoch);
    let mut state = world.get_resource_or_init::<HierarchyGenerationState>();
    state.baseline_valid = true;
    state.baseline_root_epoch = root_epoch;
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
        app.init_resource::<HierarchyGenerationState>()?;
        app.init_resource::<__private::CompletedHierarchyProjection>()?;
        app.init_resource::<RetainedHierarchyValidationScratch>()?;
        app.add_systems(
            StartupStage::Tooling,
            validate_dirty_hierarchy.in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.configure_sets(
            CoreStage::FixedUpdate,
            HierarchySet::ValidateAndComplete
                .after(FixedUpdateSet::Simulate)
                .before(FixedUpdateSet::Finalize),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            validate_dirty_hierarchy.in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.add_systems(
            CoreStage::PostUpdate,
            validate_dirty_hierarchy.in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.add_systems(
            CoreStage::Extract,
            validate_dirty_hierarchy.in_set(HierarchySet::ValidateAndComplete),
        )?;
        Ok(())
    }
}

fn mark_topology_dirty(world: &mut World) {
    world
        .get_resource_or_init::<HierarchyGenerationState>()
        .mark_dirty();
    world.remove_resource::<__private::CompletedHierarchyProjection>();
}

fn hierarchy_component_changed(mut world: DeferredWorld<'_>, _context: HookContext) {
    if let Some(mut state) = world.get_resource_mut::<HierarchyGenerationState>() {
        state.mark_dirty();
    }
}

fn validate_dirty_hierarchy(world: &mut World) -> Result<(), BevyError> {
    world.get_resource_or_init::<HierarchyGenerationState>();

    let root_epoch = world
        .get_resource::<HierarchyRoots>()
        .map(HierarchyRoots::epoch);
    let order_epoch = world
        .get_resource::<HierarchyOrder>()
        .and_then(HierarchyOrder::root_epoch);
    {
        let mut state = world.resource_mut::<HierarchyGenerationState>();
        if state.baseline_valid && state.baseline_root_epoch != root_epoch {
            state.mark_dirty();
        } else if state.completed_generation == Some(state.topology_generation)
            && order_epoch != root_epoch
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
    let root_epoch = world.resource::<HierarchyRoots>().epoch();
    let roots = world.resource::<HierarchyRoots>().as_slice().to_vec();
    let mut entities = Vec::new();
    let mut pending = roots.iter().rev().copied().collect::<Vec<_>>();
    while let Some(entity) = pending.pop() {
        entities.push(entity);
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter().rev());
        }
    }
    world.resource_scope(|_world, mut order: Mut<HierarchyOrder>| {
        order.replace(generation, Some(root_epoch), roots, entities);
    });
    world
        .resource_mut::<HierarchyGenerationState>()
        .completed_generation = Some(generation);
    mark_hierarchy_baseline_valid(world);
    world.insert_resource(__private::CompletedHierarchyProjection::new(generation));
    Ok(())
}

pub mod prelude {
    pub use crate::{
        Children, HierarchyCommandsExt, HierarchyOperation, HierarchyOrder, HierarchyPlacement,
        HierarchyPlacementAnchor, HierarchyPlacementOwner, HierarchyPlugin, HierarchyRoots,
        HierarchyWriter, Parent,
    };
}

#[cfg(test)]
mod tests;
