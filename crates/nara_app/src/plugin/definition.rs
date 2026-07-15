use std::{
    any::TypeId,
    fmt::{self, Debug, Formatter},
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use thiserror::Error;

use super::{
    Plugin, PluginDeclaration, PluginId, PluginPlanError,
    fingerprint::{FingerprintEncoder, write_digest},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginDefinitionId {
    pub(super) id: &'static str,
    pub(super) version: u32,
}

impl PluginDefinitionId {
    #[must_use]
    pub const fn new(id: &'static str, version: u32) -> Self {
        Self { id, version }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.id
    }

    #[must_use]
    pub const fn version(self) -> u32 {
        self.version
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginConfigurationFingerprint(pub(super) [u8; 32]);

impl PluginConfigurationFingerprint {
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl Debug for PluginConfigurationFingerprint {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write_digest(formatter, &self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginDefinitionKey {
    pub(super) definition: PluginDefinitionId,
    pub(super) configuration: PluginConfigurationFingerprint,
}

impl PluginDefinitionKey {
    #[must_use]
    pub const fn definition(self) -> PluginDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn configuration(self) -> PluginConfigurationFingerprint {
        self.configuration
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("plugin preparation rejected with {code}")]
pub struct PluginPrepareFailure {
    code: &'static str,
}

impl PluginPrepareFailure {
    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        self.code
    }
}

trait ErasedPluginFactory: Send + Sync {
    fn prepare(&self) -> Result<Arc<dyn Plugin>, PluginPrepareFailure>;
}

struct TypedPluginFactory<P, F> {
    factory: F,
    marker: PhantomData<fn() -> P>,
}

impl<P, F> ErasedPluginFactory for TypedPluginFactory<P, F>
where
    P: Plugin,
    F: Fn() -> Result<P, PluginPrepareFailure> + Send + Sync + 'static,
{
    fn prepare(&self) -> Result<Arc<dyn Plugin>, PluginPrepareFailure> {
        (self.factory)().map(|plugin| Arc::new(plugin) as Arc<dyn Plugin>)
    }
}

#[derive(Clone)]
pub struct PluginDefinition {
    declaration: Option<&'static PluginDeclaration>,
    pub(super) declaration_provider: fn() -> &'static PluginDeclaration,
    pub(super) key: Option<PluginDefinitionKey>,
    pub(super) canonical_configuration: Arc<[u8]>,
    factory: Arc<dyn ErasedPluginFactory>,
    binding_type_id: TypeId,
}

impl Debug for PluginDefinition {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginDefinition")
            .field(
                "plugin",
                &self.declaration.map(|declaration| declaration.id),
            )
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl PluginDefinition {
    #[must_use]
    pub fn for_default<P>() -> Self
    where
        P: Plugin + Default,
    {
        Self::new::<P, _>(None, &[], move || Ok(P::default()))
    }

    #[must_use]
    pub fn infallible<P, F>(
        definition: PluginDefinitionId,
        canonical_configuration: impl AsRef<[u8]>,
        factory: F,
    ) -> Self
    where
        P: Plugin,
        F: Fn() -> P + Send + Sync + 'static,
    {
        Self::fallible::<P, _>(definition, canonical_configuration, move || Ok(factory()))
    }

    #[must_use]
    pub fn fallible<P, F>(
        definition: PluginDefinitionId,
        canonical_configuration: impl AsRef<[u8]>,
        factory: F,
    ) -> Self
    where
        P: Plugin,
        F: Fn() -> Result<P, PluginPrepareFailure> + Send + Sync + 'static,
    {
        Self::new::<P, F>(Some(definition), canonical_configuration, factory)
    }

    fn new<P, F>(
        definition: Option<PluginDefinitionId>,
        canonical_configuration: impl AsRef<[u8]>,
        factory: F,
    ) -> Self
    where
        P: Plugin,
        F: Fn() -> Result<P, PluginPrepareFailure> + Send + Sync + 'static,
    {
        let canonical_configuration: Arc<[u8]> = canonical_configuration.as_ref().into();
        let mut encoder = FingerprintEncoder::new(b"nara.plugin-configuration.v2");
        encoder.bytes(b"canonical-configuration", &canonical_configuration);
        let configuration = encoder.finish_configuration();
        let typed = TypedPluginFactory::<P, F> {
            factory,
            marker: PhantomData,
        };
        Self {
            declaration: None,
            declaration_provider: P::declaration,
            key: definition.map(|definition| PluginDefinitionKey {
                definition,
                configuration,
            }),
            canonical_configuration,
            factory: Arc::new(typed),
            binding_type_id: TypeId::of::<F>(),
        }
    }

    #[must_use]
    pub const fn declaration(&self) -> Option<&'static PluginDeclaration> {
        self.declaration
    }

    #[must_use]
    pub const fn key(&self) -> Option<PluginDefinitionKey> {
        self.key
    }

    pub(super) fn resolve_declaration(mut self) -> Result<Self, PluginPlanError> {
        if self.declaration.is_none() {
            self.declaration = Some(
                catch_unwind(AssertUnwindSafe(|| (self.declaration_provider)()))
                    .map_err(|_| PluginPlanError::DeclarationPanicked)?,
            );
        }
        if self.key.is_none() {
            let declaration = self.resolved_declaration();
            let mut encoder = FingerprintEncoder::new(b"nara.plugin-configuration.v2");
            encoder.bytes(b"canonical-configuration", &self.canonical_configuration);
            self.key = Some(PluginDefinitionKey {
                definition: PluginDefinitionId::new(declaration.id.as_str(), 1),
                configuration: encoder.finish_configuration(),
            });
        }
        Ok(self)
    }

    pub(super) fn resolved_declaration(&self) -> &'static PluginDeclaration {
        self.declaration
            .expect("plugin definitions are resolved before comparison or preparation")
    }

    pub(super) fn resolved_key(&self) -> PluginDefinitionKey {
        self.key
            .expect("plugin definitions have stable keys after declaration resolution")
    }

    pub(super) fn exact_eq(&self, other: &Self) -> bool {
        self.declaration == other.declaration
            && self.key == other.key
            && self.binding_type_id == other.binding_type_id
            && self.canonical_configuration == other.canonical_configuration
    }

    pub(super) fn prepare(&self) -> Result<Arc<dyn Plugin>, PluginPrepareError> {
        let declaration = self.resolved_declaration();
        catch_unwind(AssertUnwindSafe(|| self.factory.prepare()))
            .map_err(|_| PluginPrepareError::Panicked {
                plugin: declaration.id,
            })?
            .map_err(|failure| PluginPrepareError::Failed {
                plugin: declaration.id,
                code: failure.code,
            })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginPrepareError {
    #[error("plugin {plugin} preparation failed with {code}")]
    Failed {
        plugin: PluginId,
        code: &'static str,
    },
    #[error("plugin {plugin} preparation panicked")]
    Panicked { plugin: PluginId },
}

impl PluginPrepareError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Failed { code, .. } => code,
            Self::Panicked { .. } => "plugin.prepare.panicked",
        }
    }
}
