//! Bounded, data-only persistent schema composition for project authoring.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
    PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
    PersistentFileEnvelope, PersistentFileHeader, SerdeShapeLimits,
    decode_persistent_file_with_preflight, preflight_serde_shape,
};

use crate::{
    ComponentSchemaCatalog, ComponentTypeId, PersistentComponentSchemaOwnerId,
    PersistentComponentSchemaOwnerIdError, SchemaCompositionFingerprint,
    format::{
        ComponentCatalogCounts, ComponentCatalogFileBudgetError, ComponentCatalogFileLimits,
        ComponentSchemaCatalogWire, accumulate_catalog_budget,
    },
    registry::prepare_owner_catalogs,
};

const AUTHORING_SCHEMA_BUNDLE_KIND: &str = "authoring_schema_bundle";
const CANONICAL_V1_ENGINE_MIN_VERSION: &str = "0.1.0";
const FORMAT_GENERATOR_NAME: &str = "nara";

/// Allocation and validation-work limits for an authoring schema bundle file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoringSchemaBundleLimits {
    encoded_bytes: ByteLimit,
    shape: SerdeShapeLimits,
    owners: ItemLimit,
    catalogs: ComponentCatalogFileLimits,
}

impl Default for AuthoringSchemaBundleLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(16 * 1024 * 1024)
                .expect("authoring schema bundle byte limit is non-zero"),
            shape: SerdeShapeLimits::new(
                DepthLimit::new(64).expect("authoring schema bundle depth is non-zero"),
                ItemLimit::new(1_000_000).expect("authoring schema bundle node limit is non-zero"),
                ItemLimit::new(200_000)
                    .expect("authoring schema bundle container limit is non-zero"),
                ByteLimit::new(256 * 1024)
                    .expect("authoring schema bundle string limit is non-zero"),
                ByteLimit::new(8 * 1024 * 1024)
                    .expect("authoring schema bundle total string limit is non-zero"),
            ),
            owners: ItemLimit::new(4_096).expect("authoring schema bundle owner limit is non-zero"),
            catalogs: ComponentCatalogFileLimits::default(),
        }
    }
}

impl AuthoringSchemaBundleLimits {
    #[must_use]
    pub const fn encoded_bytes(self) -> ByteLimit {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn shape(self) -> SerdeShapeLimits {
        self.shape
    }

    #[must_use]
    pub const fn owners(self) -> ItemLimit {
        self.owners
    }

    #[must_use]
    pub const fn catalogs(self) -> ComponentCatalogFileLimits {
        self.catalogs
    }

    #[must_use]
    pub const fn with_encoded_bytes(mut self, limit: ByteLimit) -> Self {
        self.encoded_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_shape(mut self, limits: SerdeShapeLimits) -> Self {
        self.shape = limits;
        self
    }

    #[must_use]
    pub const fn with_owners(mut self, limit: ItemLimit) -> Self {
        self.owners = limit;
        self
    }

    #[must_use]
    pub const fn with_catalogs(mut self, limits: ComponentCatalogFileLimits) -> Self {
        self.catalogs = limits;
        self
    }
}

/// One persistent schema owner and its complete owner-local current lineage boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoringSchemaBundleOwner {
    owner_id: PersistentComponentSchemaOwnerId,
    current: ComponentSchemaCatalog,
    predecessor: Option<ComponentSchemaCatalog>,
}

impl AuthoringSchemaBundleOwner {
    /// Creates an unpublished owner candidate. Bundle construction validates and canonicalizes it.
    #[must_use]
    pub fn new(
        owner_id: PersistentComponentSchemaOwnerId,
        current: ComponentSchemaCatalog,
        predecessor: Option<ComponentSchemaCatalog>,
    ) -> Self {
        Self {
            owner_id,
            current,
            predecessor,
        }
    }

    #[must_use]
    pub const fn owner_id(&self) -> &PersistentComponentSchemaOwnerId {
        &self.owner_id
    }

    #[must_use]
    pub const fn current(&self) -> &ComponentSchemaCatalog {
        &self.current
    }

