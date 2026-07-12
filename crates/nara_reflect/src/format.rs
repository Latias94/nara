//! Canonical persistent component schema catalog file boundary.

use std::{
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
    CatalogFingerprint, ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog,
    ComponentTypeId, registry::prepare_catalog_candidate,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentSchemaCatalogWire {
    generation: u64,
    #[serde(default)]
    predecessor: Option<CatalogFingerprint>,
    components: Vec<ComponentSchema>,
    #[serde(default)]
    type_tombstones: Vec<ComponentTypeId>,
}

impl From<ComponentSchemaCatalogWire> for ComponentSchemaCatalog {
    fn from(wire: ComponentSchemaCatalogWire) -> Self {
        Self {
            generation: wire.generation,
            predecessor: wire.predecessor,
            components: wire.components,
            type_tombstones: wire.type_tombstones,
        }
    }
}

const COMPONENT_SCHEMA_CATALOG_KIND: &str = "component_schema_catalog";
const CANONICAL_V1_ENGINE_MIN_VERSION: &str = "0.1.0";
const FORMAT_GENERATOR_NAME: &str = "nara";

/// Allocation and validation-work limits for persistent component schema catalogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentCatalogFileLimits {
    encoded_bytes: ByteLimit,
    shape: SerdeShapeLimits,
    components: ItemLimit,
    fields: ItemLimit,
    aliases: ItemLimit,
    tombstones: ItemLimit,
    diagnostic_sources: ItemLimit,
}

impl Default for ComponentCatalogFileLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(8 * 1024 * 1024)
                .expect("component catalog byte limit is non-zero"),
            shape: SerdeShapeLimits::new(
                DepthLimit::new(64).expect("component catalog depth is non-zero"),
                ItemLimit::new(500_000).expect("component catalog node limit is non-zero"),
                ItemLimit::new(100_000)
                    .expect("component catalog container item limit is non-zero"),
                ByteLimit::new(256 * 1024).expect("component catalog string limit is non-zero"),
                ByteLimit::new(4 * 1024 * 1024)
                    .expect("component catalog total string limit is non-zero"),
            ),
            components: ItemLimit::new(65_536)
                .expect("component catalog component limit is non-zero"),
            fields: ItemLimit::new(500_000).expect("component catalog field limit is non-zero"),
            aliases: ItemLimit::new(500_000).expect("component catalog alias limit is non-zero"),
            tombstones: ItemLimit::new(500_000)
                .expect("component catalog tombstone limit is non-zero"),
            diagnostic_sources: ItemLimit::new(500_000)
                .expect("component catalog diagnostic source limit is non-zero"),
        }
    }
}

