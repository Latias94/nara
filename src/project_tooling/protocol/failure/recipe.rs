use std::any::Any;

use nara_app::PluginId;
use nara_core::diagnostic::PublicDiagnosticIdentifier;

use super::{ProjectToolFailure, ProjectToolFailureStage};
use crate::ProductRecipeError;

/// A bounded projection that never retains the original error or unadmitted identity.
///
/// Omitted fields count top-level fields of known recipe variants, including a rejected plugin
/// identifier. Nested errors and vectors each count as one unexpanded field. Opaque errors have
/// no inspected fields and retain only the generic classification.
#[derive(Debug)]
pub(in crate::project_tooling) struct RecipeFailure {
    code: &'static str,
    summary: &'static str,
    plugin: Option<PublicDiagnosticIdentifier>,
    omitted_fields: u64,
}

impl RecipeFailure {
    pub(in crate::project_tooling) fn from_error<E: 'static>(error: E) -> Self {
        let Some(error) = (&error as &dyn Any).downcast_ref::<ProductRecipeError>() else {
            return Self {
                code: "project.tool.recipe-failed",
                summary: "Project recipe construction failed",
                plugin: None,
                omitted_fields: 0,
            };
        };
        Self::from_recipe(error)
    }

    fn from_recipe(error: &ProductRecipeError) -> Self {
        use ProductRecipeError as Error;

        let (code, summary, plugin, omitted_fields): (_, _, Option<PluginId>, u64) = match error {
            #[cfg(feature = "render-wgpu")]
            Error::WgpuPolicyRequiresStockCapture { plugin } => (
                "project.tool.recipe-wgpu-policy-requires-stock-capture",
                "The selected WGPU policy plugin must require the stock capture plugin",
                Some(*plugin),
                0,
            ),
            #[cfg(feature = "render-wgpu")]
            Error::SelectedWgpuPolicyConfiguration { plugin } => (
                "project.tool.recipe-selected-wgpu-policy-configuration",
                "The selected WGPU policy plugin must keep its paired capture adapter configuration",
                Some(*plugin),
                0,
            ),
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            Error::RuntimeCodec(_) => (
                "project.tool.recipe-runtime-codec-rejected",
                "The product recipe runtime codec selection was rejected",
                None,
                1,
            ),
            Error::ReservedAssetDomain { .. } => (
                "project.tool.recipe-reserved-asset-domain",
                "The product recipe asset domain identity is empty or reserved",
                None,
                1,
            ),
            Error::DuplicateAssetDomain { .. } => (
                "project.tool.recipe-duplicate-asset-domain",
                "An asset domain appears more than once in the product recipe",
                None,
                1,
            ),
            Error::DuplicateDefaultConfiguration { plugin } => (
                "project.tool.recipe-duplicate-default-configuration",
                "A default plugin is configured more than once in the product recipe",
                Some(*plugin),
                0,
            ),
            Error::DeclarationPanicked { .. } => (
                "project.tool.recipe-declaration-panicked",
                "A plugin declaration panicked during product recipe construction",
                None,
                1,
            ),
            Error::MissingConfigurationIdentity { plugin } => (
                "project.tool.recipe-missing-configuration-identity",
                "A plugin did not produce a stable product recipe configuration identity",
                Some(*plugin),
                0,
            ),
            Error::DuplicatePlugin { plugin } => (
                "project.tool.recipe-duplicate-plugin",
                "A plugin appears more than once in the product recipe",
                Some(*plugin),
                0,
            ),
            Error::PluginNotPresent { plugin } => (
                "project.tool.recipe-plugin-not-present",
                "A plugin to configure is not present in the product recipe",
                Some(*plugin),
                0,
            ),
            Error::PluginTypeMismatch { plugin } => (
                "project.tool.recipe-plugin-type-mismatch",
                "A product recipe plugin is already bound to another Rust plugin type",
                Some(*plugin),
                0,
            ),
            Error::EntryKindMismatch { plugin, .. } => (
                "project.tool.recipe-entry-kind-mismatch",
                "A plugin cannot change its product recipe entry kind",
                Some(*plugin),
                2,
            ),
            Error::RuntimePluginProvidesSchema { plugin } => (
                "project.tool.recipe-runtime-plugin-provides-schema",
                "A runtime-only plugin declares schema providers and requires a contribution",
                Some(*plugin),
                0,
            ),
            Error::SchemaContributionWithoutSchema { plugin } => (
                "project.tool.recipe-schema-contribution-without-schema",
                "A schema contribution supplies neither native providers nor data owners",
                Some(*plugin),
                0,
            ),
            Error::DuplicateDataOwner { plugin, .. } => (
                "project.tool.recipe-duplicate-data-owner",
                "A product recipe schema contribution repeats a data owner",
                Some(*plugin),
                1,
            ),
            Error::DataOwnerLimit { plugin, .. } => (
                "project.tool.recipe-data-owner-limit",
                "A product recipe schema contribution exceeds its data owner allowance",
                Some(*plugin),
                1,
            ),
            Error::SchemaProviderMismatch { plugin, .. } => (
                "project.tool.recipe-schema-provider-mismatch",
                "A schema contribution does not match its declared providers",
                Some(*plugin),
                2,
            ),
            Error::DuplicateSchemaProvider { plugin, .. } => (
                "project.tool.recipe-duplicate-schema-provider",
                "A product recipe schema contribution repeats a provider",
                Some(*plugin),
                1,
            ),
        };
        let admitted_plugin =
            plugin.and_then(|plugin| PublicDiagnosticIdentifier::new(plugin.as_str()).ok());
        Self {
            code,
            summary,
            omitted_fields: omitted_fields
                + u64::from(plugin.is_some() && admitted_plugin.is_none()),
            plugin: admitted_plugin,
        }
    }

    pub(in crate::project_tooling) fn failure(&self, operation: &str) -> ProjectToolFailure {
        let mut failure = ProjectToolFailure::classified(
            operation,
            ProjectToolFailureStage::Recipe,
            self.code,
            self.summary,
        );
        failure.loss.omitted_fields = self.omitted_fields;
        if let Some(plugin) = &self.plugin {
            failure.summary = format!("{} (plugin `{}`)", self.summary, plugin.as_str());
        }
        failure
    }
}
