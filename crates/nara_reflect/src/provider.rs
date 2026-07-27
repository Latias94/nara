use std::{
    error::Error,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
};

use nara_app::PluginSchemaProviderId;

use crate::{
    CatalogFingerprint, ComponentRegistry, ComponentRegistryError, ComponentSchemaCatalog,
};

type RegisterComponentSchemas = fn(&mut ComponentRegistry) -> Result<(), ComponentRegistryError>;
type ValidateComponentSchemas = fn(&ComponentRegistry) -> Result<(), ComponentRegistryError>;

/// Trusted, deterministic source for one owner's current or predecessor catalog.
///
/// The source may decode bounded compile-time embedded bytes. It must not access a `World` or
/// registry, perform filesystem or network I/O, read mutable global state, or derive its owner
/// from the selected product recipe. Product composition executes current-head sources for every
/// trusted known definition, including definitions that are inactive in the selected recipe.
/// Untrusted package descriptors require a separate Nara-owned bounded decoder.
pub type ComponentSchemaProviderSource =
    fn() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>;

fn no_provider_validation(_: &ComponentRegistry) -> Result<(), ComponentRegistryError> {
    Ok(())
}

/// Stable identity of one durable component-schema owner.
///
/// Owner identity is distinct from plugin, provider, binding, and future package identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaOwnerId(&'static str);

impl ComponentSchemaOwnerId {
    pub const MAX_BYTES: usize = 128;

    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    pub(crate) fn is_valid(self) -> bool {
        let value = self.as_str();
        value.len() <= Self::MAX_BYTES
            && value
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    }
}

impl fmt::Display for ComponentSchemaOwnerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Bounded error identity returned by a trusted compiled schema source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentSchemaProviderSourceError {
    code: &'static str,
}

impl ComponentSchemaProviderSourceError {
    pub const MAX_BYTES: usize = 128;

    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        self.code
    }

    pub(crate) fn is_valid(self) -> bool {
        let code = self.code();
        code.len() <= Self::MAX_BYTES
            && code
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            && code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    }
}

impl fmt::Display for ComponentSchemaProviderSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl Error for ComponentSchemaProviderSourceError {}

macro_rules! fingerprint_type {
    ($doc:literal, $name:ident) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 32]);

        impl $name {
            #[must_use]
            pub fn to_hex(self) -> String {
                blake3::Hash::from_bytes(self.0).to_hex().to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.to_hex())
            }
        }
    };
}

fingerprint_type!(
    "Semantic identity of one explicit schema owner and its owner-local catalog. This value proves canonical equivalence only; it is not snapshot or capability authority.",
    ComponentSchemaOwnerFingerprint
);
fingerprint_type!(
    "Semantic identity of the canonical selected owner set. Provider input order and native binding behavior do not affect it. This value proves canonical equivalence only; managed Hosts still require exact snapshot identity.",
    SchemaCompositionFingerprint
);
fingerprint_type!(
    "Executable identity of one schema composition plus its canonical provider-to-owner binding receipts. This value proves behavior equivalence only; it never replaces exact snapshot identity inside a managed Host.",
    ExecutableRegistryFingerprint
);

/// One owner-local schema source and its lineage metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentSchemaOwnerRecord {
    owner: ComponentSchemaOwnerId,
    catalog: ComponentSchemaCatalog,
}

impl ComponentSchemaOwnerRecord {
    #[must_use]
    pub const fn owner(&self) -> ComponentSchemaOwnerId {
        self.owner
    }

    #[must_use]
    pub const fn catalog(&self) -> &ComponentSchemaCatalog {
        &self.catalog
    }

    #[must_use]
    pub fn fingerprint(&self) -> ComponentSchemaOwnerFingerprint {
        owner_fingerprint(self.owner, self.catalog.fingerprint())
    }
}

/// Stable owner-local lineage receipt published with one executable provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaOwnerContributionReceipt {
    owner: ComponentSchemaOwnerId,
    generation: u64,
    catalog: CatalogFingerprint,
    predecessor: Option<CatalogFingerprint>,
    owner_fingerprint: ComponentSchemaOwnerFingerprint,
}

impl ComponentSchemaOwnerContributionReceipt {
    fn from_record(record: &ComponentSchemaOwnerRecord) -> Self {
        let catalog = record.catalog().fingerprint();
        Self {
            owner: record.owner(),
            generation: record.catalog().generation(),
            catalog,
            predecessor: record.catalog().predecessor().copied(),
            owner_fingerprint: owner_fingerprint(record.owner(), catalog),
        }
    }

    #[must_use]
    pub const fn owner(self) -> ComponentSchemaOwnerId {
        self.owner
    }

    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn catalog(self) -> CatalogFingerprint {
        self.catalog
    }