impl ComponentCatalogFileLimits {
    #[must_use]
    pub const fn encoded_bytes(self) -> ByteLimit {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn shape(self) -> SerdeShapeLimits {
        self.shape
    }

    #[must_use]
    pub const fn components(self) -> ItemLimit {
        self.components
    }

    #[must_use]
    pub const fn fields(self) -> ItemLimit {
        self.fields
    }

    #[must_use]
    pub const fn aliases(self) -> ItemLimit {
        self.aliases
    }

    #[must_use]
    pub const fn tombstones(self) -> ItemLimit {
        self.tombstones
    }

    #[must_use]
    pub const fn diagnostic_sources(self) -> ItemLimit {
        self.diagnostic_sources
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
    pub const fn with_components(mut self, limit: ItemLimit) -> Self {
        self.components = limit;
        self
    }

    #[must_use]
    pub const fn with_fields(mut self, limit: ItemLimit) -> Self {
        self.fields = limit;
        self
    }

    #[must_use]
    pub const fn with_aliases(mut self, limit: ItemLimit) -> Self {
        self.aliases = limit;
        self
    }

    #[must_use]
    pub const fn with_tombstones(mut self, limit: ItemLimit) -> Self {
        self.tombstones = limit;
        self
    }

    #[must_use]
    pub const fn with_diagnostic_sources(mut self, limit: ItemLimit) -> Self {
        self.diagnostic_sources = limit;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentCatalogFileEncoding {
    Json,
    Ron,
}

impl Display for ComponentCatalogFileEncoding {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json => formatter.write_str("JSON"),
            Self::Ron => formatter.write_str("RON"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentCatalogFileBudgetKind {
    Components,
    Fields,
    Aliases,
    Tombstones,
    DiagnosticSources,
}

impl Display for ComponentCatalogFileBudgetKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Components => formatter.write_str("components"),
            Self::Fields => formatter.write_str("fields"),
            Self::Aliases => formatter.write_str("aliases"),
            Self::Tombstones => formatter.write_str("tombstones"),
            Self::DiagnosticSources => formatter.write_str("diagnostic sources"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentCatalogFileBudgetError {
    pub kind: ComponentCatalogFileBudgetKind,
    pub observed: usize,
    pub maximum: usize,
}

impl Display for ComponentCatalogFileBudgetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "component schema catalog contains {} {}, maximum is {}",
            self.observed, self.kind, self.maximum
        )
    }
}

impl Error for ComponentCatalogFileBudgetError {}

#[derive(Debug)]
pub enum ComponentCatalogFileError {
    EncodedBytesExceeded {
        observed: usize,
        maximum: usize,
    },
    Shape {
        encoding: ComponentCatalogFileEncoding,
        source: Box<dyn Error + Send + Sync>,
    },
    Header {
        encoding: ComponentCatalogFileEncoding,
        source: Box<dyn Error + Send + Sync>,
    },
    Contract(PersistentFileContractError),
    Payload {
        encoding: ComponentCatalogFileEncoding,
        source: Box<dyn Error + Send + Sync>,
    },
    Budget(ComponentCatalogFileBudgetError),
    Catalog(ComponentRegistryError),
    Encode {
        encoding: ComponentCatalogFileEncoding,
        source: Box<dyn Error + Send + Sync>,
    },
    Metadata(Box<dyn Error + Send + Sync>),
}

impl Display for ComponentCatalogFileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedBytesExceeded { observed, maximum } => write!(
                formatter,
                "component schema catalog contains {observed} encoded bytes, maximum is {maximum}"
            ),
            Self::Shape { encoding, source } => write!(
                formatter,
                "{encoding} component schema catalog shape is invalid: {source}"
            ),
            Self::Header { encoding, source } => write!(
                formatter,
                "{encoding} component schema catalog header is invalid: {source}"
            ),
            Self::Contract(error) => Display::fmt(error, formatter),
            Self::Payload { encoding, source } => write!(
                formatter,
                "{encoding} component schema catalog payload is invalid: {source}"
            ),
            Self::Budget(error) => Display::fmt(error, formatter),
            Self::Catalog(error) => Display::fmt(error, formatter),
            Self::Encode { encoding, source } => write!(
                formatter,
                "{encoding} component schema catalog encoding failed: {source}"
            ),
            Self::Metadata(source) => write!(
                formatter,
                "component schema catalog metadata is invalid: {source}"
            ),
        }
    }
}

impl Error for ComponentCatalogFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape { source, .. }
            | Self::Header { source, .. }
            | Self::Payload { source, .. }
            | Self::Encode { source, .. }
            | Self::Metadata(source) => Some(source.as_ref()),
            Self::Contract(error) => Some(error),
            Self::Budget(error) => Some(error),
            Self::Catalog(error) => Some(error),
            Self::EncodedBytesExceeded { .. } => None,
        }
    }
}

impl From<ComponentCatalogFileBudgetError> for ComponentCatalogFileError {
    fn from(error: ComponentCatalogFileBudgetError) -> Self {
        Self::Budget(error)
    }
}

impl ComponentSchemaCatalog {
    pub fn to_json_string(&self) -> Result<String, ComponentCatalogFileError> {
        self.to_json_string_with_predecessor(None)
    }

    pub fn to_json_string_with_predecessor(
        &self,
        predecessor: Option<&Self>,
    ) -> Result<String, ComponentCatalogFileError> {
        encode_json(self, predecessor, ComponentCatalogFileLimits::default())
    }

    pub fn to_ron_string(&self) -> Result<String, ComponentCatalogFileError> {
        self.to_ron_string_with_predecessor(None)
    }