    #[must_use]
    pub const fn predecessor(&self) -> Option<&ComponentSchemaCatalog> {
        self.predecessor.as_ref()
    }
}

/// Canonical, non-executable schema composition loaded for project authoring.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoringSchemaBundle {
    owners: Vec<AuthoringSchemaBundleOwner>,
    fingerprint: SchemaCompositionFingerprint,
}

impl AuthoringSchemaBundle {
    /// Validates and canonicalizes an in-memory bundle candidate.
    pub fn new(
        owners: impl IntoIterator<Item = AuthoringSchemaBundleOwner>,
    ) -> Result<Self, AuthoringSchemaBundleError> {
        finish_bundle(
            owners.into_iter().collect(),
            AuthoringSchemaBundleLimits::default(),
            InputOrdering::Canonicalize,
        )
    }

    #[must_use]
    pub fn owners(&self) -> &[AuthoringSchemaBundleOwner] {
        &self.owners
    }

    #[must_use]
    pub const fn fingerprint(&self) -> SchemaCompositionFingerprint {
        self.fingerprint
    }

    pub fn to_json_string(&self) -> Result<String, AuthoringSchemaBundleError> {
        let limits = AuthoringSchemaBundleLimits::default();
        let payload = AuthoringSchemaBundlePayloadRef {
            owners: self
                .owners
                .iter()
                .map(AuthoringSchemaBundleOwnerRef::from)
                .collect(),
        };
        let envelope = PersistentFileEnvelope::canonical_v1(
            format_kind()?,
            engine_version(CANONICAL_V1_ENGINE_MIN_VERSION)?,
            FormatGenerator::new(
                FORMAT_GENERATOR_NAME,
                engine_version(env!("CARGO_PKG_VERSION"))?,
            )
            .map_err(metadata_error)?,
            payload,
        );
        let encoded = serde_json::to_string_pretty(&envelope).map_err(|source| {
            AuthoringSchemaBundleError::Encode {
                source: Box::new(source),
            }
        })?;
        validate_encoded_bytes(encoded.as_bytes(), limits)?;
        Ok(encoded)
    }

    pub fn from_json_bytes(encoded: &[u8]) -> Result<Self, AuthoringSchemaBundleError> {
        Self::from_json_bytes_with_limits(encoded, AuthoringSchemaBundleLimits::default())
    }

    pub fn from_json_str(encoded: &str) -> Result<Self, AuthoringSchemaBundleError> {
        Self::from_json_bytes(encoded.as_bytes())
    }

