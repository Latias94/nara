//! Explicit project-plan inspection and non-replayable Host probes.
//!
//! Raw plan editing is intentionally separated from ordinary product intents. A
//! With `project-source`, `RuntimeProbe` adds one non-replayable definition over an ordinary
//! `ProductRecipe`. Source probe definitions cannot carry schema providers or profile edits.

#[cfg(feature = "project-source")]
use crate::fs::DirectoryCapability;
#[cfg(feature = "project-source")]
use nara_app::PluginDefinition;
#[cfg(feature = "project-source")]
use nara_ecs::Resource;

#[cfg(feature = "project-source")]
use super::composition::{
    RuntimePluginOverlay, resolve_product_recipe_with_overlay, runtime_plugin_overlay,
};
#[cfg(feature = "project-source")]
use super::runtime::{HeadlessRun, HeadlessRunIntent};
#[cfg(feature = "project-source")]
use super::{ProjectSettingsCandidate, RuntimePlan, RuntimePlanError};
#[cfg(feature = "project-source")]
use crate::ProductRecipe;
#[cfg(feature = "runtime-window")]
pub use crate::product::window_from_settings;

pub use super::composition::built_in_schema_providers;
#[cfg(feature = "project-source")]
pub use super::composition::{
    ProjectRuntimePlugins, project_runtime_plugins, resolve_runtime_plan,
};
#[cfg(feature = "serde")]
pub use super::runtime::{
    ManagedSceneDomain, ManagedSceneDomainAttempt, ManagedSceneDomainError, ManagedSceneDomainId,
    ManagedSceneDomainPhase, ManagedSceneDomainPreparation, ManagedSceneDomains,
    begin_prepared_project_scene_travel, commit_prepared_project_scene,
};

#[cfg(all(
    feature = "project-source",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
use super::{DesktopRun, DesktopRunIntent};

/// One non-replayable plugin installed over an ordinary file-backed product recipe.
///
/// This advanced Host seam is intentionally narrower than direct `App` composition. It cannot
/// edit profile-owned plugins, attach schema providers, or carry more than one probe definition.
/// Schema-owning definitions are rejected during Host composition and must use
/// `SchemaContribution` in the ordinary recipe.
#[cfg(feature = "project-source")]
#[derive(Debug)]
pub struct RuntimeProbe {
    definition: PluginDefinition,
}

#[cfg(feature = "project-source")]
impl RuntimeProbe {
    /// Creates one recipe-anchored non-replayable probe.
    ///
    /// Schema-owning definitions are rejected during pure plan resolution before an `App` exists.
    #[must_use]
    pub fn new(definition: PluginDefinition) -> Self {
        Self { definition }
    }

    pub(crate) fn into_overlay(self) -> RuntimePluginOverlay {
        runtime_plugin_overlay(self.definition)
    }
}

/// Resolves one ordinary recipe plus one non-replayable probe before creating an `App`.
#[cfg(feature = "project-source")]
#[must_use = "the resolved plan or rejection must be inspected"]
pub fn resolve_product_recipe_with_probe(
    candidate: &ProjectSettingsCandidate,
    recipe: ProductRecipe,
    probe: RuntimeProbe,
) -> Result<RuntimePlan, RuntimePlanError> {
    resolve_product_recipe_with_overlay(candidate, recipe, Some(probe.into_overlay()))
}

/// Creates one headless product action with a single non-replayable probe overlay.
#[cfg(feature = "project-source")]
#[must_use]
pub fn headless_run_with_probe<O>(
    project_root: DirectoryCapability,
    intent: HeadlessRunIntent<O>,
    probe: RuntimeProbe,
) -> HeadlessRun<O>
where
    O: Resource + Clone,
{
    HeadlessRun::new_with_overlay(project_root, intent, probe.into_overlay())
}

/// Creates one desktop product action with a single non-replayable probe overlay.
#[cfg(all(
    feature = "project-source",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[must_use]
pub fn desktop_run_with_probe(
    project_root: DirectoryCapability,
    intent: DesktopRunIntent,
    probe: RuntimeProbe,
) -> DesktopRun {
    DesktopRun::new_with_overlay(project_root, intent, probe.into_overlay())
}