    pub fn to_ron_string_with_predecessor(
        &self,
        predecessor: Option<&Self>,
    ) -> Result<String, ComponentCatalogFileError> {
        encode_ron(self, predecessor, ComponentCatalogFileLimits::default())
    }

    pub fn from_json_bytes(encoded: &[u8]) -> Result<Self, ComponentCatalogFileError> {
        Self::from_json_bytes_with_limits(encoded, ComponentCatalogFileLimits::default())
    }

    pub fn from_json_str(encoded: &str) -> Result<Self, ComponentCatalogFileError> {
        Self::from_json_bytes(encoded.as_bytes())
    }

    pub fn from_json_bytes_with_limits(
        encoded: &[u8],
        limits: ComponentCatalogFileLimits,
    ) -> Result<Self, ComponentCatalogFileError> {
        decode_json(encoded, None, limits)
    }

    pub fn from_json_bytes_with_predecessor(
        encoded: &[u8],
        predecessor: &Self,
        limits: ComponentCatalogFileLimits,
    ) -> Result<Self, ComponentCatalogFileError> {
        decode_json(encoded, Some(predecessor), limits)
    }

    pub fn from_ron_bytes(encoded: &[u8]) -> Result<Self, ComponentCatalogFileError> {
        Self::from_ron_bytes_with_limits(encoded, ComponentCatalogFileLimits::default())
    }

    pub fn from_ron_str(encoded: &str) -> Result<Self, ComponentCatalogFileError> {
        Self::from_ron_bytes(encoded.as_bytes())
    }

    pub fn from_ron_bytes_with_limits(
        encoded: &[u8],
        limits: ComponentCatalogFileLimits,
    ) -> Result<Self, ComponentCatalogFileError> {
        decode_ron(encoded, None, limits)
    }

    pub fn from_ron_bytes_with_predecessor(
        encoded: &[u8],
        predecessor: &Self,
        limits: ComponentCatalogFileLimits,
    ) -> Result<Self, ComponentCatalogFileError> {
        decode_ron(encoded, Some(predecessor), limits)
    }
}

fn encode_json(
    catalog: &ComponentSchemaCatalog,
    predecessor: Option<&ComponentSchemaCatalog>,
    limits: ComponentCatalogFileLimits,
) -> Result<String, ComponentCatalogFileError> {
    let envelope = canonical_envelope(catalog, predecessor, limits)?;
    let encoded = serde_json::to_string_pretty(&envelope).map_err(|source| {
        ComponentCatalogFileError::Encode {
            encoding: ComponentCatalogFileEncoding::Json,
            source: Box::new(source),
        }
    })?;
    validate_encoded_bytes(encoded.as_bytes(), limits)?;
    Ok(encoded)
}

fn encode_ron(
    catalog: &ComponentSchemaCatalog,
    predecessor: Option<&ComponentSchemaCatalog>,
    limits: ComponentCatalogFileLimits,
) -> Result<String, ComponentCatalogFileError> {
    let envelope = canonical_envelope(catalog, predecessor, limits)?;
    let encoded = ron::ser::to_string_pretty(
        &envelope,
        ron::ser::PrettyConfig::default().new_line("\n".to_owned()),
    )
    .map_err(|source| ComponentCatalogFileError::Encode {
        encoding: ComponentCatalogFileEncoding::Ron,
        source: Box::new(source),
    })?;
    validate_encoded_bytes(encoded.as_bytes(), limits)?;
    Ok(encoded)
}

fn canonical_envelope(
    catalog: &ComponentSchemaCatalog,
    predecessor: Option<&ComponentSchemaCatalog>,
    limits: ComponentCatalogFileLimits,
) -> Result<PersistentFileEnvelope<ComponentSchemaCatalog>, ComponentCatalogFileError> {
    let catalog = finish_catalog(catalog.clone(), predecessor, limits)?;
    Ok(PersistentFileEnvelope::canonical_v1(
        format_kind()?,
        engine_version(CANONICAL_V1_ENGINE_MIN_VERSION)?,
        FormatGenerator::new(
            FORMAT_GENERATOR_NAME,
            engine_version(env!("CARGO_PKG_VERSION"))?,
        )
        .map_err(metadata_error)?,
        catalog,
    ))
}

