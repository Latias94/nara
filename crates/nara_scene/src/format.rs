//! Canonical scene, prefab, and standalone patch file boundaries.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io::{self, Write},
};

use nara_asset::ProjectAssetDatabase;
use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
    PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
    PersistentFileEnvelope, PersistentFileHeader, SerdeShapeLimits,
    decode_persistent_file_with_preflight, preflight_serde_shape,
};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{ComponentRegistry, ComponentValueCost};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    PrefabDocument, SceneDocument, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePatchReport, patch::ScenePatchDocumentWire,
};

const SCENE_KIND: &str = "scene";
const PREFAB_KIND: &str = "prefab";
const SCENE_PATCH_KIND: &str = "scene_patch";
const CANONICAL_V1_ENGINE_MIN_VERSION: &str = "0.1.0";
const FORMAT_GENERATOR_NAME: &str = "nara";

/// Allocation and validation-work limits for scene-owned persistent files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneFileLimits {
    encoded_bytes: ByteLimit,
    shape: SerdeShapeLimits,
    entities: ItemLimit,
    components: ItemLimit,
    prefab_instances: ItemLimit,
    patch_operations: ItemLimit,
    diagnostic_sources: ItemLimit,
    component_value_nodes: ItemLimit,
    component_value_bytes: ByteLimit,
}

impl Default for SceneFileLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(16 * 1024 * 1024)
                .expect("scene file byte limit is non-zero"),
            shape: SerdeShapeLimits::new(
                DepthLimit::new(64).expect("scene shape depth is non-zero"),
                ItemLimit::new(1_000_000).expect("scene shape node limit is non-zero"),
                ItemLimit::new(100_000).expect("scene container item limit is non-zero"),
                ByteLimit::new(1024 * 1024).expect("scene string limit is non-zero"),
                ByteLimit::new(8 * 1024 * 1024).expect("scene total string limit is non-zero"),
            ),
            entities: ItemLimit::new(100_000).expect("scene entity limit is non-zero"),
            components: ItemLimit::new(500_000).expect("scene component limit is non-zero"),
            prefab_instances: ItemLimit::new(100_000)
                .expect("scene prefab instance limit is non-zero"),
            patch_operations: ItemLimit::new(100_000)
                .expect("scene patch operation limit is non-zero"),
            diagnostic_sources: ItemLimit::new(500_000)
                .expect("scene diagnostic source limit is non-zero"),
            component_value_nodes: ItemLimit::new(5_000_000)
                .expect("scene component value node limit is non-zero"),
            component_value_bytes: ByteLimit::new(64 * 1024 * 1024)
                .expect("scene component value byte limit is non-zero"),
        }
    }
}

