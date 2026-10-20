//! Explicit project-plan inspection and non-replayable Host probes.
//!
//! Raw plan editing is intentionally separated from ordinary product intents. A
//! [`RuntimeProbe`] is narrower still: it adds exactly one non-replayable definition over an
//! ordinary [`ProductRecipe`] and cannot carry schema providers or profile edits.

use nara_app::PluginDefinition;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use nara_ecs::Resource;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use nara_fs::DirectoryCapability;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use nara_gameplay::GameplayCommandSubmission;

use super::composition::{
    RuntimePluginOverlay, resolve_product_recipe_with_overlay, runtime_plugin_overlay,
};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use super::runtime::{HeadlessRun, HeadlessRunIntent};
use super::{ProjectSettingsCandidate, RuntimePlan, RuntimePlanError};
use crate::ProductRecipe;

pub use super::composition::{
    ProjectRuntimePlugins, built_in_schema_providers, project_runtime_plugins, resolve_runtime_plan,
};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use super::runtime::commit_prepared_project_scene;

#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
use super::runtime::{DesktopRun, DesktopRunIntent};
#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
use super::runtime::{
    EditorProjectOpenError, EditorProjectSession, TrustedCompiledEditorProjectIntent,
};

/// One non-replayable plugin installed over an ordinary file-backed product recipe.
///
/// This advanced Host seam is intentionally narrower than direct `App` composition. It cannot
/// edit profile-owned plugins, attach schema providers, or carry more than one probe definition.
/// Schema-owning definitions are rejected during Host composition and must use
/// `SchemaContribution` in the ordinary recipe.
#[derive(Debug)]
pub struct RuntimeProbe {
    definition: PluginDefinition,
}

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
#[must_use = "the resolved plan or rejection must be inspected"]
pub fn resolve_product_recipe_with_probe(
    candidate: &ProjectSettingsCandidate,
    recipe: ProductRecipe,
    probe: RuntimeProbe,
) -> Result<RuntimePlan, RuntimePlanError> {
    resolve_product_recipe_with_overlay(candidate, recipe, Some(probe.into_overlay()))
}

/// Creates one headless product action with a single non-replayable probe overlay.
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
#[must_use]
pub fn headless_run_with_probe<O>(
    project_root: DirectoryCapability,
    intent: HeadlessRunIntent<O>,
    probe: RuntimeProbe,
    commands: Vec<GameplayCommandSubmission>,
) -> HeadlessRun<O>
where
    O: Resource + Clone,
{
    HeadlessRun::new_with_overlay(project_root, intent, probe.into_overlay(), commands)
}

/// Creates one desktop product action with a single non-replayable probe overlay.
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
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

/// Opens one trusted in-process Editor session with a single non-replayable probe overlay.
#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
pub fn open_trusted_compiled_editor_with_probe(
    project_root: DirectoryCapability,
    intent: TrustedCompiledEditorProjectIntent,
    probe: RuntimeProbe,
) -> Result<EditorProjectSession, EditorProjectOpenError> {
    EditorProjectSession::open_trusted_compiled_with_overlay(
        project_root,
        intent,
        probe.into_overlay(),
    )
}