fn decode_json(
    encoded: &[u8],
    predecessor: Option<&ComponentSchemaCatalog>,
    limits: ComponentCatalogFileLimits,
) -> Result<ComponentSchemaCatalog, ComponentCatalogFileError> {
    let contract = file_contract()?;
    let envelope = decode_persistent_file_with_preflight(
        encoded,
        limits.encoded_bytes(),
        &contract,
        |bytes| {
            let mut deserializer = serde_json::Deserializer::from_slice(bytes);
            preflight_serde_shape(&mut deserializer, limits.shape()).map_err(JsonDecodeError::Shape)
        },
        |bytes| {
            serde_json::from_slice::<PersistentFileHeader>(bytes).map_err(JsonDecodeError::Syntax)
        },
        |bytes| {
            serde_json::from_slice::<PersistentFileEnvelope<ComponentSchemaCatalogWire>>(bytes)
                .map_err(JsonDecodeError::Syntax)
        },
    )
    .map_err(|error| map_decode_error(ComponentCatalogFileEncoding::Json, error))?;
    finish_catalog(envelope.into_payload().into(), predecessor, limits)
}

fn decode_ron(
    encoded: &[u8],
    predecessor: Option<&ComponentSchemaCatalog>,
    limits: ComponentCatalogFileLimits,
) -> Result<ComponentSchemaCatalog, ComponentCatalogFileError> {
    let contract = file_contract()?;
    let envelope = decode_persistent_file_with_preflight(
        encoded,
        limits.encoded_bytes(),
        &contract,
        |bytes| {
            let mut deserializer =
                ron::de::Deserializer::from_bytes(bytes).map_err(RonDecodeError::Syntax)?;
            preflight_serde_shape(&mut deserializer, limits.shape()).map_err(RonDecodeError::Shape)
        },
        |bytes| ron::de::from_bytes::<PersistentFileHeader>(bytes).map_err(RonDecodeError::Syntax),
        |bytes| {
            ron::de::from_bytes::<PersistentFileEnvelope<ComponentSchemaCatalogWire>>(bytes)
                .map_err(RonDecodeError::Syntax)
        },
    )
    .map_err(|error| map_decode_error(ComponentCatalogFileEncoding::Ron, error))?;
    finish_catalog(envelope.into_payload().into(), predecessor, limits)
}

fn finish_catalog(
    catalog: ComponentSchemaCatalog,
    predecessor: Option<&ComponentSchemaCatalog>,
    limits: ComponentCatalogFileLimits,
) -> Result<ComponentSchemaCatalog, ComponentCatalogFileError> {
    validate_budget(&catalog, limits)?;
    prepare_catalog_candidate(catalog, predecessor).map_err(ComponentCatalogFileError::Catalog)
}

fn validate_budget(
    catalog: &ComponentSchemaCatalog,
    limits: ComponentCatalogFileLimits,
) -> Result<(), ComponentCatalogFileBudgetError> {
    let mut counts = CatalogCounts::default();
    for schema in &catalog.components {
        increase(
            &mut counts.components,
            1,
            limits.components(),
            ComponentCatalogFileBudgetKind::Components,
        )?;
        counts.observe_diagnostic_source(limits)?;
        increase(
            &mut counts.aliases,
            schema.aliases.len(),
            limits.aliases(),
            ComponentCatalogFileBudgetKind::Aliases,
        )?;
        increase(
            &mut counts.fields,
            schema.fields.len(),
            limits.fields(),
            ComponentCatalogFileBudgetKind::Fields,
        )?;
        for field in &schema.fields {
            counts.observe_diagnostic_source(limits)?;
            increase(
                &mut counts.aliases,
                field.aliases.len(),
                limits.aliases(),
                ComponentCatalogFileBudgetKind::Aliases,
            )?;
        }
        increase(
            &mut counts.tombstones,
            schema.field_tombstones.len(),
            limits.tombstones(),
            ComponentCatalogFileBudgetKind::Tombstones,
        )?;
        for _ in &schema.field_tombstones {
            counts.observe_diagnostic_source(limits)?;
        }
    }
    increase(
        &mut counts.tombstones,
        catalog.type_tombstones.len(),
        limits.tombstones(),
        ComponentCatalogFileBudgetKind::Tombstones,
    )?;
    for _ in &catalog.type_tombstones {
        counts.observe_diagnostic_source(limits)?;
    }
    Ok(())
}