impl SceneFileLimits {
    #[must_use]
    pub const fn encoded_bytes(self) -> ByteLimit {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn shape(self) -> SerdeShapeLimits {
        self.shape
    }

    #[must_use]
    pub const fn entities(self) -> ItemLimit {
        self.entities
    }

    #[must_use]
    pub const fn components(self) -> ItemLimit {
        self.components
    }

    #[must_use]
    pub const fn prefab_instances(self) -> ItemLimit {
        self.prefab_instances
    }

    #[must_use]
    pub const fn patch_operations(self) -> ItemLimit {
        self.patch_operations
    }

    #[must_use]
    pub const fn diagnostic_sources(self) -> ItemLimit {
        self.diagnostic_sources
    }

    #[must_use]
    pub const fn component_value_nodes(self) -> ItemLimit {
        self.component_value_nodes
    }

    #[must_use]
    pub const fn component_value_bytes(self) -> ByteLimit {
        self.component_value_bytes
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
    pub const fn with_entities(mut self, limit: ItemLimit) -> Self {
        self.entities = limit;
        self
    }

    #[must_use]
    pub const fn with_components(mut self, limit: ItemLimit) -> Self {
        self.components = limit;
        self
    }

    #[must_use]
    pub const fn with_prefab_instances(mut self, limit: ItemLimit) -> Self {
        self.prefab_instances = limit;
        self
    }

    #[must_use]
    pub const fn with_patch_operations(mut self, limit: ItemLimit) -> Self {
        self.patch_operations = limit;
        self
    }

    #[must_use]
    pub const fn with_diagnostic_sources(mut self, limit: ItemLimit) -> Self {
        self.diagnostic_sources = limit;
        self
    }

    #[must_use]
    pub const fn with_component_value_nodes(mut self, limit: ItemLimit) -> Self {
        self.component_value_nodes = limit;
        self
    }

    #[must_use]
    pub const fn with_component_value_bytes(mut self, limit: ByteLimit) -> Self {
        self.component_value_bytes = limit;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneFileEncoding {
    Json,
    Ron,
}

impl Display for SceneFileEncoding {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json => formatter.write_str("JSON"),
            Self::Ron => formatter.write_str("RON"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneFileBudgetKind {
    Entities,
    Components,
    PrefabInstances,
    PatchOperations,
    DiagnosticSources,
    ComponentValueNodes,
    ComponentValueBytes,
}

impl Display for SceneFileBudgetKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entities => formatter.write_str("entities"),
            Self::Components => formatter.write_str("components"),
            Self::PrefabInstances => formatter.write_str("prefab instances"),
            Self::PatchOperations => formatter.write_str("patch operations"),
            Self::DiagnosticSources => formatter.write_str("diagnostic sources"),
            Self::ComponentValueNodes => formatter.write_str("component value nodes"),
            Self::ComponentValueBytes => formatter.write_str("component value bytes"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneFileBudgetError {
    pub kind: SceneFileBudgetKind,
    pub observed: usize,
    pub maximum: usize,
}

impl Display for SceneFileBudgetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "persistent scene data contains {} {}, maximum is {}",
            self.observed, self.kind, self.maximum
        )
    }
}

impl Error for SceneFileBudgetError {}

#[derive(Debug)]
pub enum SceneFormatError {
    EncodedBytesExceeded {
        observed: usize,
        maximum: usize,
    },
    Shape {
        encoding: SceneFileEncoding,
        source: Box<dyn Error + Send + Sync>,
    },
    Header {
        encoding: SceneFileEncoding,
        source: Box<dyn Error + Send + Sync>,
    },
    Contract(PersistentFileContractError),
    EmbeddedPatchFormatVersion {
        observed: u32,
        expected: u32,
    },
    Payload {
        encoding: SceneFileEncoding,
        source: Box<dyn Error + Send + Sync>,
    },
    Budget(SceneFileBudgetError),
    Encode {
        encoding: SceneFileEncoding,
        source: Box<dyn Error + Send + Sync>,
    },
    Metadata(Box<dyn Error + Send + Sync>),
}

impl Display for SceneFormatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedBytesExceeded { observed, maximum } => write!(
                formatter,
                "persistent scene file contains {observed} encoded bytes, maximum is {maximum}"
            ),
            Self::Shape { encoding, source } => {
                write!(
                    formatter,
                    "{encoding} scene file shape is invalid: {source}"
                )
            }
            Self::Header { encoding, source } => {
                write!(
                    formatter,
                    "{encoding} scene file header is invalid: {source}"
                )
            }
            Self::Contract(error) => Display::fmt(error, formatter),
            Self::EmbeddedPatchFormatVersion { observed, expected } => write!(
                formatter,
                "embedded scene patch format version is {observed}, expected {expected}"
            ),
            Self::Payload { encoding, source } => {
                write!(
                    formatter,
                    "{encoding} scene file payload is invalid: {source}"
                )
            }
            Self::Budget(error) => Display::fmt(error, formatter),
            Self::Encode { encoding, source } => {
                write!(formatter, "{encoding} scene file encoding failed: {source}")
            }
            Self::Metadata(source) => {
                write!(formatter, "scene file metadata is invalid: {source}")
            }
        }
    }
}

impl Error for SceneFormatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape { source, .. }
            | Self::Header { source, .. }
            | Self::Payload { source, .. }
            | Self::Encode { source, .. }
            | Self::Metadata(source) => Some(source.as_ref()),
            Self::Contract(error) => Some(error),
            Self::Budget(error) => Some(error),
            Self::EncodedBytesExceeded { .. } | Self::EmbeddedPatchFormatVersion { .. } => None,
        }
    }
}

