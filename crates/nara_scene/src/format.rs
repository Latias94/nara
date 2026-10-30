//! Canonical scene, prefab, and standalone patch file boundaries.

#[cfg(test)]
mod diagnostic_tests;
mod publication_shape;
#[cfg(test)]
mod publication_shape_tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    io::{self, Write},
};

use nara_asset::AssetReferenceResolver;
use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
    PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
    PersistentFileEnvelope, PersistentFileHeader, SerdeShapeLimits,
    decode_persistent_file_with_preflight, preflight_serde_shape,
};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{
    ComponentDecodeContext, ComponentRegistry, ComponentValueCost,
    ComponentValueMaterializationLimits,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    patch::ScenePatchDocumentWire,
    prefab::{
        PrefabOverlayCostError, PrefabOverlayCostKind, PrefabOverlayCostLimits,
        prefab_instance_cost,
    },
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
    placements: ItemLimit,
    placement_depth: DepthLimit,
    components: ItemLimit,
    prefab_instances: ItemLimit,
    patch_operations: ItemLimit,
    overlay_declarations: ItemLimit,
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
            placements: ItemLimit::new(100_000).expect("scene placement limit is non-zero"),
            placement_depth: DepthLimit::new(1_024)
                .expect("scene placement depth limit is non-zero"),
            components: ItemLimit::new(500_000).expect("scene component limit is non-zero"),
            prefab_instances: ItemLimit::new(100_000)
                .expect("scene prefab instance limit is non-zero"),
            patch_operations: ItemLimit::new(100_000)
                .expect("scene patch operation limit is non-zero"),
            overlay_declarations: ItemLimit::new(100_000)
                .expect("scene overlay declaration limit is non-zero"),
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
    pub(crate) const fn component_materialization_limits(
        self,
    ) -> ComponentValueMaterializationLimits {
        ComponentValueMaterializationLimits::new(
            self.component_value_nodes(),
            self.component_value_bytes(),
            self.shape().depth(),
        )
    }

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
    pub const fn placements(self) -> ItemLimit {
        self.placements
    }

    #[must_use]
    pub const fn placement_depth(self) -> DepthLimit {
        self.placement_depth
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

    /// Raw overlays, declarations, property claims and reference bindings across all instances.
    #[must_use]
    pub const fn overlay_declarations(self) -> ItemLimit {
        self.overlay_declarations
    }

    #[must_use]
    pub const fn diagnostic_sources(self) -> ItemLimit {
        self.diagnostic_sources
    }

    #[must_use]
    pub const fn component_value_nodes(self) -> ItemLimit {
        self.component_value_nodes
    }

    /// Aggregate logical payload bytes, including raw overlay identities and record metadata.
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
    pub const fn with_placements(mut self, limit: ItemLimit) -> Self {
        self.placements = limit;
        self
    }

    #[must_use]
    pub const fn with_placement_depth(mut self, limit: DepthLimit) -> Self {
        self.placement_depth = limit;
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
    pub const fn with_overlay_declarations(mut self, limit: ItemLimit) -> Self {
        self.overlay_declarations = limit;
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
    Placements,
    PlacementDepth,
    Components,
    PrefabInstances,
    PatchOperations,
    OverlayDeclarations,
    DiagnosticSources,
    ComponentValueNodes,
    ComponentValueBytes,
}

impl Display for SceneFileBudgetKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entities => formatter.write_str("entities"),
            Self::Placements => formatter.write_str("placement references"),
            Self::PlacementDepth => formatter.write_str("placement depth"),
            Self::Components => formatter.write_str("components"),
            Self::PrefabInstances => formatter.write_str("prefab instances"),
            Self::PatchOperations => formatter.write_str("patch operations"),
            Self::OverlayDeclarations => formatter.write_str("overlay declarations"),
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
    PatchFormatVersion {
        observed: u32,
        expected: u32,
    },
    LegacyPlacementShape {
        entity_index: Option<usize>,
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
            Self::PatchFormatVersion { observed, expected } => write!(
                formatter,
                "scene patch format version is {observed}, expected {expected}"
            ),
            Self::LegacyPlacementShape { entity_index } => {
                if let Some(entity_index) = entity_index {
                    write!(
                        formatter,
                        "persistent entity document uses the removed record-owned parent shape at entity index {entity_index}; rewrite it with document roots and parent-owned children because sibling order cannot be inferred"
                    )?;
                } else {
                    formatter.write_str(
                        "persistent entity document is missing its ordered roots; rewrite it with document roots and parent-owned children because sibling order cannot be inferred",
                    )?;
                }
                Ok(())
            }
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
            Self::EncodedBytesExceeded { .. }
            | Self::PatchFormatVersion { .. }
            | Self::LegacyPlacementShape { .. } => None,
        }
    }
}

impl SceneFormatError {
    /// Returns a safe parser location without exposing parser messages or document contents.
    #[must_use]
    pub fn parser_location(&self) -> Option<(usize, usize)> {
        let mut source = self.parser_error();
        while let Some(error) = source {
            if let Some(error) = error.downcast_ref::<serde_json::Error>() {
                return Some((error.line(), error.column()));
            }
            if let Some(error) = error.downcast_ref::<ron::error::SpannedError>() {
                return Some((error.position.line, error.position.col));
            }
            source = error.source();
        }
        None
    }

    /// Returns whether a structural/header/payload parser failure was a syntax error.
    #[must_use]
    pub fn is_syntax_error(&self) -> bool {
        let mut source = self.parser_error();
        while let Some(error) = source {
            if error
                .downcast_ref::<serde_json::Error>()
                .is_some_and(|error| error.is_syntax() || error.is_eof())
                || error
                    .downcast_ref::<ron::error::SpannedError>()
                    .is_some_and(|error| {
                        // RON also wraps Serde data errors in SpannedError. Only lexical and delimiter
                        // failures establish invalid syntax; type/field errors retain their decode stage.
                        matches!(
                            error.code,
                            ron::error::Error::Eof
                                | ron::error::Error::ExpectedArrayEnd
                                | ron::error::Error::ExpectedAttribute
                                | ron::error::Error::ExpectedAttributeEnd
                                | ron::error::Error::ExpectedChar
                                | ron::error::Error::ExpectedComma
                                | ron::error::Error::FloatUnderscore
                                | ron::error::Error::ExpectedOptionEnd
                                | ron::error::Error::ExpectedMapColon
                                | ron::error::Error::ExpectedMapEnd
                                | ron::error::Error::ExpectedStructLikeEnd
                                | ron::error::Error::ExpectedStringEnd
                                | ron::error::Error::InvalidEscape(_)
                                | ron::error::Error::UnclosedBlockComment
                                | ron::error::Error::UnderscoreAtBeginning
                                | ron::error::Error::UnexpectedByte(_)
                                | ron::error::Error::Utf8Error(_)
                                | ron::error::Error::TrailingCharacters
                                | ron::error::Error::InvalidIdentifier(_)
                                | ron::error::Error::SuggestRawIdentifier(_)
                        )
                    })
            {
                return true;
            }
            source = error.source();
        }
        false
    }

    /// Returns a typed structural budget failure, if one caused parsing to stop.
    #[must_use]
    pub fn shape_error(&self) -> Option<&nara_core::SerdeShapeError> {
        let mut source = self.parser_error();
        while let Some(error) = source {
            if let Some(shape) = error.downcast_ref::<nara_core::SerdeShapeError>() {
                return Some(shape);
            }
            source = error.source();
        }
        None
    }

    fn parser_error(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape { source, .. }
            | Self::Header { source, .. }
            | Self::Payload { source, .. } => Some(source.as_ref()),
            Self::EncodedBytesExceeded { .. }
            | Self::Contract(_)
            | Self::PatchFormatVersion { .. }
            | Self::LegacyPlacementShape { .. }
            | Self::Budget(_)
            | Self::Encode { .. }
            | Self::Metadata(_) => None,
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

/// Bounded Prefab declarations awaiting source-local semantic validation.
///
/// File decoding checks the envelope, encoded shape and domain limits. Reusing an owned
/// document checks domain limits without a wire round trip. Neither path validates component
/// values or grants standalone authoring or runtime publication.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefabDocumentCandidate {
    document: PrefabDocument,
    limits: SceneFileLimits,
}

/// A migrated canonical scene candidate that is not yet published authoring state.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalSceneDocumentCandidate {
    document: SceneDocument,
    limits: SceneFileLimits,
    source_upgrade_required: bool,
}

impl CanonicalSceneDocumentCandidate {
    /// Consumes bounded canonical source declarations before selected Prefab activation.
    ///
    /// Returns whether direct source normalization changed the stored representation. Overlay
    /// intent remains unactivated. The Host must still capture sources and compile/validate the
    /// selected effective candidate; this value does not authorize runtime materialization.
    #[must_use]
    pub fn into_source_document(self) -> (SceneDocument, bool) {
        (self.document, self.source_upgrade_required)
    }

    /// Returns the canonical value for host policy checks before semantic publication.
    #[must_use]
    pub const fn document(&self) -> &SceneDocument {
        &self.document
    }

    pub fn publish(
        self,
        registry: &ComponentRegistry,
    ) -> Result<PublishedSceneDocument, SceneFilePublicationError> {
        self.publish_with_optional_asset_resolution(registry, None)
    }

    /// Publishes with the asset identities admitted by the content source owner.
    pub fn publish_with_asset_resolution(
        self,
        registry: &ComponentRegistry,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> Result<PublishedSceneDocument, SceneFilePublicationError> {
        self.publish_with_optional_asset_resolution(registry, Some(asset_resolution))
    }

    fn publish_with_optional_asset_resolution(
        self,
        registry: &ComponentRegistry,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
    ) -> Result<PublishedSceneDocument, SceneFilePublicationError> {
        let mut context = ComponentDecodeContext::new()
            .with_materialization_limits(self.limits.component_materialization_limits());
        if let Some(asset_resolution) = asset_resolution {
            context = context.with_asset_resolution(asset_resolution);
        }
        let diagnostics = crate::validation::preflight_authoring_scene_with_context(
            &self.document,
            registry,
            &mut context,
        )
        .diagnostics;
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
    limits: SceneFileLimits,
    source_upgrade_required: bool,
}

impl CanonicalPrefabDocumentCandidate {
    /// Consumes bounded canonical source declarations without activating their nested overlays.
    ///
    /// The boolean reports direct source normalization. Source identity, actual dependency
    /// capture and selected effective validation remain the consuming Host/compiler's obligations.
    #[must_use]
    pub fn into_source_document(self) -> (PrefabDocument, bool) {
        (self.document, self.source_upgrade_required)
    }

    /// Returns the canonical value for host policy checks before semantic publication.
    #[must_use]
    pub const fn document(&self) -> &PrefabDocument {
        &self.document
    }

    pub fn publish(
        self,
        registry: &ComponentRegistry,
    ) -> Result<PublishedPrefabDocument, SceneFilePublicationError> {
        self.publish_with_optional_asset_resolution(registry, None)
    }

    /// Publishes with the asset identities admitted by the content source owner.
    pub fn publish_with_asset_resolution(
        self,
        registry: &ComponentRegistry,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> Result<PublishedPrefabDocument, SceneFilePublicationError> {
        self.publish_with_optional_asset_resolution(registry, Some(asset_resolution))
    }

    fn publish_with_optional_asset_resolution(
        self,
        registry: &ComponentRegistry,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
    ) -> Result<PublishedPrefabDocument, SceneFilePublicationError> {
        let document = self.document.instantiate();
        let mut context = ComponentDecodeContext::new()
            .with_materialization_limits(self.limits.component_materialization_limits());
        if let Some(asset_resolution) = asset_resolution {
            context = context.with_asset_resolution(asset_resolution);
        }
        let diagnostics = crate::validation::preflight_authoring_scene_with_context(
            &document,
            registry,
            &mut context,
        )
        .diagnostics;
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
    #[serde(default)]
    roots: OrderedRootsField,
    entities: Vec<SceneEntityRecordWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrefabDocumentWire {
    #[serde(default)]
    roots: OrderedRootsField,
    entities: Vec<SceneEntityRecordWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneEntityRecordWire {
    id: SceneEntityId,
    #[serde(default)]
    children: Vec<SceneEntityId>,
    #[serde(default)]
    components: BTreeMap<nara_reflect::ComponentTypeId, SceneComponentRecord>,
    #[serde(default)]
    prefab: Option<PrefabInstance>,
    #[serde(default, rename = "parent")]
    legacy_parent: LegacyParentField,
}

#[derive(Default)]
struct LegacyParentField {
    present: bool,
}

#[derive(Default)]
struct OrderedRootsField {
    value: Option<Vec<SceneEntityId>>,
}

impl<'de> Deserialize<'de> for OrderedRootsField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self {
            value: Some(Vec::<SceneEntityId>::deserialize(deserializer)?),
        })
    }
}

impl<'de> Deserialize<'de> for LegacyParentField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let _ = serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self { present: true })
    }
}

impl SceneEntityRecordWire {
    fn into_record(self, entity_index: usize) -> Result<SceneEntityRecord, SceneFormatError> {
        if self.legacy_parent.present {
            return Err(SceneFormatError::LegacyPlacementShape {
                entity_index: Some(entity_index),
            });
        }
        Ok(SceneEntityRecord {
            id: self.id,
            children: self.children,
            components: self.components,
            prefab: self.prefab,
        })
    }
}

fn document_wire_parts(
    roots: OrderedRootsField,
    entities: Vec<SceneEntityRecordWire>,
) -> Result<(Vec<SceneEntityId>, Vec<SceneEntityRecord>), SceneFormatError> {
    let entities = entities
        .into_iter()
        .enumerate()
        .map(|(entity_index, entity)| entity.into_record(entity_index))
        .collect::<Result<Vec<_>, _>>()?;
    let Some(roots) = roots.value else {
        return Err(SceneFormatError::LegacyPlacementShape { entity_index: None });
    };
    Ok((roots, entities))
}

trait SceneFilePayload: Clone + Serialize {
    type Wire: DeserializeOwned;

    const KIND: &'static str;

    fn from_wire(wire: Self::Wire) -> Result<Self, SceneFormatError>;
    fn canonicalize_payload(&mut self);
    fn validate_contract(&self) -> Result<(), SceneFormatError>;
    fn validate_budget(&self, limits: SceneFileLimits) -> Result<(), SceneFileBudgetError>;

    fn entity_document(&self) -> Option<(&[SceneEntityId], &[SceneEntityRecord])> {
        None
    }
}

impl SceneFilePayload for SceneDocument {
    type Wire = SceneDocumentWire;

    const KIND: &'static str = SCENE_KIND;

    fn from_wire(wire: Self::Wire) -> Result<Self, SceneFormatError> {
        let (roots, entities) = document_wire_parts(wire.roots, wire.entities)?;
        Ok(Self { roots, entities })
    }

    fn canonicalize_payload(&mut self) {
        self.canonicalize();
    }

    fn validate_contract(&self) -> Result<(), SceneFormatError> {
        Ok(())
    }

    fn validate_budget(&self, limits: SceneFileLimits) -> Result<(), SceneFileBudgetError> {
        validate_scene_value_budget(self, limits)
    }

    fn entity_document(&self) -> Option<(&[SceneEntityId], &[SceneEntityRecord])> {
        Some((&self.roots, &self.entities))
    }
}

impl SceneFilePayload for PrefabDocument {
    type Wire = PrefabDocumentWire;

    const KIND: &'static str = PREFAB_KIND;

    fn from_wire(wire: Self::Wire) -> Result<Self, SceneFormatError> {
        let (roots, entities) = document_wire_parts(wire.roots, wire.entities)?;
        Ok(Self { roots, entities })
    }

    fn canonicalize_payload(&mut self) {
        self.canonicalize();
    }

    fn validate_contract(&self) -> Result<(), SceneFormatError> {
        Ok(())
    }

    fn validate_budget(&self, limits: SceneFileLimits) -> Result<(), SceneFileBudgetError> {
        SceneFileCounts::validate_entities(&self.roots, &self.entities, limits)
    }

    fn entity_document(&self) -> Option<(&[SceneEntityId], &[SceneEntityRecord])> {
        Some((&self.roots, &self.entities))
    }
}

impl SceneFilePayload for ScenePatchDocument {
    type Wire = ScenePatchDocumentWire;

    const KIND: &'static str = SCENE_PATCH_KIND;

    fn from_wire(wire: Self::Wire) -> Result<Self, SceneFormatError> {
        Ok(wire.into_document())
    }

    fn canonicalize_payload(&mut self) {}

    fn validate_contract(&self) -> Result<(), SceneFormatError> {
        validate_patch_version(self)
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

pub(crate) fn validate_scene_value_budget(
    document: &SceneDocument,
    limits: SceneFileLimits,
) -> Result<(), SceneFileBudgetError> {
    SceneFileCounts::validate_entities(&document.roots, &document.entities, limits)
}

pub(crate) fn validate_prefab_publication_shape(
    document: &PrefabDocument,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    validate_publication_shape(document, limits)
}

#[derive(Debug)]
pub(crate) enum PublicationShapeAdmissionError<E> {
    Format(SceneFormatError),
    Admission(E),
}

impl<E> Display for PublicationShapeAdmissionError<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => Display::fmt(error, formatter),
            Self::Admission(_) => formatter.write_str("publication buffer admission refused"),
        }
    }
}