    pub fn from_json_bytes_with_limits(
        encoded: &[u8],
        limits: AuthoringSchemaBundleLimits,
    ) -> Result<Self, AuthoringSchemaBundleError> {
        let contract = file_contract()?;
        let envelope = decode_persistent_file_with_preflight(
            encoded,
            limits.encoded_bytes(),
            &contract,
            |bytes| {
                let mut deserializer = serde_json::Deserializer::from_slice(bytes);
                preflight_serde_shape(&mut deserializer, limits.shape())
                    .map_err(JsonDecodeError::Shape)
            },
            |bytes| {
                serde_json::from_slice::<PersistentFileHeader>(bytes)
                    .map_err(JsonDecodeError::Syntax)
            },
            |bytes| {
                serde_json::from_slice::<PersistentFileEnvelope<AuthoringSchemaBundleWire>>(bytes)
                    .map_err(JsonDecodeError::Syntax)
            },
        )
        .map_err(map_decode_error)?;
        let owners = envelope
            .into_payload()
            .owners
            .into_iter()
            .map(AuthoringSchemaBundleOwner::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        finish_bundle(owners, limits, InputOrdering::RequireCanonical)
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoringSchemaBundleWire {
    owners: Vec<AuthoringSchemaBundleOwnerWire>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoringSchemaBundleOwnerWire {
    owner: String,
    current: ComponentSchemaCatalogWire,
    #[serde(default)]
    predecessor: Option<ComponentSchemaCatalogWire>,
}

impl TryFrom<AuthoringSchemaBundleOwnerWire> for AuthoringSchemaBundleOwner {
    type Error = AuthoringSchemaBundleError;

    fn try_from(wire: AuthoringSchemaBundleOwnerWire) -> Result<Self, Self::Error> {
        let owner_id = PersistentComponentSchemaOwnerId::try_new(wire.owner)
            .map_err(AuthoringSchemaBundleError::InvalidOwnerId)?;
        Ok(Self::new(
            owner_id,
            wire.current.into(),
            wire.predecessor.map(Into::into),
        ))
    }
}

#[derive(serde::Serialize)]
struct AuthoringSchemaBundlePayloadRef<'a> {
    owners: Vec<AuthoringSchemaBundleOwnerRef<'a>>,
}

#[derive(serde::Serialize)]
struct AuthoringSchemaBundleOwnerRef<'a> {
    owner: &'a str,
    current: &'a ComponentSchemaCatalog,
    #[serde(skip_serializing_if = "Option::is_none")]
    predecessor: Option<&'a ComponentSchemaCatalog>,
}

impl<'a> From<&'a AuthoringSchemaBundleOwner> for AuthoringSchemaBundleOwnerRef<'a> {
    fn from(owner: &'a AuthoringSchemaBundleOwner) -> Self {
        Self {
            owner: owner.owner_id().as_str(),
            current: owner.current(),
            predecessor: owner.predecessor(),
        }
    }
}

#[derive(Debug)]
pub enum AuthoringSchemaBundleError {
    EncodedBytesExceeded {
        observed: usize,
        maximum: usize,
    },
    Shape {
        source: Box<dyn Error + Send + Sync>,
    },
    Header {
        source: Box<dyn Error + Send + Sync>,
    },
    Contract(PersistentFileContractError),
    Payload {
        source: Box<dyn Error + Send + Sync>,
    },
    InvalidOwnerId(PersistentComponentSchemaOwnerIdError),
    OwnerLimitExceeded {
        observed: usize,
        maximum: usize,
    },
    DuplicateOwner {
        owner: PersistentComponentSchemaOwnerId,
    },
    NonCanonicalOwnerOrder {
        previous: PersistentComponentSchemaOwnerId,
        current: PersistentComponentSchemaOwnerId,
    },
    NonCanonicalCatalog {
        owner: PersistentComponentSchemaOwnerId,
        generation: u64,
    },
    Catalog {
        owner: PersistentComponentSchemaOwnerId,
        source: Box<dyn Error + Send + Sync>,
    },
    DivergentComponentOwnership {
        component_id: ComponentTypeId,
        first_owner: PersistentComponentSchemaOwnerId,
        second_owner: PersistentComponentSchemaOwnerId,
    },
    Budget(ComponentCatalogFileBudgetError),
    Encode {
        source: Box<dyn Error + Send + Sync>,
    },
    Metadata(Box<dyn Error + Send + Sync>),
}

impl Display for AuthoringSchemaBundleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedBytesExceeded { observed, maximum } => write!(
                formatter,
                "authoring schema bundle contains {observed} encoded bytes, maximum is {maximum}"
            ),
            Self::Shape { source } => {
                write!(
                    formatter,
                    "JSON authoring schema bundle shape is invalid: {source}"
                )
            }
            Self::Header { source } => {
                write!(
                    formatter,
                    "JSON authoring schema bundle header is invalid: {source}"
                )
            }
            Self::Contract(error) => Display::fmt(error, formatter),
            Self::Payload { source } => {
                write!(
                    formatter,
                    "JSON authoring schema bundle payload is invalid: {source}"
                )
            }
            Self::InvalidOwnerId(error) => Display::fmt(error, formatter),
            Self::OwnerLimitExceeded { observed, maximum } => write!(
                formatter,
                "authoring schema bundle contains {observed} owners, maximum is {maximum}"
            ),
            Self::DuplicateOwner { owner } => {
                write!(formatter, "authoring schema bundle repeats owner '{owner}'")
            }
            Self::NonCanonicalOwnerOrder { previous, current } => write!(
                formatter,
                "authoring schema bundle owner '{current}' is not ordered after '{previous}'"
            ),
            Self::NonCanonicalCatalog { owner, generation } => write!(
                formatter,
                "authoring schema bundle owner '{owner}' catalog generation {generation} is not canonical"
            ),
            Self::Catalog { owner, source } => write!(
                formatter,
                "authoring schema bundle owner '{owner}' catalog is invalid: {source}"
            ),
            Self::DivergentComponentOwnership {
                component_id,
                first_owner,
                second_owner,
            } => write!(
                formatter,
                "component '{component_id}' is claimed by both '{first_owner}' and '{second_owner}'"
            ),
            Self::Budget(error) => Display::fmt(error, formatter),
            Self::Encode { source } => {
                write!(
                    formatter,
                    "authoring schema bundle encoding failed: {source}"
                )
            }
            Self::Metadata(source) => {
                write!(
                    formatter,
                    "authoring schema bundle metadata is invalid: {source}"
                )
            }
        }
    }
}