impl From<SceneFileBudgetError> for SceneFormatError {
    fn from(error: SceneFileBudgetError) -> Self {
        Self::Budget(error)
    }
}

/// A scene file that passed encoded, structural, envelope, and domain-count checks.
///
/// The candidate is not authoring or runtime state until a frozen component registry validates it.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneDocumentCandidate {
    document: SceneDocument,
    limits: SceneFileLimits,
}

/// A prefab file that passed encoded, structural, envelope, and domain-count checks.
///
/// The candidate must be validated before a prefab source owner publishes it.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefabDocumentCandidate {
    document: PrefabDocument,
    limits: SceneFileLimits,
}

/// A migrated canonical scene candidate that is not yet published authoring state.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalSceneDocumentCandidate {
    document: SceneDocument,
    source_upgrade_required: bool,
}

impl CanonicalSceneDocumentCandidate {
    /// Returns the canonical value for host policy checks before semantic publication.
    #[must_use]
    pub const fn document(&self) -> &SceneDocument {
        &self.document
    }

    pub fn publish(
        self,
        registry: &ComponentRegistry,
    ) -> Result<PublishedSceneDocument, SceneFilePublicationError> {
        let diagnostics = self.document.validate_authoring(registry);
        if diagnostics.has_errors() {
            return Err(SceneFilePublicationError::new(diagnostics));
        }
        Ok(PublishedSceneDocument {
            document: self.document,
            source_upgrade_required: self.source_upgrade_required,
        })
    }
}

/// A migrated canonical prefab candidate that is not yet published authoring state.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalPrefabDocumentCandidate {
    document: PrefabDocument,
    source_upgrade_required: bool,
}

impl CanonicalPrefabDocumentCandidate {
    /// Returns the canonical value for host policy checks before semantic publication.
    #[must_use]
    pub const fn document(&self) -> &PrefabDocument {
        &self.document
    }

    pub fn publish(
        self,
        registry: &ComponentRegistry,
    ) -> Result<PublishedPrefabDocument, SceneFilePublicationError> {
        let diagnostics = self.document.instantiate().validate_authoring(registry);
        if diagnostics.has_errors() {
            return Err(SceneFilePublicationError::new(diagnostics));
        }
        Ok(PublishedPrefabDocument {
            document: self.document,
            source_upgrade_required: self.source_upgrade_required,
        })
    }
}

/// A canonical scene value validated against one frozen schema authority.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedSceneDocument {
    document: SceneDocument,
    source_upgrade_required: bool,
}

impl PublishedSceneDocument {
    #[must_use]
    pub const fn document(&self) -> &SceneDocument {
        &self.document
    }

    #[must_use]
    pub const fn source_upgrade_required(&self) -> bool {
        self.source_upgrade_required
    }

    #[must_use]
    pub fn into_document(self) -> SceneDocument {
        self.document
    }
}

/// A canonical prefab value validated against one frozen schema authority.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedPrefabDocument {
    document: PrefabDocument,
    source_upgrade_required: bool,
}

impl PublishedPrefabDocument {
    #[must_use]
    pub const fn document(&self) -> &PrefabDocument {
        &self.document
    }

    #[must_use]
    pub const fn source_upgrade_required(&self) -> bool {
        self.source_upgrade_required
    }

    #[must_use]
    pub fn into_document(self) -> PrefabDocument {
        self.document
    }
}

/// A standalone patch file that passed encoded, structural, envelope, and domain-count checks.
///
/// Patch semantics depend on the target document and are validated by the apply transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenePatchDocumentCandidate {
    document: ScenePatchDocument,
    limits: SceneFileLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneFilePublicationError {
    diagnostics: Box<DiagnosticReport>,
}

impl SceneFilePublicationError {
    pub(crate) fn new(diagnostics: DiagnosticReport) -> Self {
        Self {
            diagnostics: Box::new(diagnostics),
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    #[must_use]
    pub fn into_diagnostics(self) -> DiagnosticReport {
        *self.diagnostics
    }
}

impl Display for SceneFilePublicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "persistent scene candidate failed semantic publication with {} error(s)",
            self.diagnostics.stats().observed_errors()
        )
    }
}