    #[must_use]
    pub const fn predecessor(self) -> Option<CatalogFingerprint> {
        self.predecessor
    }

    #[must_use]
    pub const fn owner_fingerprint(self) -> ComponentSchemaOwnerFingerprint {
        self.owner_fingerprint
    }
}

/// Stable identity of one native schema-registration policy.
///
/// The binding, codec, and migration versions are deliberately explicit. They are the
/// process-independent receipts for executable schema behavior; function addresses are never
/// used as identity because they are not stable across builds or reloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaProviderBindingId {
    id: &'static str,
    version: u32,
    codec_version: u32,
    migration_version: u32,
}

impl ComponentSchemaProviderBindingId {
    #[must_use]
    pub const fn new(id: &'static str, version: u32) -> Self {
        Self {
            id,
            version,
            codec_version: version,
            migration_version: version,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.id
    }

    #[must_use]
    pub const fn version(self) -> u32 {
        self.version
    }

    #[must_use]
    pub const fn codec_version(self) -> u32 {
        self.codec_version
    }

    #[must_use]
    pub const fn migration_version(self) -> u32 {
        self.migration_version
    }

    #[must_use]
    pub const fn with_codec_version(mut self, version: u32) -> Self {
        self.codec_version = version;
        self
    }

    #[must_use]
    pub const fn with_migration_version(mut self, version: u32) -> Self {
        self.migration_version = version;
        self
    }
}

/// Stable receipt for one provider's executable schema behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaProviderReceipt {
    provider: PluginSchemaProviderId,
    binding: ComponentSchemaProviderBindingId,
}

/// Atomic provenance for one executable provider and its durable schema owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaContributionReceipt {
    owner: ComponentSchemaOwnerContributionReceipt,
    provider: ComponentSchemaProviderReceipt,
}

impl ComponentSchemaContributionReceipt {
    pub(crate) const fn new(
        owner: ComponentSchemaOwnerContributionReceipt,
        provider: ComponentSchemaProviderReceipt,
    ) -> Self {
        Self { owner, provider }
    }

    #[must_use]
    pub const fn owner(self) -> ComponentSchemaOwnerContributionReceipt {
        self.owner
    }

    #[must_use]
    pub const fn provider(self) -> ComponentSchemaProviderReceipt {
        self.provider
    }
}

impl ComponentSchemaProviderReceipt {
    #[must_use]
    pub const fn new(
        provider: PluginSchemaProviderId,
        binding: ComponentSchemaProviderBindingId,
    ) -> Self {
        Self { provider, binding }
    }

    #[must_use]
    pub const fn provider(self) -> PluginSchemaProviderId {
        self.provider
    }

    #[must_use]
    pub const fn binding(self) -> ComponentSchemaProviderBindingId {
        self.binding
    }
}

/// A stable, replayable component-schema contribution owned by one plugin declaration.
///
/// `current` and the optional predecessor obey [`ComponentSchemaProviderSource`]'s trusted,
/// bounded contract. The source is loaded for every trusted known definition so inactive owners
/// still reserve their claims. `register` runs only for a selected provider and receives a fresh
/// owner-local candidate; it must not replace that candidate or recursively register another
/// provider. `validate` observes the same owner-local candidate and must not depend on foreign
/// owner state. Product composition freezes and compares the candidate with the declared source
/// before it can enter the aggregate registry.
#[derive(Clone, Copy)]
pub struct ComponentSchemaProviderDefinition {
    owner: ComponentSchemaOwnerId,
    id: PluginSchemaProviderId,
    binding: ComponentSchemaProviderBindingId,
    current: ComponentSchemaProviderSource,
    predecessor: Option<ComponentSchemaProviderSource>,
    validate: ValidateComponentSchemas,
    register: RegisterComponentSchemas,
}

impl ComponentSchemaProviderDefinition {
    /// Defines one owner-local schema source and its executable registration callback.
    ///
    /// See the type-level contract for source determinism, callback isolation, and inactive-source
    /// execution. The initial owner generation has no predecessor; use [`Self::with_predecessor`]
    /// only for an immediate successor.
    #[must_use]
    pub const fn new(
        owner: ComponentSchemaOwnerId,
        id: PluginSchemaProviderId,
        binding: ComponentSchemaProviderBindingId,
        current: ComponentSchemaProviderSource,
        register: RegisterComponentSchemas,
    ) -> Self {
        Self::with_validation(
            owner,
            id,
            binding,
            current,
            no_provider_validation,
            register,
        )
    }