impl Error for AuthoringSchemaBundleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape { source }
            | Self::Header { source }
            | Self::Payload { source }
            | Self::Encode { source }
            | Self::Metadata(source) => Some(source.as_ref()),
            Self::Contract(error) => Some(error),
            Self::InvalidOwnerId(error) => Some(error),
            Self::Catalog { source, .. } => Some(source.as_ref()),
            Self::Budget(error) => Some(error),
            Self::EncodedBytesExceeded { .. }
            | Self::OwnerLimitExceeded { .. }
            | Self::DuplicateOwner { .. }
            | Self::NonCanonicalOwnerOrder { .. }
            | Self::NonCanonicalCatalog { .. }
            | Self::DivergentComponentOwnership { .. } => None,
        }
    }
}

impl From<ComponentCatalogFileBudgetError> for AuthoringSchemaBundleError {
    fn from(error: ComponentCatalogFileBudgetError) -> Self {
        Self::Budget(error)
    }
}

#[derive(Clone, Copy)]
enum InputOrdering {
    Canonicalize,
    RequireCanonical,
}

fn finish_bundle(
    mut owners: Vec<AuthoringSchemaBundleOwner>,
    limits: AuthoringSchemaBundleLimits,
    input_ordering: InputOrdering,
) -> Result<AuthoringSchemaBundle, AuthoringSchemaBundleError> {
    validate_owner_count(owners.len(), limits)?;
    match input_ordering {
        InputOrdering::Canonicalize => {
            owners.sort_by(|left, right| left.owner_id().as_str().cmp(right.owner_id().as_str()))
        }
        InputOrdering::RequireCanonical => validate_owner_order(&owners)?,
    }
    validate_duplicate_owners(&owners)?;

    let mut counts = ComponentCatalogCounts::default();
    for owner in &mut owners {
        accumulate_catalog_budget(owner.current(), limits.catalogs(), &mut counts)?;
        if let Some(predecessor) = owner.predecessor() {
            accumulate_catalog_budget(predecessor, limits.catalogs(), &mut counts)?;
        }

        let original_current = owner.current.clone();
        let original_predecessor = owner.predecessor.clone();
        let (current, predecessor) =
            prepare_owner_catalogs(std::mem::take(&mut owner.current), owner.predecessor.take())
                .map_err(|source| AuthoringSchemaBundleError::Catalog {
                    owner: owner.owner_id.clone(),
                    source: Box::new(source),
                })?;
        if matches!(input_ordering, InputOrdering::RequireCanonical)
            && (current != original_current || predecessor != original_predecessor)
        {
            return Err(AuthoringSchemaBundleError::NonCanonicalCatalog {
                owner: owner.owner_id.clone(),
                generation: current.generation(),
            });
        }
        owner.current = current;
        owner.predecessor = predecessor;
    }

    validate_component_ownership(&owners)?;
    let fingerprint = SchemaCompositionFingerprint::from_persistent_owner_catalogs(
        owners
            .iter()
            .map(|owner| (owner.owner_id(), owner.current().fingerprint())),
    );
    Ok(AuthoringSchemaBundle {
        owners,
        fingerprint,
    })
}

fn validate_owner_count(
    observed: usize,
    limits: AuthoringSchemaBundleLimits,
) -> Result<(), AuthoringSchemaBundleError> {
    let maximum = limits.owners().get();
    if observed > maximum {
        return Err(AuthoringSchemaBundleError::OwnerLimitExceeded { observed, maximum });
    }
    Ok(())
}