#[derive(Default)]
struct CatalogCounts {
    components: usize,
    fields: usize,
    aliases: usize,
    tombstones: usize,
    diagnostic_sources: usize,
}

impl CatalogCounts {
    fn observe_diagnostic_source(
        &mut self,
        limits: ComponentCatalogFileLimits,
    ) -> Result<(), ComponentCatalogFileBudgetError> {
        increase(
            &mut self.diagnostic_sources,
            1,
            limits.diagnostic_sources(),
            ComponentCatalogFileBudgetKind::DiagnosticSources,
        )
    }
}

fn increase(
    counter: &mut usize,
    amount: usize,
    limit: ItemLimit,
    kind: ComponentCatalogFileBudgetKind,
) -> Result<(), ComponentCatalogFileBudgetError> {
    let observed = counter.saturating_add(amount);
    if observed > limit.get() {
        return Err(ComponentCatalogFileBudgetError {
            kind,
            observed,
            maximum: limit.get(),
        });
    }
    *counter = observed;
    Ok(())
}

fn validate_encoded_bytes(
    encoded: &[u8],
    limits: ComponentCatalogFileLimits,
) -> Result<(), ComponentCatalogFileError> {
    if encoded.len() > limits.encoded_bytes().get() {
        return Err(ComponentCatalogFileError::EncodedBytesExceeded {
            observed: encoded.len(),
            maximum: limits.encoded_bytes().get(),
        });
    }
    Ok(())
}

fn map_decode_error<E>(
    encoding: ComponentCatalogFileEncoding,
    error: PersistentFileDecodeError<E>,
) -> ComponentCatalogFileError
where
    E: Error + Send + Sync + 'static,
{
    match error {
        PersistentFileDecodeError::EncodedBytesExceeded { observed, maximum } => {
            ComponentCatalogFileError::EncodedBytesExceeded { observed, maximum }
        }
        PersistentFileDecodeError::Shape(source) => ComponentCatalogFileError::Shape {
            encoding,
            source: Box::new(source),
        },
        PersistentFileDecodeError::Header(source) => ComponentCatalogFileError::Header {
            encoding,
            source: Box::new(source),
        },
        PersistentFileDecodeError::Contract(error) => ComponentCatalogFileError::Contract(error),
        PersistentFileDecodeError::Payload(source) => ComponentCatalogFileError::Payload {
            encoding,
            source: Box::new(source),
        },
    }
}

fn file_contract() -> Result<PersistentFileContract, ComponentCatalogFileError> {
    Ok(PersistentFileContract::canonical_v1(
        format_kind()?,
        engine_version(env!("CARGO_PKG_VERSION"))?,
    ))
}

fn format_kind() -> Result<FormatKind, ComponentCatalogFileError> {
    FormatKind::new(COMPONENT_SCHEMA_CATALOG_KIND).map_err(metadata_error)
}

fn engine_version(version: &str) -> Result<EngineVersion, ComponentCatalogFileError> {
    EngineVersion::parse(version).map_err(metadata_error)
}

fn metadata_error(error: impl Error + Send + Sync + 'static) -> ComponentCatalogFileError {
    ComponentCatalogFileError::Metadata(Box::new(error))
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

#[derive(Debug)]
enum RonDecodeError {
    Shape(nara_core::SerdeShapePreflightError<ron::Error>),
    Syntax(ron::error::SpannedError),
}

impl Display for RonDecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape(error) => Display::fmt(error, formatter),
            Self::Syntax(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for RonDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape(error) => Some(error),
            Self::Syntax(error) => Some(error),
        }
    }
}
