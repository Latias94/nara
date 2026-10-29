use std::fmt;

use nara_app::PluginPlan;
use nara_project::{EffectiveRuntimeConfiguration, ProductCapabilitySet};

use super::{
    CompiledProductCapabilities, ProjectRuntimeAppPlugins, RuntimePlanError, SchemaValidationInput,
    built_in_schema_providers, configured_profile_plugins, resolve_runtime_composition,
};
use crate::project_host::ingest::{ProjectCandidateError, validate_runtime_capabilities};

/// Immutable executable composition admitted from Runtime configuration, without source lineage.
///
/// The selected recipe and compiled capability ceiling determine executable and Schema authority.
/// This plan does not prove content membership, target agreement, or permission to acquire a
/// Runtime base. Those checks belong to the content Host before materialization.
#[derive(Clone)]
pub struct RuntimeConfigurationPlan {
    pub(super) settings: EffectiveRuntimeConfiguration,
    pub(super) compiled: CompiledProductCapabilities,
    pub(super) explicit: ProductCapabilitySet,
    pub(super) normalized: ProductCapabilitySet,
    pub(super) required: ProductCapabilitySet,
    pub(super) plugin_plan: PluginPlan,
    pub(super) schema_validation: SchemaValidationInput,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(crate) runtime_codecs: crate::runtime_content::RuntimeCodecs,
}

impl fmt::Debug for RuntimeConfigurationPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeConfigurationPlan")
            .field("compiled", &self.compiled)
            .field("explicit", &self.explicit)
            .field("normalized", &self.normalized)
            .field("required", &self.required)
            .field("plugin_plan_fingerprint", &self.plugin_plan.fingerprint())
            .field("schema_validation", &self.schema_validation)
            .finish_non_exhaustive()
    }
}

impl RuntimeConfigurationPlan {
    /// Frozen Runtime codec agreement, separate from plugin and component Schema composition.
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    #[must_use]
    pub fn runtime_codecs_fingerprint(&self) -> crate::runtime_content::RuntimeCodecsFingerprint {
        self.runtime_codecs.fingerprint()
    }

    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(crate) fn runtime_codecs(&self) -> &crate::runtime_content::RuntimeCodecs {
        &self.runtime_codecs
    }

    /// Returns only the admitted values consumed by the Runtime composition.
    #[must_use]
    pub const fn settings(&self) -> &EffectiveRuntimeConfiguration {
        &self.settings
    }

    #[must_use]
    pub const fn compiled_capabilities(&self) -> CompiledProductCapabilities {
        self.compiled
    }

    #[must_use]
    pub const fn explicit_capabilities(&self) -> ProductCapabilitySet {
        self.explicit
    }

    #[must_use]
    pub const fn normalized_capabilities(&self) -> ProductCapabilitySet {
        self.normalized
    }

    /// Returns the conservative product-capability witness used to admit this plan.
    ///
    /// Every requested alternative matching a plugin's any-of clause is retained. This witness
    /// is deterministic and monotonic; it is not a minimal dependency solution.
    #[must_use]
    pub const fn required_capabilities(&self) -> ProductCapabilitySet {
        self.required
    }

    #[must_use]
    pub const fn plugin_plan(&self) -> &PluginPlan {
        &self.plugin_plan
    }

    #[must_use]
    pub const fn schema_validation(&self) -> &SchemaValidationInput {
        &self.schema_validation
    }
}

/// A capability request or executable composition rejected before Runtime creation.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeConfigurationPlanError {
    /// The shared product-capability admission rejected the requested Runtime configuration.
    Capabilities(ProjectCandidateError),
    /// Plugin or Schema resolution rejected the selected executable recipe.
    Resolution(RuntimePlanError),
}

impl fmt::Display for RuntimeConfigurationPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capabilities(error) => {
                write!(formatter, "Runtime capability admission failed: {error}")
            }
            Self::Resolution(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for RuntimeConfigurationPlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Capabilities(error) => Some(error),
            Self::Resolution(error) => Some(error),
        }
    }
}

/// Resolves Runtime configuration and one compiled product recipe without a source project.
///
/// Reuses source composition's capability admission, configured defaults, plugin closure and
/// native/data Schema selection. Capability rejection precedes plugin or Schema callbacks;
/// resolution never creates an `App` or acquires runtime authority.
pub fn resolve_runtime_configuration(
    configuration: &EffectiveRuntimeConfiguration,
    recipe: crate::ProductRecipe,
) -> Result<RuntimeConfigurationPlan, RuntimeConfigurationPlanError> {
    let capabilities = validate_runtime_capabilities(configuration)
        .map_err(RuntimeConfigurationPlanError::Capabilities)?;
    let plugins = configured_profile_plugins(
        configuration,
        crate::product::ProjectProfilePlugins::new(configuration, capabilities.normalized),
    );
    let resolved = resolve_runtime_composition(
        configuration,
        capabilities,
        ProjectRuntimeAppPlugins {
            plugins,
            recipe,
            overlay: None,
            host_policy: None,
        },
        built_in_schema_providers(),
    )
    .map_err(RuntimeConfigurationPlanError::Resolution)?;
    Ok(resolved.configuration)
}
