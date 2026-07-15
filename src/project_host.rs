mod composition;
mod ingest;

pub use composition::{
    CompiledProductCapabilities, CompositionError, ProjectRuntimePlugins, ProjectSettingsCandidate,
    ProjectSettingsLineage, RuntimePlan, RuntimePlanError, SchemaValidationInput,
    built_in_schema_providers, compiled_product_capabilities, project_runtime_plugins,
    resolve_runtime_plan,
};
pub use ingest::{ProjectCandidateError, ProjectCandidateErrorKind, ingest_project_manifest};
