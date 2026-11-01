use nara_reflect::{
    __private::{register_resolved_schema_provider, resolve_schema_provider},
    AuthoringSchemaBundle, ComponentCapability, ComponentRegistryBuilder, ComponentRegistryError,
    ComponentRegistrySnapshot, ComponentSchemaProviderDefinition,
};
use nara_scene::__private::{AuthoringAdapterReceipt, AuthoringAdapterReceiptError};

const FIRST_PARTY_SCHEMA_ADAPTER_ID: &str = "nara.first-party-authoring";
const FIRST_PARTY_SCHEMA_ADAPTER_VERSION: u32 = 1;
const FIRST_PARTY_SCHEMA_ADAPTER_GENERATION: u64 = 1;

/// Uses the actual compiled registry's native receipts and the selected complete metadata.
/// A data declaration never manufactures a native codec or native Adapter contribution.
#[cfg(feature = "tooling")]
pub(crate) fn compiled_authoring_adapters(
    bundle: &AuthoringSchemaBundle,
    native: &ComponentRegistrySnapshot,
) -> Result<Box<[AuthoringAdapterReceipt]>, AuthoringAdapterReceiptError> {
    let mut adapters = Vec::new();
    for contribution in native.contribution_receipts() {
        let owner = contribution.owner().owner();
        let coverage: Vec<_> = native
            .schemas()
            .filter(|schema| {
                native.component_schema_owner(schema.id()) == Some(owner)
                    && schema.has_capability(ComponentCapability::Scene)
                    && schema.has_capability(ComponentCapability::Inspect)
            })
            .map(|schema| schema.id().clone())
            .collect();
        if !coverage.is_empty() {
            adapters.push(AuthoringAdapterReceipt::from_trusted_provider(
                contribution,
                "nara.compiled-authoring",
                1,
                1,
                coverage,
            )?);
        }
    }
    let native_coverage = native.schemas().map(|schema| schema.id().clone()).collect();
    adapters.extend(
        bundle.owners().iter().filter_map(|owner| {
            AuthoringAdapterReceipt::from_schema_owner(owner, &native_coverage)
        }),
    );
    Ok(adapters.into_boxed_slice())
}

pub(crate) fn project_authoring_adapters(
    bundle: &AuthoringSchemaBundle,
) -> Result<Box<[AuthoringAdapterReceipt]>, FirstPartyAuthoringAdapterError> {
    let mut adapters = first_party_authoring_adapters(
        crate::project_host::composition::built_in_schema_providers(),
    )?
    .into_vec();
    let native_coverage = adapters
        .iter()
        .flat_map(|adapter| adapter.components().cloned())
        .collect();
    adapters.extend(
        bundle.owners().iter().filter_map(|owner| {
            AuthoringAdapterReceipt::from_schema_owner(owner, &native_coverage)
        }),
    );
    Ok(adapters.into_boxed_slice())
}

pub(crate) fn first_party_authoring_adapters(
    providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<Box<[AuthoringAdapterReceipt]>, FirstPartyAuthoringAdapterError> {
    providers
        .into_iter()
        .map(|provider| {
            let resolved = resolve_schema_provider(provider)
                .map_err(FirstPartyAuthoringAdapterError::ProviderSource)?;
            let coverage = resolved
                .current()
                .catalog()
                .components()
                .iter()
                .filter(|schema| {
                    schema.has_capability(ComponentCapability::Scene)
                        && schema.has_capability(ComponentCapability::Inspect)
                })
                .map(|schema| schema.id().clone())
                .collect::<Vec<_>>();
            if coverage.is_empty() {
                return Ok(None);
            }
            AuthoringAdapterReceipt::from_trusted_provider(
                resolved.contribution_receipt(),
                FIRST_PARTY_SCHEMA_ADAPTER_ID,
                FIRST_PARTY_SCHEMA_ADAPTER_VERSION,
                FIRST_PARTY_SCHEMA_ADAPTER_GENERATION,
                coverage,
            )
            .map(Some)
            .map_err(FirstPartyAuthoringAdapterError::Receipt)
        })
        .filter_map(|result| result.transpose())
        .collect::<Result<Vec<_>, _>>()
        .map(Vec::into_boxed_slice)
}

pub(crate) fn first_party_authoring_preview_registry(
    providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<ComponentRegistrySnapshot, FirstPartyAuthoringAdapterError> {
    let mut registry = ComponentRegistryBuilder::new();
    for provider in providers {
        let resolved = resolve_schema_provider(provider)
            .map_err(FirstPartyAuthoringAdapterError::ProviderSource)?;
        let supports_authoring_preview =
            resolved
                .current()
                .catalog()
                .components()
                .iter()
                .any(|schema| {
                    schema.has_capability(ComponentCapability::Scene)
                        && schema.has_capability(ComponentCapability::Inspect)
                });
        if supports_authoring_preview {
            register_resolved_schema_provider(&mut registry, resolved)
                .map_err(FirstPartyAuthoringAdapterError::ProviderSource)?;
        }
    }
    registry
        .freeze()
        .map_err(|error| FirstPartyAuthoringAdapterError::ProviderSource(error.into_error()))
}

#[derive(Debug)]
pub(crate) enum FirstPartyAuthoringAdapterError {
    ProviderSource(ComponentRegistryError),
    Receipt(AuthoringAdapterReceiptError),
}

impl std::fmt::Display for FirstPartyAuthoringAdapterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ProviderSource(_) => "first-party authoring Adapter source is invalid",
            Self::Receipt(_) => "first-party authoring Adapter receipt is invalid",
        })
    }
}

impl std::error::Error for FirstPartyAuthoringAdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ProviderSource(error) => Some(error),
            Self::Receipt(error) => Some(error),
        }
    }
}