pub(crate) fn validate_scene_publication_shape_with_admission<E>(
    document: &SceneDocument,
    limits: SceneFileLimits,
    admission: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), PublicationShapeAdmissionError<E>> {
    validate_publication_shape_with_admission(document, limits, admission)
}

pub(crate) fn validate_prefab_publication_shape_with_admission<E>(
    document: &PrefabDocument,
    limits: SceneFileLimits,
    admission: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), PublicationShapeAdmissionError<E>> {
    validate_publication_shape_with_admission(document, limits, admission)
}

pub(crate) fn validate_patch_publication_shape(
    document: &ScenePatchDocument,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    validate_publication_shape(document, limits)
}

impl SceneDocumentCandidate {
    /// Consumes bounded decoded declarations for deferred mixed Scene/Prefab composition.
    ///
    /// Decoding already enforced the selected file and decoded-value limits. This step checks
    /// the local hierarchy without interpreting component payloads or stored overlays. The
    /// consumer must retain source evidence and select the exact Schema basis for compilation.
    /// Success does not establish standalone publication or runtime materialization readiness.
    pub fn into_composition_source(self) -> Result<SceneDocument, SceneFilePublicationError> {
        let diagnostics = crate::validation::validate_scene_structure(&self.document);
        if diagnostics.has_errors() {
            return Err(SceneFilePublicationError::new(diagnostics));
        }
        Ok(self.document)
    }