impl Error for SceneFilePublicationError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneDocumentWire {
    entities: Vec<SceneEntityRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrefabDocumentWire {
    entities: Vec<SceneEntityRecord>,
}

trait SceneFilePayload: Clone + Serialize {
    type Wire: DeserializeOwned;

    const KIND: &'static str;

    fn from_wire(wire: Self::Wire) -> Self;
    fn canonicalize_payload(&mut self);
    fn validate_contract(&self) -> Result<(), SceneFormatError>;
    fn validate_budget(&self, limits: SceneFileLimits) -> Result<(), SceneFileBudgetError>;
}

impl SceneFilePayload for SceneDocument {
    type Wire = SceneDocumentWire;

    const KIND: &'static str = SCENE_KIND;

    fn from_wire(wire: Self::Wire) -> Self {
        Self {
            entities: wire.entities,
        }
    }

    fn canonicalize_payload(&mut self) {
        self.canonicalize();
    }

    fn validate_contract(&self) -> Result<(), SceneFormatError> {
        validate_embedded_patch_versions(&self.entities)
    }

    fn validate_budget(&self, limits: SceneFileLimits) -> Result<(), SceneFileBudgetError> {
        SceneFileCounts::validate_entities(&self.entities, limits)
    }
}

impl SceneFilePayload for PrefabDocument {
    type Wire = PrefabDocumentWire;

    const KIND: &'static str = PREFAB_KIND;

    fn from_wire(wire: Self::Wire) -> Self {
        Self {
            entities: wire.entities,
        }
    }

    fn canonicalize_payload(&mut self) {
        self.canonicalize();
    }

    fn validate_contract(&self) -> Result<(), SceneFormatError> {
        validate_embedded_patch_versions(&self.entities)
    }

    fn validate_budget(&self, limits: SceneFileLimits) -> Result<(), SceneFileBudgetError> {
        SceneFileCounts::validate_entities(&self.entities, limits)
    }
}

impl SceneFilePayload for ScenePatchDocument {
    type Wire = ScenePatchDocumentWire;

    const KIND: &'static str = SCENE_PATCH_KIND;

    fn from_wire(wire: Self::Wire) -> Self {
        wire.into_document()
    }

    fn canonicalize_payload(&mut self) {}

    fn validate_contract(&self) -> Result<(), SceneFormatError> {
        validate_patch_tree_version(self)
    }

    fn validate_budget(&self, limits: SceneFileLimits) -> Result<(), SceneFileBudgetError> {
        SceneFileCounts::validate_patch(self, limits)
    }
}

macro_rules! impl_scene_file_encoder {
    ($payload:ty) => {
        impl $payload {
            pub fn to_json_string(&self) -> Result<String, SceneFormatError> {
                encode_json(self, SceneFileLimits::default())
            }

            pub fn to_ron_string(&self) -> Result<String, SceneFormatError> {
                encode_ron(self, SceneFileLimits::default())
            }
        }
    };
}

macro_rules! impl_scene_file_candidate {
    ($candidate:ty, $payload:ty) => {
        impl $candidate {
            pub fn decode_json_str(input: &str) -> Result<Self, SceneFormatError> {
                Self::decode_json_bytes(input.as_bytes())
            }

            pub fn decode_ron_str(input: &str) -> Result<Self, SceneFormatError> {
                Self::decode_ron_bytes(input.as_bytes())
            }

            pub fn decode_json_bytes(input: &[u8]) -> Result<Self, SceneFormatError> {
                Self::decode_json_bytes_with_limits(input, SceneFileLimits::default())
            }

            pub fn decode_ron_bytes(input: &[u8]) -> Result<Self, SceneFormatError> {
                Self::decode_ron_bytes_with_limits(input, SceneFileLimits::default())
            }

            pub fn decode_json_bytes_with_limits(
                input: &[u8],
                limits: SceneFileLimits,
            ) -> Result<Self, SceneFormatError> {
                decode_json::<$payload>(input, limits).map(|document| Self { document, limits })
            }

            pub fn decode_ron_bytes_with_limits(
                input: &[u8],
                limits: SceneFileLimits,
            ) -> Result<Self, SceneFormatError> {
                decode_ron::<$payload>(input, limits).map(|document| Self { document, limits })
            }

            pub fn to_json_string(&self) -> Result<String, SceneFormatError> {
                self.document.to_json_string()
            }

            pub fn to_ron_string(&self) -> Result<String, SceneFormatError> {
                self.document.to_ron_string()
            }
        }
    };
}

impl_scene_file_encoder!(SceneDocument);
impl_scene_file_encoder!(PrefabDocument);
impl_scene_file_encoder!(ScenePatchDocument);
impl_scene_file_candidate!(SceneDocumentCandidate, SceneDocument);
impl_scene_file_candidate!(PrefabDocumentCandidate, PrefabDocument);
impl_scene_file_candidate!(ScenePatchDocumentCandidate, ScenePatchDocument);

pub(crate) fn validate_scene_publication_shape(
    document: &SceneDocument,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    validate_publication_shape(document, limits)
}

pub(crate) fn validate_prefab_publication_shape(
    document: &PrefabDocument,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    validate_publication_shape(document, limits)
}

pub(crate) fn validate_patch_publication_shape(
    document: &ScenePatchDocument,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    validate_publication_shape(document, limits)
}

impl SceneDocumentCandidate {
    pub fn canonicalize(
        self,
        registry: &ComponentRegistry,
    ) -> Result<CanonicalSceneDocumentCandidate, SceneFilePublicationError> {
        let (document, source_upgrade_required) = self.into_canonical_document(registry)?;
        Ok(CanonicalSceneDocumentCandidate {
            document,
            source_upgrade_required,
        })
    }

