mod composition;
mod ingest;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
mod runtime;

#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
mod persistence;

#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
pub use persistence::EditorPersistenceReceipt;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use crate::project_content::{
    ProjectContentBudgetError, ProjectContentBudgetHost, ProjectContentBudgetKind,
    ProjectContentBudgetSnapshot, ProjectContentError, ProjectContentErrorKind,
    ProjectContentLimits, ProjectContentLoader, ProjectContentRevision, ProjectContentSnapshot,
    ProjectImageContent, ProjectPrefabContent,
};
pub use composition::{
    CompiledProductCapabilities, CompositionError, ProjectRuntimePlugins, ProjectSettingsCandidate,
    ProjectSettingsLineage, RuntimePlan, RuntimePlanError, SchemaValidationInput,
    built_in_schema_providers, compiled_product_capabilities, project_runtime_plugins,
    project_runtime_plugins_with_recipe, resolve_product_recipe, resolve_runtime_plan,
};
pub use ingest::{ProjectCandidateError, ProjectCandidateErrorKind, ingest_project_manifest};
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub use runtime::{DesktopRun, DesktopRunIntent, DesktopRunOutcome, DesktopRunReport};
#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
pub use runtime::{EditorProjectIntent, EditorProjectOpenError, EditorProjectSession};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use runtime::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport};