    /// Defines one owner-local schema source with an additional read-only validation callback.
    ///
    /// Validation receives only the private owner-local candidate. It cannot inspect or mutate the
    /// aggregate Runtime registry.
    #[must_use]
    pub const fn with_validation(
        owner: ComponentSchemaOwnerId,
        id: PluginSchemaProviderId,
        binding: ComponentSchemaProviderBindingId,
        current: ComponentSchemaProviderSource,
        validate: ValidateComponentSchemas,
        register: RegisterComponentSchemas,
    ) -> Self {
        Self {
            owner,
            id,
            binding,
            current,
            predecessor: None,
            validate,
            register,
        }
    }

    /// Declares the immediate predecessor source for the same explicit owner.
    ///
    /// A previous flattened Runtime composition is not an owner predecessor.
    #[must_use]
    pub const fn with_predecessor(mut self, predecessor: ComponentSchemaProviderSource) -> Self {
        self.predecessor = Some(predecessor);
        self
    }

    #[must_use]
    pub const fn owner(self) -> ComponentSchemaOwnerId {
        self.owner
    }

    #[must_use]
    pub const fn id(self) -> PluginSchemaProviderId {
        self.id
    }

    #[must_use]
    pub const fn binding(self) -> ComponentSchemaProviderBindingId {
        self.binding
    }

    #[must_use]
    pub const fn receipt(self) -> ComponentSchemaProviderReceipt {
        ComponentSchemaProviderReceipt::new(self.id, self.binding)
    }

    #[doc(hidden)]
    pub(crate) fn resolve(self) -> Result<ResolvedComponentSchemaProvider, ComponentRegistryError> {
        if !self.owner.is_valid() {
            return Err(ComponentRegistryError::InvalidSchemaOwnerId { owner: self.owner });
        }
        let current = load_source(self.id, self.current)?;
        let predecessor = self
            .predecessor
            .map(|source| load_source(self.id, source))
            .transpose()?;
        let (current, predecessor) = crate::registry::prepare_owner_catalogs(current, predecessor)?;
        let current = ComponentSchemaOwnerRecord {
            owner: self.owner,
            catalog: current,
        };
        let predecessor = predecessor.map(|catalog| ComponentSchemaOwnerRecord {
            owner: self.owner,
            catalog,
        });
        let owner_receipt = ComponentSchemaOwnerContributionReceipt::from_record(&current);
        Ok(ResolvedComponentSchemaProvider {
            definition: self,
            current,
            predecessor,
            owner_receipt,
        })
    }

    pub(crate) fn register_into(
        self,
        registry: &mut ComponentRegistry,
    ) -> Result<(), ComponentRegistryError> {
        (self.register)(registry)
    }

    pub(crate) fn validate_into(
        self,
        registry: &ComponentRegistry,
    ) -> Result<(), ComponentRegistryError> {
        (self.validate)(registry)
    }

    /// Preflights this provider without mutating the aggregate registry.
    ///
    /// A building registry loads the owner source and validates owner, catalog, and aggregate
    /// collisions without invoking validation or registration callbacks. Those callbacks each run
    /// once later against the same private owner-local candidate. A frozen registry compares stable
    /// owner and executable receipts and never replays provider code.
    pub fn preflight(self, registry: &ComponentRegistry) -> Result<(), ComponentRegistryError> {
        registry.validate_schema_provider(self)
    }

    /// Registers this provider once or validates it against an admitted frozen snapshot.
    pub fn register_or_validate_into(
        self,
        registry: &mut ComponentRegistry,
    ) -> Result<(), ComponentRegistryError> {
        registry
            .register_or_validate_schema_provider(self)
            .map(|_| ())
    }
}

impl fmt::Debug for ComponentSchemaProviderDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentSchemaProviderDefinition")
            .field("owner", &self.owner)
            .field("id", &self.id)
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}

/// A validated owner-local source paired with its executable provider definition.
#[doc(hidden)]
#[derive(Clone)]
pub struct ResolvedComponentSchemaProvider {
    definition: ComponentSchemaProviderDefinition,
    current: ComponentSchemaOwnerRecord,
    predecessor: Option<ComponentSchemaOwnerRecord>,
    owner_receipt: ComponentSchemaOwnerContributionReceipt,
}

impl ResolvedComponentSchemaProvider {
    #[must_use]
    pub const fn definition(&self) -> ComponentSchemaProviderDefinition {
        self.definition
    }

    #[must_use]
    pub const fn current(&self) -> &ComponentSchemaOwnerRecord {
        &self.current
    }

    #[must_use]
    pub const fn predecessor(&self) -> Option<&ComponentSchemaOwnerRecord> {
        self.predecessor.as_ref()
    }