    pub fn publish(
        self,
        registry: &ComponentRegistry,
    ) -> Result<PublishedSceneDocument, SceneFilePublicationError> {
        self.canonicalize(registry)?.publish(registry)
    }

    pub(crate) fn into_canonical_document(
        self,
        registry: &ComponentRegistry,
    ) -> Result<(SceneDocument, bool), SceneFilePublicationError> {
        crate::migration::canonicalize_scene_document(self.document, registry, self.limits)
    }
}

impl PrefabDocumentCandidate {
    pub fn canonicalize(
        self,
        registry: &ComponentRegistry,
    ) -> Result<CanonicalPrefabDocumentCandidate, SceneFilePublicationError> {
        let (document, source_upgrade_required) = self.into_canonical_document(registry)?;
        Ok(CanonicalPrefabDocumentCandidate {
            document,
            source_upgrade_required,
        })
    }

    pub fn publish(
        self,
        registry: &ComponentRegistry,
    ) -> Result<PublishedPrefabDocument, SceneFilePublicationError> {
        self.canonicalize(registry)?.publish(registry)
    }

    pub(crate) fn into_canonical_document(
        self,
        registry: &ComponentRegistry,
    ) -> Result<(PrefabDocument, bool), SceneFilePublicationError> {
        crate::migration::canonicalize_prefab_document(self.document, registry, self.limits)
    }
}

impl ScenePatchDocumentCandidate {
    pub(crate) fn into_canonical_document(
        self,
        registry: &ComponentRegistry,
    ) -> Result<(ScenePatchDocument, bool), SceneFilePublicationError> {
        crate::migration::canonicalize_scene_patch(self.document, registry, self.limits)
    }

    pub fn apply_to_scene(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        match self.clone().into_canonical_document(registry) {
            Ok((patch, _)) => patch.apply_to_scene(document, registry),
            Err(error) => publication_error_patch_report(error),
        }
    }

    pub fn apply_to_scene_with_asset_database(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        match self.clone().into_canonical_document(registry) {
            Ok((patch, _)) => {
                patch.apply_to_scene_with_asset_database(document, registry, database)
            }
            Err(error) => publication_error_patch_report(error),
        }
    }
}

pub(crate) fn publication_error_patch_report(error: SceneFilePublicationError) -> ScenePatchReport {
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics: error.into_diagnostics(),
    }
}

fn encode_json<T: SceneFilePayload>(
    payload: &T,
    limits: SceneFileLimits,
) -> Result<String, SceneFormatError> {
    let envelope = canonical_envelope(payload, limits)?;
    let encoded =
        serde_json::to_string_pretty(&envelope).map_err(|source| SceneFormatError::Encode {
            encoding: SceneFileEncoding::Json,
            source: Box::new(source),
        })?;
    validate_encoded_bytes(encoded.as_bytes(), limits)?;
    Ok(encoded)
}