fn validate_owner_order(
    owners: &[AuthoringSchemaBundleOwner],
) -> Result<(), AuthoringSchemaBundleError> {
    for pair in owners.windows(2) {
        let previous = pair[0].owner_id();
        let current = pair[1].owner_id();
        if current < previous {
            return Err(AuthoringSchemaBundleError::NonCanonicalOwnerOrder {
                previous: previous.clone(),
                current: current.clone(),
            });
        }
    }
    Ok(())
}

fn validate_duplicate_owners(
    owners: &[AuthoringSchemaBundleOwner],
) -> Result<(), AuthoringSchemaBundleError> {
    for pair in owners.windows(2) {
        if pair[0].owner_id() == pair[1].owner_id() {
            return Err(AuthoringSchemaBundleError::DuplicateOwner {
                owner: pair[0].owner_id().clone(),
            });
        }
    }
    Ok(())
}

fn validate_component_ownership(
    owners: &[AuthoringSchemaBundleOwner],
) -> Result<(), AuthoringSchemaBundleError> {
    let mut claims = BTreeMap::<ComponentTypeId, PersistentComponentSchemaOwnerId>::new();
    for owner in owners {
        let mut owner_claims = BTreeSet::new();
        for catalog in std::iter::once(owner.current()).chain(owner.predecessor()) {
            owner_claims.extend(
                catalog
                    .components()
                    .iter()
                    .map(|schema| schema.id().clone()),
            );
            owner_claims.extend(catalog.type_tombstones().iter().cloned());
        }
        for component_id in owner_claims {
            if let Some(first_owner) = claims.insert(component_id.clone(), owner.owner_id().clone())
            {
                return Err(AuthoringSchemaBundleError::DivergentComponentOwnership {
                    component_id,
                    first_owner,
                    second_owner: owner.owner_id().clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_encoded_bytes(
    encoded: &[u8],
    limits: AuthoringSchemaBundleLimits,
) -> Result<(), AuthoringSchemaBundleError> {
    if encoded.len() > limits.encoded_bytes().get() {
        return Err(AuthoringSchemaBundleError::EncodedBytesExceeded {
            observed: encoded.len(),
            maximum: limits.encoded_bytes().get(),
        });
    }
    Ok(())
}

fn map_decode_error<E>(error: PersistentFileDecodeError<E>) -> AuthoringSchemaBundleError
where
    E: Error + Send + Sync + 'static,
{
    match error {
        PersistentFileDecodeError::EncodedBytesExceeded { observed, maximum } => {
            AuthoringSchemaBundleError::EncodedBytesExceeded { observed, maximum }
        }
        PersistentFileDecodeError::Shape(source) => AuthoringSchemaBundleError::Shape {
            source: Box::new(source),
        },
        PersistentFileDecodeError::Header(source) => AuthoringSchemaBundleError::Header {
            source: Box::new(source),
        },
        PersistentFileDecodeError::Contract(error) => AuthoringSchemaBundleError::Contract(error),
        PersistentFileDecodeError::Payload(source) => AuthoringSchemaBundleError::Payload {
            source: Box::new(source),
        },
    }
}

fn file_contract() -> Result<PersistentFileContract, AuthoringSchemaBundleError> {
    Ok(PersistentFileContract::canonical_v1(
        format_kind()?,
        engine_version(env!("CARGO_PKG_VERSION"))?,
    ))
}

fn format_kind() -> Result<FormatKind, AuthoringSchemaBundleError> {
    FormatKind::new(AUTHORING_SCHEMA_BUNDLE_KIND).map_err(metadata_error)
}

fn engine_version(version: &str) -> Result<EngineVersion, AuthoringSchemaBundleError> {
    EngineVersion::parse(version).map_err(metadata_error)
}

fn metadata_error(error: impl Error + Send + Sync + 'static) -> AuthoringSchemaBundleError {
    AuthoringSchemaBundleError::Metadata(Box::new(error))
}

#[derive(Debug)]
enum JsonDecodeError {
    Shape(nara_core::SerdeShapePreflightError<serde_json::Error>),
    Syntax(serde_json::Error),
}

impl Display for JsonDecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape(error) => Display::fmt(error, formatter),
            Self::Syntax(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for JsonDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape(error) => Some(error),
            Self::Syntax(error) => Some(error),
        }
    }
}