    #[must_use]
    pub const fn owner_receipt(&self) -> ComponentSchemaOwnerContributionReceipt {
        self.owner_receipt
    }

    #[must_use]
    pub const fn provider_receipt(&self) -> ComponentSchemaProviderReceipt {
        self.definition.receipt()
    }

    #[must_use]
    pub const fn contribution_receipt(&self) -> ComponentSchemaContributionReceipt {
        ComponentSchemaContributionReceipt::new(self.owner_receipt, self.definition.receipt())
    }
}

impl fmt::Debug for ResolvedComponentSchemaProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedComponentSchemaProvider")
            .field("definition", &self.definition)
            .field("current", &self.current)
            .field("predecessor", &self.predecessor)
            .field("owner_receipt", &self.owner_receipt)
            .finish()
    }
}

impl SchemaCompositionFingerprint {
    pub(crate) fn from_owner_receipts(
        receipts: impl IntoIterator<Item = ComponentSchemaOwnerContributionReceipt>,
    ) -> Self {
        let mut receipts = receipts.into_iter().collect::<Vec<_>>();
        receipts.sort_by_key(|receipt| receipt.owner());
        let mut hasher = blake3::Hasher::new();
        feed_bytes(&mut hasher, b"nara.schema-composition-fingerprint.v1");
        feed_len(&mut hasher, receipts.len());
        for receipt in receipts {
            feed_str(&mut hasher, receipt.owner().as_str());
            feed_bytes(&mut hasher, receipt.owner_fingerprint().0.as_slice());
        }
        Self(*hasher.finalize().as_bytes())
    }
}

impl ExecutableRegistryFingerprint {
    pub(crate) fn from_contributions(
        composition: SchemaCompositionFingerprint,
        contributions: impl IntoIterator<Item = ComponentSchemaContributionReceipt>,
    ) -> Self {
        let mut contributions = contributions.into_iter().collect::<Vec<_>>();
        contributions.sort_by_key(|receipt| receipt.provider().provider());
        let mut hasher = blake3::Hasher::new();
        feed_bytes(&mut hasher, b"nara.executable-registry-fingerprint.v1");
        feed_bytes(&mut hasher, composition.0.as_slice());
        feed_len(&mut hasher, contributions.len());
        for contribution in contributions {
            let owner = contribution.owner();
            let receipt = contribution.provider();
            let binding = receipt.binding();
            feed_str(&mut hasher, owner.owner().as_str());
            feed_bytes(&mut hasher, owner.owner_fingerprint().0.as_slice());
            feed_str(&mut hasher, receipt.provider().as_str());
            feed_str(&mut hasher, binding.as_str());
            feed_bytes(&mut hasher, &binding.version().to_le_bytes());
            feed_bytes(&mut hasher, &binding.codec_version().to_le_bytes());
            feed_bytes(&mut hasher, &binding.migration_version().to_le_bytes());
        }
        Self(*hasher.finalize().as_bytes())
    }
}

fn owner_fingerprint(
    owner: ComponentSchemaOwnerId,
    catalog: CatalogFingerprint,
) -> ComponentSchemaOwnerFingerprint {
    let mut hasher = blake3::Hasher::new();
    feed_bytes(&mut hasher, b"nara.component-schema-owner-fingerprint.v1");
    feed_str(&mut hasher, owner.as_str());
    feed_bytes(&mut hasher, catalog.as_bytes());
    ComponentSchemaOwnerFingerprint(*hasher.finalize().as_bytes())
}

fn load_source(
    provider: PluginSchemaProviderId,
    source: ComponentSchemaProviderSource,
) -> Result<ComponentSchemaCatalog, ComponentRegistryError> {
    match catch_unwind(AssertUnwindSafe(source)) {
        Ok(Ok(catalog)) => Ok(catalog),
        Ok(Err(source)) if source.is_valid() => {
            Err(ComponentRegistryError::SchemaProviderSourceRejected { provider, source })
        }
        Ok(Err(_)) => Err(ComponentRegistryError::SchemaProviderSourceRejected {
            provider,
            source: ComponentSchemaProviderSourceError::new(
                "schema-provider-source-invalid-error-code",
            ),
        }),
        Err(_) => Err(ComponentRegistryError::SchemaProviderSourcePanicked { provider }),
    }
}

fn feed_len(hasher: &mut blake3::Hasher, length: usize) {
    let length = u64::try_from(length).unwrap_or_else(|_| std::process::abort());
    feed_bytes(hasher, &length.to_le_bytes());
}

fn feed_str(hasher: &mut blake3::Hasher, value: &str) {
    feed_bytes(hasher, value.as_bytes());
}

fn feed_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}
