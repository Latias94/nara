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

use nara_app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_ecs::{
    Mut, RemovedComponents, ResMut, Resource, World, error::BevyError,
    schedule::IntoScheduleConfigs,
};
use validation::{HierarchyValidationScratch, validate_hierarchy_with_scratch};

#[doc(hidden)]
pub mod __private {
    use nara_ecs::SystemSet;

    /// Internal schedule boundary after a dirty runtime hierarchy generation has been validated.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
    pub enum HierarchySet {
        ValidateAndComplete,
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
        app.init_resource::<RetainedHierarchyValidationScratch>()?;
        app.add_systems(
            CoreStage::PostUpdate,
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
}

fn detect_removed_hierarchy_edges(
    mut removed_parents: RemovedComponents<Parent>,
    mut removed_children: RemovedComponents<Children>,
    mut state: ResMut<HierarchyGenerationState>,
) {
    let removed_parent = removed_parents.read().count() != 0;
    let removed_children = removed_children.read().count() != 0;
    if removed_parent || removed_children {
        state.mark_dirty();
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