fn encode_ron<T: SceneFilePayload>(
    payload: &T,
    limits: SceneFileLimits,
) -> Result<String, SceneFormatError> {
    let envelope = canonical_envelope(payload, limits)?;
    let encoded =
        ron::ser::to_string_pretty(&envelope, canonical_ron_pretty_config()).map_err(|source| {
            SceneFormatError::Encode {
                encoding: SceneFileEncoding::Ron,
                source: Box::new(source),
            }
        })?;
    validate_encoded_bytes(encoded.as_bytes(), limits)?;
    Ok(encoded)
}

fn validate_publication_shape<T: SceneFilePayload>(
    payload: &T,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    let envelope = canonical_envelope(payload, limits)?;
    validate_json_publication_shape(&envelope, limits)?;
    validate_ron_publication_shape(&envelope, limits)
}

fn validate_json_publication_shape<T: Serialize>(
    envelope: &T,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    let mut encoded = BoundedEncodedBuffer::new(limits.encoded_bytes().get());
    if let Err(source) = serde_json::to_writer_pretty(&mut encoded, envelope) {
        if let Some(observed) = encoded.exceeded_at() {
            return Err(SceneFormatError::EncodedBytesExceeded {
                observed,
                maximum: limits.encoded_bytes().get(),
            });
        }
        return Err(SceneFormatError::Encode {
            encoding: SceneFileEncoding::Json,
            source: Box::new(source),
        });
    }
    let mut deserializer = serde_json::Deserializer::from_slice(encoded.as_slice());
    preflight_serde_shape(&mut deserializer, limits.shape()).map_err(|source| {
        SceneFormatError::Shape {
            encoding: SceneFileEncoding::Json,
            source: Box::new(source),
        }
    })
}

fn validate_ron_publication_shape<T: Serialize>(
    envelope: &T,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    let mut encoded = BoundedEncodedBuffer::new(limits.encoded_bytes().get());
    if let Err(source) =
        ron::ser::to_writer_pretty(&mut encoded, envelope, canonical_ron_pretty_config())
    {
        if let Some(observed) = encoded.exceeded_at() {
            return Err(SceneFormatError::EncodedBytesExceeded {
                observed,
                maximum: limits.encoded_bytes().get(),
            });
        }
        return Err(SceneFormatError::Encode {
            encoding: SceneFileEncoding::Ron,
            source: Box::new(source),
        });
    }
    let mut deserializer =
        ron::de::Deserializer::from_bytes(encoded.as_slice()).map_err(|source| {
            SceneFormatError::Shape {
                encoding: SceneFileEncoding::Ron,
                source: Box::new(source),
            }
        })?;
    preflight_serde_shape(&mut deserializer, limits.shape()).map_err(|source| {
        SceneFormatError::Shape {
            encoding: SceneFileEncoding::Ron,
            source: Box::new(source),
        }
    })
}

fn canonical_ron_pretty_config() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::default().new_line("\n".to_owned())
}

struct BoundedEncodedBuffer {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded_at: Option<usize>,
}

impl BoundedEncodedBuffer {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(maximum.min(64 * 1024)),
            maximum,
            exceeded_at: None,
        }
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    fn exceeded_at(&self) -> Option<usize> {
        self.exceeded_at
    }
}

