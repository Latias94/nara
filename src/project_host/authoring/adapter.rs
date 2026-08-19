use nara_reflect::{
    __private::resolve_schema_provider, ComponentCapability, ComponentRegistryError,
    ComponentSchemaProviderDefinition,
};
use nara_scene::__private::{AuthoringAdapterReceipt, AuthoringAdapterReceiptError};

const FIRST_PARTY_SCHEMA_ADAPTER_ID: &str = "nara.first-party-authoring";
const FIRST_PARTY_SCHEMA_ADAPTER_VERSION: u32 = 1;
const FIRST_PARTY_SCHEMA_ADAPTER_GENERATION: u64 = 1;

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