    pub(crate) fn into_authoring_parts(self) -> (SceneDocument, SceneFileLimits) {
        (self.document, self.limits)
    }

    pub fn canonicalize(
        self,
        registry: &ComponentRegistry,
    ) -> Result<CanonicalSceneDocumentCandidate, SceneFilePublicationError> {
        let limits = self.limits;
        let (document, source_upgrade_required) = self.into_canonical_document(registry)?;
        Ok(CanonicalSceneDocumentCandidate {
            document,
            limits,
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
    /// Reuses an owned source document without a wire round trip.
    ///
    /// This checks domain limits, not encoded bytes or wire shape. A captured source retains
    /// that evidence from its original decoding. Standalone authoring publication must still
    /// pass `admit_authoring_with_basis`; native publication requires registry validation.
    #[doc(hidden)]
    pub fn from_source_document(
        document: PrefabDocument,
        limits: SceneFileLimits,
    ) -> Result<Self, SceneFormatError> {
        Ok(Self {
            document: finish_decode(document, limits)?,
            limits,
        })
    }

    /// Consumes bounded decoded declarations for deferred composition.
    ///
    /// File decoding already enforces its selected file limits; an owned-document candidate
    /// retains only its domain checks here. This step validates the local hierarchy without
    /// interpreting component payloads or stored overlays. The Host retains source evidence
    /// and the exact Schema basis for deferred compilation. Successful composition admission
    /// does not establish standalone publishability or editable source state.
    #[doc(hidden)]
    pub fn into_composition_source(self) -> Result<PrefabDocument, SceneFilePublicationError> {
        let document = SceneDocument {
            roots: self.document.roots,
            entities: self.document.entities,
        };
        let diagnostics = crate::validation::validate_scene_structure(&document);
        if diagnostics.has_errors() {
            return Err(SceneFilePublicationError::new(diagnostics));
        }
        Ok(PrefabDocument {
            roots: document.roots,
            entities: document.entities,
        })
    }

    pub(crate) fn into_authoring_parts(self) -> (PrefabDocument, SceneFileLimits) {
        (self.document, self.limits)
    }

    pub fn canonicalize(
        self,
        registry: &ComponentRegistry,
    ) -> Result<CanonicalPrefabDocumentCandidate, SceneFilePublicationError> {
        let limits = self.limits;
        let (document, source_upgrade_required) = self.into_canonical_document(registry)?;
        Ok(CanonicalPrefabDocumentCandidate {
            document,
            limits,
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

    pub fn apply_to_scene_with_asset_resolution(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> ScenePatchReport {
        match self.clone().into_canonical_document(registry) {
            Ok((patch, _)) => {
                patch.apply_to_scene_with_asset_resolution(document, registry, asset_resolution)
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
    let certified = envelope
        .payload()
        .entity_document()
        .is_some_and(|(roots, entities)| {
            publication_shape::certifies_entity_document(&envelope, roots, entities, limits.shape())
        });
    validate_json_publication_shape(&envelope, limits, certified, None)?;
    validate_ron_publication_shape(&envelope, limits, certified, None)
}

fn validate_publication_shape_with_admission<T: SceneFilePayload, E>(
    payload: &T,
    limits: SceneFileLimits,
    mut admission: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), PublicationShapeAdmissionError<E>> {
    use PublicationShapeAdmissionError::{Admission, Format};
    payload.validate_contract().map_err(Format)?;
    payload
        .validate_budget(limits)
        .map_err(|error| Format(error.into()))?;
    // The owning caller has already ordered its candidate. Shape checks borrow it;
    // serializing an envelope does not require a second canonical document copy.
    let envelope = publication_envelope(payload, T::KIND).map_err(Format)?;
    let certified = payload.entity_document().is_some_and(|(roots, entities)| {
        publication_shape::certifies_entity_document(&envelope, roots, entities, limits.shape())
    });
    let mut refusal = None;
    let mut admit = |bytes| {
        admission(bytes).map_err(|error| {
            refusal = Some(error);
            io::Error::other("publication buffer admission refused")
        })
    };
    let result = validate_json_publication_shape(&envelope, limits, certified, Some(&mut admit))
        .and_then(|()| {
            validate_ron_publication_shape(&envelope, limits, certified, Some(&mut admit))
        });
    match refusal {
        Some(error) => Err(Admission(error)),
        None => result.map_err(Format),
    }
}

fn validate_json_publication_shape<T: Serialize>(
    envelope: &T,
    limits: SceneFileLimits,
    certified: bool,
    admission: Option<&mut dyn FnMut(usize) -> io::Result<()>>,
) -> Result<(), SceneFormatError> {
    let mut encoded =
        BoundedEncodedBuffer::new(limits.encoded_bytes().get(), !certified, admission);
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
    if certified {
        return Ok(());
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
    certified: bool,
    admission: Option<&mut dyn FnMut(usize) -> io::Result<()>>,
) -> Result<(), SceneFormatError> {
    let mut encoded =
        BoundedEncodedBuffer::new(limits.encoded_bytes().get(), !certified, admission);
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
    if certified {
        return Ok(());
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

struct BoundedEncodedBuffer<'a> {
    bytes: Vec<u8>,
    retain_bytes: bool,
    written: usize,
    maximum: usize,
    exceeded_at: Option<usize>,
    admission: Option<&'a mut dyn FnMut(usize) -> io::Result<()>>,
}

impl<'a> BoundedEncodedBuffer<'a> {
    fn new(
        maximum: usize,
        retain_bytes: bool,
        admission: Option<&'a mut dyn FnMut(usize) -> io::Result<()>>,
    ) -> Self {
        Self {
            bytes: if retain_bytes && admission.is_none() {
                Vec::with_capacity(maximum.min(64 * 1024))
            } else {
                Vec::new()
            },
            retain_bytes,
            written: 0,
            maximum,
            exceeded_at: None,
            admission,
        }
    }

    fn as_slice(&self) -> &[u8] {
        debug_assert!(self.retain_bytes);
        &self.bytes
    }

    fn exceeded_at(&self) -> Option<usize> {
        self.exceeded_at
    }
}

impl Write for BoundedEncodedBuffer<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let observed = self.written.saturating_add(buffer.len());
        if observed > self.maximum {
            self.exceeded_at = Some(observed);
            return Err(io::Error::other("persistent encoded byte limit exceeded"));
        }
        if self.retain_bytes {
            if let Some(admission) = &mut self.admission {
                admission(buffer.len())?;
            }
            self.bytes.extend_from_slice(buffer);
        }
        self.written = observed;
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
    // Admit caller-owned raw declarations before Clone or serializer recursion can copy
    // rejected payloads. This includes superseded claims and both exact snapshot sides.
    payload.validate_contract()?;
    payload.validate_budget(limits)?;
    let mut payload = payload.clone();
    payload.canonicalize_payload();
    publication_envelope(payload, T::KIND)
}

fn publication_envelope<T>(
    payload: T,
    kind: &'static str,
) -> Result<PersistentFileEnvelope<T>, SceneFormatError> {
    Ok(PersistentFileEnvelope::canonical_v1(
        format_kind(kind)?,
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
    finish_decode(T::from_wire(envelope.into_payload())?, limits)
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
            preflight_serde_shape(&mut deserializer, limits.shape()).map_err(|error| match error {
                nara_core::SerdeShapePreflightError::Shape(error) => RonDecodeError::Shape(error),
                nara_core::SerdeShapePreflightError::Parse(error) => {
                    RonDecodeError::Syntax(deserializer.span_error(error))
                }
            })
        },
        |bytes| ron::de::from_bytes::<PersistentFileHeader>(bytes).map_err(RonDecodeError::Syntax),
        |bytes| {
            ron::de::from_bytes::<PersistentFileEnvelope<T::Wire>>(bytes)
                .map_err(RonDecodeError::Syntax)
        },
    )
    .map_err(|error| map_decode_error(SceneFileEncoding::Ron, error))?;
    finish_decode(T::from_wire(envelope.into_payload())?, limits)
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
    Shape(nara_core::SerdeShapeError),
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

fn validate_patch_version(patch: &ScenePatchDocument) -> Result<(), SceneFormatError> {
    let observed = patch.format_version;
    if observed == ScenePatchDocument::CURRENT_FORMAT_VERSION {
        return Ok(());
    }
    Err(SceneFormatError::PatchFormatVersion {
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
    placements: usize,
    components: usize,
    prefab_instances: usize,
    patch_operations: usize,
    overlay_declarations: usize,
    diagnostic_sources: usize,
    component_value_cost: ComponentValueCost,
    structural_logical_bytes: usize,
    overlay_nodes: usize,
}

impl SceneFileCounts {
    fn validate_entities(
        roots: &[SceneEntityId],
        entities: &[SceneEntityRecord],
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        let mut counts = Self::default();
        counts.observe_placements(roots.len(), limits)?;
        for entity in entities {
            counts.observe_entity(entity, limits)?;
        }
        validate_placement_depth(roots, entities, limits)?;
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
        self.observe_placements(entity.children.len(), limits)?;
        self.observe_diagnostic_source(limits)?;
        increase(
            &mut self.components,
            entity.components.len(),
            limits.components(),
            SceneFileBudgetKind::Components,
        )?;
        for component in entity.components.keys() {
            self.observe_diagnostic_source(limits)?;
            self.observe_structural_bytes(component.as_str().len(), limits)?;
        }
        for component in entity.components.values() {
            self.observe_component_record(component, limits)?;
        }
        if let Some(prefab) = &entity.prefab {
            self.observe_prefab_instance(prefab, limits)?;
        }
        Ok(())
    }

    fn observe_prefab_instance(
        &mut self,
        prefab: &PrefabInstance,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        let used_nodes = self
            .component_value_cost
            .nodes()
            .saturating_add(self.overlay_nodes);
        let used_bytes = self
            .component_value_cost
            .logical_bytes()
            .saturating_add(self.structural_logical_bytes);
        let overlay_limits = PrefabOverlayCostLimits {
            nodes: limits
                .component_value_nodes()
                .get()
                .saturating_sub(used_nodes),
            depth: limits
                .shape()
                .depth()
                .get()
                .min(limits.placement_depth().get()),
            declarations: limits
                .overlay_declarations()
                .get()
                .saturating_sub(self.overlay_declarations),
            entities: limits.entities().get().saturating_sub(self.entities),
            placements: limits.placements().get().saturating_sub(self.placements),
            components: limits.components().get().saturating_sub(self.components),
            prefab_instances: limits
                .prefab_instances()
                .get()
                .saturating_sub(self.prefab_instances),
            value_nodes: limits
                .component_value_nodes()
                .get()
                .saturating_sub(self.component_value_cost.nodes()),
            value_bytes: limits
                .component_value_bytes()
                .get()
                .saturating_sub(self.component_value_cost.logical_bytes()),
            logical_bytes: limits
                .component_value_bytes()
                .get()
                .saturating_sub(used_bytes),
        };
        let cost = prefab_instance_cost(prefab, overlay_limits)
            .map_err(|error| self.overlay_budget_error(error, limits))?;
        self.entities = self.entities.saturating_add(cost.entities);
        self.placements = self.placements.saturating_add(cost.placements);
        self.components = self.components.saturating_add(cost.components);
        self.prefab_instances = self.prefab_instances.saturating_add(cost.prefab_instances);
        self.overlay_declarations = self.overlay_declarations.saturating_add(cost.declarations);
        self.overlay_nodes = self
            .overlay_nodes
            .saturating_add(cost.nodes.saturating_sub(cost.value_cost.nodes()));
        self.structural_logical_bytes = self
            .structural_logical_bytes
            .saturating_add(cost.structural_logical_bytes);
        self.component_value_cost = self.component_value_cost.saturating_add(cost.value_cost);
        increase(
            &mut self.diagnostic_sources,
            cost.declarations
                .saturating_add(cost.components)
                .saturating_add(cost.prefab_instances),
            limits.diagnostic_sources(),
            SceneFileBudgetKind::DiagnosticSources,
        )
    }

    fn overlay_budget_error(
        &self,
        error: PrefabOverlayCostError,
        limits: SceneFileLimits,
    ) -> SceneFileBudgetError {
        let (kind, previous, maximum) = match error.kind {
            PrefabOverlayCostKind::Nodes | PrefabOverlayCostKind::ValueNodes => (
                SceneFileBudgetKind::ComponentValueNodes,
                self.component_value_cost
                    .nodes()
                    .saturating_add(self.overlay_nodes),
                limits.component_value_nodes().get(),
            ),
            PrefabOverlayCostKind::Depth => (SceneFileBudgetKind::PlacementDepth, 0, error.maximum),
            PrefabOverlayCostKind::Declarations => (
                SceneFileBudgetKind::OverlayDeclarations,
                self.overlay_declarations,
                limits.overlay_declarations().get(),
            ),
            PrefabOverlayCostKind::Entities => (
                SceneFileBudgetKind::Entities,
                self.entities,
                limits.entities().get(),
            ),
            PrefabOverlayCostKind::Placements => (
                SceneFileBudgetKind::Placements,
                self.placements,
                limits.placements().get(),
            ),
            PrefabOverlayCostKind::Components => (
                SceneFileBudgetKind::Components,
                self.components,
                limits.components().get(),
            ),
            PrefabOverlayCostKind::PrefabInstances => (
                SceneFileBudgetKind::PrefabInstances,
                self.prefab_instances,
                limits.prefab_instances().get(),
            ),
            PrefabOverlayCostKind::ValueBytes | PrefabOverlayCostKind::LogicalBytes => (
                SceneFileBudgetKind::ComponentValueBytes,
                self.component_value_cost
                    .logical_bytes()
                    .saturating_add(self.structural_logical_bytes),
                limits.component_value_bytes().get(),
            ),
        };
        SceneFileBudgetError {
            kind,
            observed: previous.saturating_add(error.observed),
            maximum,
        }
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
                ScenePatchOperation::AddEntity { entity, .. } => {
                    self.observe_placements(1, limits)?;
                    self.observe_entity(entity, limits)?;
                }
                ScenePatchOperation::RestoreEntity {
                    entity,
                    descendants,
                    ..
                } => {
                    self.observe_placements(1, limits)?;
                    self.observe_entity(entity, limits)?;
                    for descendant in descendants {
                        self.observe_entity(descendant, limits)?;
                    }
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
                    self.observe_component_record(value, limits)?;
                }
                ScenePatchOperation::SetField { value, .. } => {
                    self.observe_component_value(value.cost(), limits)?;
                }
                ScenePatchOperation::SetPrefabInstance {
                    expected, value, ..
                } => {
                    // Both exact snapshots are retained input, including the precondition side.
                    // Count their complete trees without inventing an entity or placement.
                    for prefab in expected.iter().chain(value.iter()) {
                        self.observe_prefab_instance(prefab, limits)?;
                    }
                }
                ScenePatchOperation::RemoveEntity { .. } => {
                    self.observe_placements(1, limits)?;
                }
                ScenePatchOperation::MoveEntity { .. } => {
                    self.observe_placements(2, limits)?;
                }
                ScenePatchOperation::RemoveComponent { .. }
                | ScenePatchOperation::RemoveField { .. }
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

    fn observe_placements(
        &mut self,
        amount: usize,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        increase(
            &mut self.placements,
            amount,
            limits.placements(),
            SceneFileBudgetKind::Placements,
        )
    }

    fn observe_component_value(
        &mut self,
        cost: ComponentValueCost,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        let observed = self.component_value_cost.saturating_add(cost);
        self.validate_raw_cost(observed, self.structural_logical_bytes, limits)?;
        self.component_value_cost = observed;
        Ok(())
    }

    fn observe_component_record(
        &mut self,
        record: &SceneComponentRecord,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        self.observe_structural_bytes(
            5_usize.saturating_add(record.owner().map_or(0, |owner| owner.as_str().len())),
            limits,
        )?;
        self.observe_component_value(record.value.cost(), limits)
    }

    fn observe_structural_bytes(
        &mut self,
        amount: usize,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        let observed = self.structural_logical_bytes.saturating_add(amount);
        self.validate_raw_cost(self.component_value_cost, observed, limits)?;
        self.structural_logical_bytes = observed;
        Ok(())
    }

    fn validate_raw_cost(
        &self,
        values: ComponentValueCost,
        structural_bytes: usize,
        limits: SceneFileLimits,
    ) -> Result<(), SceneFileBudgetError> {
        for (kind, observed, maximum) in [
            (
                SceneFileBudgetKind::ComponentValueNodes,
                values.nodes().saturating_add(self.overlay_nodes),
                limits.component_value_nodes().get(),
            ),
            (
                SceneFileBudgetKind::ComponentValueBytes,
                values.logical_bytes().saturating_add(structural_bytes),
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
        Ok(())
    }
}

fn validate_placement_depth(
    roots: &[SceneEntityId],
    entities: &[SceneEntityRecord],
    limits: SceneFileLimits,
) -> Result<(), SceneFileBudgetError> {
    let children = entities
        .iter()
        .map(|entity| (&entity.id, entity.children.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let mut visited = BTreeSet::<&SceneEntityId>::new();
    let mut pending = roots
        .iter()
        .rev()
        .map(|root| (root, 1_usize))
        .collect::<Vec<_>>();
    while let Some((entity, depth)) = pending.pop() {
        if depth > limits.placement_depth().get() {
            return Err(SceneFileBudgetError {
                kind: SceneFileBudgetKind::PlacementDepth,
                observed: depth,
                maximum: limits.placement_depth().get(),
            });
        }
        if !visited.insert(entity) {
            continue;
        }
        if let Some(children) = children.get(entity) {
            pending.extend(
                children
                    .iter()
                    .rev()
                    .map(|child| (child, depth.saturating_add(1))),
            );
        }
    }
    Ok(())
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

#[cfg(test)]
mod overlay_storage_tests {
    use nara_asset::{AssetPath, AssetRef};
    use nara_reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue,
    };

    use super::*;
    use crate::{
        PrefabAddition, PrefabComponentIntent, PrefabEntityTarget, PrefabOverlay,
        PrefabPropertyClaim, PrefabPropertyIntent, SourceDocumentKey, SourceDocumentKind,
    };

    fn key() -> SourceDocumentKey {
        SourceDocumentKey::named(
            "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
            "unresolved-base",
            SourceDocumentKind::Prefab,
        )
        .unwrap()
    }

    fn instance(overlay: PrefabOverlay) -> PrefabInstance {
        PrefabInstance {
            source: AssetRef::Path(AssetPath::new("missing.prefab.json").unwrap()),
            overrides: Some(overlay),
        }
    }

    #[test]
    fn json_and_ron_retain_unresolved_and_obsolete_claims_through_source_canonicalization() {
        let mut overlay = PrefabOverlay::new(key());
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(SceneEntityId::new("missing-entity").unwrap()),
            component: ComponentTypeId::new("missing.component"),
            version: ComponentSchemaVersion(99),
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("removed-field")),
                intent: PrefabPropertyIntent::Set(ComponentValue::String("original".to_owned())),
                bindings: Vec::new(),
            }],
        });
        let mut addition = PrefabAddition::new(SceneEntityId::new("added").unwrap());
        addition.components.insert(
            ComponentTypeId::new("unavailable.component"),
            SceneComponentRecord::new(ComponentSchemaVersion(99), ComponentValue::Null),
        );
        overlay.additions.push(addition);
        let mut entity = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        entity.prefab = Some(instance(overlay));
        let document = SceneDocument::from_ordered_roots([entity]);
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        for candidate in [
            SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap(),
            SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap()).unwrap(),
        ] {
            let canonical = candidate.canonicalize(&registry).unwrap();
            assert_eq!(canonical.document(), &document);
        }
        let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
        for candidate in [
            PrefabDocumentCandidate::decode_json_str(&prefab.to_json_string().unwrap()).unwrap(),
            PrefabDocumentCandidate::decode_ron_str(&prefab.to_ron_string().unwrap()).unwrap(),
        ] {
            assert_eq!(
                candidate.canonicalize(&registry).unwrap().document(),
                &prefab
            );
        }
    }

    #[test]
    fn standalone_snapshot_sides_share_all_overlay_domain_ceilings() {
        let mut overlay = PrefabOverlay::new(key());
        let mut addition = PrefabAddition::new(SceneEntityId::new("added").unwrap());
        addition.prefab = Some(instance(PrefabOverlay::new(key())));
        overlay.additions.push(addition);
        let snapshot = instance(overlay);
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("anchor").unwrap(),
            expected: Some(snapshot.clone()),
            value: Some(snapshot),
        }]);
        let exact = SceneFileLimits::default()
            .with_entities(ItemLimit::new(2).unwrap())
            .with_prefab_instances(ItemLimit::new(4).unwrap())
            .with_patch_operations(ItemLimit::new(1).unwrap())
            .with_overlay_declarations(ItemLimit::new(6).unwrap());
        assert!(patch.validate_budget(exact).is_ok());
        for (limits, kind) in [
            (
                exact.with_entities(ItemLimit::new(1).unwrap()),
                SceneFileBudgetKind::Entities,
            ),
            (
                exact.with_prefab_instances(ItemLimit::new(3).unwrap()),
                SceneFileBudgetKind::PrefabInstances,
            ),
            (
                exact.with_overlay_declarations(ItemLimit::new(5).unwrap()),
                SceneFileBudgetKind::OverlayDeclarations,
            ),
        ] {
            assert_eq!(patch.validate_budget(limits).unwrap_err().kind, kind);
        }
        let json = patch.to_json_string().unwrap();
        let ron = patch.to_ron_string().unwrap();
        for result in [
            decode_json::<ScenePatchDocument>(json.as_bytes(), exact),
            decode_ron::<ScenePatchDocument>(ron.as_bytes(), exact),
        ] {
            assert!(result.is_ok());
        }
        let insufficient = exact.with_overlay_declarations(ItemLimit::new(5).unwrap());
        for result in [
            decode_json::<ScenePatchDocument>(json.as_bytes(), insufficient),
            decode_ron::<ScenePatchDocument>(ron.as_bytes(), insufficient),
        ] {
            assert!(matches!(
                result,
                Err(SceneFormatError::Budget(SceneFileBudgetError {
                    kind: SceneFileBudgetKind::OverlayDeclarations,
                    ..
                }))
            ));
        }
    }

    #[test]
    fn metadata_is_admitted_before_encoder_copies_the_payload() {
        let mut overlay = PrefabOverlay::new(key());
        overlay.suppressions.push(PrefabEntityTarget::source(
            SceneEntityId::new("x".repeat(1_024)).unwrap(),
        ));
        let mut entity = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        entity.prefab = Some(instance(overlay));
        let document = SceneDocument::from_ordered_roots([entity]);
        let limits =
            SceneFileLimits::default().with_component_value_bytes(ByteLimit::new(512).unwrap());
        assert!(matches!(
            canonical_envelope(&document, limits),
            Err(SceneFormatError::Budget(SceneFileBudgetError {
                kind: SceneFileBudgetKind::ComponentValueBytes,
                ..
            }))
        ));
    }
}