impl Write for BoundedEncodedBuffer {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let observed = self.bytes.len().saturating_add(buffer.len());
        if observed > self.maximum {
            self.exceeded_at = Some(observed);
            return Err(io::Error::other("persistent encoded byte limit exceeded"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn canonical_envelope<T: SceneFilePayload>(
    payload: &T,
    limits: SceneFileLimits,
) -> Result<PersistentFileEnvelope<T>, SceneFormatError> {
    let mut payload = payload.clone();
    payload.canonicalize_payload();
    payload.validate_contract()?;
    payload.validate_budget(limits)?;
    Ok(PersistentFileEnvelope::canonical_v1(
        format_kind(T::KIND)?,
        engine_version(CANONICAL_V1_ENGINE_MIN_VERSION)?,
        FormatGenerator::new(
            FORMAT_GENERATOR_NAME,
            engine_version(env!("CARGO_PKG_VERSION"))?,
        )
        .map_err(metadata_error)?,
        payload,
    ))
}

fn decode_json<T: SceneFilePayload>(
    encoded: &[u8],
    limits: SceneFileLimits,
) -> Result<T, SceneFormatError> {
    let contract = file_contract(T::KIND)?;
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
            serde_json::from_slice::<PersistentFileEnvelope<T::Wire>>(bytes)
                .map_err(JsonDecodeError::Syntax)
        },
    )
    .map_err(|error| map_decode_error(SceneFileEncoding::Json, error))?;
    finish_decode(T::from_wire(envelope.into_payload()), limits)
}

fn decode_ron<T: SceneFilePayload>(
    encoded: &[u8],
    limits: SceneFileLimits,
) -> Result<T, SceneFormatError> {
    let contract = file_contract(T::KIND)?;
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
            ron::de::from_bytes::<PersistentFileEnvelope<T::Wire>>(bytes)
                .map_err(RonDecodeError::Syntax)
        },
    )
    .map_err(|error| map_decode_error(SceneFileEncoding::Ron, error))?;
    finish_decode(T::from_wire(envelope.into_payload()), limits)
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

fn finish_decode<T: SceneFilePayload>(
    mut payload: T,
    limits: SceneFileLimits,
) -> Result<T, SceneFormatError> {
    payload.canonicalize_payload();
    payload.validate_contract()?;
    payload.validate_budget(limits)?;
    Ok(payload)
}

fn validate_embedded_patch_versions(
    entities: &[SceneEntityRecord],
) -> Result<(), SceneFormatError> {
    for entity in entities {
        if let Some(prefab) = &entity.prefab {
            validate_patch_tree_version(&prefab.overrides)?;
        }
    }
    Ok(())
}

fn validate_patch_tree_version(patch: &ScenePatchDocument) -> Result<(), SceneFormatError> {
    let Some(observed) = patch.unsupported_format_version() else {
        return Ok(());
    };
    Err(SceneFormatError::EmbeddedPatchFormatVersion {
        observed,
        expected: ScenePatchDocument::CURRENT_FORMAT_VERSION,
    })
}

fn validate_encoded_bytes(encoded: &[u8], limits: SceneFileLimits) -> Result<(), SceneFormatError> {
    if encoded.len() > limits.encoded_bytes().get() {
        return Err(SceneFormatError::EncodedBytesExceeded {
            observed: encoded.len(),
            maximum: limits.encoded_bytes().get(),
        });
    }
    Ok(())
}

fn map_decode_error<E>(
    encoding: SceneFileEncoding,
    error: PersistentFileDecodeError<E>,
) -> SceneFormatError
where
    E: Error + Send + Sync + 'static,
{
    match error {
        PersistentFileDecodeError::EncodedBytesExceeded { observed, maximum } => {
            SceneFormatError::EncodedBytesExceeded { observed, maximum }
        }
        PersistentFileDecodeError::Shape(source) => SceneFormatError::Shape {
            encoding,
            source: Box::new(source),
        },
        PersistentFileDecodeError::Header(source) => SceneFormatError::Header {
            encoding,
            source: Box::new(source),
        },
        PersistentFileDecodeError::Contract(error) => SceneFormatError::Contract(error),
        PersistentFileDecodeError::Payload(source) => SceneFormatError::Payload {
            encoding,
            source: Box::new(source),
        },
    }
}

fn file_contract(kind: &str) -> Result<PersistentFileContract, SceneFormatError> {
    Ok(PersistentFileContract::canonical_v1(
        format_kind(kind)?,
        engine_version(env!("CARGO_PKG_VERSION"))?,
    ))
}

fn format_kind(kind: &str) -> Result<FormatKind, SceneFormatError> {
    FormatKind::new(kind).map_err(metadata_error)
}

fn engine_version(version: &str) -> Result<EngineVersion, SceneFormatError> {
    EngineVersion::parse(version).map_err(metadata_error)
}

fn metadata_error(error: impl Error + Send + Sync + 'static) -> SceneFormatError {
    SceneFormatError::Metadata(Box::new(error))
}

#[derive(Default)]
struct SceneFileCounts {
    entities: usize,
    components: usize,
    prefab_instances: usize,
    patch_operations: usize,
    diagnostic_sources: usize,
    component_value_cost: ComponentValueCost,
}

impl SceneFileCounts {
    fn validate_entities(
        entities: &[SceneEntityRecord],
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        let mut counts = Self::default();
        for entity in entities {
            counts.observe_entity(entity, limits)?;
        }
        Ok(())
    }

