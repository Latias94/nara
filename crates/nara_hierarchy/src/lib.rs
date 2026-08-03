//! Runtime-only structural hierarchy for Nara.
//!
//! The module owns one non-linked parent relationship, its derived reverse projection, bounded
//! validation, and construction-only writers. It does not own persistence, transform math,
//! visibility, UI layout, prefab provenance, or entity lifetime.

mod error;
mod relation;
mod validation;
mod writer;

pub use error::HierarchyError;
pub use relation::{Children, Parent};
pub use validation::validate_hierarchy;
pub use writer::{HierarchyCommandsExt, HierarchyConstructionEdge, HierarchyConstructionWriter};

use nara_app::{
    App, CoreStage, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
    PluginId, StartupStage,
};
use nara_ecs::{
    Mut, Resource, World, error::BevyError, lifecycle::RemovedComponentReader,
    schedule::IntoScheduleConfigs,
};
use validation::{HierarchyValidationScratch, validate_hierarchy_with_scratch};

#[doc(hidden)]
pub mod __private {
    use nara_ecs::{Resource, SystemSet, World};

    use super::{
        HierarchyConstructionEdge, HierarchyError, HierarchyGenerationState,
        writer::prepare_hierarchy_construction_batch,
    };

    pub use super::writer::PreparedHierarchyConstructionBatch;

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
        (state.completed_generation == Some(completed.generation())
            && state.topology_generation == completed.generation())
        .then_some(completed.generation())
    }

    /// Prepares candidate construction edges without publishing a hierarchy generation.
    pub fn prepare_construction_batch(
        world: &World,
        edges: &[HierarchyConstructionEdge],
    ) -> Result<PreparedHierarchyConstructionBatch, HierarchyError> {
        prepare_hierarchy_construction_batch(world, edges)
    }
}

use __private::HierarchySet;

#[derive(Debug, Resource)]
struct HierarchyGenerationState {
    topology_generation: u64,
    completed_generation: Option<u64>,
    #[cfg(test)]
    validation_scans: u64,
}

impl Default for HierarchyGenerationState {
    fn default() -> Self {
        Self {
            topology_generation: 0,
            completed_generation: Some(0),
            #[cfg(test)]
            validation_scans: 0,
        }
    }
}

#[derive(Debug, Default, Resource)]
struct RetainedHierarchyValidationScratch(HierarchyValidationScratch);

#[derive(Debug, Default, Resource)]
struct HierarchyRemovalReaders {
    parents: RemovedComponentReader<Parent>,
    children: RemovedComponentReader<Children>,
}

impl HierarchyGenerationState {
    fn mark_dirty(&mut self) {
        self.topology_generation = self.topology_generation.saturating_add(1);
        self.completed_generation = None;
    }

    fn needs_validation(&self) -> bool {
        self.completed_generation != Some(self.topology_generation)
    }
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
        app.init_resource::<HierarchyGenerationState>()?;
        app.init_resource::<__private::CompletedHierarchyProjection>()?;
        app.init_resource::<RetainedHierarchyValidationScratch>()?;
        app.init_resource::<HierarchyRemovalReaders>()?;
        app.add_systems(
            StartupStage::Tooling,
            (detect_removed_hierarchy_edges, validate_dirty_hierarchy)
                .chain()
                .in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.configure_sets(
            CoreStage::FixedUpdate,
            HierarchySet::ValidateAndComplete
                .after(FixedUpdateSet::Simulate)
                .before(FixedUpdateSet::Finalize),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            (detect_removed_hierarchy_edges, validate_dirty_hierarchy)
                .chain()
                .in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.add_systems(
            CoreStage::PostUpdate,
            (detect_removed_hierarchy_edges, validate_dirty_hierarchy)
                .chain()
                .in_set(HierarchySet::ValidateAndComplete),
        )?;
        app.add_systems(
            CoreStage::Extract,
            (detect_removed_hierarchy_edges, validate_dirty_hierarchy)
                .chain()
                .in_set(HierarchySet::ValidateAndComplete),
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

fn detect_removed_hierarchy_edges(world: &mut World) {
    let parent_component = world.component_id::<Parent>();
    let children_component = world.component_id::<Children>();
    let removed = world.resource_scope(|world, mut readers: Mut<HierarchyRemovalReaders>| {
        let messages = world.removed_components();
        let removed_parent = parent_component
            .and_then(|component| messages.get(component))
            .is_some_and(|messages| readers.parents.read(messages).count() != 0);
        let removed_children = children_component
            .and_then(|component| messages.get(component))
            .is_some_and(|messages| readers.children.read(messages).count() != 0);
        removed_parent || removed_children
    });

    if removed {
        mark_topology_dirty(world);
    }
}

fn validate_dirty_hierarchy(world: &mut World) -> Result<(), BevyError> {
    world.get_resource_or_init::<HierarchyGenerationState>();

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
    world
        .resource_mut::<HierarchyGenerationState>()
        .completed_generation = Some(generation);
    world.insert_resource(__private::CompletedHierarchyProjection::new(generation));
    Ok(())
}

pub mod prelude {
    pub use crate::{
        Children, HierarchyCommandsExt, HierarchyConstructionEdge, HierarchyConstructionWriter,
        HierarchyPlugin, Parent,
    };
}

#[cfg(test)]
mod tests;