    fn validate_patch(
        patch: &ScenePatchDocument,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        let mut counts = Self::default();
        counts.observe_patch(patch, limits)
    }

    fn observe_entity(
        &mut self,
        entity: &SceneEntityRecord,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        increase(
            &mut self.entities,
            1,
            limits.entities(),
            SceneFileBudgetKind::Entities,
        )?;
        self.observe_diagnostic_source(limits)?;
        increase(
            &mut self.components,
            entity.components.len(),
            limits.components(),
            SceneFileBudgetKind::Components,
        )?;
        for _ in &entity.components {
            self.observe_diagnostic_source(limits)?;
        }
        for component in entity.components.values() {
            self.observe_component_value(component.value.cost(), limits)?;
        }
        if let Some(prefab) = &entity.prefab {
            increase(
                &mut self.prefab_instances,
                1,
                limits.prefab_instances(),
                SceneFileBudgetKind::PrefabInstances,
            )?;
            self.observe_diagnostic_source(limits)?;
            self.observe_patch(&prefab.overrides, limits)?;
        }
        Ok(())
    }

    fn observe_patch(
        &mut self,
        patch: &ScenePatchDocument,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        for operation in &patch.operations {
            increase(
                &mut self.patch_operations,
                1,
                limits.patch_operations(),
                SceneFileBudgetKind::PatchOperations,
            )?;
            self.observe_diagnostic_source(limits)?;
            match operation {
                ScenePatchOperation::AddEntity { entity } => {
                    self.observe_entity(entity, limits)?;
                }
                ScenePatchOperation::AddComponent { value, .. }
                | ScenePatchOperation::ReplaceComponent { value, .. } => {
                    increase(
                        &mut self.components,
                        1,
                        limits.components(),
                        SceneFileBudgetKind::Components,
                    )?;
                    self.observe_diagnostic_source(limits)?;
                    self.observe_component_value(value.value.cost(), limits)?;
                }
                ScenePatchOperation::SetField { value, .. } => {
                    self.observe_component_value(value.cost(), limits)?;
                }
                ScenePatchOperation::RemoveEntity { .. }
                | ScenePatchOperation::RemoveComponent { .. }
                | ScenePatchOperation::RemoveField { .. }
                | ScenePatchOperation::Reparent { .. }
                | ScenePatchOperation::SetAssetRefField { .. } => {}
            }
        }
        Ok(())
    }

    fn observe_diagnostic_source(
        &mut self,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        increase(
            &mut self.diagnostic_sources,
            1,
            limits.diagnostic_sources(),
            SceneFileBudgetKind::DiagnosticSources,
        )
    }

    fn observe_component_value(
        &mut self,
        cost: ComponentValueCost,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        let observed = self.component_value_cost.saturating_add(cost);
        for (kind, observed, maximum) in [
            (
                SceneFileBudgetKind::ComponentValueNodes,
                observed.nodes(),
                limits.component_value_nodes().get(),
            ),
            (
                SceneFileBudgetKind::ComponentValueBytes,
                observed.logical_bytes(),
                limits.component_value_bytes().get(),
            ),
        ] {
            if observed > maximum {
                return Err(SceneFileBudgetError {
                    kind,
                    observed,
                    maximum,
                });
            }
        }
        self.component_value_cost = observed;
        Ok(())
    }
}

fn increase(
    counter: &mut usize,
    amount: usize,
    limit: ItemLimit,
    kind: SceneFileBudgetKind,
) -> Result<(), SceneFileBudgetError> {
    let observed = counter.saturating_add(amount);
    if observed > limit.get() {
        return Err(SceneFileBudgetError {
            kind,
            observed,
            maximum: limit.get(),
        });
    }
    *counter = observed;
    Ok(())
}
